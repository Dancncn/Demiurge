import assert from "node:assert/strict";
import test from "node:test";

import { eventBelongsToSession, NavigationEpoch, turnBelongsToSession } from "../src/lib/navigationEpoch.ts";

function deferred() {
  let resolve = (_value: string) => {};
  const promise = new Promise<string>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}

test("a delayed older navigation cannot overwrite the newest session", async () => {
  const navigation = new NavigationEpoch();
  const first = deferred();
  const second = deferred();
  const committed: string[] = [];

  const firstTicket = navigation.begin("session-a");
  const firstRequest = first.promise.then((sessionId) => {
    if (navigation.accepts(firstTicket, sessionId)) committed.push(sessionId);
  });

  const secondTicket = navigation.begin("session-b");
  const secondRequest = second.promise.then((sessionId) => {
    if (navigation.accepts(secondTicket, sessionId)) committed.push(sessionId);
  });

  second.resolve("session-b");
  await secondRequest;
  first.resolve("session-a");
  await firstRequest;

  assert.deepEqual(committed, ["session-b"]);
});

test("refresh tickets and turn events remain bound to their captured session", () => {
  const navigation = new NavigationEpoch();
  navigation.begin("session-a");
  const refresh = navigation.capture("session-a");
  navigation.begin("session-b");

  assert.equal(navigation.accepts(refresh, "session-a"), false);
  assert.equal(turnBelongsToSession("session-b", { id: "turn-b", session_id: "session-b", status: "running" }), true);
  assert.equal(turnBelongsToSession("session-b", { id: "turn-a", session_id: "session-a", status: "completed" }), false);
  assert.equal(turnBelongsToSession("session-b"), false);
  assert.equal(eventBelongsToSession("session-b", "session-b"), true);
  assert.equal(eventBelongsToSession("session-b", "session-a"), false);
});

test("an older refresh in the same navigation epoch is discarded", () => {
  const navigation = new NavigationEpoch();
  navigation.begin("session-a");
  const firstRefresh = navigation.capture("session-a");
  const secondRefresh = navigation.capture("session-a");

  assert.equal(navigation.accepts(secondRefresh, "session-a"), true);
  assert.equal(navigation.accepts(firstRefresh, "session-a"), false);
});
