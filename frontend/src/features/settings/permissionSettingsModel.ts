import type {
  PermissionPanelState,
  PermissionRuleInput,
  PermissionRuleView,
  PermissionScope,
  ShellPolicyState,
} from "../../lib/types.ts";

export interface PermissionSettingsApi {
  permissionPanelState(): Promise<PermissionPanelState>;
  shellPolicyState(): Promise<ShellPolicyState>;
  permissionUpsertRule(input: PermissionRuleInput): Promise<PermissionPanelState>;
  permissionResetRule(scope: PermissionScope, tool: string, sessionId?: string, workspaceIdentity?: string): Promise<PermissionPanelState>;
}

type RuleDraft = Omit<PermissionRuleInput, "scope"> & { scope: Exclude<PermissionScope, "once"> };

export interface PermissionSettingsSnapshot {
  permissions: PermissionPanelState | null;
  shellPolicy: ShellPolicyState | null;
  draft: RuleDraft;
  busy: boolean;
  error: string;
}

/** Owns the immediate permission-rule operations, independently of the settings save form. */
export class PermissionSettingsModel {
  private readonly api: PermissionSettingsApi;
  private generation = 0;
  private isOpen = false;
  private mutationPending = false;
  private readonly listeners = new Set<() => void>();
  private snapshot: PermissionSettingsSnapshot = {
    permissions: null,
    shellPolicy: null,
    draft: { tool: "shell", effect: "ask", scope: "session", reason: "" },
    busy: false,
    error: "",
  };

  constructor(api: PermissionSettingsApi) {
    this.api = api;
  }

  getSnapshot = () => this.snapshot;

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => { this.listeners.delete(listener); };
  };

  private publish(patch: Partial<PermissionSettingsSnapshot>): void {
    this.snapshot = { ...this.snapshot, ...patch };
    for (const listener of this.listeners) listener();
  }

  async open(): Promise<void> {
    this.isOpen = true;
    await this.refresh();
  }

  close(): void {
    this.isOpen = false;
    this.mutationPending = false;
    this.generation += 1;
    this.publish({ busy: false });
  }

  async refresh(): Promise<void> {
    if (!this.isOpen || this.mutationPending) return;
    const generation = ++this.generation;
    this.publish({ busy: true, error: "" });
    const [permissions, shellPolicy] = await Promise.allSettled([
      this.api.permissionPanelState(), this.api.shellPolicyState(),
    ]);
    if (generation === this.generation) {
      this.publish({
        permissions: permissions.status === "fulfilled" ? permissions.value : this.snapshot.permissions,
        shellPolicy: shellPolicy.status === "fulfilled" ? shellPolicy.value : this.snapshot.shellPolicy,
        busy: false,
        error: [permissions, shellPolicy]
          .filter((result) => result.status === "rejected")
          .map((result) => String(result.reason))
          .join("\n"),
      });
    }
  }

  editRule(rule: PermissionRuleView): void {
    if (rule.scope === "once") return;
    this.publish({
      draft: {
        tool: rule.tool, effect: rule.effect, scope: rule.scope, reason: rule.reason,
        session_id: rule.session_id, workspace_identity: rule.workspace_identity,
      },
    });
  }

  updateDraft(patch: Partial<Pick<RuleDraft, "tool" | "effect" | "scope" | "reason">>): void {
    const draft = { ...this.snapshot.draft, ...patch };
    if ("tool" in patch || "scope" in patch) {
      draft.session_id = undefined;
      draft.workspace_identity = undefined;
    }
    this.publish({ draft });
  }

  async save(): Promise<void> {
    const input = { ...this.snapshot.draft };
    await this.mutate(() => this.api.permissionUpsertRule(input));
  }

  async reset(rule: PermissionRuleView): Promise<void> {
    await this.mutate(() => this.api.permissionResetRule(rule.scope, rule.tool, rule.session_id, rule.workspace_identity));
  }

  private async mutate(operation: () => Promise<PermissionPanelState>): Promise<void> {
    if (!this.isOpen || this.snapshot.busy) return;
    const generation = ++this.generation;
    this.mutationPending = true;
    this.publish({ busy: true, error: "" });
    try {
      const permissions = await operation();
      if (generation === this.generation) this.publish({ permissions });
    } catch (error) {
      if (generation === this.generation) this.publish({ error: String(error) });
    } finally {
      if (generation === this.generation) {
        this.mutationPending = false;
        this.publish({ busy: false });
      }
    }
  }
}
