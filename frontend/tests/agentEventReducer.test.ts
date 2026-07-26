import assert from "node:assert/strict";
import test from "node:test";

import { AgentEventReducer } from "../src/lib/agentEventReducer.ts";

const turn = (sessionId: string, turnId: string) => ({
  sessionId,
  turnId,
});

test("assistant_done replaces an incomplete delta stream with the canonical body", () => {
  const events = new AgentEventReducer();
  const current = turn("session-a", "turn-1");

  assert.equal(events.accept({ ...current, kind: "start" }).accepted, true);
  events.accept({ ...current, kind: "delta", text: "hello " });
  events.accept({ ...current, kind: "delta", text: "world" });

  const done = events.accept({
    ...current,
    kind: "done",
    text: "hello brave world",
  });

  assert.equal(done.accepted, true);
  assert.equal(done.streamedText, "hello world");
  assert.equal(done.canonicalText, "hello brave world");
  assert.equal(done.repaired, true);
});

test("terminal events are idempotent per session and turn", () => {
  const events = new AgentEventReducer();
  const current = turn("session-a", "turn-1");

  events.accept({ ...current, kind: "start" });
  assert.equal(events.accept({ ...current, kind: "done", text: "answer" }).accepted, true);
  assert.equal(events.accept({ ...current, kind: "done", text: "answer" }).accepted, false);

  const sameTurnInAnotherSession = events.accept({
    ...turn("session-b", "turn-1"),
    kind: "done",
    text: "another answer",
  });
  assert.equal(sameTurnInAnotherSession.accepted, true);
});

test("a newly started turn rejects late events from the superseded turn", () => {
  const events = new AgentEventReducer();
  const older = turn("session-a", "turn-old");
  const newer = turn("session-a", "turn-new");

  events.accept({ ...older, kind: "start" });
  events.accept({ ...older, kind: "delta", text: "old" });
  assert.equal(events.accept({ ...newer, kind: "start" }).accepted, true);

  assert.equal(events.accept({ ...older, kind: "delta", text: " late" }).accepted, false);
  assert.equal(events.accept({ ...older, kind: "done", text: "old late" }).accepted, false);
  assert.equal(events.accept({ ...newer, kind: "delta", text: "new" }).accepted, true);
  assert.equal(events.accept({ ...newer, kind: "done", text: "new answer" }).accepted, true);
});
