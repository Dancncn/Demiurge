import assert from "node:assert/strict";
import test from "node:test";
import {
  applyDoNotDisturb,
  parseDoNotDisturbWindow,
} from "../src/lib/companionGovernance.ts";

test("parses same-day and overnight do-not-disturb windows", () => {
  const daytime = parseDoNotDisturbWindow("12:30-14:00");
  assert.equal(daytime?.contains(13 * 60), true);
  assert.equal(daytime?.contains(15 * 60), false);

  const overnight = parseDoNotDisturbWindow("23:00 - 08:00");
  assert.equal(overnight?.contains(23 * 60 + 30), true);
  assert.equal(overnight?.contains(7 * 60 + 59), true);
  assert.equal(overnight?.contains(12 * 60), false);
});

test("active do-not-disturb suppresses proactive candidates with an explanation", () => {
  const result = applyDoNotDisturb(
    [
      { kind: "mood", priority: 3, text: "support" },
      { kind: "reminder_time", priority: 1, text: "come back" },
      { kind: "reminder_weather", priority: 1, text: "bring an umbrella" },
    ],
    "23:00-08:00",
    new Date("2026-07-22T23:30:00"),
  );

  assert.equal(result.active, true);
  assert.equal(result.suppressedCount, 2);
  assert.deepEqual(result.suggestions.map((item) => item.kind), ["mood", "reminder_suppressed"]);
  assert.match(result.suggestions[1].text, /2/);
  assert.match(result.suggestions[1].text, /23:00-08:00/);
});

test("invalid do-not-disturb text is reported without hiding candidates", () => {
  const result = applyDoNotDisturb(
    [{ kind: "reminder_time", priority: 1, text: "come back" }],
    "after midnight",
    new Date("2026-07-22T23:30:00"),
  );

  assert.equal(result.active, false);
  assert.equal(result.suppressedCount, 0);
  assert.equal(result.suggestions[0].kind, "reminder_time");
  assert.equal(result.suggestions[1].kind, "reminder_policy_invalid");
});
