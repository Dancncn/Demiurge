import assert from "node:assert/strict";
import test from "node:test";
import {
  createPomodoroNotificationSubscription,
  type PomodoroNotificationEvent,
} from "../src/lib/pomodoroNotifications.ts";

const event: PomodoroNotificationEvent = {
  title: "Focus complete",
  body: "Take a break",
};

test("application subscription keeps delivering independently of the card", async () => {
  let handler: ((value: PomodoroNotificationEvent) => void) | undefined;
  let unlistenCalls = 0;
  const shown: PomodoroNotificationEvent[] = [];
  const subscription = createPomodoroNotificationSubscription(
    async (next) => {
      handler = next;
      return () => {
        unlistenCalls += 1;
      };
    },
    (value) => shown.push(value),
  );

  await subscription.ready;
  handler?.(event);
  assert.deepEqual(shown, [event]);

  subscription.dispose();
  handler?.(event);
  assert.deepEqual(shown, [event]);
  assert.equal(unlistenCalls, 1);
});

test("dispose handles an asynchronously-created listener", async () => {
  let resolveListener: ((unlisten: () => void) => void) | undefined;
  let unlistenCalls = 0;
  const subscription = createPomodoroNotificationSubscription(
    () =>
      new Promise((resolve) => {
        resolveListener = resolve;
      }),
    () => undefined,
  );

  subscription.dispose();
  resolveListener?.(() => {
    unlistenCalls += 1;
  });
  await subscription.ready;

  assert.equal(unlistenCalls, 1);
});
