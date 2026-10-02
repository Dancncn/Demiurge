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

test("Goal continuations can complete separate answers within one engine turn", () => {
  const events = new AgentEventReducer();
  const engine = turn("session-a", "turn-goal");
  const first = { ...engine, responseId: "answer-1" };
  const continuation = { ...engine, responseId: "answer-2" };

  events.accept({ ...first, kind: "start" });
  assert.equal(events.accept({ ...first, kind: "done", text: "first answer" }).accepted, true);
  assert.equal(events.accept({ ...continuation, kind: "start" }).accepted, true);
  events.accept({ ...continuation, kind: "delta", text: "continued" });
  const done = events.accept({ ...continuation, kind: "done", text: "continued answer" });
  assert.equal(done.accepted, true);
  assert.equal(done.canonicalText, "continued answer");
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

test("closed answers and superseded engine turns cannot be reopened", () => {
  const events = new AgentEventReducer();
  const first = { ...turn("session-a", "old-turn"), responseId: "answer-1" };
  const second = { ...first, responseId: "answer-2" };
  events.accept({ ...first, kind: "start" });
  events.accept({ ...first, kind: "done", text: "first" });
  events.accept({ ...second, kind: "start" });
  events.accept({ ...second, kind: "done", text: "second" });
  assert.equal(events.accept({ ...first, kind: "done", text: "duplicate" }).accepted, false);
  assert.equal(events.accept({ ...first, kind: "start" }).accepted, false);

  const newest = { ...turn("session-a", "new-turn"), responseId: "answer-3" };
  events.accept({ ...newest, kind: "start" });
  events.accept({ ...newest, kind: "done", text: "newest" });
  // A never-before-seen answer from the old engine turn must also be rejected.
  assert.equal(events.accept({ ...first, responseId: "late-answer", kind: "start" }).accepted, false);
});

test("retrying an answer resets its stream and direct replies need no start event", () => {
  const events = new AgentEventReducer();
  const identity = { ...turn("session-a", "turn-1"), responseId: "answer-1" };
  events.accept({ ...identity, kind: "start" });
  events.accept({ ...identity, kind: "delta", text: "discarded" });
  events.accept({ ...identity, kind: "reasoning", text: "old reasoning" });
  events.accept({ ...identity, kind: "start" });
  events.accept({ ...identity, kind: "delta", text: "retry" });
  const done = events.accept({ ...identity, kind: "done", text: "retry" });
  assert.equal(done.streamedText, "retry");
  assert.equal(done.reasoning, "");
  assert.equal(events.accept({ ...identity, responseId: "slash", kind: "done", text: "direct" }).accepted, true);
});
