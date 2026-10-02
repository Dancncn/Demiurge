import type { ConfirmRequestEvent, SessionEnginePanelState, TurnRunState } from "./types.ts";

/** Owns prompt validity independently of delayed native event delivery. */
export class ToolConfirmationState {
  private sessionId = "";
  private turn: TurnRunState | undefined;
  private pending: ConfirmRequestEvent | null = null;
  private readonly closedTurns = new Set<string>();
  private readonly handledPrompts = new Set<string>();
  private readonly listeners = new Set<() => void>();

  readonly getSnapshot = () => this.pending;
  readonly subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => { this.listeners.delete(listener); };
  };

  selectSession(sessionId: string) {
    if (sessionId !== this.sessionId) this.clear();
    this.sessionId = sessionId;
  }

  updateEngine(engine: SessionEnginePanelState) {
    const next = engine.busy && !engine.cancel_requested && engine.active_turn?.status === "running"
      ? engine.active_turn : undefined;
    if (this.turn && this.turn.id !== next?.id) {
      this.remember(this.closedTurns, this.turn.id);
      this.clear();
    }
    this.turn = next;
    if (!next) this.clear();
  }

  receive(request: ConfirmRequestEvent) {
    if (!request.turn_id || request.session_id !== this.sessionId ||
      request.session_id !== this.turn?.session_id || request.turn_id !== this.turn?.id ||
      this.closedTurns.has(request.turn_id) || this.handledPrompts.has(request.id)) return false;
    this.pending = request;
    this.notify();
    return true;
  }

  /** Answer completion clears its prompt, while Goal continuation keeps its engine turn. */
  clear() {
    if (!this.pending) return;
    this.remember(this.handledPrompts, this.pending.id);
    this.pending = null;
    this.notify();
  }

  cancel() {
    if (this.turn) this.remember(this.closedTurns, this.turn.id);
    this.turn = undefined;
    this.clear();
  }

  private remember(entries: Set<string>, value: string) {
    entries.add(value);
    if (entries.size > 512) entries.delete(entries.values().next().value!);
  }

  private notify() { this.listeners.forEach((listener) => listener()); }
}
