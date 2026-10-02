import assert from "node:assert/strict";
import test from "node:test";
import { PermissionSettingsModel, type PermissionSettingsApi } from "../src/features/settings/permissionSettingsModel.ts";
import type { PermissionPanelState, PermissionRuleInput, PermissionRuleView, ShellPolicyState } from "../src/lib/types.ts";

function panel(notice: string): PermissionPanelState {
  return { rules: [], audit: [], tools: [], notices: [notice] };
}

const shell: ShellPolicyState = {
  platform: "windows", default_isolation: "standard", strict_timeout_secs: 15, max_timeout_secs: 120,
  env_allowlist: ["PATH"], strict_blocked_risks: [], risk_rules: [],
  containment: { process_group: true, kill_process_tree_on_timeout: true, filesystem_sandbox: "partial", network_sandbox: "partial", notes: [] },
};

function adapter(overrides: Partial<PermissionSettingsApi> = {}): PermissionSettingsApi {
  return {
    permissionPanelState: async () => panel("loaded"),
    shellPolicyState: async () => shell,
    permissionUpsertRule: async () => panel("saved"),
    permissionResetRule: async () => panel("reset"),
    ...overrides,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

test("opening permission settings loads rules and shell policy while keeping its draft", async () => {
  const model = new PermissionSettingsModel(adapter());
  await model.open();
  assert.deepEqual(model.getSnapshot().permissions, panel("loaded"));
  assert.deepEqual(model.getSnapshot().shellPolicy, shell);
  assert.deepEqual(model.getSnapshot().draft, { tool: "shell", effect: "ask", scope: "session", reason: "" });
  assert.equal(model.getSnapshot().busy, false);
  assert.equal(model.getSnapshot().error, "");
});

test("a late permission load cannot overwrite the newer view", async () => {
  const older = deferred<PermissionPanelState>();
  const newer = deferred<PermissionPanelState>();
  let calls = 0;
  const model = new PermissionSettingsModel(adapter({
    permissionPanelState: () => (++calls === 1 ? older.promise : newer.promise),
  }));
  const first = model.open();
  const second = model.refresh();
  newer.resolve(panel("newer"));
  await second;
  older.resolve(panel("older"));
  await first;
  assert.deepEqual(model.getSnapshot().permissions, panel("newer"));
});

test("a failed reload keeps the last permission view and exposes the error without rejecting", async () => {
  let fail = false;
  const model = new PermissionSettingsModel(adapter({
    permissionPanelState: async () => {
      if (fail) throw new Error("permission unavailable");
      return panel("known rules");
    },
  }));
  await model.open();
  fail = true;
  await assert.doesNotReject(model.open());
  assert.deepEqual(model.getSnapshot().permissions, panel("known rules"));
  assert.deepEqual(model.getSnapshot().shellPolicy, shell);
  assert.match(model.getSnapshot().error, /permission unavailable/);
  assert.equal(model.getSnapshot().busy, false);
});

test("closing and reopening invalidates older loads and their failures", async () => {
  const oldRead = deferred<PermissionPanelState>();
  const newRead = deferred<PermissionPanelState>();
  let calls = 0;
  const model = new PermissionSettingsModel(adapter({
    permissionPanelState: () => (++calls === 1 ? oldRead.promise : newRead.promise),
  }));
  const oldLoad = model.open();
  model.close();
  assert.equal(model.getSnapshot().busy, false);
  const newLoad = model.open();
  oldRead.reject(new Error("old window failed"));
  await oldLoad;
  assert.equal(model.getSnapshot().error, "");
  assert.equal(model.getSnapshot().busy, true);
  newRead.resolve(panel("reopened"));
  await newLoad;
  assert.deepEqual(model.getSnapshot().permissions, panel("reopened"));
});

test("saving an edited project rule captures its identity without discarding later draft edits", async () => {
  const saved = deferred<PermissionPanelState>();
  const inputs: PermissionRuleInput[] = [];
  const model = new PermissionSettingsModel(adapter({
    permissionUpsertRule: (input) => { inputs.push(input); return saved.promise; },
  }));
  await model.open();
  const rule: PermissionRuleView = { tool: "shell", effect: "deny", scope: "project", reason: "original", workspace_identity: "project-a", updated_at: 1 };
  model.editRule(rule);
  model.updateDraft({ reason: "save this" });
  const saving = model.save();
  assert.deepEqual(inputs, [{ tool: "shell", effect: "deny", scope: "project", reason: "save this", workspace_identity: "project-a", session_id: undefined }]);
  assert.equal(model.getSnapshot().busy, true);
  model.updateDraft({ reason: "next draft" });
  saved.resolve(panel("saved project-a"));
  await saving;
  assert.deepEqual(model.getSnapshot().permissions, panel("saved project-a"));
  assert.equal(model.getSnapshot().draft.reason, "next draft");
  assert.equal(model.getSnapshot().busy, false);
});

test("changing the edited tool or scope drops the old rule identity before saving", async () => {
  const inputs: PermissionRuleInput[] = [];
  const model = new PermissionSettingsModel(adapter({
    permissionUpsertRule: async (input) => { inputs.push(input); return panel("saved"); },
  }));
  await model.open();
  model.editRule({ tool: "shell", effect: "ask", scope: "project", reason: "", workspace_identity: "project-a", updated_at: 1 });
  model.updateDraft({ scope: "session" });
  await model.save();
  assert.equal(inputs[0].workspace_identity, undefined);
  assert.equal(inputs[0].scope, "session");
  model.editRule({ tool: "shell", effect: "ask", scope: "session", reason: "", session_id: "session-a", updated_at: 1 });
  model.updateDraft({ tool: "write_file" });
  await model.save();
  assert.equal(inputs[1].session_id, undefined);
  assert.equal(inputs[1].tool, "write_file");
});

test("reset uses the displayed rule identity and keeps rules and draft when the backend rejects it", async () => {
  const resets: unknown[][] = [];
  const model = new PermissionSettingsModel(adapter({
    permissionResetRule: async (...args) => { resets.push(args); throw new Error("workspace changed"); },
  }));
  await model.open();
  model.updateDraft({ reason: "unsaved draft" });
  const rule: PermissionRuleView = { tool: "shell", effect: "ask", scope: "project", reason: "", workspace_identity: "project-a", updated_at: 1 };
  await assert.doesNotReject(model.reset(rule));
  assert.deepEqual(resets, [["project", "shell", undefined, "project-a"]]);
  assert.deepEqual(model.getSnapshot().permissions, panel("loaded"));
  assert.equal(model.getSnapshot().draft.reason, "unsaved draft");
  assert.match(model.getSnapshot().error, /workspace changed/);
  assert.equal(model.getSnapshot().busy, false);
  await model.save();
  assert.deepEqual(model.getSnapshot().permissions, panel("saved"));
  assert.equal(model.getSnapshot().error, "");
});

test("subscribers observe loading, loaded data and edits, and can detach", async () => {
  const model = new PermissionSettingsModel(adapter());
  const observed: boolean[] = [];
  const unsubscribe = model.subscribe(() => observed.push(model.getSnapshot().busy));
  await model.open();
  assert.deepEqual(observed, [true, false]);
  const beforeEdit = model.getSnapshot();
  model.updateDraft({ reason: "edited" });
  assert.notEqual(model.getSnapshot(), beforeEdit);
  assert.deepEqual(observed, [true, false, false]);
  unsubscribe();
  model.close();
  assert.deepEqual(observed, [true, false, false]);
});

test("a rejected save keeps the editable draft and the last good rules", async () => {
  const model = new PermissionSettingsModel(adapter({
    permissionUpsertRule: async () => { throw new Error("session changed"); },
  }));
  await model.open();
  model.editRule({ tool: "shell", effect: "deny", scope: "session", reason: "keep this", session_id: "session-a", updated_at: 1 });
  await assert.doesNotReject(model.save());
  assert.deepEqual(model.getSnapshot().permissions, panel("loaded"));
  assert.equal(model.getSnapshot().draft.reason, "keep this");
  assert.equal(model.getSnapshot().draft.session_id, "session-a");
  assert.match(model.getSnapshot().error, /session changed/);
  assert.equal(model.getSnapshot().busy, false);
});

test("an in-flight save excludes another mutation or refresh and cannot update a reopened view", async () => {
  const saved = deferred<PermissionPanelState>();
  let reads = 0;
  let writes = 0;
  const model = new PermissionSettingsModel(adapter({
    permissionPanelState: async () => panel(`read-${++reads}`),
    permissionUpsertRule: () => { writes += 1; return saved.promise; },
  }));
  await model.open();
  model.updateDraft({ reason: "retained across close" });
  const saving = model.save();
  await model.save();
  await model.refresh();
  assert.equal(writes, 1);
  assert.equal(reads, 1);
  model.close();
  await model.open();
  saved.reject(new Error("failure from previous opening"));
  await saving;
  assert.deepEqual(model.getSnapshot().permissions, panel("read-2"));
  assert.equal(model.getSnapshot().draft.reason, "retained across close");
  assert.equal(model.getSnapshot().error, "");
  assert.equal(model.getSnapshot().busy, false);
});
