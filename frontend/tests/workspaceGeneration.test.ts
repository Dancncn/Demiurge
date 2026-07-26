import assert from "node:assert/strict";
import test from "node:test";

import { RequestGeneration, normalizeExplorerTab } from "../src/lib/agentEventReducer.ts";

test("only the latest response for a scope and workspace may commit", () => {
  const requests = new RequestGeneration();
  const first = requests.issue("directory:src", "workspace-a:git");
  const second = requests.issue("directory:src", "workspace-a:git");

  assert.equal(requests.accepts(first, "workspace-a:git"), false);
  assert.equal(requests.accepts(second, "workspace-a:git"), true);
  assert.equal(requests.accepts(second, "workspace-b:git"), false);
});

test("a non-Git workspace always returns the explorer to Files", () => {
  assert.equal(normalizeExplorerTab("changes", false), "files");
  assert.equal(normalizeExplorerTab("changes", true), "changes");
  assert.equal(normalizeExplorerTab("files", false), "files");
});
