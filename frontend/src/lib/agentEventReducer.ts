interface EventIdentity {
  sessionId: string;
  turnId: string;
  /** One runner answer, including its tool steps and provider retries. */
  responseId?: string;
}

export type AgentPresentationEvent = EventIdentity & (
  | { kind: "start" | "activity" | "error" | "interrupted" }
  | { kind: "delta" | "reasoning" | "done"; text: string }
);

export interface AgentEventDecision {
  accepted: boolean;
  key: string;
  reason?: "closed" | "out_of_order" | "invalid_identity";
  streamedText?: string;
  reasoning?: string;
  canonicalText?: string;
  repaired?: boolean;
}

interface ActiveResponse {
  responseId: string;
  content: string;
  reasoning: string;
}

interface SessionPresentation {
  turnId: string;
  active?: ActiveResponse;
}

function eventKey(...parts: string[]) {
  return parts.join("\u0000");
}

/**
 * Answers can finish while their engine turn continues a Goal. Retain both
 * closed answers and superseded engine turns so neither can be reopened by
 * delayed events. Legacy envelopes without responseId retain one-answer turns.
 */
export class AgentEventReducer {
  private readonly activeBySession = new Map<string, SessionPresentation>();
  private readonly closedKeys = new Set<string>();
  private readonly closedOrder: string[] = [];
  private readonly maxClosedTurns: number;

  constructor(maxClosedTurns = 512) {
    this.maxClosedTurns = maxClosedTurns;
  }

  /** The caller has committed authoritative history or begun a new submission. */
  finishTurn(sessionId: string) {
    const session = this.activeBySession.get(sessionId);
    if (!session) return;
    this.close(eventKey("turn", sessionId, session.turnId));
    session.active = undefined;
  }

  accept(event: AgentPresentationEvent): AgentEventDecision {
    const sessionId = event.sessionId.trim();
    const turnId = event.turnId.trim();
    const responseId = event.responseId === undefined ? turnId : event.responseId.trim();
    const turnKey = eventKey("turn", sessionId, turnId);
    const key = eventKey("answer", sessionId, turnId, responseId);
    if (!sessionId || !turnId || !responseId) {
      return { accepted: false, key, reason: "invalid_identity" };
    }
    if (this.closedKeys.has(key) || this.closedKeys.has(turnKey)) {
      return { accepted: false, key, reason: "closed" };
    }

    let session = this.activeBySession.get(sessionId);
    if (session && session.turnId !== turnId) {
      if (session.active && event.kind !== "start") {
        return { accepted: false, key, reason: "out_of_order" };
      }
      this.close(eventKey("turn", sessionId, session.turnId));
      session = undefined;
    }
    if (!session) {
      session = { turnId };
      this.activeBySession.set(sessionId, session);
    }

    if (session.active && session.active.responseId !== responseId) {
      if (event.kind !== "start") return { accepted: false, key, reason: "out_of_order" };
      this.close(eventKey("answer", sessionId, turnId, session.active.responseId));
      session.active = undefined;
    }
    const active = session.active ??= { responseId, content: "", reasoning: "" };

    if (event.kind === "start") {
      active.content = "";
      active.reasoning = "";
      return { accepted: true, key };
    }
    if (event.kind === "delta") {
      active.content += event.text;
      return { accepted: true, key, streamedText: active.content };
    }
    if (event.kind === "reasoning") {
      active.reasoning += event.text;
      return { accepted: true, key, reasoning: active.reasoning };
    }
    if (event.kind === "activity") {
      return { accepted: true, key };
    }

    const decision: AgentEventDecision = {
      accepted: true,
      key,
      streamedText: active.content,
      reasoning: active.reasoning,
    };
    if (event.kind === "done") {
      decision.canonicalText = event.text;
      decision.repaired = active.content !== event.text;
    }
    session.active = undefined;
    this.close(key);
    return decision;
  }

  private close(key: string) {
    if (this.closedKeys.has(key)) return;
    this.closedKeys.add(key);
    this.closedOrder.push(key);
    while (this.closedOrder.length > this.maxClosedTurns) {
      const oldest = this.closedOrder.shift();
      if (oldest) this.closedKeys.delete(oldest);
    }
  }
}

export interface RequestTicket {
  readonly scope: string;
  readonly generation: number;
  readonly workspaceKey: string;
}

/** A monotonic commit gate for independent async UI request scopes. */
export class RequestGeneration {
  private generation = 0;
  private readonly latestByScope = new Map<string, number>();

  issue(scope: string, workspaceKey: string): RequestTicket {
    this.generation += 1;
    this.latestByScope.set(scope, this.generation);
    return { scope, generation: this.generation, workspaceKey };
  }

  accepts(ticket: RequestTicket, currentWorkspaceKey: string): boolean {
    return (
      ticket.workspaceKey === currentWorkspaceKey &&
      this.latestByScope.get(ticket.scope) === ticket.generation
    );
  }

  invalidateAll() {
    this.latestByScope.clear();
  }
}

export type ExplorerTab = "files" | "changes";

export function normalizeExplorerTab(tab: ExplorerTab, isGit: boolean): ExplorerTab {
  return isGit ? tab : "files";
}

export interface ScrollGeometry {
  scrollHeight: number;
  scrollTop: number;
  clientHeight: number;
}

export function isScrollNearBottom(geometry: ScrollGeometry, threshold = 96): boolean {
  const distance = geometry.scrollHeight - geometry.scrollTop - geometry.clientHeight;
  return distance <= threshold;
}
