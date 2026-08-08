import assert from "node:assert/strict";
import test from "node:test";

import { computeSelectMenuPlacement } from "../src/lib/selectPosition.ts";

test("select menu opens upward when the viewport has more room above", () => {
  const placement = computeSelectMenuPlacement({
    trigger: { top: 700, right: 500, bottom: 740, left: 300, width: 200 },
    menu: { width: 240, height: 260 },
    viewport: { width: 900, height: 768 },
    align: "left",
    direction: "auto",
  });

  assert.equal(placement.direction, "up");
  assert.equal(placement.top, 436);
  assert.equal(placement.maxHeight, 260);
});

test("select menu stays below when it fits and is clamped inside the viewport", () => {
  const placement = computeSelectMenuPlacement({
    trigger: { top: 80, right: 895, bottom: 120, left: 795, width: 100 },
    menu: { width: 240, height: 180 },
    viewport: { width: 900, height: 768 },
    align: "right",
    direction: "auto",
  });

  assert.equal(placement.direction, "down");
  assert.equal(placement.top, 124);
  assert.equal(placement.left, 652);
  assert.equal(placement.maxHeight, 180);
});

test("select menu caps its height to the available side", () => {
  const placement = computeSelectMenuPlacement({
    trigger: { top: 330, right: 420, bottom: 370, left: 220, width: 200 },
    menu: { width: 220, height: 500 },
    viewport: { width: 640, height: 480 },
    align: "left",
    direction: "down",
  });

  assert.equal(placement.direction, "down");
  assert.equal(placement.maxHeight, 98);
});
