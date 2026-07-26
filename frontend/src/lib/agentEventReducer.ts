export type AgentPresentationEvent =
  | { kind: "start" | "activity" | "error" | "interrupted"; sessionId: string; turnId: string }
  | { kind: "delta" | "reasoning" | "done"; sessionId: string; turnId: string; text: string };

export interface AgentEventDecision {
  accepted: boolean;
  key: string;
  reason?: "closed" | "out_of_order" | "invalid_identity";
  streamedText?: string;
  reasoning?: string;
  canonicalText?: string;
  repaired?: boolean;
}

interface ActiveTurn {
  turnId: string;
  content: string;
  reasoning: string;
}

function eventKey(sessionId: string, turnId: string) {
  return `${sessionId}\u0000${turnId}`;
}

/**
 * Orders frontend presentation events independently for every session.
 * Terminal turn keys are retained in a bounded set so duplicate or delayed
 * events cannot reopen an answer after its canonical body was committed.
 */
export class AgentEventReducer {
  private readonly activeBySession = new Map<string, ActiveTurn>();
  private readonly closedKeys = new Set<string>();
  private readonly closedOrder: string[] = [];
  private readonly maxClosedTurns: number;

  constructor(maxClosedTurns = 512) {
    this.maxClosedTurns = maxClosedTurns;
  }

  accept(event: AgentPresentationEvent): AgentEventDecision {
    const sessionId = event.sessionId.trim();
    const turnId = event.turnId.trim();
    const key = eventKey(sessionId, turnId);
    if (!sessionId || !turnId) {
      return { accepted: false, key, reason: "invalid_identity" };
    }
    if (this.closedKeys.has(key)) {
      return { accepted: false, key, reason: "closed" };
    }

    let active = this.activeBySession.get(sessionId);
    if (active && active.turnId !== turnId) {
      if (event.kind !== "start") {
        return { accepted: false, key, reason: "out_of_order" };
      }
      this.close(eventKey(sessionId, active.turnId));
      active = undefined;
    }

    if (!active) {
      active = { turnId, content: "", reasoning: "" };
      this.activeBySession.set(sessionId, active);
    }

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
    this.activeBySession.delete(sessionId);
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
