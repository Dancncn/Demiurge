import { AgentEventReducer } from "./agentEventReducer.ts";
import type { AssistantErrorEvent, DisplayItem, GoalProgressEvent, Message, ToolEndEvent, ToolStartEvent, TurnEventContext } from "./types.ts";

interface FrameScheduler {
  request: (callback: () => void) => number;
  cancel: (id: number) => void;
}

interface ResponseIdentity {
  turn?: TurnEventContext;
  responseId?: string;
}

export type MessagePresentationEvent = ResponseIdentity & (
  | { kind: "start" | "interrupted" }
  | { kind: "error"; error: AssistantErrorEvent }
  | { kind: "delta" | "reasoning" | "done"; text: string }
  | { kind: "tool_start"; tool: ToolStartEvent }
  | { kind: "tool_end"; tool: ToolEndEvent }
);

export interface SubmissionTicket {
  readonly sessionId: string;
  readonly generation: number;
}

/** Owns the conversation projection, including state between rendered frames. */
export class MessageProjection {
  private readonly scheduler: FrameScheduler;
  private readonly events = new AgentEventReducer();
  private readonly listeners = new Set<() => void>();
  private items: DisplayItem[] = [];
  private sessionId = "";
  private sequence = 0;
  private generation = 0;
  private assistantId: string | null = null;
  private responseKey: string | null = null;
  private readonly toolIds = new Map<string, string>();
  private frame: number | null = null;
  private pending = { text: "", reasoning: "" };
  private awaitingAnswer = false;
  private retryText = "";
  private errorId: string | null = null;
  private eventErrorReceived = false;

  constructor(scheduler: FrameScheduler) {
    this.scheduler = scheduler;
  }

  readonly getSnapshot = () => this.items;

  readonly subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => { this.listeners.delete(listener); };
  };

  replaceHistory(sessionId: string, history: Message[]) {
    this.events.finishTurn(this.sessionId);
    this.discardPending();
    this.sessionId = sessionId;
    this.generation += 1;
    this.assistantId = null;
    this.responseKey = null;
    this.toolIds.clear();
    this.awaitingAnswer = false;
    this.errorId = null;
    this.eventErrorReceived = false;
    this.retryText = "";
    this.publish(buildHistory(history, () => this.nextId()));
  }

  beginSubmission(displayText: string, retryText: string): SubmissionTicket {
    this.finalizeAssistant();
    this.events.finishTurn(this.sessionId);
    this.generation += 1;
    this.awaitingAnswer = true;
    this.retryText = retryText;
    this.errorId = null;
    this.eventErrorReceived = false;
    this.publish([...this.items, { id: this.nextId(), kind: "user", text: displayText }]);
    return { sessionId: this.sessionId, generation: this.generation };
  }

  needsHistory(sessionId: string) {
    return sessionId === this.sessionId && this.awaitingAnswer;
  }

  failSubmission(ticket: SubmissionTicket, error: unknown) {
    if (ticket.sessionId !== this.sessionId || ticket.generation !== this.generation) return;
    if (!this.eventErrorReceived) this.reportError(error);
  }

  appendGoalProgress(event: GoalProgressEvent) {
    if (event.session_id !== this.sessionId) return false;
    if (event.status === "active") this.awaitingAnswer = true;
    this.publish([...this.items, {
      id: this.nextId(), kind: "tool", name: "goal",
      args: { turns_executed: event.turns_executed, tokens_used: event.tokens_used, token_budget: event.token_budget },
      status: event.status === "active" ? "running" : "done", result: event.message, description: "Goal progress",
    }]);
    return true;
  }

  appendWarning(text: string) {
    this.publish([...this.items, { id: this.nextId(), kind: "assistant", text, streaming: false, error: true }]);
  }

  consume(event: MessagePresentationEvent): boolean {
    if (!event.turn || event.turn.session_id !== this.sessionId) return false;
    const identity = {
      sessionId: event.turn.session_id,
      turnId: event.turn.id,
      responseId: event.responseId,
    };
    const decision = this.events.accept(
      event.kind === "delta" || event.kind === "reasoning" || event.kind === "done"
        ? { ...identity, kind: event.kind, text: event.text }
        : { ...identity, kind: event.kind === "tool_start" || event.kind === "tool_end" ? "activity" : event.kind },
    );
    if (!decision.accepted) return false;
    if (this.responseKey !== decision.key) {
      this.finalizeAssistant();
      this.responseKey = decision.key;
    }

    switch (event.kind) {
      case "start":
        // Another start inside this answer is a provider retry. Discard its
        // unfinished attempt, but preserve earlier tool/preamble items.
        this.discardPending();
        if (this.assistantId) this.publish(this.items.filter((item) => item.id !== this.assistantId));
        this.assistantId = null;
        this.awaitingAnswer = true;
        break;
      case "delta":
      case "reasoning":
        this.pending[event.kind === "delta" ? "text" : "reasoning"] += event.text;
        this.awaitingAnswer = true;
        this.scheduleFlush();
        break;
      case "done": {
        this.flush();
        const text = decision.canonicalText ?? event.text;
        if (this.assistantId) {
          this.publish(this.items.map((item) => item.id === this.assistantId && item.kind === "assistant"
            ? { ...item, text, streaming: false } : item));
        } else if (text.trim()) {
          this.publish([...this.items, { id: this.nextId(), kind: "assistant", text, streaming: false }]);
        }
        this.assistantId = null;
        this.awaitingAnswer = false;
        break;
      }
      case "tool_start": {
        this.finalizeAssistant();
        const tool = event.tool;
        const key = `${decision.key}\u0000${tool.tool_call_id}`;
        if (this.toolIds.has(key)) break;
        const id = this.nextId();
        this.toolIds.set(key, id);
        this.publish([...this.items, {
          id, kind: "tool", tool_call_id: tool.tool_call_id, name: tool.name, args: tool.args,
          status: "running", preview: tool.preview, affected_paths: tool.affected_paths,
          description: tool.description, risk: tool.risk, permission_effect: tool.permission_effect,
        }]);
        break;
      }
      case "tool_end": {
        const tool = event.tool;
        const id = this.toolIds.get(`${decision.key}\u0000${tool.tool_call_id}`);
        if (!id) break;
        this.publish(this.items.map((item) => item.id === id && item.kind === "tool" ? {
          ...item, status: tool.denied ? "denied" : tool.ok ? "done" : "failed", result: tool.result,
          duration_ms: tool.duration_ms, error_hint: tool.error_hint, source_quality: tool.source_quality,
        } : item));
        break;
      }
      case "error":
        this.eventErrorReceived = true;
        this.reportError(event.error.message, event.error);
        break;
      case "interrupted":
        this.finalizeAssistant();
        this.awaitingAnswer = false;
        break;
    }
    return true;
  }

  dispose() {
    this.discardPending();
  }

  private nextId() { return `message_${++this.sequence}`; }

  private reportError(error: unknown, event?: AssistantErrorEvent) {
    this.finalizeAssistant();
    this.awaitingAnswer = false;
    const friendly = friendlyAssistantError(error, event);
    const id = this.errorId ?? this.nextId();
    const item: DisplayItem = {
      id, kind: "assistant", text: friendly.message, streaming: false, error: true,
      errorTitle: friendly.title, errorHint: friendly.hint,
      retryText: friendly.retryable ? this.retryText : undefined,
    };
    this.publish(this.errorId ? this.items.map((current) => current.id === id ? item : current) : [...this.items, item]);
    this.errorId = id;
  }

  private publish(items: DisplayItem[]) {
    this.items = items;
    this.listeners.forEach((listener) => listener());
  }

  private discardPending() {
    if (this.frame !== null) this.scheduler.cancel(this.frame);
    this.frame = null;
    this.pending = { text: "", reasoning: "" };
  }

  private scheduleFlush() {
    if (this.frame !== null) return;
    this.frame = this.scheduler.request(() => { this.frame = null; this.flush(); });
  }

  private flush() {
    const { text, reasoning } = this.pending;
    this.discardPending();
    if (!text && !reasoning) return;
    let items = this.items;
    if (!this.assistantId) {
      this.assistantId = this.nextId();
      items = [...items, { id: this.assistantId, kind: "assistant", text: "", reasoning: "", streaming: true }];
    }
    this.publish(items.map((item) => item.id === this.assistantId && item.kind === "assistant"
      ? { ...item, text: item.text + text, reasoning: (item.reasoning ?? "") + reasoning }
      : item));
  }

  private finalizeAssistant() {
    this.flush();
    if (this.assistantId) {
      this.publish(this.items.map((item) => item.id === this.assistantId && item.kind === "assistant"
        ? { ...item, streaming: false } : item));
      this.assistantId = null;
    }
  }
}

function friendlyAssistantError(err: unknown, event?: AssistantErrorEvent) {
  const raw = event?.message || String(err);
  const lower = raw.toLowerCase();
  let title = "Request failed";
  let hint = event?.hint || "Check the provider settings and try again.";

  if (event?.kind === "llm" || lower.includes("llm") || lower.includes("model")) {
    title = "Model request failed";
    hint = event?.hint || "Verify the model name, base URL, API key, and provider capability settings.";
  }
  if (lower.includes("401") || lower.includes("403") || lower.includes("unauthorized") || lower.includes("api key")) {
    title = "Provider authentication failed";
    hint = event?.hint || "Re-save the provider API key in Settings, then retry the same request.";
  } else if (lower.includes("timeout") || lower.includes("timed out")) {
    title = "Request timed out";
    hint = event?.hint || "The provider or network was slow. Retry once; if it repeats, lower context size or switch endpoint.";
  } else if (
    lower.includes("network") ||
    lower.includes("connection") ||
    lower.includes("dns") ||
    lower.includes("econn") ||
    lower.includes("fetch")
  ) {
    title = "Network request failed";
    hint = event?.hint || "Check the endpoint and local network path. If you use a proxy, confirm the app can reach it.";
  }

  return { title, message: raw.replace(/^Error:\s*/i, ""), hint, retryable: event?.retryable ?? true };
}


function affectedPathsFromTool(name: string, args: unknown): string[] {
  if (!args || typeof args !== "object") return [];
  const value = args as Record<string, unknown>;
  if (name === "write_file" || name === "edit_file") {
    return typeof value.path === "string" && value.path.trim() ? [value.path] : [];
  }
  if (name === "multi_edit" && Array.isArray(value.edits)) {
    return Array.from(
      new Set(
        value.edits
          .map((edit) => (edit && typeof edit === "object" ? (edit as Record<string, unknown>).path : null))
          .filter((path): path is string => typeof path === "string" && Boolean(path.trim())),
      ),
    );
  }
  if (name === "apply_patch" && Array.isArray(value.hunks)) {
    return Array.from(
      new Set(
        value.hunks
          .map((hunk) => (hunk && typeof hunk === "object" ? (hunk as Record<string, unknown>).path : null))
          .filter((path): path is string => typeof path === "string" && Boolean(path.trim())),
      ),
    );
  }
  return [];
}

function buildHistory(msgs: Message[], id: () => string): DisplayItem[] {
  const out: DisplayItem[] = [];
  const results = new Map<string, string>();
  for (const m of msgs) {
    if (m.role === "tool" && m.tool_call_id) results.set(m.tool_call_id, m.content ?? "");
  }
  for (const m of msgs) {
    if (m.role === "user") {
      const text = m.content ?? "";
      if (!text.startsWith("[Goal ")) {
        out.push({ id: id(), kind: "user", text, ...(m.context ? { context: m.context } : {}) });
      }
    } else if (m.role === "assistant") {
      if (m.content && m.content.trim().toLowerCase() !== "[[minecraft:no_reply]]") {
        out.push({ id: id(), kind: "assistant", text: m.content, streaming: false });
      }
      for (const tc of m.tool_calls ?? []) {
        let args: unknown = {};
        try {
          args = JSON.parse(tc.function.arguments || "{}");
        } catch {
          args = tc.function.arguments;
        }
        out.push({
          id: id(),
          kind: "tool",
          tool_call_id: tc.id,
          name: tc.function.name,
          args,
          status: "done",
          result: results.get(tc.id),
          affected_paths: affectedPathsFromTool(tc.function.name, args),
        });
      }
    }
  }
  return out;
}
