import assert from "node:assert/strict";
import test from "node:test";
import { ToolConfirmationState } from "../src/lib/confirmationState.ts";

const engine = (id: string, session_id = "session-a") => ({
  busy: true, cancel_requested: false,
  active_turn: { id, session_id, status: "running" as const, entrypoint: "send" as const,
    input_preview: "", agent_names: [], started_at: 1, updated_at: 1 },
});
const request = (id: string, turn_id: string, session_id = "session-a") => ({
  id, turn_id, session_id, tool: "write_file", args: "{}",
});

test("stop rejects late confirmations, including after another turn starts in the same session", () => {
  const state = new ToolConfirmationState();
  state.selectSession("session-a");
  state.updateEngine(engine("turn-1"));
  assert.equal(state.receive(request("prompt-1", "turn-1")), true);
  state.cancel();
  assert.equal(state.getSnapshot(), null);
  assert.equal(state.receive(request("late-1", "turn-1")), false);
  state.updateEngine(engine("turn-2"));
  assert.equal(state.receive(request("late-2", "turn-1")), false);
  assert.equal(state.receive(request("prompt-2", "turn-2")), true);
});

test("answer completion clears its prompt while a Goal continuation can ask again", () => {
  const state = new ToolConfirmationState();
  state.selectSession("session-a");
  state.updateEngine(engine("turn-goal"));
  const first = request("prompt-1", "turn-goal");
  state.receive(first);
  state.clear();
  assert.equal(state.receive(first), false);
  assert.equal(state.receive(request("prompt-2", "turn-goal")), true);
});

test("engine completion and session navigation clear prompts and reject unowned requests", () => {
  const state = new ToolConfirmationState();
  state.selectSession("session-a");
  state.updateEngine(engine("turn-1"));
  assert.equal(state.receive({ id: "unowned", session_id: "session-a", tool: "write_file", args: "{}" }), false);
  state.receive(request("prompt-1", "turn-1"));
  state.updateEngine({ busy: false, cancel_requested: false });
  assert.equal(state.getSnapshot(), null);
  assert.equal(state.receive(request("late", "turn-1")), false);
  state.updateEngine(engine("turn-2"));
  state.receive(request("prompt-2", "turn-2"));
  state.selectSession("session-b");
  assert.equal(state.getSnapshot(), null);
  assert.equal(state.receive(request("wrong-session", "turn-2")), false);
});
