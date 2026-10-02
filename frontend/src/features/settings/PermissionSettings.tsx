import { useEffect, useState, useSyncExternalStore } from "react";
import * as api from "@/lib/api";
import type { PermissionEffect, PermissionScope } from "@/lib/types";
import { useI18n, type TFunction } from "@/lib/i18n";
import { PermissionSettingsModel } from "./permissionSettingsModel";

const inputCls =
  "md-text-field md-type-body-medium h-10 w-full rounded-md border border-[#d9d9d9] bg-white px-3 text-[#202124] outline-none transition focus:border-[#7a7f87] focus:ring-1 focus:ring-[#202124]/10";
const labelCls = "md-type-label-large mb-1.5 block text-[#5f6368]";
const secondaryButtonCls =
  "md-button md-button-outlined cf-press inline-flex items-center justify-center rounded-md border border-[#d9d9d9] bg-white px-3 text-[#333] hover:bg-[#f5f5f5] disabled:cursor-not-allowed disabled:opacity-50";

function permissionEffectLabel(effect: string, t: TFunction) {
  if (effect === "allow") return t("settings.perm.effect.allow");
  if (effect === "deny") return t("settings.perm.effect.deny");
  return t("settings.perm.effect.ask");
}

function permissionScopeLabel(scope: string, t: TFunction) {
  if (scope === "user") return t("settings.perm.scope.user");
  if (scope === "session") return t("settings.perm.scope.session");
  if (scope === "project") return t("settings.perm.scope.project");
  return t("settings.perm.scope.once");
}

function permissionRiskLabel(risk: string, t: TFunction) {
  if (risk === "read_only") return t("settings.perm.risk.readOnly");
  if (risk === "mutating") return t("settings.perm.risk.mutating");
  if (risk === "external") return t("settings.perm.risk.external");
  if (risk === "privileged") return t("settings.perm.risk.privileged");
  return risk;
}

function shellPolicyValue(value: string) {
  return value.replaceAll("_", " ");
}

function formatTime(ms: number) {
  if (!ms) return "-";
  return new Date(ms).toLocaleString();
}

function Section({
  title,
  description,
  children,
}: {
  title: string;
  description?: string;
  children: React.ReactNode;
}) {
  return (
    <section className="md-settings-section border-b border-[#eceff3] py-6 first:pt-0 last:border-b-0">
      <div className="md-settings-section-header mb-4">
        <h3 className="md-type-title-small font-semibold text-[#202124]">{title}</h3>
        {description && <p className="md-type-body-small mt-1 max-w-2xl text-[#7a8088]">{description}</p>}
      </div>
      <div className="md-settings-section-body min-w-0">{children}</div>
    </section>
  );
}

function Field({
  label,
  help,
  children,
}: {
  label: string;
  help?: string;
  children: React.ReactNode;
}) {
  return (
    <label className="block">
      <span className={labelCls}>{label}</span>
      {children}
      {help && <span className="md-type-body-small mt-1.5 block text-[#8a9099]">{help}</span>}
    </label>
  );
}

/** Keeps rule drafts alive across tab changes and dialog closes; owns all permission IPC. */
export function usePermissionSettings(open: boolean) {
  const [model] = useState(() => new PermissionSettingsModel(api));
  useEffect(() => {
    if (!open) return;
    void model.open();
    return () => model.close();
  }, [model, open]);
  return <PermissionSettings model={model} />;
}

function PermissionSettings({ model }: { model: PermissionSettingsModel }) {
  const { t } = useI18n();
  const {
    permissions: permissionState, shellPolicy: shellPolicyState,
    busy: permissionBusy, error: permissionError, draft: permissionDraft,
  } = useSyncExternalStore(model.subscribe, model.getSnapshot);
  const selectedPermissionTool = permissionState?.tools.find((tool) => tool.tool === permissionDraft.tool) ?? null;
  return (
    <Section title={t("settings.perm.title")} description={t("settings.perm.desc")}>
      <div className="mb-3 flex justify-between gap-3">
        <div className="text-[12px] leading-5 text-[#7a8088]">
          {t("settings.perm.resolutionOrder")}
        </div>
        <button
          className={secondaryButtonCls}
          type="button"
          disabled={permissionBusy}
          onClick={() => void model.refresh()}
        >
          {t("settings.perm.refresh")}
        </button>
      </div>

      <div className="mb-4 rounded-lg border border-[#e2e5ea] bg-[#fbfcfd] p-3">
        <div className="grid gap-3 md:grid-cols-[1fr_140px_140px]">
          <Field label={t("settings.perm.tool")}>
            <select
              className={inputCls}
              value={permissionDraft.tool}
              onChange={(e) =>
                model.updateDraft({ tool: e.target.value })
              }
            >
              {(permissionState?.tools.length ? permissionState.tools : []).map((tool) => (
                <option key={tool.tool} value={tool.tool}>
                  {tool.tool}
                </option>
              ))}
            </select>
          </Field>
          <Field label={t("settings.perm.effect")}>
            <select
              className={inputCls}
              value={permissionDraft.effect}
              onChange={(e) =>
                model.updateDraft({ effect: e.target.value as PermissionEffect })
              }
            >
              <option value="ask">{t("settings.perm.effect.ask")}</option>
              <option value="allow">{t("settings.perm.effect.allow")}</option>
              <option value="deny">{t("settings.perm.effect.deny")}</option>
            </select>
          </Field>
          <Field label={t("settings.perm.scope")}>
            <select
              className={inputCls}
              value={permissionDraft.scope}
              onChange={(e) =>
                model.updateDraft({ scope: e.target.value as Exclude<PermissionScope, "once"> })
              }
            >
              <option value="session">{t("settings.perm.scope.session")}</option>
              <option value="project">{t("settings.perm.scope.project")}</option>
              <option value="user">{t("settings.perm.scope.user")}</option>
            </select>
          </Field>
        </div>
        <div className="mt-3 grid gap-3 md:grid-cols-[1fr_auto]">
          <Field label={t("settings.perm.reason")}>
            <input
              className={inputCls}
              value={permissionDraft.reason}
              placeholder={t("settings.perm.reasonPlaceholder")}
              onChange={(e) => model.updateDraft({ reason: e.target.value })}
            />
          </Field>
          <button
            className="mt-[22px] inline-flex h-9 items-center justify-center rounded-md bg-[#111827] px-4 text-[12px] font-medium text-white transition hover:bg-[#2b3442] disabled:cursor-not-allowed disabled:bg-[#b8bec8]"
            type="button"
            disabled={permissionBusy || !permissionDraft.tool}
            onClick={() => void model.save()}
          >
            {t("settings.perm.saveRule")}
          </button>
        </div>
        {selectedPermissionTool && (
          <div className="mt-3 rounded-md border border-[#e8ebef] bg-white p-3 text-[12px] leading-5 text-[#6f7782]">
            <div className="font-medium text-[#202124]">
              {permissionRiskLabel(selectedPermissionTool.risk, t)} / {t("settings.perm.defaultPrefix")}{" "}
              {permissionEffectLabel(selectedPermissionTool.default_effect, t)} /{" "}
              {permissionScopeLabel(selectedPermissionTool.default_scope, t)}
            </div>
            <div className="mt-1">{selectedPermissionTool.description}</div>
            <div className="mt-1 text-[#8a9099]">{selectedPermissionTool.default_reason}</div>
            {selectedPermissionTool.card_preference && (
              <div className="mt-1 text-[#3f6212]">
                {t("settings.perm.cardOverlay")}:{" "}
                <code className="rounded bg-[#f1f3f5] px-1">
                  {selectedPermissionTool.card_preference}
                </code>
                <span className="ml-1 text-[#8a9099]">{t("settings.perm.cardOverlayHint")}</span>
              </div>
            )}
          </div>
        )}
      </div>

      {shellPolicyState && (
        <div className="mb-4 rounded-lg border border-[#e2e5ea] bg-white p-3">
          <div className="flex flex-wrap items-start justify-between gap-3">
            <div>
              <div className="text-[13px] font-semibold text-[#202124]">{t("settings.shell.title")}</div>
              <div className="mt-1 text-[12px] text-[#7a8088]">
                {t("settings.shell.summary", {
                  platform: shellPolicyState.platform,
                  isolation: shellPolicyValue(shellPolicyState.default_isolation),
                  timeout: shellPolicyState.strict_timeout_secs,
                })}
              </div>
            </div>
            <div className="flex flex-wrap gap-1 text-[11px]">
              <span className="rounded-md bg-[#eef1f5] px-2 py-1">
                {t("settings.shell.processGroup", {
                  state: shellPolicyState.containment.process_group ? t("settings.shell.on") : t("settings.shell.off"),
                })}
              </span>
              <span className="rounded-md bg-[#eef1f5] px-2 py-1">
                {t("settings.shell.treeKill", {
                  state: shellPolicyState.containment.kill_process_tree_on_timeout ? t("settings.shell.on") : t("settings.shell.off"),
                })}
              </span>
            </div>
          </div>

          <div className="mt-3 grid gap-3 md:grid-cols-2">
            <div className="rounded-md border border-[#e8ebef] bg-[#fbfcfd] p-3 text-[12px] leading-5 text-[#6f7782]">
              <div className="font-medium text-[#202124]">{t("settings.shell.containment")}</div>
              <div className="mt-1">{t("settings.shell.filesystem", { value: shellPolicyState.containment.filesystem_sandbox })}</div>
              <div>{t("settings.shell.network", { value: shellPolicyState.containment.network_sandbox })}</div>
              <div className="mt-2 flex flex-wrap gap-1">
                {shellPolicyState.strict_blocked_risks.map((risk) => (
                  <span key={risk.id} className="rounded-md bg-[#fff1f1] px-2 py-0.5 text-[#9f1d1d]">
                    {t("settings.shell.deny", { value: shellPolicyValue(risk.id) })}
                  </span>
                ))}
              </div>
            </div>
            <div className="rounded-md border border-[#e8ebef] bg-[#fbfcfd] p-3 text-[12px] leading-5 text-[#6f7782]">
              <div className="font-medium text-[#202124]">{t("settings.shell.envAllowlist")}</div>
              <div className="mt-2 flex flex-wrap gap-1">
                {shellPolicyState.env_allowlist.map((name) => (
                  <span key={name} className="rounded-md bg-white px-2 py-0.5 font-mono text-[11px] text-[#344054]">
                    {name}
                  </span>
                ))}
              </div>
            </div>
          </div>

          <div className="mt-3 rounded-md border border-[#e8ebef] bg-[#fbfcfd] p-3">
            <div className="mb-2 text-[12px] font-medium text-[#202124]">{t("settings.shell.commandPolicy")}</div>
            <div className="grid gap-2 md:grid-cols-2">
              {shellPolicyState.risk_rules.map((rule) => (
                <div key={`${rule.class.id}:${rule.reason}`} className="rounded-md bg-white p-2 text-[12px] leading-5 text-[#6f7782]">
                  <div className="flex items-center justify-between gap-2">
                    <span className="font-medium text-[#202124]">{shellPolicyValue(rule.class.id)}</span>
                    <span className={rule.blocked_in_strict ? "text-[#9f1d1d]" : "text-[#7a8088]"}>
                      {rule.blocked_in_strict ? t("settings.shell.strictDeny") : rule.class.severity}
                    </span>
                  </div>
                  <div className="mt-1">{rule.reason}</div>
                  <div className="mt-1 truncate font-mono text-[11px] text-[#8a9099]">
                    {rule.patterns.slice(0, 8).join(", ")}
                    {rule.patterns.length > 8 ? ", ..." : ""}
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>
      )}

      {(permissionError || permissionState?.notices.length) && (
        <div className="mb-3 space-y-1 rounded-lg border border-[#f0c8c8] bg-[#fff7f7] p-3 text-[12px] leading-5 text-[#8f2d2d]">
          {permissionError && <div>{permissionError}</div>}
          {permissionState?.notices.map((notice) => (
            <div key={notice}>{notice}</div>
          ))}
        </div>
      )}

      <div className="space-y-2">
        {permissionState?.rules.length ? (
          permissionState.rules.map((rule) => (
            <div
              key={`${rule.scope}:${rule.tool}:${rule.session_id ?? rule.workspace_identity ?? "user"}`}
              className="rounded-lg border border-[#e2e5ea] bg-white p-3"
            >
              <div className="flex items-start gap-3">
                <div className="min-w-0 flex-1">
                  <div className="truncate text-[13px] font-semibold text-[#202124]">{rule.tool}</div>
                  <div className="mt-1 text-[12px] text-[#7a8088]">
                    {permissionEffectLabel(rule.effect, t)} / {permissionScopeLabel(rule.scope, t)} /{" "}
                    {formatTime(rule.updated_at)}
                  </div>
                  <div className="mt-1 text-[12px] leading-5 text-[#8a9099]">{rule.reason}</div>
                  {(rule.session_id || rule.workspace_identity) && (
                    <div className="mt-1 truncate font-mono text-[10px] text-[#9aa0a9]">
                      {rule.session_id ? `session: ${rule.session_id}` : `workspace: ${rule.workspace_identity}`}
                    </div>
                  )}
                </div>
                <button
                  className={secondaryButtonCls}
                  type="button"
                  disabled={permissionBusy || rule.scope === "once"}
                  onClick={() => model.editRule(rule)}
                >
                  {t("settings.perm.edit")}
                </button>
                <button
                  className={secondaryButtonCls}
                  type="button"
                  disabled={permissionBusy}
                  onClick={() => void model.reset(rule)}
                >
                  {t("settings.perm.clear")}
                </button>
              </div>
            </div>
          ))
        ) : (
          <div className="rounded-lg border border-dashed border-[#d8dde5] bg-[#fbfcfd] p-4 text-[12px] text-[#7a8088]">
            {t("settings.perm.noRules")}
          </div>
        )}
      </div>
      <div className="mt-4 max-h-44 overflow-auto rounded-lg border border-[#e2e5ea] bg-[#fbfcfd] p-3 text-[12px] text-[#6f7782]">
        <div className="mb-2 font-semibold text-[#202124]">{t("settings.perm.recentAudit")}</div>
        {permissionState?.audit.length ? (
          permissionState.audit.slice(0, 8).map((entry) => (
            <div key={`${entry.timestamp}:${entry.tool}:${entry.reason}`} className="border-t border-[#e8ebef] py-2 first:border-t-0">
              <span className="font-medium text-[#202124]">{entry.tool}</span> /{" "}
              {permissionEffectLabel(entry.effect, t)} / {permissionScopeLabel(entry.scope, t)} /{" "}
              {formatTime(entry.timestamp)}
              <div className="text-[#8a9099]">{entry.reason}</div>
              {(entry.session_id || entry.workspace_identity) && (
                <div className="truncate font-mono text-[10px] text-[#9aa0a9]">
                  {[entry.session_id && `session: ${entry.session_id}`, entry.workspace_identity && `workspace: ${entry.workspace_identity}`]
                    .filter(Boolean)
                    .join(" / ")}
                </div>
              )}
            </div>
          ))
        ) : (
          <div>{t("settings.perm.noAudit")}</div>
        )}
      </div>
    </Section>
  );
}
