/**
 * Edits the dedicated Minecraft integration while persisting it through the
 * existing MCP server contract. The independent Minecraft project remains a
 * child process and never becomes a Demiurge build dependency.
 */
import { useState } from "react";
import { pickFile, pickFolder } from "@/lib/folderPicker";
import { useI18n } from "@/lib/i18n";
import type { McpServerConfig, McpServerView } from "@/lib/types";
import { Select } from "@/shared/components/Select";
import {
  findMinecraftMcpServer,
  minecraftProjectPaths,
  readMinecraftMcpSettings,
  removeMinecraftMcpServer,
  upsertMinecraftMcpServer,
  type MinecraftMcpSettings,
} from "./minecraftMcpConfig";

interface Props {
  servers: McpServerConfig[];
  runtime?: McpServerView;
  onChange: (servers: McpServerConfig[]) => void;
  onRefresh: () => void;
  isRefreshing: boolean;
}

const inputCls =
  "md-text-field md-type-body-medium h-10 w-full rounded-md border border-[#d9d9d9] bg-white px-3 text-[#202124] outline-none transition focus:border-[#7a7f87] focus:ring-1 focus:ring-[#202124]/10";
const buttonCls =
  "md-button md-button-outlined cf-press inline-flex h-10 items-center justify-center rounded-md border border-[#d9d9d9] bg-white px-3 text-[#333] hover:bg-[#f5f5f5] disabled:cursor-not-allowed disabled:opacity-50";

function Field({ label, help, children }: { label: string; help?: string; children: React.ReactNode }) {
  return (
    <label className="block min-w-0">
      <span className="md-type-label-large mb-1.5 block text-[#5f6368]">{label}</span>
      {children}
      {help && <span className="mt-1 block text-[11px] leading-4 text-[#8a9099]">{help}</span>}
    </label>
  );
}

function Toggle({ checked, label, onChange }: { checked: boolean; label: string; onChange: (value: boolean) => void }) {
  return (
    <label className="flex items-center justify-between gap-3 rounded-lg border border-[#e2e5ea] bg-[#fbfcfd] px-3 py-2.5 text-[12px] text-[#4f5661]">
      <span>{label}</span>
      <input type="checkbox" className="md-switch md-switch-compact" checked={checked} onChange={(event) => onChange(event.target.checked)} />
    </label>
  );
}

export function MinecraftSettingsPanel({ servers, runtime, onChange, onRefresh, isRefreshing }: Props) {
  const { t } = useI18n();
  const server = findMinecraftMcpServer(servers);
  const settings = readMinecraftMcpSettings(server);
  const [pathError, setPathError] = useState("");

  function update(patch: Partial<MinecraftMcpSettings>) {
    onChange(upsertMinecraftMcpServer(servers, { ...settings, ...patch }));
  }

  async function chooseProject() {
    const outcome = await pickFolder(t("settings.minecraft.pickProject"));
    if (outcome.status === "selected") {
      setPathError("");
      update(minecraftProjectPaths(outcome.path));
    } else if (outcome.status === "failed") {
      setPathError(outcome.error);
    } else if (outcome.status === "unavailable") {
      setPathError(t("settings.minecraft.desktopOnly"));
    }
  }

  async function chooseNode() {
    const outcome = await pickFile(t("settings.minecraft.pickNode"), navigator.userAgent.includes("Windows") ? ["exe"] : undefined);
    if (outcome.status === "selected") {
      setPathError("");
      update({ nodeCommand: outcome.path });
    } else if (outcome.status === "failed") {
      setPathError(outcome.error);
    } else if (outcome.status === "unavailable") {
      setPathError(t("settings.minecraft.desktopOnly"));
    }
  }

  if (!server) {
    return (
      <div className="rounded-xl border border-dashed border-[#cfd6df] bg-[#fbfcfd] p-5">
        <div className="text-[14px] font-semibold text-[#202124]">{t("settings.minecraft.notConfigured")}</div>
        <p className="mt-1 max-w-2xl text-[12px] leading-5 text-[#6f7782]">{t("settings.minecraft.notConfiguredDesc")}</p>
        <button className={`${buttonCls} mt-4`} type="button" onClick={() => update({})}>
          {t("settings.minecraft.configure")}
        </button>
      </div>
    );
  }

  const status = runtime?.status ?? (settings.enabled ? "pending" : "disabled");
  return (
    <div className="space-y-5">
      <div className="rounded-xl border border-[#dfe5ec] bg-[#f7faff] p-4">
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-[14px] font-semibold text-[#202124]">{t("settings.minecraft.connection")}</span>
          <span className="rounded-md bg-white px-2 py-1 text-[11px] text-[#59616d]">{status}</span>
          {runtime?.tool_count ? <span className="rounded-md bg-white px-2 py-1 text-[11px] text-[#59616d]">{t("settings.minecraft.tools", { n: runtime.tool_count })}</span> : null}
          <button className={`${buttonCls} ml-auto`} type="button" disabled={isRefreshing} onClick={onRefresh}>
            {t("settings.minecraft.refresh")}
          </button>
        </div>
        <p className="mt-2 text-[12px] leading-5 text-[#66717f]">{t("settings.minecraft.singleLlm")}</p>
        {runtime?.error && <div className="mt-3 rounded-md border border-[#ffd7d7] bg-white p-2 text-[12px] text-[#b42318]">{runtime.error}</div>}
      </div>

      <div>
        <h3 className="text-[13px] font-semibold text-[#202124]">{t("settings.minecraft.runtime")}</h3>
        <div className="mt-3 grid gap-3 sm:grid-cols-2">
          <Field label={t("settings.minecraft.project")} help={t("settings.minecraft.projectHelp")}>
            <div className="flex gap-2">
              <input className={inputCls} value={settings.bridgeScript} onChange={(event) => update({ bridgeScript: event.target.value })} />
              <button className={buttonCls} type="button" onClick={chooseProject}>{t("settings.minecraft.choose")}</button>
            </div>
          </Field>
          <Field label={t("settings.minecraft.node")} help={t("settings.minecraft.nodeHelp")}>
            <div className="flex gap-2">
              <input className={inputCls} value={settings.nodeCommand} onChange={(event) => update({ nodeCommand: event.target.value })} />
              <button className={buttonCls} type="button" onClick={chooseNode}>{t("settings.minecraft.choose")}</button>
            </div>
          </Field>
          <Field label={t("settings.minecraft.dataDir")}>
            <input className={inputCls} value={settings.dataDir} onChange={(event) => update({ dataDir: event.target.value })} />
          </Field>
          <Toggle checked={settings.enabled} label={t("settings.minecraft.enabled")} onChange={(enabled) => update({ enabled })} />
        </div>
        {pathError && <div className="mt-2 text-[12px] text-[#b42318]">{pathError}</div>}
      </div>

      <div>
        <h3 className="text-[13px] font-semibold text-[#202124]">{t("settings.minecraft.server")}</h3>
        <div className="mt-3 grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
          <Field label={t("settings.minecraft.host")}><input className={inputCls} value={settings.host} onChange={(event) => update({ host: event.target.value })} /></Field>
          <Field label={t("settings.minecraft.port")}><input className={inputCls} type="number" min={1} max={65535} value={settings.port} onChange={(event) => update({ port: event.target.value })} /></Field>
          <Field label={t("settings.minecraft.version")} help={t("settings.minecraft.versionHelp")}><input className={inputCls} value={settings.version} onChange={(event) => update({ version: event.target.value })} /></Field>
          <Field label={t("settings.minecraft.username")} help={t("settings.minecraft.usernameHelp")}><input className={inputCls} value={settings.username} onChange={(event) => update({ username: event.target.value })} /></Field>
          <Field label={t("settings.minecraft.owner")} help={t("settings.minecraft.ownerHelp")}><input className={inputCls} value={settings.owner} onChange={(event) => update({ owner: event.target.value })} /></Field>
          <Field label={t("settings.minecraft.auth")}>
            <Select value={settings.auth} onChange={(value) => update({ auth: value as "offline" | "microsoft" })} options={[{ value: "offline", label: t("settings.minecraft.authOffline") }, { value: "microsoft", label: "Microsoft" }]} />
          </Field>
          <Field label={t("settings.minecraft.reconnect")}><input className={inputCls} type="number" min={1000} value={settings.reconnectDelayMs} onChange={(event) => update({ reconnectDelayMs: event.target.value })} /></Field>
          <Field label={t("settings.minecraft.maxDistance")}><input className={inputCls} type="number" min={1} value={settings.maxActionDistance} onChange={(event) => update({ maxActionDistance: event.target.value })} /></Field>
        </div>
      </div>

      <div>
        <h3 className="text-[13px] font-semibold text-[#202124]">{t("settings.minecraft.safety")}</h3>
        <div className="mt-3 grid gap-3 sm:grid-cols-3">
          <Toggle checked={settings.allowPvp} label={t("settings.minecraft.allowPvp")} onChange={(allowPvp) => update({ allowPvp })} />
          <Toggle checked={settings.allowDropItems} label={t("settings.minecraft.allowDrop")} onChange={(allowDropItems) => update({ allowDropItems })} />
          <Toggle checked={settings.autoResume} label={t("settings.minecraft.autoResume")} onChange={(autoResume) => update({ autoResume })} />
        </div>
      </div>

      <details className="rounded-lg border border-[#e2e5ea] bg-[#fbfcfd] p-3">
        <summary className="cursor-pointer text-[13px] font-semibold text-[#202124]">{t("settings.minecraft.vision")}</summary>
        <div className="mt-3 grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
          <Toggle checked={settings.visionEnabled} label={t("settings.minecraft.visionEnabled")} onChange={(visionEnabled) => update({ visionEnabled })} />
          <Toggle checked={settings.vlmEnabled} label={t("settings.minecraft.vlmEnabled")} onChange={(vlmEnabled) => update({ vlmEnabled })} />
          <Field label={t("settings.minecraft.frameUrl")}><input className={inputCls} value={settings.visionFrameUrl} onChange={(event) => update({ visionFrameUrl: event.target.value })} /></Field>
          <Field label={t("settings.minecraft.visionInterval")}><input className={inputCls} type="number" min={100} value={settings.visionIntervalMs} onChange={(event) => update({ visionIntervalMs: event.target.value })} /></Field>
          <Field label={t("settings.minecraft.residualThreshold")}><input className={inputCls} type="number" min={1} max={255} value={settings.residualThreshold} onChange={(event) => update({ residualThreshold: event.target.value })} /></Field>
          <Field label={t("settings.minecraft.minRegion")}><input className={inputCls} type="number" min={1} value={settings.minRegionPixels} onChange={(event) => update({ minRegionPixels: event.target.value })} /></Field>
        </div>
      </details>

      <button className={`${buttonCls} text-[#b42318]`} type="button" onClick={() => onChange(removeMinecraftMcpServer(servers))}>
        {t("settings.minecraft.remove")}
      </button>
    </div>
  );
}
