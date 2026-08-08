import assert from "node:assert/strict";
import test from "node:test";

import { PetActivityState } from "../src/features/pet/petRuntime.ts";

test("pet activity returns to thinking after all concurrent tools finish", () => {
  const state = new PetActivityState();

  assert.equal(state.dispatch({ type: "agent.thinking" }).action, "thinking");
  assert.equal(
    state.dispatch({ type: "agent.tool_started", toolCallId: "write-1", category: "working" }).action,
    "working",
  );
  assert.equal(
    state.dispatch({ type: "agent.tool_started", toolCallId: "read-1", category: "reviewing" }).action,
    "working",
  );
  assert.equal(state.dispatch({ type: "agent.tool_finished", toolCallId: "write-1", ok: true }).action, "reviewing");
  assert.equal(state.dispatch({ type: "agent.tool_finished", toolCallId: "read-1", ok: true }).action, "thinking");
});

test("terminal reactions settle to idle while taps restore current activity", () => {
  const state = new PetActivityState();

  state.dispatch({ type: "agent.thinking" });
  assert.equal(state.dispatch({ type: "interaction.tap" }).action, "tap");
  assert.equal(state.dispatch({ type: "animation.completed" }).action, "thinking");

  assert.equal(state.dispatch({ type: "agent.success" }).action, "success");
  assert.equal(state.dispatch({ type: "animation.completed" }).action, "idle");
});

test("tool failures take priority and then restore remaining work", () => {
  const state = new PetActivityState();

  state.dispatch({ type: "agent.tool_started", toolCallId: "write-1", category: "working" });
  state.dispatch({ type: "agent.tool_started", toolCallId: "read-1", category: "reviewing" });
  assert.equal(state.dispatch({ type: "agent.tool_finished", toolCallId: "write-1", ok: false }).action, "failure");
  assert.equal(state.dispatch({ type: "animation.completed" }).action, "reviewing");
});

test("hover reactions restore the active agent state when the pointer leaves", () => {
  const state = new PetActivityState();

  state.dispatch({ type: "agent.thinking" });
  assert.equal(state.dispatch({ type: "interaction.hover_started" }).action, "hover");
  assert.equal(state.dispatch({ type: "interaction.hover_ended" }).action, "thinking");
});

test("drag motion plays directional walking and restores the previous state", () => {
  const state = new PetActivityState();

  state.dispatch({ type: "agent.thinking" });
  state.dispatch({ type: "interaction.hover_started" });
  assert.equal(state.dispatch({ type: "motion.started", direction: "right" }).action, "movingRight");
  assert.equal(state.dispatch({ type: "motion.started", direction: "left" }).action, "movingLeft");
  assert.equal(state.dispatch({ type: "motion.ended" }).action, "hover");
});
