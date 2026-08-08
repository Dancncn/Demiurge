import type { TurnEventContext } from "@/lib/types";

export interface NavigationTicket {
  readonly epoch: number;
  readonly request: number;
  readonly expectedSessionId?: string;
}

/**
 * Monotonic commit gate for session navigation and navigation-owned refreshes.
 * Starting a navigation invalidates every older ticket; background refreshes
 * capture the current epoch without superseding an in-flight navigation.
 */
export class NavigationEpoch {
  private epoch = 0;
  private request = 0;

  begin(expectedSessionId?: string): NavigationTicket {
    this.epoch += 1;
    this.request = 0;
    return { epoch: this.epoch, request: this.request, expectedSessionId };
  }

  capture(expectedSessionId?: string): NavigationTicket {
    this.request += 1;
    return { epoch: this.epoch, request: this.request, expectedSessionId };
  }

  accepts(ticket: NavigationTicket, actualSessionId: string): boolean {
    return (
      ticket.epoch === this.epoch && ticket.request === this.request &&
      (ticket.expectedSessionId === undefined || ticket.expectedSessionId === actualSessionId)
    );
  }
}

export function turnBelongsToSession(activeSessionId: string, turn?: TurnEventContext): boolean {
  return eventBelongsToSession(activeSessionId, turn?.session_id);
}

export function eventBelongsToSession(activeSessionId: string, eventSessionId?: string): boolean {
  return Boolean(activeSessionId && eventSessionId === activeSessionId);
}
