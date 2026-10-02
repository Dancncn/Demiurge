import assert from "node:assert/strict";
import test from "node:test";
import { MessageProjection } from "../src/lib/messageProjection.ts";

function fixture() {
  let nextId = 0;
  const frames = new Map<number, () => void>();
  const projection = new MessageProjection({
    request: (callback) => { const id = ++nextId; frames.set(id, callback); return id; },
    cancel: (id) => { frames.delete(id); },
  });
  projection.replaceHistory("session-a", []);
  return {
    projection,
    paint() { const pending = [...frames.values()]; frames.clear(); pending.forEach((callback) => callback()); },
    pendingFrames: () => frames.size,
  };
}

const identity = (responseId: string, turnId = "turn-goal", sessionId = "session-a") => ({
  turn: { id: turnId, session_id: sessionId, status: "running" as const },
  responseId,
});

test("one engine turn projects both Goal answers and ignores duplicate completion", () => {
  const { projection, paint, pendingFrames } = fixture();
  projection.beginSubmission("question", "question");
  for (const [responseId, text] of [["answer-1", "first answer"], ["answer-2", "continued answer"]]) {
    projection.consume({ ...identity(responseId), kind: "start" });
    projection.consume({ ...identity(responseId), kind: "delta", text: "partial" });
    projection.consume({ ...identity(responseId), kind: "delta", text: " stream" });
    assert.equal(pendingFrames(), 1);
    paint();
    projection.consume({ ...identity(responseId), kind: "done", text });
    assert.equal(projection.consume({ ...identity(responseId), kind: "done", text }), false);
  }
  assert.deepEqual(projection.getSnapshot().filter((item) => item.kind === "assistant").map((item) => item.text), [
    "first answer", "continued answer",
  ]);
  assert.equal(projection.needsHistory("session-a"), false);
});

test("tool preambles do not satisfy final-answer recovery and survive the next model step", () => {
  const { projection, paint } = fixture();
  projection.beginSubmission("inspect", "inspect");
  const current = identity("answer-1");
  projection.consume({ ...current, kind: "start" });
  projection.consume({ ...current, kind: "delta", text: "I will inspect the file." });
  paint();
  projection.consume({ ...current, kind: "tool_start", tool: {
    tool_call_id: "call-1", name: "read_file", args: { path: "a.txt" },
  } });
  projection.consume({ ...current, kind: "tool_end", tool: {
    tool_call_id: "call-1", name: "read_file", ok: true, denied: false, result: "file body", duration_ms: 10,
  } });
  assert.equal(projection.needsHistory("session-a"), true);
  projection.consume({ ...current, kind: "start" });
  projection.consume({ ...current, kind: "done", text: "The file is valid." });
  assert.deepEqual(projection.getSnapshot().map((item) => item.kind), ["user", "assistant", "tool", "assistant"]);
  const tool = projection.getSnapshot().find((item) => item.kind === "tool");
  assert.equal(tool?.status, "done");
  assert.equal(tool?.result, "file body");
  assert.equal(projection.needsHistory("session-a"), false);
});

test("IPC rejection and assistant error update one error regardless of arrival order", () => {
  for (const eventFirst of [true, false]) {
    const { projection } = fixture();
    const ticket = projection.beginSubmission("question", "question with attachments");
    const current = identity("answer-error");
    projection.consume({ ...current, kind: "start" });
    projection.consume({ ...current, kind: "delta", text: "partial answer" });
    const emittedError = () => projection.consume({ ...current, kind: "error", error: {
      kind: "llm", message: "model unavailable", hint: "try another model", retryable: false,
    } });
    const rejectedCommand = () => projection.failSubmission(ticket, new Error("model unavailable"));
    if (eventFirst) { emittedError(); rejectedCommand(); } else { rejectedCommand(); emittedError(); }
    const errors = projection.getSnapshot().filter((item) => item.kind === "assistant").filter((item) => item.error);
    assert.equal(errors.length, 1);
    assert.equal(errors[0].text, "model unavailable");
    assert.equal(errors[0].errorHint, "try another model");
    assert.equal(errors[0].retryText, undefined);
    assert.equal(projection.needsHistory("session-a"), false);
  }
});

test("interruption commits the last buffered text and prevents a late answer reopening", () => {
  const { projection, paint, pendingFrames } = fixture();
  projection.beginSubmission("question", "question");
  const current = identity("interrupted");
  projection.consume({ ...current, kind: "start" });
  projection.consume({ ...current, kind: "delta", text: "kept partial" });
  projection.consume({ ...current, kind: "interrupted" });
  assert.equal(pendingFrames(), 0);
  paint();
  const answer = projection.getSnapshot().find((item) => item.kind === "assistant");
  assert.equal(answer?.text, "kept partial");
  assert.equal(answer?.streaming, false);
  assert.equal(projection.consume({ ...current, kind: "delta", text: "late" }), false);
  assert.equal(projection.needsHistory("session-a"), false);
});

test("history replacement clears buffered work and rebuilds tool results without synthetic Goal prompts", () => {
  const { projection, paint, pendingFrames } = fixture();
  const ticket = projection.beginSubmission("old question", "old question");
  projection.consume({ ...identity("old"), kind: "delta", text: "discard this buffer" });
  projection.replaceHistory("session-b", [
    { role: "user", content: "saved question" },
    { role: "assistant", content: "Checking", tool_calls: [{
      id: "saved-tool", type: "function", function: { name: "write_file", arguments: '{"path":"a.txt"}' },
    }] },
    { role: "tool", tool_call_id: "saved-tool", content: "saved result" },
    { role: "user", content: "[Goal continuation #2]" },
    { role: "assistant", content: "saved answer" },
  ]);
  assert.equal(pendingFrames(), 0);
  paint();
  projection.failSubmission(ticket, new Error("late old failure"));
  assert.equal(projection.consume({ ...identity("old"), kind: "done", text: "late old answer" }), false);
  assert.deepEqual(projection.getSnapshot().map((item) => item.kind), ["user", "assistant", "tool", "assistant"]);
  const tool = projection.getSnapshot().find((item) => item.kind === "tool");
  assert.equal(tool?.result, "saved result");
  assert.deepEqual(tool?.affected_paths, ["a.txt"]);
  assert.equal(projection.needsHistory("session-b"), false);
});

test("provider retry removes only its unfinished attempt, including reasoning and buffered deltas", () => {
  const { projection, paint } = fixture();
  projection.beginSubmission("question", "question");
  const current = identity("answer-retry");
  projection.consume({ ...current, kind: "start" });
  projection.consume({ ...current, kind: "reasoning", text: "discarded reasoning" });
  projection.consume({ ...current, kind: "delta", text: "discarded visible text" });
  paint();
  projection.consume({ ...current, kind: "delta", text: "discarded unpainted text" });
  projection.consume({ ...current, kind: "start" });
  projection.consume({ ...current, kind: "delta", text: "fresh answer" });
  projection.consume({ ...current, kind: "done", text: "fresh canonical answer" });
  paint();
  const answers = projection.getSnapshot().filter((item) => item.kind === "assistant");
  assert.equal(answers.length, 1);
  assert.equal(answers[0].text, "fresh canonical answer");
  assert.equal(answers[0].reasoning, "");
});

test("direct slash replies and identical Goal answers remain separate without start or delta", () => {
  const { projection } = fixture();
  projection.beginSubmission("/workflows", "/workflows");
  projection.consume({ ...identity("slash-reply"), kind: "done", text: "same answer" });
  projection.consume({ ...identity("goal-reply"), kind: "done", text: "same answer" });
  assert.deepEqual(projection.getSnapshot().filter((item) => item.kind === "assistant").map((item) => item.text), [
    "same answer", "same answer",
  ]);
});

test("old answers and old engine turns cannot alter the current conversation", () => {
  const { projection } = fixture();
  projection.beginSubmission("question", "question");
  const first = identity("answer-old", "turn-old");
  const latest = identity("answer-new", "turn-new");
  projection.consume({ ...first, kind: "start" });
  projection.consume({ ...first, kind: "done", text: "old answer" });
  projection.consume({ ...latest, kind: "start" });
  projection.consume({ ...latest, kind: "done", text: "new answer" });
  const before = projection.getSnapshot();
  assert.equal(projection.consume({ ...first, kind: "start" }), false);
  assert.equal(projection.consume({ ...first, responseId: "unseen-old-answer", kind: "done", text: "late" }), false);
  assert.equal(projection.getSnapshot(), before);
});

test("Goal progress requires final-answer recovery until its continuation actually completes", () => {
  const { projection } = fixture();
  projection.beginSubmission("question", "question");
  projection.consume({ ...identity("first"), kind: "done", text: "first answer" });
  const progress = { session_id: "session-a", status: "active", message: "continuing", turns_executed: 2, tokens_used: 5 };
  assert.equal(projection.appendGoalProgress(progress), true);
  assert.equal(projection.needsHistory("session-a"), true);
  assert.equal(projection.appendGoalProgress({ ...progress, session_id: "session-other" }), false);
  projection.consume({ ...identity("continuation"), kind: "done", text: "second answer" });
  assert.equal(projection.needsHistory("session-a"), false);
});

test("authoritative history recovery closes its stream and allows the next direct reply", () => {
  const { projection } = fixture();
  projection.beginSubmission("question", "question");
  const missingDone = identity("answer-1", "turn-1");
  projection.consume({ ...missingDone, kind: "start" });
  projection.consume({ ...missingDone, kind: "delta", text: "partial" });
  projection.replaceHistory("session-a", [
    { role: "user", content: "question" }, { role: "assistant", content: "saved final answer" },
  ]);
  assert.equal(projection.consume({ ...missingDone, kind: "done", text: "saved final answer" }), false);
  projection.beginSubmission("/skills", "/skills");
  assert.equal(projection.consume({ ...identity("slash", "turn-2"), kind: "done", text: "skill list" }), true);
  assert.deepEqual(projection.getSnapshot().filter((item) => item.kind === "assistant").map((item) => item.text), [
    "saved final answer", "skill list",
  ]);
});
