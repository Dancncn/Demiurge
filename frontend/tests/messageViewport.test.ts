import assert from "node:assert/strict";
import test from "node:test";

import { isScrollNearBottom } from "../src/lib/agentEventReducer.ts";

test("streaming auto-scroll remains pinned only near the bottom", () => {
  assert.equal(
    isScrollNearBottom({ scrollHeight: 1_000, scrollTop: 680, clientHeight: 250 }),
    true,
  );
  assert.equal(
    isScrollNearBottom({ scrollHeight: 1_000, scrollTop: 500, clientHeight: 250 }),
    false,
  );
});
