import { useEffect, useMemo, useRef, useState, type DragEvent } from "react";
import * as api from "../lib/api";
import type {
  AgentEditorFile,
  AgentPanelState,
  AgentValidationResult,
  CompanionMemoryQueueState,
  CompanionMemorySuggestion,
  ConnectionTestResult,
  ContextPanelState,
  MemoryPanelState,
  McpPanelState,
  McpServerConfig,
  OcrDownloadProgress,
  OcrModelSource,
  OcrModelStatus,
  PackManifest,
  PermissionEffect,
  PermissionPanelState,
  PermissionScope,
  AppTheme,
  ProviderKind,
  ReasoningEffort,
  Settings,
  ShellPolicyState,
  WebDavBackupFile,
  WebDavConfig,
  WebSearchProvider,
} from "../lib/types";
import { CheckIcon, CloseIcon, DownloadIcon } from "./Icons";
import { Select } from "./Select";
import { PROVIDER_OPTIONS, PROVIDER_ICON_SET, modelContextWindow, autoContextBudget } from "../lib/providers";
import { useI18n, type TFunction } from "../lib/i18n";

interface Props {
  open: boolean;
  settings: Settings;
  packs: PackManifest[];
  agentPanel: AgentPanelState;
  initialTab?: SettingsTab;
  onClose: () => void;
  onSave: (s: Settings) => void;
  onPreviewTheme?: (theme: AppTheme) => void;
  onPacksChange: (packs: PackManifest[]) => void;
  onAgentPanelChange: (state: AgentPanelState) => void;
}

export type SettingsTab =
  | "general"
  | "provider"
  | "persona"
  | "media"
  | "companion"
  | "web"
  | "files"
  | "context"
  | "tools"
  | "voice"
  | "advanced";


const webSearchProviders: { value: WebSearchProvider; label: string; helpKey: string }[] = [
  { value: "auto", label: "Auto", helpKey: "settings.web.help.auto" },
  { value: "bing", label: "Bing", helpKey: "settings.web.help.bing" },
  { value: "duckduckgo", label: "DuckDuckGo", helpKey: "settings.web.help.duckduckgo" },
  { value: "tavily", label: "Tavily", helpKey: "settings.web.help.tavily" },
  { value: "brave", label: "Brave", helpKey: "settings.web.help.brave" },
  { value: "exa", label: "Exa", helpKey: "settings.web.help.exa" },
];

const ocrSources: { value: OcrModelSource; label: string; noteKey: string; url: string }[] = [
  {
    value: "modelscope",
    label: "ModelScope",
    noteKey: "settings.ocr.note.modelscope",
    url: "https://modelscope.cn/models/greatv/oar-ocr",
  },
  {
    value: "huggingface",
    label: "Hugging Face",
    noteKey: "settings.ocr.note.huggingface",
    url: "https://huggingface.co/monkt/paddleocr-onnx",
  },
];

const reasoningEfforts: { value: ReasoningEffort; label: string; helpKey: string }[] = [
  { value: "auto", label: "Auto", helpKey: "settings.effort.auto" },
  { value: "low", label: "Low", helpKey: "settings.effort.low" },
  { value: "medium", label: "Medium", helpKey: "settings.effort.medium" },
  { value: "high", label: "High", helpKey: "settings.effort.high" },
  { value: "xhigh", label: "XHigh", helpKey: "settings.effort.xhigh" },
  { value: "max", label: "Max", helpKey: "settings.effort.max" },
];

const themeOptions: { value: AppTheme; labelKey: string; helpKey: string }[] = [
  { value: "system", labelKey: "settings.general.theme.system", helpKey: "settings.general.theme.systemHelp" },
  { value: "light", labelKey: "settings.general.theme.light", helpKey: "settings.general.theme.lightHelp" },
  { value: "dark", labelKey: "settings.general.theme.dark", helpKey: "settings.general.theme.darkHelp" },
];

const inputCls =
  "h-9 w-full rounded-md border border-[#d9d9d9] bg-white px-3 text-[13px] text-[#202124] outline-none transition focus:border-[#7a7f87] focus:ring-1 focus:ring-[#202124]/10";
const labelCls = "mb-1.5 block text-[12px] font-medium text-[#5f6368]";
const secondaryButtonCls =
  "cf-press inline-flex h-8 items-center justify-center rounded-md border border-[#d9d9d9] bg-white px-3 text-[12px] font-medium text-[#333] hover:bg-[#f5f5f5] disabled:cursor-not-allowed disabled:opacity-50";

function formatBytes(n: number) {
  if (!Number.isFinite(n) || n <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  let value = n;
  let idx = 0;
  while (value >= 1024 && idx < units.length - 1) {
    value /= 1024;
    idx += 1;
  }
  return `${value.toFixed(idx === 0 ? 0 : 1)} ${units[idx]}`;
}

function formatTokenWindow(n: number) {
  if (!Number.isFinite(n) || n <= 0) return "";
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(n % 1_000_000 === 0 ? 0 : 1)}M`;
  if (n >= 1_000) return `${Math.round(n / 1_000)}K`;
  return String(n);
}

function formatConnectionTestResult(result: ConnectionTestResult) {
  const latency = Number.isFinite(result.latency_ms) ? ` (${result.latency_ms} ms)` : "";
  return `${result.detail}${latency}\n${result.target}`;
}

function normalizeWebSearchProvider(value: string): WebSearchProvider {
  return webSearchProviders.some((p) => p.value === value) ? (value as WebSearchProvider) : "auto";
}

function normalizeReasoningEffort(value: string): ReasoningEffort {
  return reasoningEfforts.some((effort) => effort.value === value) ? (value as ReasoningEffort) : "auto";
}

function normalizeTheme(value: string): AppTheme {
  return themeOptions.some((theme) => theme.value === value) ? (value as AppTheme) : "system";
}

function modelSupportsReasoningEffort(provider: ProviderKind, model: string) {
  const normalized = model.trim().toLowerCase().replace(/^openai\//, "");
  if (provider === "openai") {
    return (
      normalized.startsWith("o1") ||
      normalized.startsWith("o3") ||
      normalized.startsWith("o4") ||
      normalized.startsWith("gpt-5") ||
      normalized.includes("codex")
    );
  }
  if (provider === "anthropic") {
    return (
      normalized.includes("opus-4-7") ||
      normalized.includes("opus-4.7") ||
      normalized.includes("opus-4-6") ||
      normalized.includes("opus-4.6") ||
      normalized.includes("sonnet-4-6") ||
      normalized.includes("sonnet-4.6") ||
      normalized.includes("deepseek-v4-pro")
    );
  }
  if (provider === "gemini") {
    return (
      normalized.includes("gemini-2.5") ||
      normalized.includes("gemini-3") ||
      normalized.includes("thinking")
    );
  }
  return false;
}

function normalizeMediaProvider(value: string) {
  return value.trim() || "dashscope";
}

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

function downloadTextFile(fileName: string, text: string, type = "application/json") {
  const blob = new Blob([text], { type });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = fileName;
  document.body.appendChild(link);
  link.click();
  link.remove();
  window.setTimeout(() => URL.revokeObjectURL(url), 0);
}

function nextMcpServerName(servers: McpServerConfig[]) {
  let idx = servers.length + 1;
  while (servers.some((server) => server.name === `mcp-server-${idx}`)) idx += 1;
  return `mcp-server-${idx}`;
}

function createMcpServer(servers: McpServerConfig[]): McpServerConfig {
  return {
    name: nextMcpServerName(servers),
    enabled: true,
    transport: "stdio",
    command: "",
    args: [],
    env: [],
  };
}

function splitLines(value: string) {
  return value
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
}

function looksSecretEnvKey(key: string) {
  const normalized = key.toLowerCase().replace(/[^a-z0-9]+/g, "_");
  return /(api_?key|token|secret|password|passwd|credential|authorization|bearer)/.test(normalized);
}

function parseEnvLines(value: string) {
  return splitLines(value).map((line) => {
    const idx = line.indexOf("=");
    if (idx === -1) {
      const key = line.trim();
      return { key, value: "", secret: looksSecretEnvKey(key) };
    }
    const key = line.slice(0, idx).trim();
    return {
      key,
      value: line.slice(idx + 1),
      secret: looksSecretEnvKey(key),
    };
  });
}

function formatEnvLines(server: McpServerConfig) {
  return server.env.map((env) => `${env.key}=${env.value}`).join("\n");
}

function mcpStatusLabel(status: string | undefined, t: TFunction) {
  if (status === "connected") return t("settings.mcp.status.connected");
  if (status === "failed") return t("settings.mcp.status.failed");
  if (status === "pending") return t("settings.mcp.status.pending");
  if (status === "disabled") return t("settings.mcp.status.disabled");
  return t("settings.mcp.status.notStarted");
}

function ProviderMark({ short, selected }: { short: string; selected?: boolean }) {
  return (
    <span
      className={`grid size-8 shrink-0 place-items-center rounded-lg border text-[11px] font-semibold ${
        selected ? "border-[#111827] bg-[#111827] text-white" : "border-[#dcdfe4] bg-[#f8f9fb] text-[#49515c]"
      }`}
    >
      {short}
    </span>
  );
}

function ProviderLogo({
  value,
  short,
  selected,
}: {
  value: ProviderKind;
  short: string;
  selected?: boolean;
}) {
  const [failed, setFailed] = useState(false);
  if (!PROVIDER_ICON_SET.has(value) || failed) {
    return <ProviderMark short={short} selected={selected} />;
  }
  return (
    <span className="grid size-8 shrink-0 place-items-center rounded-lg border border-[#e2e5ea] bg-white p-1.5">
      <img
        src={`/providers/${value}.svg`}
        alt=""
        className="h-full w-full object-contain"
        onError={() => setFailed(true)}
      />
    </span>
  );
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
    <section className="border-b border-[#eceff3] py-5 first:pt-0 last:border-b-0">
      <div className="mb-4">
        <h3 className="text-[14px] font-semibold text-[#202124]">{title}</h3>
        {description && <p className="mt-1 max-w-2xl text-[12px] leading-5 text-[#7a8088]">{description}</p>}
      </div>
      {children}
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
      {help && <span className="mt-1.5 block text-[12px] leading-5 text-[#8a9099]">{help}</span>}
    </label>
  );
}

function ToggleRow({
  checked,
  title,
  description,
  onChange,
}: {
  checked: boolean;
  title: string;
  description: string;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className="flex items-start justify-between gap-4 rounded-lg border border-[#e2e5ea] bg-[#fbfcfd] px-3 py-3">
      <span>
        <span className="block text-[13px] font-medium text-[#202124]">{title}</span>
        <span className="mt-1 block text-[12px] leading-5 text-[#7a8088]">{description}</span>
      </span>
      <input
        className="mt-0.5 h-4 w-4 shrink-0 accent-[#111827]"
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
      />
    </label>
  );
}

function ContextMetric({ label, value }: { label: string; value: number }) {
  return (
    <div className="rounded-lg border border-[#e5e8ed] bg-white px-3 py-2">
      <div className="text-[11px] text-[#8a9099]">{label}</div>
      <div className="mt-1 text-[15px] font-semibold text-[#202124]">{value.toLocaleString()}</div>
    </div>
  );
}

function contextPct(value: number, total: number) {
  if (!Number.isFinite(value) || !Number.isFinite(total) || total <= 0) return 0;
  return Math.max(0, Math.min(100, Math.round((value / total) * 100)));
}

const contextBudgetColors: Record<string, string> = {
  system: "bg-[#334155]",
  tools: "bg-[#0f766e]",
  history: "bg-[#7c3aed]",
  output_reserve: "bg-[#c2410c]",
};

function ContextBudgetBreakdown({ state, t }: { state: ContextPanelState | null; t: TFunction }) {
  if (!state) return null;
  const maxInput = Math.max(1, state.max_input_tokens);
  const projectedPct = contextPct(state.projected_total_tokens, maxInput);
  return (
    <div className="mt-3 rounded-lg border border-[#e2e5ea] bg-white p-3">
      <div className="mb-2 flex items-center justify-between gap-3 text-[12px]">
        <span className="font-medium text-[#202124]">{t("settings.context.budgetAllocation")}</span>
        <span className={state.projected_total_tokens > state.max_input_tokens ? "text-[#b42318]" : "text-[#7a8088]"}>
          {t("settings.context.tokensRatio", {
            used: state.projected_total_tokens.toLocaleString(),
            total: state.max_input_tokens.toLocaleString(),
          })}
        </span>
      </div>
      <div className="flex h-2 overflow-hidden rounded-full bg-[#e8ebef]">
        {state.budget_items.map((item) => {
          const width = contextPct(item.tokens, maxInput);
          return (
            <div
              key={item.id}
              className={`${contextBudgetColors[item.id] ?? "bg-[#64748b]"} h-full`}
              style={{ width: `${width}%` }}
              title={`${item.label}: ${item.tokens.toLocaleString()} tokens`}
            />
          );
        })}
      </div>
      <div className="mt-1 h-1 overflow-hidden rounded-full bg-transparent">
        <div
          className={projectedPct >= 100 ? "h-full bg-[#b42318]" : "h-full bg-[#9aa3af]"}
          style={{ width: `${projectedPct}%` }}
        />
      </div>
      <div className="mt-3 grid gap-2 sm:grid-cols-2">
        {state.budget_items.map((item) => (
          <div key={item.id} className="rounded-md border border-[#edf0f4] bg-[#fbfcfd] px-3 py-2">
            <div className="flex items-center justify-between gap-2">
              <span className="flex items-center gap-2 text-[12px] font-medium text-[#202124]">
                <span className={`h-2 w-2 rounded-full ${contextBudgetColors[item.id] ?? "bg-[#64748b]"}`} />
                {item.label}
              </span>
              <span className="font-mono text-[11px] tabular-nums text-[#59616d]">
                {item.tokens.toLocaleString()}
              </span>
            </div>
            <div className="mt-1 text-[11px] leading-4 text-[#7a8088]">{item.detail}</div>
          </div>
        ))}
      </div>
    </div>
  );
}

function ContextHistoryBreakdown({ buckets, t }: { buckets: ContextPanelState["history_buckets"]; t: TFunction }) {
  if (!buckets.length) return null;
  return (
    <div className="mt-3 overflow-hidden rounded-lg border border-[#e2e5ea] bg-white">
      <div className="grid grid-cols-[minmax(0,1fr)_80px_96px] gap-3 border-b border-[#eceff3] bg-[#fbfcfd] px-3 py-2 text-[11px] font-medium uppercase text-[#7a8088]">
        <span>{t("settings.context.colHistory")}</span>
        <span className="text-right">{t("settings.context.colMessages")}</span>
        <span className="text-right">{t("settings.context.colTokens")}</span>
      </div>
      {buckets.map((bucket) => (
        <div
          key={bucket.role}
          className="grid grid-cols-[minmax(0,1fr)_80px_96px] items-center gap-3 border-b border-[#f0f2f5] px-3 py-2 text-[12px] last:border-b-0"
        >
          <span className="truncate font-medium text-[#202124]">{bucket.label}</span>
          <span className="text-right font-mono text-[11px] tabular-nums text-[#59616d]">
            {bucket.messages.toLocaleString()}
          </span>
          <span className="text-right font-mono text-[11px] tabular-nums text-[#59616d]">
            {bucket.tokens.toLocaleString()}
          </span>
        </div>
      ))}
    </div>
  );
}

function ContextMemorySources({ sources, t }: { sources: ContextPanelState["memory_sources"]; t: TFunction }) {
  if (!sources.length) return null;
  return (
    <div className="mt-3 grid gap-2">
      {sources.map((source) => (
        <div key={source.id} className="rounded-lg border border-[#e2e5ea] bg-white p-3">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div className="text-[12px] font-medium text-[#202124]">{source.label}</div>
            <div className="flex items-center gap-2 text-[11px] text-[#7a8088]">
              <span className={source.exists ? "text-[#177245]" : "text-[#8a9099]"}>
                {source.exists ? t("settings.context.loaded") : t("settings.context.missing")}
              </span>
              <span>{t("settings.context.entries", { n: source.entries.toLocaleString() })}</span>
              <span>{t("settings.context.colTokens")} {source.tokens.toLocaleString()}</span>
            </div>
          </div>
          <div className="mt-1 break-all text-[11px] leading-4 text-[#8a9099]">{source.path}</div>
        </div>
      ))}
    </div>
  );
}

function PromptSectionList({ sections, t }: { sections: ContextPanelState["prompt_sections"]; t: TFunction }) {
  if (!sections.length) return null;
  const ordered = [...sections].sort((a, b) => b.priority - a.priority);
  return (
    <div className="mt-3 overflow-hidden rounded-lg border border-[#e2e5ea] bg-white">
      <div className="grid grid-cols-[minmax(0,1fr)_82px_92px_92px] gap-3 border-b border-[#eceff3] bg-[#fbfcfd] px-3 py-2 text-[11px] font-medium uppercase text-[#7a8088]">
        <span>{t("settings.context.colPromptSections")}</span>
        <span>{t("settings.context.colPriority")}</span>
        <span className="text-right">{t("settings.context.colTokens")}</span>
        <span className="text-right">{t("settings.context.colChars")}</span>
      </div>
      <div className="max-h-52 overflow-y-auto">
        {ordered.map((section) => (
          <div
            key={section.id}
            className="grid grid-cols-[minmax(0,1fr)_82px_92px_92px] items-center gap-3 border-b border-[#f0f2f5] px-3 py-2 text-[12px] last:border-b-0"
          >
            <div className="min-w-0">
              <div className="truncate font-medium text-[#202124]">{section.title}</div>
              <div className="mt-0.5 text-[11px] text-[#8a9099]">
                {section.included ? t("settings.context.included") : t("settings.context.skipped")}
                {section.truncated ? t("settings.context.truncated") : ""}
                {section.original_chars > section.chars
                  ? t("settings.context.originalChars", { n: section.original_chars.toLocaleString() })
                  : ""}
              </div>
            </div>
            <span className="rounded-md bg-[#eef1f5] px-2 py-1 font-mono text-[11px] text-[#59616d]">
              {section.priority}
            </span>
            <span className="text-right font-mono text-[11px] tabular-nums text-[#59616d]">
              {section.tokens.toLocaleString()}
            </span>
            <span className="text-right font-mono text-[11px] tabular-nums text-[#59616d]">
              {section.chars.toLocaleString()}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

export default function SettingsDialog({
  open,
  settings,
  packs,
  agentPanel,
  initialTab = "general",
  onClose,
  onSave,
  onPreviewTheme,
  onPacksChange,
  onAgentPanelChange,
}: Props) {
  const { t, setLang } = useI18n();
  const [form, setForm] = useState<Settings>(settings);
  const [agentState, setAgentState] = useState<AgentPanelState>(agentPanel);
  const [agentFile, setAgentFile] = useState<AgentEditorFile | null>(null);
  const [agentJson, setAgentJson] = useState("");
  const [agentFileName, setAgentFileName] = useState("");
  const [agentValidation, setAgentValidation] = useState<AgentValidationResult | null>(null);
  const [agentBusy, setAgentBusy] = useState(false);
  const [agentStatus, setAgentStatus] = useState("");
  const [activeTab, setActiveTab] = useState<SettingsTab>(initialTab);
  const [providerQuery, setProviderQuery] = useState("");
  const [providerTestBusy, setProviderTestBusy] = useState(false);
  const [providerTestStatus, setProviderTestStatus] = useState("");
  const [webSearchTestBusy, setWebSearchTestBusy] = useState(false);
  const [webSearchTestStatus, setWebSearchTestStatus] = useState("");
  const [ocrStatus, setOcrStatus] = useState<OcrModelStatus | null>(null);
  const [ocrProgress, setOcrProgress] = useState<OcrDownloadProgress | null>(null);
  const [ocrBusy, setOcrBusy] = useState(false);
  const [ocrError, setOcrError] = useState("");
  const [permissionState, setPermissionState] = useState<PermissionPanelState | null>(null);
  const [shellPolicyState, setShellPolicyState] = useState<ShellPolicyState | null>(null);
  const [permissionBusy, setPermissionBusy] = useState(false);
  const [permissionDraft, setPermissionDraft] = useState<{
    tool: string;
    effect: PermissionEffect;
    scope: Exclude<PermissionScope, "once">;
    reason: string;
  }>({ tool: "shell", effect: "ask", scope: "session", reason: "" });
  const [mcpState, setMcpState] = useState<McpPanelState | null>(null);
  const [mcpBusy, setMcpBusy] = useState(false);
  const [mcpError, setMcpError] = useState("");
  const [contextState, setContextState] = useState<ContextPanelState | null>(null);
  const [memoryState, setMemoryState] = useState<MemoryPanelState | null>(null);
  const [memoryBusy, setMemoryBusy] = useState(false);
  const [memoryError, setMemoryError] = useState("");
  const [memoryDrafts, setMemoryDrafts] = useState<Record<string, { kind: string; text: string }>>({});
  const [companionMemorySuggestions, setCompanionMemorySuggestions] = useState<CompanionMemorySuggestion[]>([]);
  const [companionMemoryQueue, setCompanionMemoryQueue] = useState<CompanionMemoryQueueState | null>(null);
  const [companionMemoryBusy, setCompanionMemoryBusy] = useState(false);
  const [companionMemoryStatus, setCompanionMemoryStatus] = useState("");
  const [webdavBusy, setWebdavBusy] = useState(false);
  const [webdavStatus, setWebdavStatus] = useState("");
  const [webdavFiles, setWebdavFiles] = useState<WebDavBackupFile[]>([]);
  const [packImportBusy, setPackImportBusy] = useState(false);
  const [packImportStatus, setPackImportStatus] = useState("");
  const [packImportFailed, setPackImportFailed] = useState(false);
  const [packManifestJson, setPackManifestJson] = useState("");
  const [packManifestBusy, setPackManifestBusy] = useState(false);
  const [packManifestStatus, setPackManifestStatus] = useState("");
  const [packManifestFailed, setPackManifestFailed] = useState(false);
  const [lorePreviewQuery, setLorePreviewQuery] = useState("");
  const [lorePreviewText, setLorePreviewText] = useState("");
  const [lorePreviewBusy, setLorePreviewBusy] = useState(false);
  const [lorePreviewFailed, setLorePreviewFailed] = useState(false);
  const agentImportInputRef = useRef<HTMLInputElement>(null);
  const packImportInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (open) {
      setForm(settings);
      setAgentState(agentPanel);
      setActiveTab(initialTab);
      setProviderTestStatus("");
      setWebSearchTestStatus("");
      setWebdavStatus("");
      setWebdavFiles([]);
      setMcpError("");
      setPackImportStatus("");
      setPackImportFailed(false);
      setPackManifestStatus("");
      setPackManifestFailed(false);
      setLorePreviewQuery("");
      setLorePreviewText("");
      setLorePreviewFailed(false);
      setMemoryDrafts({});
    }
  }, [open, settings, agentPanel, initialTab]);

  useEffect(() => {
    if (!open || !form.current_pack) return;
    let cancelled = false;
    setPackManifestBusy(true);
    setPackManifestStatus("");
    setPackManifestFailed(false);
    api
      .readPackManifestJson(form.current_pack)
      .then((json) => {
        if (!cancelled) setPackManifestJson(json);
      })
      .catch((err) => {
        if (!cancelled) {
          setPackManifestJson("");
          setPackManifestFailed(true);
          setPackManifestStatus(String(err));
        }
      })
      .finally(() => {
        if (!cancelled) setPackManifestBusy(false);
      });
    return () => {
      cancelled = true;
    };
  }, [open, form.current_pack]);

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    void api.ocrModelStatus().then(
      (status) => {
        if (!cancelled) setOcrStatus(status);
      },
      (err) => {
        if (!cancelled) setOcrError(String(err));
      },
    );
    void api.contextPanelState().then(
      (state) => {
        if (!cancelled) setContextState(state);
      },
      (err) => {
        if (!cancelled) console.error("Failed to read context state", err);
      },
    );
    void api.memoryPanelState().then(
      (state) => {
        if (!cancelled) setMemoryState(state);
      },
      (err) => {
        if (!cancelled) setMemoryError(String(err));
      },
    );
    void api.companionMemorySuggestions().then(
      (suggestions) => {
        if (!cancelled) setCompanionMemorySuggestions(suggestions);
      },
      (err) => {
        if (!cancelled) setCompanionMemoryStatus(String(err));
      },
    );
    void api.companionMemoryQueueState().then(
      (queue) => {
        if (!cancelled) setCompanionMemoryQueue(queue);
      },
      (err) => {
        if (!cancelled) setCompanionMemoryStatus(String(err));
      },
    );
    void api.permissionPanelState().then(
      (state) => {
        if (!cancelled) setPermissionState(state);
      },
      (err) => {
        if (!cancelled) console.error("Failed to read permission state", err);
      },
    );
    void api.shellPolicyState().then(
      (state) => {
        if (!cancelled) setShellPolicyState(state);
      },
      (err) => {
        if (!cancelled) console.error("Failed to read shell policy state", err);
      },
    );
    void api.mcpPanelState().then(
      (state) => {
        if (!cancelled) setMcpState(state);
      },
      (err) => {
        if (!cancelled) setMcpError(String(err));
      },
    );
    return () => {
      cancelled = true;
    };
  }, [open]);

  useEffect(() => {
    if (!open) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void api.listenOcrDownloadProgress((event) => {
      if (!disposed) setOcrProgress(event);
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [open]);

  useEffect(() => {
    if (!open) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void api.listenMcpUpdated((event) => {
      if (!disposed) setMcpState(event);
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [open]);

  const selectedProvider = useMemo(
    () => PROVIDER_OPTIONS.find((p) => p.value === form.provider) ?? PROVIDER_OPTIONS[0],
    [form.provider],
  );
  const modelWindow = modelContextWindow(form.provider, form.model);
  const modelOptions = useMemo(() => {
    const current = form.model.trim();
    const models = selectedProvider.models;
    const options = current && !models.includes(current) ? [current, ...models] : models;
    return options.map((model) => {
      const window = modelContextWindow(form.provider, model);
      const currentCustom = current === model && !models.includes(model);
      return {
        value: model,
        label: model,
        hint: currentCustom
          ? t("settings.provider.currentCustomModel")
          : window
            ? t("settings.provider.contextWindow", { tokens: formatTokenWindow(window) })
            : undefined,
      };
    });
  }, [form.model, form.provider, selectedProvider.models, t]);
  const effortSupported = modelSupportsReasoningEffort(form.provider, form.model);
  const selectedWebSearchProvider = useMemo(
    () => webSearchProviders.find((p) => p.value === form.web_search_provider) ?? webSearchProviders[0],
    [form.web_search_provider],
  );
  const selectedOcrSource = useMemo(
    () => ocrSources.find((source) => source.value === form.ocr_model_source) ?? ocrSources[0],
    [form.ocr_model_source],
  );
  const selectedAgentDefinition = useMemo(
    () => (agentFile ? agentState.definitions.find((agent) => agent.name === agentFile.name) ?? null : null),
    [agentFile, agentState.definitions],
  );
  const selectedPermissionTool = useMemo(
    () => permissionState?.tools.find((tool) => tool.tool === permissionDraft.tool) ?? null,
    [permissionDraft.tool, permissionState?.tools],
  );
  const packManifestDraft = useMemo(() => {
    if (!packManifestJson.trim()) return null;
    try {
      return JSON.parse(packManifestJson) as PackManifest;
    } catch {
      return null;
    }
  }, [packManifestJson]);
  const loreEntries = packManifestDraft?.lorebook ?? [];

  if (!open) return null;

  // When the budget follows the model, keep it in sync as provider/model change.
  useEffect(() => {
    if (!form.context_budget_auto) return;
    const budget = autoContextBudget(form.provider, form.model);
    if (!budget) return;
    setForm((f) => {
      if (f.max_input_tokens === budget.maxInput && f.reserved_output_tokens === budget.reservedOutput) return f;
      return { ...f, max_input_tokens: budget.maxInput, reserved_output_tokens: budget.reservedOutput };
    });
  }, [form.provider, form.model, form.context_budget_auto]);

  const set = <K extends keyof Settings>(k: K, v: Settings[K]) => {
    const key = String(k);
    setForm((f) => ({ ...f, [k]: v }));
    if (["provider", "base_url", "api_key", "model"].includes(key)) {
      setProviderTestStatus("");
    }
    if (["web_search_provider", "tavily_api_key", "brave_search_api_key", "exa_api_key"].includes(key)) {
      setWebSearchTestStatus("");
    }
  };
  const runMemoryAction = async (action: () => Promise<MemoryPanelState>) => {
    setMemoryBusy(true);
    setMemoryError("");
    try {
      setMemoryState(await action());
    } catch (err) {
      setMemoryError(String(err));
    } finally {
      setMemoryBusy(false);
    }
  };

  const enqueueCompanionMemorySuggestion = async (id: string) => {
    setCompanionMemoryBusy(true);
    setCompanionMemoryStatus("");
    try {
      setCompanionMemoryQueue(await api.companionEnqueueMemorySuggestion(id));
      setCompanionMemorySuggestions(await api.companionMemorySuggestions());
      setCompanionMemoryStatus(t("settings.companion.memoryQueued"));
    } catch (err) {
      setCompanionMemoryStatus(String(err));
    } finally {
      setCompanionMemoryBusy(false);
    }
  };

  const saveCompanionMemoryQueueItem = async (id: string) => {
    setCompanionMemoryBusy(true);
    setCompanionMemoryStatus("");
    try {
      setCompanionMemoryQueue(await api.companionSaveMemoryQueueItem(id));
      setMemoryState(await api.memoryPanelState());
      setCompanionMemoryStatus(t("settings.companion.memorySaved"));
    } catch (err) {
      setCompanionMemoryStatus(String(err));
    } finally {
      setCompanionMemoryBusy(false);
    }
  };

  const saveAllCompanionMemoryQueueItems = async () => {
    setCompanionMemoryBusy(true);
    setCompanionMemoryStatus("");
    try {
      setCompanionMemoryQueue(await api.companionSaveAllMemoryQueueItems());
      setMemoryState(await api.memoryPanelState());
      setCompanionMemoryStatus(t("settings.companion.memorySaved"));
    } catch (err) {
      setCompanionMemoryStatus(String(err));
    } finally {
      setCompanionMemoryBusy(false);
    }
  };

  const ignoreCompanionMemoryQueueItem = async (id: string) => {
    setCompanionMemoryBusy(true);
    setCompanionMemoryStatus("");
    try {
      setCompanionMemoryQueue(await api.companionIgnoreMemoryQueueItem(id));
      setCompanionMemoryStatus(t("settings.companion.memoryIgnored"));
    } catch (err) {
      setCompanionMemoryStatus(String(err));
    } finally {
      setCompanionMemoryBusy(false);
    }
  };

  const ignoreAllCompanionMemoryQueueItems = async () => {
    setCompanionMemoryBusy(true);
    setCompanionMemoryStatus("");
    try {
      setCompanionMemoryQueue(await api.companionIgnoreAllMemoryQueueItems());
      setCompanionMemoryStatus(t("settings.companion.memoryIgnored"));
    } catch (err) {
      setCompanionMemoryStatus(String(err));
    } finally {
      setCompanionMemoryBusy(false);
    }
  };

  const undoCompanionMemoryQueueItem = async (id: string) => {
    setCompanionMemoryBusy(true);
    setCompanionMemoryStatus("");
    try {
      setCompanionMemoryQueue(await api.companionUndoMemoryQueueItem(id));
      setMemoryState(await api.memoryPanelState());
      setCompanionMemoryStatus(t("settings.companion.memoryUndone"));
    } catch (err) {
      setCompanionMemoryStatus(String(err));
    } finally {
      setCompanionMemoryBusy(false);
    }
  };

  async function importPackZip(file?: File | null) {
    if (!file) return;
    setPackImportBusy(true);
    setPackImportStatus("");
    setPackImportFailed(false);
    try {
      const bytes = Array.from(new Uint8Array(await file.arrayBuffer()));
      const imported = await api.importPackZip(file.name, bytes);
      const nextPacks = await api.listPacks();
      onPacksChange(nextPacks);
      set("current_pack", imported.id);
      setPackImportStatus(t("settings.provider.importResult", { name: imported.name, id: imported.id }));
    } catch (err) {
      setPackImportFailed(true);
      setPackImportStatus(t("settings.provider.importFailed", { error: String(err) }));
    } finally {
      setPackImportBusy(false);
      if (packImportInputRef.current) packImportInputRef.current.value = "";
    }
  }

  async function saveCurrentPackManifest() {
    if (!form.current_pack) return;
    setPackManifestBusy(true);
    setPackManifestStatus("");
    setPackManifestFailed(false);
    try {
      const saved = await api.savePackManifestJson(form.current_pack, packManifestJson);
      const nextPacks = await api.listPacks();
      onPacksChange(nextPacks);
      setPackManifestJson(await api.readPackManifestJson(saved.id));
      setPackManifestStatus(`角色卡已保存：${saved.name}`);
    } catch (err) {
      setPackManifestFailed(true);
      setPackManifestStatus(String(err));
    } finally {
      setPackManifestBusy(false);
    }
  }

  function exportCurrentPackManifest() {
    if (!form.current_pack || !packManifestJson.trim()) return;
    const blob = new Blob([packManifestJson.endsWith("\n") ? packManifestJson : `${packManifestJson}\n`], {
      type: "application/json;charset=utf-8",
    });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = `${form.current_pack}-manifest.json`;
    document.body.appendChild(link);
    link.click();
    link.remove();
    URL.revokeObjectURL(url);
  }

  function addLoreDirectoryTemplate() {
    if (!packManifestJson.trim()) return;
    let manifest: PackManifest;
    try {
      manifest = JSON.parse(packManifestJson) as PackManifest;
    } catch {
      setPackManifestFailed(true);
      setPackManifestStatus("manifest JSON 暂时无法解析，修正后再添加 lore 目录。");
      return;
    }
    const lorebook = Array.isArray(manifest.lorebook) ? [...manifest.lorebook] : [];
    if (lorebook.some((entry) => entry.path === "lore")) {
      setPackManifestFailed(false);
      setPackManifestStatus("当前 manifest 已包含 lore 目录。");
      return;
    }
    lorebook.push({
      path: "lore",
      title: "角色扩展设定",
      tags: ["lore", "plot"],
      recursive: true,
      extensions: ["md", "txt"],
      priority: 0.5,
    });
    setPackManifestJson(JSON.stringify({ ...manifest, lorebook }, null, 2));
    setPackManifestFailed(false);
    setPackManifestStatus("已添加 lore 目录模板，保存后生效。");
  }

  async function previewCurrentPackLorebook() {
    const query = lorePreviewQuery.trim();
    if (!form.current_pack || !query) return;
    setLorePreviewBusy(true);
    setLorePreviewFailed(false);
    setLorePreviewText("");
    try {
      const text = await api.previewPackLorebook(form.current_pack, query);
      setLorePreviewText(text.trim() || "没有召回相关 lore 片段。");
    } catch (err) {
      setLorePreviewFailed(true);
      setLorePreviewText(String(err));
    } finally {
      setLorePreviewBusy(false);
    }
  }

  function handlePackDrop(event: DragEvent<HTMLDivElement>) {
    event.preventDefault();
    if (packImportBusy) return;
    void importPackZip(event.dataTransfer.files.item(0));
  }

  const webdavConfig: WebDavConfig = {
    url: form.webdav_url,
    username: form.webdav_username,
    password: form.webdav_password,
    path: form.webdav_path,
  };

  async function resetPermissionRule(scope: PermissionScope, tool: string) {
    setPermissionBusy(true);
    try {
      setPermissionState(await api.permissionResetRule(scope, tool));
    } finally {
      setPermissionBusy(false);
    }
  }

  async function savePermissionRule() {
    setPermissionBusy(true);
    try {
      setPermissionState(
        await api.permissionUpsertRule({
          tool: permissionDraft.tool,
          effect: permissionDraft.effect,
          scope: permissionDraft.scope,
          reason: permissionDraft.reason,
        }),
      );
    } finally {
      setPermissionBusy(false);
    }
  }

  function editPermissionRule(scope: PermissionScope, tool: string, effect: PermissionEffect, reason: string) {
    if (scope === "once") return;
    setPermissionDraft({ tool, effect, scope, reason });
  }

  async function refreshPermissionState() {
    const [permissions, shellPolicy] = await Promise.all([api.permissionPanelState(), api.shellPolicyState()]);
    setPermissionState(permissions);
    setShellPolicyState(shellPolicy);
  }

  function setAgentPanelState(next: AgentPanelState) {
    setAgentState(next);
    onAgentPanelChange(next);
  }

  async function refreshAgents() {
    setAgentBusy(true);
    setAgentStatus("");
    try {
      const next = await api.agentPanelState();
      setAgentPanelState(next);
      setAgentStatus(t("settings.agents.listRefreshed"));
    } catch (err) {
      setAgentStatus(String(err));
    } finally {
      setAgentBusy(false);
    }
  }

  async function newAgentTemplate() {
    setAgentBusy(true);
    setAgentStatus("");
    try {
      const raw = await api.agentTemplateJson();
      setAgentFile(null);
      setAgentFileName("researcher.json");
      setAgentJson(raw);
      setAgentValidation(await api.agentValidateJson(raw));
      setAgentStatus(t("settings.agents.templateReady"));
    } catch (err) {
      setAgentStatus(String(err));
    } finally {
      setAgentBusy(false);
    }
  }

  async function loadAgent(name: string) {
    setAgentBusy(true);
    setAgentStatus("");
    try {
      const file = await api.agentReadFile(name);
      setAgentFile(file);
      setAgentFileName(file.file_name);
      setAgentJson(file.raw_json);
      setAgentValidation(await api.agentValidateJson(file.raw_json));
      setAgentStatus(t("settings.agents.loaded", { name: file.file_name }));
    } catch (err) {
      setAgentStatus(String(err));
    } finally {
      setAgentBusy(false);
    }
  }

  async function validateAgentJson() {
    setAgentBusy(true);
    setAgentStatus("");
    try {
      const result = await api.agentValidateJson(agentJson);
      setAgentValidation(result);
      if (!agentFileName.trim()) setAgentFileName(result.suggested_file_name);
      setAgentStatus(result.ok ? t("settings.agents.validationPassedMsg") : t("settings.agents.validationFailedMsg"));
    } catch (err) {
      setAgentStatus(String(err));
    } finally {
      setAgentBusy(false);
    }
  }

  async function saveAgentJson() {
    setAgentBusy(true);
    setAgentStatus("");
    try {
      const validation = await api.agentValidateJson(agentJson);
      setAgentValidation(validation);
      if (!validation.ok) {
        setAgentStatus(t("settings.agents.fixBeforeSave"));
        return;
      }
      const next = await api.agentSaveFile(agentFileName || validation.suggested_file_name, agentJson);
      setAgentPanelState(next);
      setAgentFileName(agentFileName || validation.suggested_file_name);
      setAgentStatus(t("settings.agents.saved"));
    } catch (err) {
      setAgentStatus(String(err));
    } finally {
      setAgentBusy(false);
    }
  }

  async function deleteAgentJson(name: string) {
    if (!window.confirm(t("settings.agents.confirmDelete", { name }))) return;
    setAgentBusy(true);
    setAgentStatus("");
    try {
      const next = await api.agentDeleteFile(name);
      setAgentPanelState(next);
      if (agentFile?.name === name) {
        setAgentFile(null);
        setAgentJson("");
        setAgentFileName("");
        setAgentValidation(null);
      }
      setAgentStatus(t("settings.agents.deleted"));
    } catch (err) {
      setAgentStatus(String(err));
    } finally {
      setAgentBusy(false);
    }
  }

  async function importAgentJson(file?: File | null) {
    if (!file) return;
    setAgentBusy(true);
    setAgentStatus("");
    try {
      const raw = await file.text();
      const validation = await api.agentValidateJson(raw);
      setAgentFile(null);
      setAgentJson(raw);
      setAgentValidation(validation);
      setAgentFileName(file.name.endsWith(".json") ? file.name : validation.suggested_file_name);
      setAgentStatus(
        validation.ok
          ? t("settings.agents.importedReview", { name: file.name })
          : t("settings.agents.importedFailed", { name: file.name }),
      );
    } catch (err) {
      setAgentStatus(String(err));
    } finally {
      setAgentBusy(false);
    }
  }

  function exportCurrentAgentJson() {
    if (!agentJson.trim()) return;
    const fileName = agentFileName.trim() || agentValidation?.suggested_file_name || "agent.json";
    downloadTextFile(fileName, agentJson);
    setAgentStatus(t("settings.agents.exported", { name: fileName }));
  }

  async function exportAllAgents() {
    if (!agentState.definitions.length) return;
    setAgentBusy(true);
    setAgentStatus("");
    try {
      const files = await Promise.all(agentState.definitions.map((agent) => api.agentReadFile(agent.name)));
      const payload = {
        exported_at: new Date().toISOString(),
        agents_dir: agentState.agents_dir,
        agents: files.map((file) => ({
          name: file.name,
          file_name: file.file_name,
          path: file.path,
          json: JSON.parse(file.raw_json),
        })),
      };
      downloadTextFile("demiurge-agents-export.json", JSON.stringify(payload, null, 2));
      setAgentStatus(t("settings.agents.exportedAll", { n: files.length }));
    } catch (err) {
      setAgentStatus(String(err));
    } finally {
      setAgentBusy(false);
    }
  }

  function updateMcpServer(index: number, patch: Partial<McpServerConfig>) {
    setForm((current) => ({
      ...current,
      mcp_servers: current.mcp_servers.map((server, i) => (i === index ? { ...server, ...patch } : server)),
    }));
  }

  function addMcpServer() {
    setForm((current) => ({
      ...current,
      mcp_servers: [...current.mcp_servers, createMcpServer(current.mcp_servers)],
    }));
  }

  function removeMcpServer(index: number) {
    setForm((current) => ({
      ...current,
      mcp_servers: current.mcp_servers.filter((_, i) => i !== index),
    }));
  }

  async function refreshMcp() {
    setMcpBusy(true);
    setMcpError("");
    try {
      setMcpState(await api.mcpRefresh());
    } catch (err) {
      setMcpError(String(err));
    } finally {
      setMcpBusy(false);
    }
  }

  async function setSavedMcpServerEnabled(name: string, enabled: boolean) {
    setMcpBusy(true);
    setMcpError("");
    try {
      setMcpState(await api.mcpSetServerEnabled(name, enabled));
      setForm((current) => ({
        ...current,
        mcp_servers: current.mcp_servers.map((server) => (server.name === name ? { ...server, enabled } : server)),
      }));
    } catch (err) {
      setMcpError(String(err));
    } finally {
      setMcpBusy(false);
    }
  }

  async function refreshOcrStatus() {
    setOcrError("");
    try {
      setOcrStatus(await api.ocrModelStatus());
    } catch (err) {
      setOcrError(String(err));
    }
  }

  async function downloadOcrModels() {
    setOcrBusy(true);
    setOcrError("");
    setOcrProgress(null);
    try {
      setOcrStatus(await api.ocrDownloadModels(form.ocr_model_source));
    } catch (err) {
      setOcrError(String(err));
    } finally {
      setOcrBusy(false);
    }
  }

  async function checkProviderConnection() {
    setProviderTestBusy(true);
    setProviderTestStatus("");
    try {
      const result = await api.providerCheckConnection(form);
      setProviderTestStatus(formatConnectionTestResult(result));
    } catch (err) {
      setProviderTestStatus(String(err));
    } finally {
      setProviderTestBusy(false);
    }
  }

  async function checkWebSearchConnection() {
    setWebSearchTestBusy(true);
    setWebSearchTestStatus("");
    try {
      const result = await api.webSearchCheckConnection(form, form.web_search_provider);
      setWebSearchTestStatus(formatConnectionTestResult(result));
    } catch (err) {
      setWebSearchTestStatus(String(err));
    } finally {
      setWebSearchTestBusy(false);
    }
  }

  async function checkWebDav() {
    setWebdavBusy(true);
    setWebdavStatus("");
    try {
      const message = await api.webdavCheckConnection(webdavConfig);
      setWebdavStatus(message);
    } catch (err) {
      setWebdavStatus(String(err));
    } finally {
      setWebdavBusy(false);
    }
  }

  async function backupToWebDav() {
    setWebdavBusy(true);
    setWebdavStatus("");
    try {
      const fileName = await api.webdavBackupNow(webdavConfig);
      setWebdavStatus(t("settings.files.backupCreated", { name: fileName }));
      setWebdavFiles(await api.webdavListBackups(webdavConfig));
    } catch (err) {
      setWebdavStatus(String(err));
    } finally {
      setWebdavBusy(false);
    }
  }

  async function refreshWebDavFiles() {
    setWebdavBusy(true);
    setWebdavStatus("");
    try {
      setWebdavFiles(await api.webdavListBackups(webdavConfig));
      setWebdavStatus(t("settings.files.backupListRefreshed"));
    } catch (err) {
      setWebdavStatus(String(err));
    } finally {
      setWebdavBusy(false);
    }
  }

  async function deleteWebDavFile(fileName: string) {
    setWebdavBusy(true);
    setWebdavStatus("");
    try {
      await api.webdavDeleteBackup(webdavConfig, fileName);
      setWebdavFiles((files) => files.filter((file) => file.file_name !== fileName));
      setWebdavStatus(t("settings.files.deleted", { name: fileName }));
    } catch (err) {
      setWebdavStatus(String(err));
    } finally {
      setWebdavBusy(false);
    }
  }

  const ocrFileProgressPct =
    ocrProgress?.totalBytes && ocrProgress.totalBytes > 0
      ? Math.min(100, Math.round((ocrProgress.downloadedBytes / ocrProgress.totalBytes) * 100))
      : null;
  const ocrOverallProgressPct = ocrProgress
    ? Math.min(
        100,
        Math.round(
          ((ocrProgress.done
            ? ocrProgress.completedFiles
            : Math.max(0, ocrProgress.index - 1) + (ocrFileProgressPct ?? 0) / 100) /
            Math.max(1, ocrProgress.totalFiles)) *
            100,
        ),
      )
    : null;
  const historyPct = Math.min(
    100,
    Math.round(
      ((contextState?.estimated_history_tokens ?? 0) / Math.max(1, contextState?.history_budget_tokens ?? 1)) * 100,
    ),
  );

  const navItems: { id: SettingsTab; label: string; detail: string }[] = [
    { id: "general", label: t("settings.general"), detail: form.language === "zh" ? "简体中文" : "English" },
    { id: "provider", label: t("settings.nav.providers"), detail: selectedProvider.label },
    {
      id: "persona",
      label: t("settings.nav.persona"),
      detail: packs.find((pack) => pack.id === form.current_pack)?.name ?? form.current_pack,
    },
    { id: "media", label: t("settings.nav.media"), detail: form.image_model || t("settings.nav.detail.media") },
    {
      id: "companion",
      label: t("settings.nav.companion"),
      detail: form.companion_enabled ? t("settings.nav.detail.enabled") : t("settings.nav.detail.disabled"),
    },
    { id: "web", label: t("settings.nav.web"), detail: selectedWebSearchProvider.label },
    {
      id: "files",
      label: t("settings.nav.files"),
      detail: form.webdav_enabled ? t("settings.nav.detail.webdavOn") : t("settings.nav.detail.docsBackup"),
    },
    { id: "context", label: t("settings.nav.context"), detail: t("settings.nav.detail.tokens", { n: form.max_input_tokens }) },
    {
      id: "tools",
      label: t("settings.nav.tools"),
      detail: ocrStatus?.installed
        ? t("settings.nav.detail.mcpOcrReady", { n: form.mcp_servers.length })
        : t("settings.nav.detail.mcpOcr", { n: form.mcp_servers.length }),
    },
    {
      id: "voice",
      label: t("settings.nav.voice"),
      detail: form.voice_enabled ? t("settings.nav.detail.enabled") : t("settings.nav.detail.disabled"),
    },
    { id: "advanced", label: t("settings.nav.advanced"), detail: t("settings.nav.detail.storage") },
  ];

  function save() {
    const maxInput = Math.max(4000, form.max_input_tokens || 0);
    const reserved = Math.min(Math.max(512, form.reserved_output_tokens || 0), maxInput - 512);
    onSave({
      ...form,
      base_url: form.base_url.trim(),
      api_key: form.api_key.trim(),
      model: form.model.trim(),
      max_context_chars: Math.max(2000, form.max_context_chars || 0),
      max_input_tokens: maxInput,
      reserved_output_tokens: reserved,
      theme: normalizeTheme(form.theme),
      launch_on_startup: form.launch_on_startup,
      reasoning_effort: normalizeReasoningEffort(form.reasoning_effort),
      companion_memory_extraction_scope: form.companion_memory_extraction_scope.trim() || "recent_turn",
      companion_tone: form.companion_tone.trim() || "gentle",
      companion_mood: form.companion_mood.trim() || "neutral",
      companion_energy: form.companion_energy.trim() || "normal",
      companion_focus: form.companion_focus.trim() || "available",
      companion_do_not_disturb: form.companion_do_not_disturb.trim(),
      weather_location_mode: form.weather_location_mode.trim() || "manual",
      weather_city: form.weather_city.trim(),
      voice_stt_backend: form.voice_stt_backend.trim() || "none",
      voice_tts_backend: form.voice_tts_backend.trim() || "none",
      voice_id: form.voice_id.trim(),
      ocr_model_source: form.ocr_model_source || "modelscope",
      web_search_provider: normalizeWebSearchProvider(form.web_search_provider),
      tavily_api_key: form.tavily_api_key.trim(),
      brave_search_api_key: form.brave_search_api_key.trim(),
      exa_api_key: form.exa_api_key.trim(),
      webdav_url: form.webdav_url.trim(),
      webdav_username: form.webdav_username.trim(),
      webdav_password: form.webdav_password.trim(),
      webdav_path: form.webdav_path.trim() || "Demiurge",
      media_provider: normalizeMediaProvider(form.media_provider),
      media_base_url: form.media_base_url.trim() || "https://dashscope.aliyuncs.com",
      media_api_key: form.media_api_key.trim(),
      image_model: form.image_model.trim() || "qwen-image-2.0",
      image_size: form.image_size.trim() || "1024*1024",
      tts_model: form.tts_model.trim() || "qwen3-tts-flash",
      tts_voice: form.tts_voice.trim() || "Cherry",
      mcp_servers: form.mcp_servers
        .map((server) => ({
          ...server,
          name: server.name.trim(),
          transport: "stdio" as const,
          command: server.command.trim(),
          args: server.args.map((arg) => arg.trim()).filter(Boolean),
          env: server.env
            .map((env) => ({ ...env, key: env.key.trim() }))
            .filter((env) => env.key.length > 0),
        }))
        .filter((server) => server.name.length > 0 && server.command.length > 0),
    });
  }

  return (
    <div className="flex h-full min-h-0 w-full overflow-hidden bg-[#f6f7f9]">
        <aside className="flex w-[232px] shrink-0 flex-col border-r border-[#dfe3e8] bg-[#eef1f5]">
          <div className="flex h-12 items-center border-b border-[#dfe3e8] px-4">
            <div className="text-[13px] font-semibold text-[#202124]">{t("settings.heading")}</div>
            <button
              className="ml-auto grid size-8 place-items-center rounded-md text-[#69707a] transition hover:bg-[#e3e7ed] hover:text-[#202124]"
              onClick={onClose}
              aria-label={t("settings.close")}
            >
              <CloseIcon size={16} />
            </button>
          </div>
          <nav className="min-h-0 flex-1 overflow-y-auto p-2">
            {navItems.map((item) => {
              const selected = item.id === activeTab;
              return (
                <button
                  key={item.id}
                  type="button"
                  onClick={() => setActiveTab(item.id)}
                  className={`cf-press mb-1 flex w-full items-center gap-3 rounded-lg px-3 py-2.5 text-left ${
                    selected ? "bg-white text-[#111827] shadow-sm" : "text-[#4f5661] hover:bg-[#e4e8ee]"
                  }`}
                >
                  <span
                    className={`size-1.5 shrink-0 rounded-full ${selected ? "bg-[#111827]" : "bg-[#a6adb8]"}`}
                    aria-hidden
                  />
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-[13px] font-medium">{item.label}</span>
                    <span className="mt-0.5 block truncate text-[11px] text-[#8a9099]">{item.detail}</span>
                  </span>
                </button>
              );
            })}
          </nav>
          <div className="border-t border-[#dfe3e8] p-3 text-[11px] leading-5 text-[#7a8088]">
            {t("settings.credentialNote")}
          </div>
        </aside>

        <section className="flex min-w-0 flex-1 flex-col bg-white">
          <header className="flex h-12 shrink-0 items-center border-b border-[#eceff3] px-5">
            <div className="min-w-0">
              <div className="text-[14px] font-semibold text-[#202124]">
                {navItems.find((item) => item.id === activeTab)?.label}
              </div>
            </div>
            <div className="ml-auto flex items-center gap-2">
              <button type="button" className={secondaryButtonCls} onClick={onClose}>
                {t("settings.cancel")}
              </button>
              <button
                type="button"
                className="cf-press inline-flex h-8 items-center justify-center rounded-md bg-[#111827] px-4 text-[12px] font-medium text-white hover:bg-[#2b3442]"
                onClick={save}
              >
                {t("settings.save")}
              </button>
            </div>
          </header>

          <div className="min-h-0 flex-1 overflow-y-auto">
            <div className="mx-auto max-w-[760px] px-6 py-5">
              {activeTab === "general" && (
                <>
                  <Section title={t("settings.language")} description={t("settings.languageDesc")}>
                    <div className="inline-flex rounded-lg border border-[#dfe3e8] bg-[#f6f7f9] p-1">
                      {(["zh", "en"] as const).map((lng) => {
                        const active = form.language === lng;
                        return (
                          <button
                            key={lng}
                            type="button"
                            onClick={() => {
                              set("language", lng);
                              setLang(lng);
                            }}
                            className={`cf-press min-w-[120px] rounded-md px-4 py-2 text-[13px] font-medium transition ${
                              active ? "bg-white text-[#111827] shadow-sm" : "text-[#5f6368] hover:text-[#202124]"
                            }`}
                          >
                            {lng === "zh" ? t("settings.langZh") : t("settings.langEn")}
                          </button>
                        );
                      })}
                    </div>
                  </Section>
                  <Section title={t("settings.general.themeTitle")} description={t("settings.general.themeDesc")}>
                    <div className="grid gap-2 sm:grid-cols-3">
                      {themeOptions.map((theme) => {
                        const selected = normalizeTheme(form.theme) === theme.value;
                        return (
                          <button
                            key={theme.value}
                            type="button"
                            onClick={() => {
                              set("theme", theme.value);
                              onPreviewTheme?.(theme.value);
                            }}
                            className={`cf-press min-h-16 rounded-lg border px-3 py-3 text-left transition ${
                              selected
                                ? "border-[#111827] bg-[#f8f9fb] text-[#111827]"
                                : "border-[#e2e5ea] bg-white text-[#4f5661] hover:bg-[#f8f9fb]"
                            }`}
                          >
                            <span className="flex items-center gap-2 text-[13px] font-semibold">
                              {t(theme.labelKey)}
                              {selected && <CheckIcon size={14} className="ml-auto shrink-0" />}
                            </span>
                            <span className="mt-1 block text-[12px] leading-5 text-[#7a8088]">
                              {t(theme.helpKey)}
                            </span>
                          </button>
                        );
                      })}
                    </div>
                  </Section>
                  <Section title={t("settings.general.startupTitle")} description={t("settings.general.startupDesc")}>
                    <ToggleRow
                      checked={form.launch_on_startup}
                      title={t("settings.general.launchOnStartup")}
                      description={t("settings.general.launchOnStartupDesc")}
                      onChange={(checked) => set("launch_on_startup", checked)}
                    />
                  </Section>
                </>
              )}

              {activeTab === "provider" && (
                <>
                  <Section title={t("settings.provider.title")} description={t("settings.provider.desc")}>
                    <div className="grid gap-3 md:grid-cols-[240px_minmax(0,1fr)]">
                      <div className="flex flex-col gap-2">
                        <input
                          value={providerQuery}
                          onChange={(e) => setProviderQuery(e.target.value)}
                          placeholder={t("settings.provider.searchPlaceholder")}
                          className="h-8 w-full rounded-md border border-[#d9d9d9] bg-white px-2.5 text-[12px] text-[#202124] outline-none transition placeholder:text-[#9aa1ab] focus:border-[#7a7f87] focus:ring-1 focus:ring-[#202124]/10"
                        />
                        <div className="space-y-1 rounded-lg border border-[#e2e5ea] bg-[#f8f9fb] p-1.5">
                          {PROVIDER_OPTIONS
                            .filter((provider) =>
                              `${provider.label} ${provider.value} ${provider.short}`
                                .toLowerCase()
                                .includes(providerQuery.trim().toLowerCase()),
                            )
                            .map((provider) => {
                          const selected = provider.value === form.provider;
                          return (
                            <button
                              key={provider.value}
                              type="button"
                              onClick={() => {
                                set("provider", provider.value);
                                set("base_url", provider.baseUrl);
                                set("model", provider.model);
                              }}
                              className={`cf-press flex w-full items-center gap-2.5 rounded-md px-2.5 py-2 text-left ${
                                selected ? "bg-white shadow-sm" : "hover:bg-white/70"
                              }`}
                            >
                              <ProviderLogo value={provider.value} short={provider.short} selected={selected} />
                              <span className="min-w-0 flex-1">
                                <span className="block truncate text-[13px] font-medium text-[#202124]">
                                  {provider.label}
                                </span>
                                <span className="block truncate text-[11px] text-[#8a9099]">{provider.model}</span>
                              </span>
                              {selected && <CheckIcon size={14} className="text-[#111827]" />}
                            </button>
                          );
                          })}
                        </div>
                      </div>

                      <div className="rounded-lg border border-[#e2e5ea] bg-white">
                        <div className="flex items-center gap-3 border-b border-[#eceff3] px-4 py-3">
                          <ProviderLogo value={selectedProvider.value} short={selectedProvider.short} selected />
                          <div className="min-w-0 flex-1">
                            <div className="truncate text-[14px] font-semibold text-[#202124]">
                              {selectedProvider.label}
                            </div>
                            <div className="mt-0.5 truncate text-[12px] text-[#7a8088]">{selectedProvider.help}</div>
                          </div>
                        </div>
                        <div className="grid gap-4 p-4">
                          <Field label={t("settings.provider.baseUrl")} help={t("settings.provider.baseUrlHelp", { url: selectedProvider.baseUrl })}>
                            <input
                              className={inputCls}
                              value={form.base_url}
                              placeholder={selectedProvider.baseUrl}
                              onChange={(e) => set("base_url", e.target.value)}
                            />
                          </Field>
                          <Field label={t("settings.provider.apiKey")} help={t("settings.provider.apiKeyHelp")}>
                            <input
                              className={inputCls}
                              type="password"
                              value={form.api_key}
                              placeholder="sk-..."
                              onChange={(e) => set("api_key", e.target.value)}
                            />
                          </Field>
                          <Field label={t("settings.provider.model")} help={t("settings.provider.modelHelp")}>
                            <div className="grid gap-3 sm:grid-cols-[minmax(0,1fr)_minmax(220px,0.9fr)]">
                              <div>
                                <div className="mb-1.5 text-[11px] font-medium uppercase text-[#8a9099]">
                                  {t("settings.provider.modelPreset")}
                                </div>
                                <Select
                                  value={form.model}
                                  onChange={(value) => set("model", value)}
                                  options={modelOptions}
                                  disabled={!modelOptions.length}
                                  placeholder={t("settings.provider.noModelPresets")}
                                />
                              </div>
                              <div>
                                <div className="mb-1.5 text-[11px] font-medium uppercase text-[#8a9099]">
                                  {t("settings.provider.customModel")}
                                </div>
                                <input
                                  className={inputCls}
                                  value={form.model}
                                  placeholder={selectedProvider.model || "model-id"}
                                  onChange={(e) => set("model", e.target.value)}
                                />
                              </div>
                            </div>
                            <div className="mt-2 flex flex-wrap items-center gap-2 text-[11px] text-[#7a8088]">
                              <span className="rounded-md bg-[#f3f4f7] px-2 py-1">
                                {modelWindow
                                  ? t("settings.provider.contextWindow", { tokens: formatTokenWindow(modelWindow) })
                                  : t("settings.provider.unknownContextWindow")}
                              </span>
                              {form.context_budget_auto && modelWindow && (
                                <span className="rounded-md bg-[#eef6ff] px-2 py-1 text-[#2559a8]">
                                  {t("settings.provider.autoBudgetFollowsModel")}
                                </span>
                              )}
                            </div>
                          </Field>
                          <Field
                            label={t("settings.provider.effort")}
                            help={
                              effortSupported
                                ? t("settings.provider.effortHelpOn")
                                : t("settings.provider.effortHelpOff")
                            }
                          >
                            <div className="grid grid-cols-2 gap-2 sm:grid-cols-3">
                              {reasoningEfforts.map((effort) => {
                                const selected = normalizeReasoningEffort(form.reasoning_effort) === effort.value;
                                return (
                                  <button
                                    key={effort.value}
                                    type="button"
                                    onClick={() => set("reasoning_effort", effort.value)}
                                    className={`min-h-12 rounded-md border px-3 py-2 text-left transition ${
                                      selected
                                        ? "border-[#111827] bg-[#f8f9fb] text-[#111827]"
                                        : "border-[#e2e5ea] bg-white text-[#4f5661] hover:bg-[#f8f9fb]"
                                    }`}
                                  >
                                    <span className="flex items-center gap-2 text-[12px] font-semibold">
                                      {effort.label}
                                      {selected && <CheckIcon size={13} className="ml-auto shrink-0" />}
                                    </span>
                                    <span className="mt-0.5 block truncate text-[11px] text-[#8a9099]">
                                      {t(effort.helpKey)}
                                    </span>
                                  </button>
                                );
                              })}
                            </div>
                          </Field>
                          {false && (
                            <>
                          <Field label={t("settings.provider.pack")}>
                            <Select
                              value={form.current_pack}
                              onChange={(v) => set("current_pack", v)}
                              placeholder={t("settings.provider.packPlaceholder")}
                              options={packs.map((p) => ({ value: p.id, label: p.name, hint: p.id }))}
                            />
                          </Field>
                          <div
                            className="rounded-lg border border-dashed border-[#cfd5dd] bg-[#fbfcfd] p-3"
                            onDragOver={(event) => {
                              event.preventDefault();
                              event.dataTransfer.dropEffect = "copy";
                            }}
                            onDrop={handlePackDrop}
                          >
                            <input
                              ref={packImportInputRef}
                              className="hidden"
                              type="file"
                              accept=".zip,application/zip,application/x-zip-compressed"
                              onChange={(event) => void importPackZip(event.target.files?.[0])}
                            />
                            <div className="flex flex-wrap items-center gap-3">
                              <div className="grid size-10 shrink-0 place-items-center rounded-lg border border-[#dfe3e8] bg-white text-[#59616d]">
                                <DownloadIcon size={18} />
                              </div>
                              <div className="min-w-[180px] flex-1">
                                <div className="text-[13px] font-medium text-[#202124]">{t("settings.provider.importPack")}</div>
                                <div className="mt-0.5 text-[12px] leading-5 text-[#7a8088]">
                                  {t("settings.provider.importPackDesc")}
                                </div>
                              </div>
                              <button
                                type="button"
                                className={secondaryButtonCls}
                                disabled={packImportBusy}
                                onClick={() => packImportInputRef.current?.click()}
                              >
                                {packImportBusy ? t("settings.provider.importing") : t("settings.provider.chooseZip")}
                              </button>
                            </div>
                            {packImportStatus && (
                              <div
                                className={`mt-3 rounded-md border px-3 py-2 text-[12px] leading-5 ${
                                  packImportFailed
                                    ? "border-[#f3c3c3] bg-[#fff7f7] text-[#b42318]"
                                    : "border-[#dce6d8] bg-[#f8fbf6] text-[#3f6212]"
                                }`}
                              >
                                {packImportStatus}
                              </div>
                            )}
                          </div>
                          <div className="rounded-lg border border-[#e2e5ea] bg-white p-3">
                            <div className="flex flex-wrap items-center gap-3">
                              <div className="min-w-[180px] flex-1">
                                <div className="text-[13px] font-medium text-[#202124]">角色卡 manifest</div>
                                <div className="mt-0.5 text-[12px] leading-5 text-[#7a8088]">
                                  编辑当前角色卡的 persona、skills、memory、voice、permissions 与 lorebook 配置；请确认导入素材与角色设定已获得授权。
                                </div>
                              </div>
                              <button
                                type="button"
                                className={secondaryButtonCls}
                                disabled={packManifestBusy || !form.current_pack}
                                onClick={() => void saveCurrentPackManifest()}
                              >
                                保存
                              </button>
                              <button
                                type="button"
                                className={secondaryButtonCls}
                                disabled={!packManifestJson.trim()}
                                onClick={exportCurrentPackManifest}
                              >
                                导出
                              </button>
                            </div>
                            <div className="mt-3 rounded-md border border-[#e2e5ea] bg-[#fbfcfd] p-3">
                              <div className="flex flex-wrap items-center gap-3">
                                <div className="min-w-[180px] flex-1">
                                  <div className="text-[12px] font-medium text-[#202124]">Lorebook RAG</div>
                                  <div className="mt-0.5 text-[12px] leading-5 text-[#7a8088]">
                                    把剧情、世界观和设定文本放进角色包 lore 目录；保存 manifest 后，会按当前输入检索相关片段注入上下文。
                                  </div>
                                </div>
                                <button
                                  type="button"
                                  className={secondaryButtonCls}
                                  disabled={!packManifestJson.trim()}
                                  onClick={addLoreDirectoryTemplate}
                                >
                                  添加目录模板
                                </button>
                              </div>
                              <div className="mt-3 space-y-2">
                                {loreEntries.length ? (
                                  loreEntries.map((entry, index) => (
                                    <div
                                      key={`${entry.path}-${index}`}
                                      className="rounded-md border border-[#e5e7eb] bg-white px-3 py-2 text-[12px] leading-5 text-[#5f6368]"
                                    >
                                      <div className="flex flex-wrap items-center gap-2">
                                        <span className="font-mono text-[#202124]">{entry.path}</span>
                                        {entry.recursive && (
                                          <span className="rounded bg-[#eef2ff] px-1.5 py-0.5 text-[#3730a3]">recursive</span>
                                        )}
                                        {entry.extensions?.length ? (
                                          <span className="rounded bg-[#ecfdf3] px-1.5 py-0.5 text-[#166534]">
                                            {entry.extensions.join(", ")}
                                          </span>
                                        ) : (
                                          <span className="rounded bg-[#f5f5f5] px-1.5 py-0.5 text-[#6b7280]">
                                            md, markdown, txt
                                          </span>
                                        )}
                                        {typeof entry.priority === "number" && (
                                          <span className="rounded bg-[#fff7ed] px-1.5 py-0.5 text-[#9a3412]">
                                            priority {entry.priority}
                                          </span>
                                        )}
                                      </div>
                                      {(entry.title || entry.tags?.length) && (
                                        <div className="mt-1 text-[#7a8088]">
                                          {[entry.title, entry.tags?.length ? `tags: ${entry.tags.join(", ")}` : ""]
                                            .filter(Boolean)
                                            .join(" / ")}
                                        </div>
                                      )}
                                    </div>
                                  ))
                                ) : (
                                  <div className="rounded-md border border-dashed border-[#d9d9d9] bg-white px-3 py-2 text-[12px] leading-5 text-[#7a8088]">
                                    当前 manifest 还没有 lorebook 条目。
                                  </div>
                                )}
                              </div>
                              <div className="mt-3 flex flex-col gap-2 sm:flex-row">
                                <input
                                  className={inputCls}
                                  value={lorePreviewQuery}
                                  onChange={(event) => {
                                    setLorePreviewQuery(event.target.value);
                                    setLorePreviewText("");
                                    setLorePreviewFailed(false);
                                  }}
                                  placeholder="输入一个剧情问题，预览召回片段"
                                />
                                <button
                                  type="button"
                                  className={secondaryButtonCls}
                                  disabled={lorePreviewBusy || !form.current_pack || !lorePreviewQuery.trim()}
                                  onClick={() => void previewCurrentPackLorebook()}
                                >
                                  {lorePreviewBusy ? "检索中" : "预览召回"}
                                </button>
                              </div>
                              {lorePreviewText && (
                                <pre
                                  className={`mt-3 max-h-48 overflow-auto whitespace-pre-wrap rounded-md border px-3 py-2 text-[12px] leading-5 ${
                                    lorePreviewFailed
                                      ? "border-[#f3c3c3] bg-[#fff7f7] text-[#b42318]"
                                      : "border-[#d9d9d9] bg-white text-[#3f4650]"
                                  }`}
                                >
                                  {lorePreviewText}
                                </pre>
                              )}
                            </div>
                            <textarea
                              className="mt-3 min-h-[220px] w-full resize-y rounded-md border border-[#d9d9d9] bg-[#fbfcfd] px-3 py-2 font-mono text-[12px] leading-5 text-[#202124] outline-none transition focus:border-[#7a7f87] focus:ring-1 focus:ring-[#202124]/10"
                              spellCheck={false}
                              value={packManifestJson}
                              onChange={(event) => {
                                setPackManifestJson(event.target.value);
                                setPackManifestStatus("");
                                setPackManifestFailed(false);
                              }}
                              placeholder={packManifestBusy ? "加载角色卡 manifest..." : "manifest.json"}
                            />
                            {packManifestStatus && (
                              <div
                                className={`mt-3 rounded-md border px-3 py-2 text-[12px] leading-5 ${
                                  packManifestFailed
                                    ? "border-[#f3c3c3] bg-[#fff7f7] text-[#b42318]"
                                    : "border-[#dce6d8] bg-[#f8fbf6] text-[#3f6212]"
                                }`}
                              >
                                {packManifestStatus}
                              </div>
                            )}
                          </div>
                            </>
                          )}
                          <div className="flex flex-wrap gap-2">
                            <button
                              type="button"
                              className={secondaryButtonCls}
                              onClick={() => {
                                set("base_url", selectedProvider.baseUrl);
                                set("model", selectedProvider.model);
                              }}
                            >
                              {t("settings.provider.useDefaults")}
                            </button>
                            <button
                              type="button"
                              className={secondaryButtonCls}
                              disabled={providerTestBusy}
                              onClick={checkProviderConnection}
                            >
                              {providerTestBusy ? t("settings.provider.testing") : t("settings.provider.testConnection")}
                            </button>
                          </div>
                          {providerTestStatus && (
                            <div className="rounded-lg border border-[#e2e5ea] bg-[#fbfcfd] p-3 text-[12px] leading-5 text-[#6f7782] whitespace-pre-wrap">
                              {providerTestStatus}
                            </div>
                          )}
                        </div>
                      </div>
                    </div>
                  </Section>
                </>
              )}

              {activeTab === "persona" && (
                <>
                  <Section title={t("settings.persona.packTitle")} description={t("settings.persona.packDesc")}>
                    <div className="grid gap-4">
                      <Field label={t("settings.persona.currentPack")}>
                        <Select
                          value={form.current_pack}
                          onChange={(v) => set("current_pack", v)}
                          placeholder={t("settings.persona.packPlaceholder")}
                          options={packs.map((p) => ({ value: p.id, label: p.name, hint: p.id }))}
                        />
                      </Field>
                      <div
                        className="rounded-lg border border-dashed border-[#cfd5dd] bg-[#fbfcfd] p-3"
                        onDragOver={(event) => {
                          event.preventDefault();
                          event.dataTransfer.dropEffect = "copy";
                        }}
                        onDrop={handlePackDrop}
                      >
                        <input
                          ref={packImportInputRef}
                          className="hidden"
                          type="file"
                          accept=".zip,application/zip,application/x-zip-compressed"
                          onChange={(event) => void importPackZip(event.target.files?.[0])}
                        />
                        <div className="flex flex-wrap items-center gap-3">
                          <div className="grid size-10 shrink-0 place-items-center rounded-lg border border-[#dfe3e8] bg-white text-[#59616d]">
                            <DownloadIcon size={18} />
                          </div>
                          <div className="min-w-[180px] flex-1">
                            <div className="text-[13px] font-medium text-[#202124]">
                              {t("settings.persona.importPack")}
                            </div>
                            <div className="mt-0.5 text-[12px] leading-5 text-[#7a8088]">
                              {t("settings.persona.importPackDesc")}
                            </div>
                          </div>
                          <button
                            type="button"
                            className={secondaryButtonCls}
                            disabled={packImportBusy}
                            onClick={() => packImportInputRef.current?.click()}
                          >
                            {packImportBusy ? t("settings.persona.importing") : t("settings.persona.chooseZip")}
                          </button>
                        </div>
                        {packImportStatus && (
                          <div
                            className={`mt-3 rounded-md border px-3 py-2 text-[12px] leading-5 ${
                              packImportFailed
                                ? "border-[#f3c3c3] bg-[#fff7f7] text-[#b42318]"
                                : "border-[#dce6d8] bg-[#f8fbf6] text-[#3f6212]"
                            }`}
                          >
                            {packImportStatus}
                          </div>
                        )}
                      </div>
                    </div>
                  </Section>

                  <Section title={t("settings.persona.manifestTitle")} description={t("settings.persona.manifestDesc")}>
                    <div className="rounded-lg border border-[#e2e5ea] bg-white p-3">
                      <div className="flex flex-wrap items-center gap-2">
                        <button
                          type="button"
                          className={secondaryButtonCls}
                          disabled={packManifestBusy || !form.current_pack}
                          onClick={() => void saveCurrentPackManifest()}
                        >
                          {t("settings.persona.saveManifest")}
                        </button>
                        <button
                          type="button"
                          className={secondaryButtonCls}
                          disabled={!packManifestJson.trim()}
                          onClick={exportCurrentPackManifest}
                        >
                          {t("settings.persona.exportManifest")}
                        </button>
                        <button
                          type="button"
                          className={secondaryButtonCls}
                          disabled={!packManifestJson.trim()}
                          onClick={addLoreDirectoryTemplate}
                        >
                          {t("settings.persona.addLoreTemplate")}
                        </button>
                      </div>

                      <div className="mt-3 rounded-md border border-[#e2e5ea] bg-[#fbfcfd] p-3">
                        <div className="mb-2 flex flex-wrap items-center justify-between gap-3">
                          <div>
                            <div className="text-[12px] font-medium text-[#202124]">Lorebook RAG</div>
                            <div className="mt-0.5 text-[12px] leading-5 text-[#7a8088]">
                              {t("settings.persona.lorebookDesc")}
                            </div>
                          </div>
                        </div>
                        <div className="space-y-2">
                          {loreEntries.length ? (
                            loreEntries.map((entry, index) => (
                              <div
                                key={`${entry.path}-${index}`}
                                className="rounded-md border border-[#e5e7eb] bg-white px-3 py-2 text-[12px] leading-5 text-[#5f6368]"
                              >
                                <div className="flex flex-wrap items-center gap-2">
                                  <span className="font-mono text-[#202124]">{entry.path}</span>
                                  {entry.recursive && (
                                    <span className="rounded bg-[#eef2ff] px-1.5 py-0.5 text-[#3730a3]">recursive</span>
                                  )}
                                  {entry.extensions?.length ? (
                                    <span className="rounded bg-[#ecfdf3] px-1.5 py-0.5 text-[#166534]">
                                      {entry.extensions.join(", ")}
                                    </span>
                                  ) : (
                                    <span className="rounded bg-[#f5f5f5] px-1.5 py-0.5 text-[#6b7280]">
                                      md, markdown, txt
                                    </span>
                                  )}
                                  {typeof entry.priority === "number" && (
                                    <span className="rounded bg-[#fff7ed] px-1.5 py-0.5 text-[#9a3412]">
                                      priority {entry.priority}
                                    </span>
                                  )}
                                </div>
                                {(entry.title || entry.tags?.length) && (
                                  <div className="mt-1 text-[#7a8088]">
                                    {[entry.title, entry.tags?.length ? `tags: ${entry.tags.join(", ")}` : ""]
                                      .filter(Boolean)
                                      .join(" / ")}
                                  </div>
                                )}
                              </div>
                            ))
                          ) : (
                            <div className="rounded-md border border-dashed border-[#d9d9d9] bg-white px-3 py-2 text-[12px] leading-5 text-[#7a8088]">
                              {t("settings.persona.noLorebook")}
                            </div>
                          )}
                        </div>
                        <div className="mt-3 flex flex-col gap-2 sm:flex-row">
                          <input
                            className={inputCls}
                            value={lorePreviewQuery}
                            onChange={(event) => {
                              setLorePreviewQuery(event.target.value);
                              setLorePreviewText("");
                              setLorePreviewFailed(false);
                            }}
                            placeholder={t("settings.persona.previewPlaceholder")}
                          />
                          <button
                            type="button"
                            className={secondaryButtonCls}
                            disabled={lorePreviewBusy || !form.current_pack || !lorePreviewQuery.trim()}
                            onClick={() => void previewCurrentPackLorebook()}
                          >
                            {lorePreviewBusy ? t("settings.persona.previewing") : t("settings.persona.preview")}
                          </button>
                        </div>
                        {lorePreviewText && (
                          <pre
                            className={`mt-3 max-h-48 overflow-auto whitespace-pre-wrap rounded-md border px-3 py-2 text-[12px] leading-5 ${
                              lorePreviewFailed
                                ? "border-[#f3c3c3] bg-[#fff7f7] text-[#b42318]"
                                : "border-[#d9d9d9] bg-white text-[#3f4650]"
                            }`}
                          >
                            {lorePreviewText}
                          </pre>
                        )}
                      </div>

                      <textarea
                        className="mt-3 min-h-[260px] w-full resize-y rounded-md border border-[#d9d9d9] bg-[#fbfcfd] px-3 py-2 font-mono text-[12px] leading-5 text-[#202124] outline-none transition focus:border-[#7a7f87] focus:ring-1 focus:ring-[#202124]/10"
                        spellCheck={false}
                        value={packManifestJson}
                        onChange={(event) => {
                          setPackManifestJson(event.target.value);
                          setPackManifestStatus("");
                          setPackManifestFailed(false);
                        }}
                        placeholder={packManifestBusy ? t("settings.persona.loadingManifest") : "manifest.json"}
                      />
                      {packManifestStatus && (
                        <div
                          className={`mt-3 rounded-md border px-3 py-2 text-[12px] leading-5 ${
                            packManifestFailed
                              ? "border-[#f3c3c3] bg-[#fff7f7] text-[#b42318]"
                              : "border-[#dce6d8] bg-[#f8fbf6] text-[#3f6212]"
                          }`}
                        >
                          {packManifestStatus}
                        </div>
                      )}
                    </div>
                  </Section>
                </>
              )}

              {activeTab === "media" && (
                <>
                  <Section
                    title={t("settings.media.title")}
                    description={t("settings.media.desc")}
                  >
                    <div className="grid gap-4">
                      <Field label={t("settings.media.provider")}>
                        <select
                          className={inputCls}
                          value={form.media_provider}
                          onChange={(e) => set("media_provider", e.target.value)}
                        >
                          <option value="dashscope">阿里云百炼 / DashScope</option>
                        </select>
                      </Field>
                      <Field label={t("settings.media.baseUrl")} help={t("settings.media.baseUrlHelp")}>
                        <input
                          className={inputCls}
                          value={form.media_base_url}
                          placeholder="https://dashscope.aliyuncs.com"
                          onChange={(e) => set("media_base_url", e.target.value)}
                        />
                      </Field>
                      <Field label={t("settings.media.apiKey")} help={t("settings.media.apiKeyHelp")}>
                        <input
                          className={inputCls}
                          type="password"
                          value={form.media_api_key}
                          placeholder="sk-..."
                          onChange={(e) => set("media_api_key", e.target.value)}
                        />
                      </Field>
                    </div>
                  </Section>
                  <Section title={t("settings.media.imageTitle")} description={t("settings.media.imageDesc")}>
                    <div className="grid gap-4 sm:grid-cols-2">
                      <Field label={t("settings.media.imageModel")}>
                        <input
                          className={inputCls}
                          value={form.image_model}
                          placeholder="qwen-image-2.0"
                          onChange={(e) => set("image_model", e.target.value)}
                        />
                      </Field>
                      <Field label={t("settings.media.imageSize")}>
                        <Select
                          value={form.image_size}
                          onChange={(v) => set("image_size", v)}
                          placeholder={t("settings.media.imageSizePlaceholder")}
                          options={["512*512", "768*768", "1024*1024", "1280*720", "720*1280"].map((size) => ({
                            value: size,
                            label: size,
                          }))}
                        />
                      </Field>
                    </div>
                  </Section>
                  <Section title={t("settings.media.ttsTitle")} description={t("settings.media.ttsDesc")}>
                    <div className="grid gap-4 sm:grid-cols-2">
                      <Field label={t("settings.media.ttsModel")}>
                        <input
                          className={inputCls}
                          value={form.tts_model}
                          placeholder="qwen3-tts-flash"
                          onChange={(e) => set("tts_model", e.target.value)}
                        />
                      </Field>
                      <Field label={t("settings.media.voice")}>
                        <input
                          className={inputCls}
                          value={form.tts_voice}
                          placeholder="Cherry"
                          onChange={(e) => set("tts_voice", e.target.value)}
                        />
                      </Field>
                    </div>
                  </Section>
                </>
              )}

              {activeTab === "companion" && (
                <>
                  <Section title={t("settings.companion.coreTitle")} description={t("settings.companion.coreDesc")}>
                    <div className="grid gap-4">
                      <ToggleRow
                        checked={form.companion_enabled}
                        title={t("settings.companion.enabled")}
                        description={t("settings.companion.enabledDesc")}
                        onChange={(checked) => set("companion_enabled", checked)}
                      />
                      <div className="grid gap-4 md:grid-cols-2">
                        <Field label={t("settings.companion.tone")}>
                          <select className={inputCls} value={form.companion_tone} onChange={(e) => set("companion_tone", e.target.value)}>
                            <option value="quiet">{t("settings.companion.tone.quiet")}</option>
                            <option value="gentle">{t("settings.companion.tone.gentle")}</option>
                            <option value="bright">{t("settings.companion.tone.bright")}</option>
                            <option value="wry">{t("settings.companion.tone.wry")}</option>
                            <option value="coach">{t("settings.companion.tone.coach")}</option>
                          </select>
                        </Field>
                        <Field label={t("settings.companion.focus")}>
                          <select className={inputCls} value={form.companion_focus} onChange={(e) => set("companion_focus", e.target.value)}>
                            <option value="available">{t("settings.companion.focus.available")}</option>
                            <option value="focusing">{t("settings.companion.focus.focusing")}</option>
                            <option value="resting">{t("settings.companion.focus.resting")}</option>
                          </select>
                        </Field>
                        <Field label={t("settings.companion.mood")}>
                          <select className={inputCls} value={form.companion_mood} onChange={(e) => set("companion_mood", e.target.value)}>
                            <option value="neutral">{t("settings.companion.mood.neutral")}</option>
                            <option value="good">{t("settings.companion.mood.good")}</option>
                            <option value="stressed">{t("settings.companion.mood.stressed")}</option>
                            <option value="down">{t("settings.companion.mood.down")}</option>
                          </select>
                        </Field>
                        <Field label={t("settings.companion.energy")}>
                          <select className={inputCls} value={form.companion_energy} onChange={(e) => set("companion_energy", e.target.value)}>
                            <option value="low">{t("settings.companion.energy.low")}</option>
                            <option value="normal">{t("settings.companion.energy.normal")}</option>
                            <option value="high">{t("settings.companion.energy.high")}</option>
                          </select>
                        </Field>
                      </div>
                      <Field label={t("settings.companion.dnd")} help={t("settings.companion.dndHelp")}>
                        <input
                          className={inputCls}
                          value={form.companion_do_not_disturb}
                          placeholder="23:00-08:30"
                          onChange={(e) => set("companion_do_not_disturb", e.target.value)}
                        />
                      </Field>
                      <div className="rounded-lg border border-[#f2d7d5] bg-[#fffafa] p-3 text-[12px] leading-5 text-[#8a4b45]">
                        {t("settings.companion.safety")}
                      </div>
                    </div>
                  </Section>
                  <Section title={t("settings.companion.weatherTitle")} description={t("settings.companion.weatherDesc")}>
                    <div className="grid gap-4">
                      <ToggleRow
                        checked={form.weather_enabled}
                        title={t("settings.companion.weatherEnabled")}
                        description={t("settings.companion.weatherEnabledDesc")}
                        onChange={(checked) => set("weather_enabled", checked)}
                      />
                      <div className="grid gap-4 md:grid-cols-2">
                        <Field label={t("settings.companion.weatherCity")}>
                          <input
                            className={inputCls}
                            value={form.weather_city}
                            placeholder="Hangzhou"
                            onChange={(e) => set("weather_city", e.target.value)}
                          />
                        </Field>
                        <Field label={t("settings.companion.locationMode")} help={t("settings.companion.locationHelp")}>
                          <select className={inputCls} value={form.weather_location_mode} onChange={(e) => set("weather_location_mode", e.target.value)}>
                            <option value="manual">{t("settings.companion.location.manual")}</option>
                            <option value="auto">{t("settings.companion.location.autoReserved")}</option>
                            <option value="off">{t("settings.companion.location.off")}</option>
                          </select>
                        </Field>
                      </div>
                      <div className="rounded-lg border border-[#e2e5ea] bg-[#fbfcfd] p-3 text-[12px] leading-5 text-[#6f7782]">
                        {t("settings.companion.weatherPrivacy")}
                      </div>
                    </div>
                  </Section>
                  <Section title={t("settings.companion.memoryTitle")} description={t("settings.companion.memoryDesc")}>
                    <div className="grid gap-3">
                      <div className="grid gap-3 rounded-lg border border-[#e2e5ea] bg-[#fbfcfd] p-3">
                        <ToggleRow
                          checked={form.companion_memory_extraction_enabled}
                          title={t("settings.companion.extractEnabled")}
                          description={t("settings.companion.extractEnabledDesc")}
                          onChange={(checked) => set("companion_memory_extraction_enabled", checked)}
                        />
                        <div className="grid gap-3 md:grid-cols-[180px_minmax(0,1fr)]">
                          <Field label={t("settings.companion.extractScope")}>
                            <select
                              className={inputCls}
                              value={form.companion_memory_extraction_scope}
                              onChange={(e) => set("companion_memory_extraction_scope", e.target.value)}
                            >
                              <option value="recent_turn">{t("settings.companion.extractScope.recentTurn")}</option>
                            </select>
                          </Field>
                          <div className="rounded-md border border-[#e8ebef] bg-white p-3 text-[12px] leading-5 text-[#6f7782]">
                            {t("settings.companion.extractAuditNote")}
                          </div>
                        </div>
                      </div>
                      <div className="rounded-lg border border-[#e2e5ea] bg-[#fbfcfd] p-3">
                        <div className="flex flex-wrap items-center justify-between gap-2">
                          <div>
                            <div className="text-[13px] font-medium text-[#202124]">
                              {t("settings.companion.memoryQueueTitle")}
                            </div>
                            <div className="mt-1 break-all text-[11px] text-[#8a9099]">
                              {companionMemoryQueue?.path || t("settings.companion.memoryQueueLoading")}
                            </div>
                          </div>
                          <span className="rounded-md bg-white px-2 py-1 text-[11px] text-[#59616d]">
                            {t("settings.companion.memoryPending", {
                              n: companionMemoryQueue?.pending_count ?? 0,
                            })}
                          </span>
                        </div>
                        <div className="mt-3 flex flex-wrap gap-2">
                          <button
                            type="button"
                            className={secondaryButtonCls}
                            disabled={companionMemoryBusy || !(companionMemoryQueue?.pending_count ?? 0)}
                            onClick={() => void saveAllCompanionMemoryQueueItems()}
                          >
                            {t("settings.companion.memorySaveAll")}
                          </button>
                          <button
                            type="button"
                            className={secondaryButtonCls}
                            disabled={companionMemoryBusy || !(companionMemoryQueue?.pending_count ?? 0)}
                            onClick={() => void ignoreAllCompanionMemoryQueueItems()}
                          >
                            {t("settings.companion.memoryIgnoreAll")}
                          </button>
                          <button type="button" className={secondaryButtonCls} onClick={() => setActiveTab("context")}>
                            {t("settings.companion.memoryOpenPanel")}
                          </button>
                        </div>
                        <div className="mt-3 grid gap-2">
                          {companionMemoryQueue?.items.length ? (
                            companionMemoryQueue.items.slice(0, 12).map((item) => (
                              <div key={item.id} className="rounded-md border border-[#e8ebef] bg-white p-2">
                                <div className="flex flex-wrap items-center gap-2 text-[11px] text-[#7a8088]">
                                  <span className="rounded-md bg-[#eef1f5] px-1.5 py-0.5 text-[#59616d]">
                                    {item.status}
                                  </span>
                                  <span>{item.scope}</span>
                                  <span>{item.kind}</span>
                                  <span>{formatTime(item.created_at)}</span>
                                  <span className="truncate">session {item.source_session}</span>
                                </div>
                                <div className="mt-1 text-[12px] leading-5 text-[#202124]">{item.text}</div>
                                <div className="mt-1 text-[11px] leading-4 text-[#8a9099]">{item.reason}</div>
                                {item.status === "pending" && (
                                  <div className="mt-2 flex justify-end gap-2">
                                    <button
                                      type="button"
                                      className={secondaryButtonCls}
                                      disabled={companionMemoryBusy}
                                      onClick={() => void ignoreCompanionMemoryQueueItem(item.id)}
                                    >
                                      {t("settings.companion.memoryIgnore")}
                                    </button>
                                    <button
                                      type="button"
                                      className={secondaryButtonCls}
                                      disabled={companionMemoryBusy}
                                      onClick={() => void saveCompanionMemoryQueueItem(item.id)}
                                    >
                                      {t("settings.companion.memorySave")}
                                    </button>
                                  </div>
                                )}
                                {item.status === "saved" && item.saved_memory_id && (
                                  <div className="mt-2 flex justify-end">
                                    <button
                                      type="button"
                                      className={secondaryButtonCls}
                                      disabled={companionMemoryBusy}
                                      onClick={() => void undoCompanionMemoryQueueItem(item.id)}
                                    >
                                      {t("settings.companion.memoryUndo")}
                                    </button>
                                  </div>
                                )}
                              </div>
                            ))
                          ) : (
                            <div className="rounded-md border border-dashed border-[#d8dde5] bg-white p-3 text-[12px] text-[#7a8088]">
                              {t("settings.companion.memoryQueueEmpty")}
                            </div>
                          )}
                        </div>
                      </div>
                      {companionMemorySuggestions.length ? (
                        companionMemorySuggestions.map((suggestion) => (
                          <div
                            key={suggestion.id}
                            className="rounded-lg border border-[#e2e5ea] bg-white p-3"
                          >
                            <div className="flex flex-wrap items-start justify-between gap-3">
                              <div className="min-w-0">
                                <div className="flex items-center gap-2">
                                  <span className="rounded-md bg-[#eef1f5] px-1.5 py-0.5 text-[11px] text-[#59616d]">
                                    {suggestion.kind}
                                  </span>
                                  <span className="text-[12px] text-[#7a8088]">{suggestion.reason}</span>
                                </div>
                                <div className="mt-2 text-[13px] leading-5 text-[#202124]">{suggestion.text}</div>
                              </div>
                              <button
                                type="button"
                                className={secondaryButtonCls}
                                disabled={companionMemoryBusy}
                                onClick={() => void enqueueCompanionMemorySuggestion(suggestion.id)}
                              >
                                {t("settings.companion.memoryEnqueue")}
                              </button>
                            </div>
                          </div>
                        ))
                      ) : (
                        <div className="rounded-lg border border-[#e2e5ea] bg-[#fbfcfd] p-3 text-[12px] text-[#7a8088]">
                          {t("settings.companion.memoryEmpty")}
                        </div>
                      )}
                      {companionMemoryStatus && (
                        <div className="rounded-lg border border-[#e2e5ea] bg-[#fbfcfd] p-3 text-[12px] text-[#6f7782]">
                          {companionMemoryStatus}
                        </div>
                      )}
                    </div>
                  </Section>
                </>
              )}

              {activeTab === "web" && (
                <>
                  <Section title={t("settings.web.providerTitle")} description={t("settings.web.providerDesc")}>
                    <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
                      {webSearchProviders.map((provider) => {
                        const selected = provider.value === form.web_search_provider;
                        return (
                          <button
                            key={provider.value}
                            type="button"
                            onClick={() => set("web_search_provider", provider.value)}
                            className={`rounded-lg border p-3 text-left transition ${
                              selected
                                ? "border-[#111827] bg-[#f8f9fb]"
                                : "border-[#e2e5ea] bg-white hover:bg-[#f8f9fb]"
                            }`}
                          >
                            <div className="flex items-center gap-2">
                              <span className="text-[13px] font-semibold text-[#202124]">{provider.label}</span>
                              {selected && <CheckIcon size={14} className="ml-auto text-[#111827]" />}
                            </div>
                            <div className="mt-1 text-[12px] leading-5 text-[#7a8088]">{t(provider.helpKey)}</div>
                          </button>
                        );
                      })}
                    </div>
                  </Section>
                  <Section title={t("settings.web.keysTitle")} description={t("settings.web.keysDesc")}>
                    <div className="grid gap-4">
                      <Field label={t("settings.web.tavilyKey")}>
                        <input
                          className={inputCls}
                          type="password"
                          value={form.tavily_api_key}
                          placeholder="tvly-..."
                          onChange={(e) => set("tavily_api_key", e.target.value)}
                        />
                      </Field>
                      <Field label={t("settings.web.braveKey")}>
                        <input
                          className={inputCls}
                          type="password"
                          value={form.brave_search_api_key}
                          placeholder="BSA..."
                          onChange={(e) => set("brave_search_api_key", e.target.value)}
                        />
                      </Field>
                      <Field label={t("settings.web.exaKey")}>
                        <input
                          className={inputCls}
                          type="password"
                          value={form.exa_api_key}
                          placeholder="exa-..."
                          onChange={(e) => set("exa_api_key", e.target.value)}
                        />
                      </Field>
                    </div>
                    <div className="mt-4 flex flex-wrap gap-2">
                      <button
                        type="button"
                        className={secondaryButtonCls}
                        disabled={webSearchTestBusy}
                        onClick={checkWebSearchConnection}
                      >
                        {webSearchTestBusy ? t("settings.provider.testing") : t("settings.provider.testConnection")}
                      </button>
                    </div>
                    {webSearchTestStatus && (
                      <div className="mt-3 rounded-lg border border-[#e2e5ea] bg-[#fbfcfd] p-3 text-[12px] leading-5 text-[#6f7782] whitespace-pre-wrap">
                        {webSearchTestStatus}
                      </div>
                    )}
                  </Section>
                </>
              )}

              {activeTab === "files" && (
                <>
                  <Section
                    title={t("settings.files.docTitle")}
                    description={t("settings.files.docDesc")}
                  >
                    <div className="grid gap-2 sm:grid-cols-2">
                      {[
                        ["settings.files.text", "settings.files.textDesc"],
                        ["settings.files.images", "settings.files.imagesDesc"],
                        ["settings.files.pdf", "settings.files.pdfDesc"],
                        ["settings.files.office", "settings.files.officeDesc"],
                        ["settings.files.mermaid", "settings.files.mermaidDesc"],
                        ["settings.files.highlight", "settings.files.highlightDesc"],
                      ].map(([titleKey, descKey]) => (
                        <div key={titleKey} className="rounded-lg border border-[#e2e5ea] bg-[#fbfcfd] p-3">
                          <div className="text-[13px] font-semibold text-[#202124]">{t(titleKey)}</div>
                          <div className="mt-1 text-[12px] leading-5 text-[#7a8088]">{t(descKey)}</div>
                        </div>
                      ))}
                    </div>
                  </Section>

                  <Section
                    title={t("settings.files.webdavTitle")}
                    description={t("settings.files.webdavDesc")}
                  >
                    <ToggleRow
                      checked={form.webdav_enabled}
                      title={t("settings.files.webdavToggle")}
                      description={t("settings.files.webdavToggleDesc")}
                      onChange={(checked) => set("webdav_enabled", checked)}
                    />
                    <div className="mt-4 grid gap-4">
                      <Field label={t("settings.files.webdavUrl")} help={t("settings.files.webdavUrlHelp")}>
                        <input
                          className={inputCls}
                          value={form.webdav_url}
                          placeholder="https://..."
                          onChange={(e) => set("webdav_url", e.target.value)}
                        />
                      </Field>
                      <div className="grid gap-4 sm:grid-cols-2">
                        <Field label={t("settings.files.username")}>
                          <input
                            className={inputCls}
                            value={form.webdav_username}
                            onChange={(e) => set("webdav_username", e.target.value)}
                          />
                        </Field>
                        <Field label={t("settings.files.password")}>
                          <input
                            className={inputCls}
                            type="password"
                            value={form.webdav_password}
                            onChange={(e) => set("webdav_password", e.target.value)}
                          />
                        </Field>
                      </div>
                      <Field label={t("settings.files.backupPath")} help={t("settings.files.backupPathHelp")}>
                        <input
                          className={inputCls}
                          value={form.webdav_path}
                          placeholder="Demiurge"
                          onChange={(e) => set("webdav_path", e.target.value)}
                        />
                      </Field>
                    </div>

                    <div className="mt-4 flex flex-wrap gap-2">
                      <button className={secondaryButtonCls} disabled={webdavBusy} type="button" onClick={checkWebDav}>
                        {t("settings.files.testConnection")}
                      </button>
                      <button className={secondaryButtonCls} disabled={webdavBusy} type="button" onClick={backupToWebDav}>
                        {t("settings.files.backupNow")}
                      </button>
                      <button className={secondaryButtonCls} disabled={webdavBusy} type="button" onClick={refreshWebDavFiles}>
                        {t("settings.files.listBackups")}
                      </button>
                    </div>
                    {webdavStatus && (
                      <div className="mt-3 rounded-lg border border-[#e2e5ea] bg-[#fbfcfd] p-3 text-[12px] leading-5 text-[#6f7782]">
                        {webdavStatus}
                      </div>
                    )}
                    <div className="mt-4 overflow-hidden rounded-lg border border-[#e2e5ea]">
                      <div className="grid grid-cols-[minmax(0,1fr)_120px_80px] gap-2 border-b border-[#eceff3] bg-[#f8f9fb] px-3 py-2 text-[11px] font-semibold text-[#7a8088]">
                        <span>{t("settings.files.colFile")}</span>
                        <span>{t("settings.files.colModified")}</span>
                        <span className="text-right">{t("settings.files.colAction")}</span>
                      </div>
                      {webdavFiles.length ? (
                        webdavFiles.map((file) => (
                          <div
                            key={file.file_name}
                            className="grid grid-cols-[minmax(0,1fr)_120px_80px] items-center gap-2 border-b border-[#f0f2f5] px-3 py-2 text-[12px] last:border-b-0"
                          >
                            <div className="min-w-0">
                              <div className="truncate font-medium text-[#202124]" title={file.file_name}>
                                {file.file_name}
                              </div>
                              <div className="text-[#8a9099]">{formatBytes(file.size)}</div>
                            </div>
                            <div className="truncate text-[#7a8088]" title={file.modified_time}>
                              {file.modified_time || "-"}
                            </div>
                            <button
                              className="justify-self-end rounded-md px-2 py-1 text-[12px] text-[#b42318] transition hover:bg-[#fff1f2]"
                              disabled={webdavBusy}
                              type="button"
                              onClick={() => deleteWebDavFile(file.file_name)}
                            >
                              {t("settings.files.delete")}
                            </button>
                          </div>
                        ))
                      ) : (
                        <div className="px-3 py-4 text-[12px] text-[#7a8088]">{t("settings.files.noBackups")}</div>
                      )}
                    </div>
                  </Section>
                </>
              )}

              {activeTab === "context" && (
                <>
                  <Section title={t("settings.context.budgetTitle")} description={t("settings.context.budgetDesc")}>
                    <ToggleRow
                      checked={form.context_budget_auto}
                      title={t("settings.context.auto")}
                      description={t("settings.context.autoDesc")}
                      onChange={(checked) => set("context_budget_auto", checked)}
                    />
                    <div className="mt-3 rounded-lg border border-[#e2e5ea] bg-[#fbfcfd] px-3 py-2.5 text-[12px]">
                      {(() => {
                        const win = modelContextWindow(form.provider, form.model);
                        return win ? (
                          <div className="flex items-center justify-between gap-2">
                            <span className="min-w-0 truncate text-[#5f6368]">
                              {t("settings.context.detected")} ·{" "}
                              <span className="font-medium text-[#202124]">
                                {form.model.trim() || selectedProvider.model}
                              </span>
                            </span>
                            <span className="shrink-0 tabular-nums font-semibold text-[#202124]">
                              {win.toLocaleString()} tok
                            </span>
                          </div>
                        ) : (
                          <span className="text-[#b54708]">{t("settings.context.detectedUnknown")}</span>
                        );
                      })()}
                    </div>
                    <div className="mt-4 grid gap-4 sm:grid-cols-2">
                      <Field label={t("settings.context.maxInput")}>
                        <input
                          className={`${inputCls} disabled:cursor-not-allowed disabled:bg-[#f2f4f7] disabled:text-[#9aa1ab]`}
                          type="number"
                          min={4000}
                          step={1000}
                          disabled={form.context_budget_auto}
                          value={form.max_input_tokens}
                          onChange={(e) => set("max_input_tokens", Number(e.target.value) || 0)}
                        />
                      </Field>
                      <Field label={t("settings.context.reservedOutput")}>
                        <input
                          className={`${inputCls} disabled:cursor-not-allowed disabled:bg-[#f2f4f7] disabled:text-[#9aa1ab]`}
                          type="number"
                          min={512}
                          step={256}
                          disabled={form.context_budget_auto}
                          value={form.reserved_output_tokens}
                          onChange={(e) => set("reserved_output_tokens", Number(e.target.value) || 0)}
                        />
                      </Field>
                    </div>
                    {!form.context_budget_auto && modelContextWindow(form.provider, form.model) != null && (
                      <button
                        type="button"
                        className={`${secondaryButtonCls} mt-3`}
                        onClick={() => {
                          const budget = autoContextBudget(form.provider, form.model);
                          if (budget) {
                            set("max_input_tokens", budget.maxInput);
                            set("reserved_output_tokens", budget.reservedOutput);
                          }
                        }}
                      >
                        {t("settings.context.applyWindow")}
                      </button>
                    )}
                    <div className="mt-4 rounded-lg border border-[#e2e5ea] bg-[#fbfcfd] p-3">
                      <div className="mb-3 flex items-center justify-between gap-3">
                        <div className="text-[13px] font-medium text-[#202124]">{t("settings.context.currentSession")}</div>
                        <button
                          className={secondaryButtonCls}
                          type="button"
                          onClick={async () => setContextState(await api.contextPanelState())}
                        >
                          {t("settings.context.refresh")}
                        </button>
                      </div>
                      <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
                        <ContextMetric label={t("settings.context.metric.messages")} value={contextState?.message_count ?? 0} />
                        <ContextMetric label={t("settings.context.metric.user")} value={contextState?.user_messages ?? 0} />
                        <ContextMetric label={t("settings.context.metric.assistant")} value={contextState?.assistant_messages ?? 0} />
                        <ContextMetric label={t("settings.context.metric.tool")} value={contextState?.tool_messages ?? 0} />
                      </div>
                      <div className="mt-2 grid grid-cols-2 gap-2 sm:grid-cols-4">
                        <ContextMetric label={t("settings.context.metric.systemChars")} value={contextState?.system_prompt_chars ?? 0} />
                        <ContextMetric label={t("settings.context.metric.systemTokens")} value={contextState?.system_prompt_tokens ?? 0} />
                        <ContextMetric label={t("settings.context.metric.toolsTokens")} value={contextState?.tools_tokens ?? 0} />
                        <ContextMetric label={t("settings.context.metric.outputReserve")} value={contextState?.reserved_output_tokens ?? 0} />
                      </div>
                      <div className="mt-2 grid grid-cols-2 gap-2 sm:grid-cols-4">
                        <ContextMetric label={t("settings.context.metric.historyTokens")} value={contextState?.estimated_history_tokens ?? 0} />
                        <ContextMetric label={t("settings.context.metric.historyBudget")} value={contextState?.history_budget_tokens ?? 0} />
                        <ContextMetric label={t("settings.context.metric.historyRemaining")} value={contextState?.history_remaining_tokens ?? 0} />
                        <ContextMetric label={t("settings.context.metric.summaryTokens")} value={contextState?.summary_tokens ?? 0} />
                      </div>
                      <div className="mt-2 grid grid-cols-2 gap-2 sm:grid-cols-4">
                        <ContextMetric label={t("settings.context.metric.inputUsed")} value={contextState?.input_budget_used_tokens ?? 0} />
                        <ContextMetric label={t("settings.context.metric.inputRemaining")} value={contextState?.input_budget_remaining_tokens ?? 0} />
                        <ContextMetric label={t("settings.context.metric.projectedTotal")} value={contextState?.projected_total_tokens ?? 0} />
                        <ContextMetric label={t("settings.context.metric.sectionTokens")} value={contextState?.prompt_section_tokens ?? 0} />
                      </div>
                      <ContextBudgetBreakdown state={contextState} t={t} />
                      <div className="mt-3 rounded-lg border border-[#e2e5ea] bg-white p-3">
                        <div className="mb-2 flex items-center justify-between gap-3 text-[12px]">
                          <span className="font-medium text-[#202124]">{t("settings.context.historyBudgetTitle")}</span>
                          <span
                            className={
                              (contextState?.history_over_budget_tokens ?? 0) > 0 ? "text-[#b42318]" : "text-[#7a8088]"
                            }
                          >
                            {historyPct}%
                          </span>
                        </div>
                        <div className="h-2 overflow-hidden rounded-full bg-[#e8ebef]">
                          <div
                            className={
                              (contextState?.history_over_budget_tokens ?? 0) > 0
                                ? "h-full rounded-full bg-[#b42318]"
                                : "h-full rounded-full bg-[#111827]"
                            }
                            style={{ width: `${historyPct}%` }}
                          />
                        </div>
                        <div className="mt-2 text-[12px] leading-5 text-[#7a8088]">
                          {t("settings.context.historySummary", {
                            history: (contextState?.estimated_history_tokens ?? 0).toLocaleString(),
                            budget: (contextState?.history_budget_tokens ?? form.max_input_tokens).toLocaleString(),
                            projected: (contextState?.projected_total_tokens ?? 0).toLocaleString(),
                            remaining: (contextState?.input_budget_remaining_tokens ?? 0).toLocaleString(),
                            summary: (contextState?.summary_chars ?? 0).toLocaleString(),
                          })}
                        </div>
                      </div>
                      <ContextHistoryBreakdown buckets={contextState?.history_buckets ?? []} t={t} />
                      <ContextMemorySources sources={contextState?.memory_sources ?? []} t={t} />
                      <PromptSectionList sections={contextState?.prompt_sections ?? []} t={t} />
                    </div>
                  </Section>
                  <Section title={t("settings.memory.title")}>
                    <ToggleRow
                      checked={form.auto_memory_enabled}
                      title={t("settings.memory.autoToggle")}
                      description={t("settings.memory.autoToggleDesc")}
                      onChange={(checked) => set("auto_memory_enabled", checked)}
                    />
                    <div className="mt-4 rounded-lg border border-[#e2e5ea] bg-[#fbfcfd] p-3">
                      <div className="flex items-start justify-between gap-3">
                        <div className="min-w-0">
                          <div className="text-[13px] font-medium text-[#202124]">{t("settings.memory.maintTitle")}</div>
                          <div className="mt-1 text-[12px] text-[#7a8088]">
                            {t("settings.memory.maintDesc")}
                          </div>
                        </div>
                        <div className="flex shrink-0 gap-2">
                          <button
                            className={secondaryButtonCls}
                            type="button"
                            disabled={memoryBusy}
                            onClick={() => runMemoryAction(api.memoryPanelState)}
                          >
                            {t("settings.memory.refresh")}
                          </button>
                          <button
                            className={secondaryButtonCls}
                            type="button"
                            disabled={memoryBusy || !memoryState?.duplicates.length}
                            onClick={() => runMemoryAction(api.memoryDedupeApply)}
                          >
                            {t("settings.memory.dedupe", { n: memoryState?.duplicates.length || 0 })}
                          </button>
                        </div>
                      </div>
                      {memoryError && (
                        <div className="mt-3 rounded-lg border border-[#ffd7d7] bg-[#fff7f7] p-3 text-[12px] text-[#b42318]">
                          {memoryError}
                        </div>
                      )}
                      <div className="mt-3 max-h-[520px] space-y-3 overflow-auto pr-1">
                        {memoryState?.scopes.length ? (
                          memoryState.scopes.map((scope) => {
                            const defaultKind =
                              scope.id === "user"
                                ? "user"
                                : scope.id === "session"
                                  ? "session"
                                  : scope.id === "pack"
                                    ? "pack"
                                    : "project";
                            const draft = memoryDrafts[scope.id] ?? { kind: defaultKind, text: "" };
                            return (
                              <div key={scope.id} className="rounded-lg border border-[#e2e5ea] bg-white">
                                <div className="border-b border-[#eef1f4] px-3 py-2">
                                  <div className="flex flex-wrap items-start justify-between gap-2">
                                    <div className="min-w-0">
                                      <div className="text-[13px] font-medium text-[#202124]">{scope.label}</div>
                                      <div className="mt-0.5 break-all text-[11px] text-[#8a9099]">{scope.path}</div>
                                    </div>
                                    <span className="rounded-md bg-[#eef1f5] px-1.5 py-0.5 text-[11px] text-[#59616d]">
                                      {t("settings.memory.entriesCount", { n: scope.entries.length })}
                                      {scope.duplicates.length ? t("settings.memory.dupesCount", { n: scope.duplicates.length }) : ""}
                                    </span>
                                  </div>
                                </div>
                                <div className="space-y-2 p-3">
                                  <div className="rounded-md border border-dashed border-[#d8dde5] bg-[#fbfcfd] p-2">
                                    <div className="grid gap-2 md:grid-cols-[130px_minmax(0,1fr)_auto]">
                                      <select
                                        className={inputCls}
                                        value={draft.kind}
                                        onChange={(e) =>
                                          setMemoryDrafts((cur) => ({
                                            ...cur,
                                            [scope.id]: { ...draft, kind: e.target.value },
                                          }))
                                        }
                                      >
                                        <option value="user">user</option>
                                        <option value="project">project</option>
                                        <option value="session">session</option>
                                        <option value="pack">pack</option>
                                        <option value="preference">preference</option>
                                      </select>
                                      <input
                                        className={inputCls}
                                        value={draft.text}
                                        placeholder={t("settings.memory.addPlaceholder", { scope: scope.label })}
                                        onChange={(e) =>
                                          setMemoryDrafts((cur) => ({
                                            ...cur,
                                            [scope.id]: { ...draft, text: e.target.value },
                                          }))
                                        }
                                      />
                                      <button
                                        className={secondaryButtonCls}
                                        type="button"
                                        disabled={memoryBusy || !draft.text.trim()}
                                        onClick={() =>
                                          runMemoryAction(async () => {
                                            const next = await api.memoryAddEntry(scope.id, draft.kind, draft.text);
                                            setMemoryDrafts((cur) => ({
                                              ...cur,
                                              [scope.id]: { ...draft, text: "" },
                                            }));
                                            return next;
                                          })
                                        }
                                      >
                                        {t("settings.memory.add")}
                                      </button>
                                    </div>
                                  </div>
                                  {scope.entries.length ? (
                                    scope.entries.map((entry) => {
                                      const duplicate = scope.duplicates.some((group) =>
                                        group.duplicate_ids.includes(entry.id),
                                      );
                                      return (
                                        <div key={entry.id} className="rounded-md border border-[#e8ebef] bg-white p-2">
                                          <div className="mb-2 flex flex-wrap items-center gap-2 text-[11px] text-[#7a8088]">
                                            <select
                                              className="h-7 rounded-md border border-[#d9d9d9] bg-white px-2 text-[11px] text-[#4f5661] outline-none"
                                              value={entry.kind}
                                              onChange={(e) => {
                                                const kind = e.target.value;
                                                setMemoryState((cur) =>
                                                  cur
                                                    ? {
                                                        ...cur,
                                                        entries: cur.entries.map((item) =>
                                                          item.id === entry.id ? { ...item, kind } : item,
                                                        ),
                                                        scopes: cur.scopes.map((itemScope) => ({
                                                          ...itemScope,
                                                          entries: itemScope.entries.map((item) =>
                                                            item.id === entry.id ? { ...item, kind } : item,
                                                          ),
                                                        })),
                                                      }
                                                    : cur,
                                                );
                                              }}
                                            >
                                              <option value="user">user</option>
                                              <option value="project">project</option>
                                              <option value="session">session</option>
                                              <option value="pack">pack</option>
                                              <option value="preference">preference</option>
                                            </select>
                                            <span>{t("settings.memory.line", { n: entry.line })}</span>
                                            {duplicate && (
                                              <span className="rounded-md bg-[#fff6dd] px-1.5 py-0.5 text-[#8a5a00]">
                                                {t("settings.memory.duplicate")}
                                              </span>
                                            )}
                                          </div>
                                          <textarea
                                            className="min-h-16 w-full resize-y rounded-md border border-[#d9d9d9] px-2 py-1.5 text-[12px] text-[#202124] outline-none transition focus:border-[#7a7f87] focus:ring-1 focus:ring-[#202124]/10"
                                            value={entry.text}
                                            onChange={(e) => {
                                              const text = e.target.value;
                                              setMemoryState((cur) =>
                                                cur
                                                  ? {
                                                      ...cur,
                                                      entries: cur.entries.map((item) =>
                                                        item.id === entry.id ? { ...item, text } : item,
                                                      ),
                                                      scopes: cur.scopes.map((itemScope) => ({
                                                        ...itemScope,
                                                        entries: itemScope.entries.map((item) =>
                                                          item.id === entry.id ? { ...item, text } : item,
                                                        ),
                                                      })),
                                                    }
                                                  : cur,
                                              );
                                            }}
                                          />
                                          <div className="mt-2 flex justify-end gap-2">
                                            <button
                                              className={secondaryButtonCls}
                                              type="button"
                                              disabled={memoryBusy}
                                              onClick={() => runMemoryAction(() => api.memoryDeleteEntry(entry.id))}
                                            >
                                              {t("settings.memory.delete")}
                                            </button>
                                            <button
                                              className="inline-flex h-8 items-center justify-center rounded-md bg-[#111827] px-3 text-[12px] font-medium text-white transition hover:bg-[#2b3442] disabled:cursor-not-allowed disabled:bg-[#b8bec8]"
                                              type="button"
                                              disabled={memoryBusy}
                                              onClick={() =>
                                                runMemoryAction(() =>
                                                  api.memoryUpdateEntry(entry.id, entry.kind, entry.text),
                                                )
                                              }
                                            >
                                              {t("settings.memory.save")}
                                            </button>
                                          </div>
                                        </div>
                                      );
                                    })
                                  ) : (
                                    <div className="rounded-md border border-dashed border-[#d8dde5] bg-white p-3 text-[12px] text-[#7a8088]">
                                      {t("settings.memory.noEntries", { scope: scope.label })}
                                    </div>
                                  )}
                                </div>
                              </div>
                            );
                          })
                        ) : (
                          <div className="rounded-lg border border-dashed border-[#d8dde5] bg-white p-4 text-[12px] text-[#7a8088]">
                            {t("settings.memory.noScopes")}
                          </div>
                        )}
                      </div>
                    </div>
                  </Section>
                  <Section title={t("settings.agents.title")} description={t("settings.agents.desc")}>
                    <div className="rounded-lg border border-[#e2e5ea] bg-[#fbfcfd]">
                      <input
                        ref={agentImportInputRef}
                        className="hidden"
                        type="file"
                        accept="application/json,.json"
                        onChange={(e) => {
                          const file = e.currentTarget.files?.[0];
                          e.currentTarget.value = "";
                          void importAgentJson(file);
                        }}
                      />
                      <div className="flex items-center justify-between gap-3 border-b border-[#e8ebef] px-3 py-3">
                        <div className="min-w-0">
                          <div className="text-[13px] font-medium text-[#202124]">
                            {t("settings.agents.definitions", { n: agentState.definitions.length })}
                          </div>
                          <div className="mt-1 break-all text-[12px] text-[#7a8088]">
                            {agentState.agents_dir || ".demiurge/agents"}
                          </div>
                        </div>
                        <div className="flex shrink-0 flex-wrap justify-end gap-2">
                          <button className={secondaryButtonCls} type="button" disabled={agentBusy} onClick={refreshAgents}>
                            {t("settings.agents.refresh")}
                          </button>
                          <button
                            className={secondaryButtonCls}
                            type="button"
                            disabled={agentBusy}
                            onClick={() => agentImportInputRef.current?.click()}
                          >
                            {t("settings.agents.import")}
                          </button>
                          <button
                            className={secondaryButtonCls}
                            type="button"
                            disabled={agentBusy || !agentState.definitions.length}
                            onClick={exportAllAgents}
                          >
                            {t("settings.agents.exportAll")}
                          </button>
                          <button className={secondaryButtonCls} type="button" disabled={agentBusy} onClick={newAgentTemplate}>
                            {t("settings.agents.newTemplate")}
                          </button>
                        </div>
                      </div>

                      <div className="grid min-h-[360px] grid-cols-1 gap-0 md:grid-cols-[240px_1fr]">
                        <div className="border-b border-[#e8ebef] p-2 md:border-b-0 md:border-r">
                          <div className="max-h-[356px] space-y-1 overflow-y-auto">
                            {agentState.definitions.length ? (
                              agentState.definitions.map((agent) => {
                                const selected = agentFile?.name === agent.name;
                                return (
                                  <button
                                    key={agent.name}
                                    type="button"
                                    onClick={() => loadAgent(agent.name)}
                                    className={`w-full rounded-md px-2.5 py-2 text-left transition hover:bg-white ${
                                      selected ? "bg-white shadow-sm" : ""
                                    }`}
                                  >
                                    <div className="truncate text-[13px] font-semibold text-[#202124]">{agent.name}</div>
                                    <div className="mt-0.5 truncate text-[11px] text-[#7a8088]">
                                      {agent.kind} / {agent.description || agent.path}
                                    </div>
                                    <div className="mt-1 flex flex-wrap gap-1 text-[10px] text-[#8a9099]">
                                      <span className="rounded bg-[#eef1f5] px-1.5 py-0.5">
                                        {t("settings.agents.runs", { n: agent.runtime.run_count })}
                                      </span>
                                      <span className="rounded bg-[#eef1f5] px-1.5 py-0.5">
                                        {t("settings.agents.tokens", { n: agent.runtime.total_tokens.toLocaleString() })}
                                      </span>
                                      {agent.runtime.error_count > 0 && (
                                        <span className="rounded bg-[#fff1f0] px-1.5 py-0.5 text-[#b42318]">
                                          {t("settings.agents.errors", { n: agent.runtime.error_count })}
                                        </span>
                                      )}
                                    </div>
                                    {agent.invalid_tools.length ? (
                                      <div className="mt-0.5 truncate text-[11px] text-[#b42318]">
                                        {t("settings.agents.invalid", { tools: agent.invalid_tools.join(", ") })}
                                      </div>
                                    ) : null}
                                  </button>
                                );
                              })
                            ) : (
                              <div className="rounded-md border border-dashed border-[#d8dde5] bg-white p-3 text-[12px] text-[#7a8088]">
                                {t("settings.agents.noDefinitions")}
                              </div>
                            )}
                          </div>
                        </div>

                        <div className="min-w-0 p-3">
                          <div className="grid gap-3 sm:grid-cols-[1fr_auto]">
                            <Field label={t("settings.agents.fileName")}>
                              <input
                                className={inputCls}
                                value={agentFileName}
                                placeholder="researcher.json"
                                onChange={(e) => setAgentFileName(e.target.value)}
                              />
                            </Field>
                            <div className="mt-[22px] flex flex-wrap gap-2">
                              <button
                                className={secondaryButtonCls}
                                type="button"
                                disabled={agentBusy || !agentJson.trim()}
                                onClick={validateAgentJson}
                              >
                                {t("settings.agents.validate")}
                              </button>
                              <button
                                className={secondaryButtonCls}
                                type="button"
                                disabled={agentBusy || !agentJson.trim()}
                                onClick={exportCurrentAgentJson}
                              >
                                {t("settings.agents.export")}
                              </button>
                              <button
                                className="inline-flex h-9 items-center justify-center rounded-md bg-[#111827] px-4 text-[12px] font-medium text-white transition hover:bg-[#2b3442] disabled:cursor-not-allowed disabled:bg-[#b8bec8]"
                                type="button"
                                disabled={agentBusy || !agentJson.trim()}
                                onClick={saveAgentJson}
                              >
                                {t("settings.agents.save")}
                              </button>
                            </div>
                          </div>

                          <textarea
                            className="mt-3 min-h-[230px] w-full resize-y rounded-md border border-[#d9d9d9] bg-white px-3 py-2 font-mono text-[12px] leading-5 text-[#202124] outline-none transition focus:border-[#7a7f87] focus:ring-1 focus:ring-[#202124]/10"
                            value={agentJson}
                            spellCheck={false}
                            placeholder={'{\n  "name": "researcher"\n}'}
                            onChange={(e) => {
                              setAgentJson(e.target.value);
                              setAgentValidation(null);
                            }}
                          />

                          {selectedAgentDefinition && (
                            <div className="mt-3 grid gap-2 rounded-lg border border-[#e2e5ea] bg-white p-3 text-[12px] text-[#6f7782] sm:grid-cols-4">
                              <div>
                                <div className="text-[11px] text-[#8a9099]">{t("settings.agents.statRuns")}</div>
                                <div className="mt-1 font-semibold text-[#202124]">
                                  {selectedAgentDefinition.runtime.run_count.toLocaleString()}
                                </div>
                              </div>
                              <div>
                                <div className="text-[11px] text-[#8a9099]">{t("settings.agents.statTokens")}</div>
                                <div className="mt-1 font-semibold text-[#202124]">
                                  {selectedAgentDefinition.runtime.total_tokens.toLocaleString()}
                                </div>
                              </div>
                              <div>
                                <div className="text-[11px] text-[#8a9099]">{t("settings.agents.statErrors")}</div>
                                <div className="mt-1 font-semibold text-[#202124]">
                                  {selectedAgentDefinition.runtime.error_count.toLocaleString()}
                                </div>
                              </div>
                              <div>
                                <div className="text-[11px] text-[#8a9099]">{t("settings.agents.statLastUsed")}</div>
                                <div className="mt-1 font-semibold text-[#202124]">
                                  {formatTime(selectedAgentDefinition.runtime.last_used_at || 0)}
                                </div>
                              </div>
                              {selectedAgentDefinition.runtime.last_error && (
                                <div className="min-w-0 sm:col-span-4">
                                  <div className="text-[11px] text-[#8a9099]">{t("settings.agents.statLastError")}</div>
                                  <div className="mt-1 break-words text-[#b42318]">
                                    {selectedAgentDefinition.runtime.last_error}
                                  </div>
                                </div>
                              )}
                            </div>
                          )}

                          <div className="mt-3 flex flex-wrap items-center gap-2">
                            {agentFile && (
                              <button
                                className={secondaryButtonCls}
                                type="button"
                                disabled={agentBusy}
                                onClick={() => deleteAgentJson(agentFile.name)}
                              >
                                {t("settings.agents.delete")}
                              </button>
                            )}
                            {agentFile?.path && (
                              <span className="min-w-0 truncate text-[12px] text-[#7a8088]" title={agentFile.path}>
                                {agentFile.path}
                              </span>
                            )}
                            {agentStatus && <span className="text-[12px] text-[#59616d]">{agentStatus}</span>}
                          </div>

                          {agentValidation && (
                            <div
                              className={`mt-3 rounded-lg border p-3 text-[12px] leading-5 ${
                                agentValidation.ok
                                  ? "border-[#cfe8d8] bg-[#f3fbf6] text-[#286444]"
                                  : "border-[#ffd7d7] bg-[#fff7f7] text-[#b42318]"
                              }`}
                            >
                              <div className="font-semibold">
                                {agentValidation.ok ? t("settings.agents.validationPassed") : t("settings.agents.validationFailed")}
                              </div>
                              {agentValidation.normalized_name && (
                                <div>
                                  {agentValidation.normalized_name} / {agentValidation.suggested_file_name}
                                </div>
                              )}
                              {agentValidation.errors.map((error) => (
                                <div key={error}>{t("settings.agents.errorPrefix", { msg: error })}</div>
                              ))}
                              {agentValidation.warnings.map((warning) => (
                                <div key={warning}>{t("settings.agents.warningPrefix", { msg: warning })}</div>
                              ))}
                            </div>
                          )}
                        </div>
                      </div>
                    </div>
                  </Section>
                </>
              )}

              {activeTab === "tools" && (
                <>
                  <Section title={t("settings.mcp.title")}>
                    <div className="mb-3 flex items-center justify-between gap-3">
                      <div className="text-[12px] text-[#7a8088]">
                        {t("settings.mcp.desc")}
                      </div>
                      <div className="flex shrink-0 gap-2">
                        <button className={secondaryButtonCls} type="button" disabled={mcpBusy} onClick={refreshMcp}>
                          {t("settings.mcp.refresh")}
                        </button>
                        <button className={secondaryButtonCls} type="button" onClick={addMcpServer}>
                          {t("settings.mcp.addServer")}
                        </button>
                      </div>
                    </div>
                    {mcpError && (
                      <div className="mb-3 rounded-lg border border-[#ffd7d7] bg-[#fff7f7] p-3 text-[12px] text-[#b42318]">
                        {mcpError}
                      </div>
                    )}
                    <div className="space-y-3">
                      {form.mcp_servers.length ? (
                        form.mcp_servers.map((server, index) => {
                          const runtime = mcpState?.servers.find((item) => item.name === server.name);
                          const saved = settings.mcp_servers.some((item) => item.name === server.name);
                          const status = runtime?.status;
                          return (
                            <div key={`${server.name}:${index}`} className="rounded-lg border border-[#e2e5ea] bg-[#fbfcfd] p-3">
                              <div className="mb-3 flex flex-wrap items-center gap-2">
                                <input
                                  className={`${inputCls} max-w-[220px]`}
                                  value={server.name}
                                  placeholder={t("settings.mcp.serverNamePlaceholder")}
                                  onChange={(e) => updateMcpServer(index, { name: e.target.value })}
                                />
                                <span className="rounded-md bg-white px-2 py-1 text-[11px] text-[#5f6368]">
                                  {mcpStatusLabel(status, t)}
                                </span>
                                {runtime?.tool_count ? (
                                  <span className="rounded-md bg-white px-2 py-1 text-[11px] text-[#5f6368]">
                                    {t("settings.mcp.toolsCount", { n: runtime.tool_count })}
                                  </span>
                                ) : null}
                                <label className="ml-auto flex items-center gap-2 text-[12px] text-[#5f6368]">
                                  <input
                                    type="checkbox"
                                    className="h-4 w-4 accent-[#111827]"
                                    checked={server.enabled}
                                    onChange={(e) => updateMcpServer(index, { enabled: e.target.checked })}
                                  />
                                  {t("settings.mcp.enabled")}
                                </label>
                                {saved && (
                                  <button
                                    className={secondaryButtonCls}
                                    type="button"
                                    disabled={mcpBusy}
                                    onClick={() => setSavedMcpServerEnabled(server.name, !server.enabled)}
                                  >
                                    {server.enabled ? t("settings.mcp.stop") : t("settings.mcp.start")}
                                  </button>
                                )}
                                <button className={secondaryButtonCls} type="button" onClick={() => removeMcpServer(index)}>
                                  {t("settings.mcp.remove")}
                                </button>
                              </div>
                              <div className="grid gap-3 sm:grid-cols-[1fr_1fr]">
                                <Field label={t("settings.mcp.command")}>
                                  <input
                                    className={inputCls}
                                    value={server.command}
                                    placeholder={navigator.userAgent.includes("Windows") ? "cmd" : "npx"}
                                    onChange={(e) => updateMcpServer(index, { command: e.target.value })}
                                  />
                                </Field>
                                <Field label={t("settings.mcp.transport")}>
                                  <input className={inputCls} value="stdio" disabled />
                                </Field>
                              </div>
                              <div className="mt-3 grid gap-3 sm:grid-cols-2">
                                <Field label={t("settings.mcp.args")} help={t("settings.mcp.argsHelp")}>
                                  <textarea
                                    className="min-h-24 w-full resize-y rounded-md border border-[#d9d9d9] bg-white px-3 py-2 text-[12px] text-[#202124] outline-none transition focus:border-[#7a7f87] focus:ring-1 focus:ring-[#202124]/10"
                                    value={server.args.join("\n")}
                                    placeholder={"/c\nnpx\n-y\n@modelcontextprotocol/server-filesystem"}
                                    onChange={(e) => updateMcpServer(index, { args: splitLines(e.target.value) })}
                                  />
                                </Field>
                                <Field label={t("settings.mcp.env")} help={t("settings.mcp.envHelp")}>
                                  <textarea
                                    className="min-h-24 w-full resize-y rounded-md border border-[#d9d9d9] bg-white px-3 py-2 text-[12px] text-[#202124] outline-none transition focus:border-[#7a7f87] focus:ring-1 focus:ring-[#202124]/10"
                                    value={formatEnvLines(server)}
                                    placeholder={"API_KEY=...\nBASE_URL=https://..."}
                                    onChange={(e) => updateMcpServer(index, { env: parseEnvLines(e.target.value) })}
                                  />
                                </Field>
                              </div>
                              {runtime?.error && (
                                <div className="mt-3 rounded-md border border-[#ffd7d7] bg-white px-3 py-2 text-[12px] text-[#b42318]">
                                  {runtime.error}
                                </div>
                              )}
                              {runtime?.stderr_tail && (
                                <pre className="mt-3 max-h-28 overflow-auto whitespace-pre-wrap rounded-md border border-[#e2e5ea] bg-white p-2 text-[11px] text-[#6f7782]">
                                  {runtime.stderr_tail}
                                </pre>
                              )}
                            </div>
                          );
                        })
                      ) : (
                        <div className="rounded-lg border border-dashed border-[#d8dde5] bg-[#fbfcfd] p-4 text-[12px] text-[#7a8088]">
                          {t("settings.mcp.noServers")}
                        </div>
                      )}
                    </div>
                    <div className="mt-4 rounded-lg border border-[#e2e5ea] bg-[#fbfcfd] p-3">
                      <div className="mb-2 text-[13px] font-medium text-[#202124]">{t("settings.mcp.discoveredTools")}</div>
                      <div className="max-h-48 overflow-auto">
                        {mcpState?.tools.length ? (
                          <div className="grid gap-2">
                            {mcpState.tools.map((tool) => (
                              <div key={tool.name} className="rounded-md border border-[#e2e5ea] bg-white p-2">
                                <div className="flex flex-wrap items-center gap-2">
                                  <span className="font-mono text-[12px] text-[#202124]">{tool.name}</span>
                                  <span className="rounded-md bg-[#eef1f5] px-1.5 py-0.5 text-[11px] text-[#5f6368]">
                                    {tool.risk}
                                  </span>
                                </div>
                                <div className="mt-1 line-clamp-2 text-[12px] text-[#7a8088]">
                                  {tool.description || tool.original_name}
                                </div>
                              </div>
                            ))}
                          </div>
                        ) : (
                          <div className="text-[12px] text-[#7a8088]">{t("settings.mcp.noTools")}</div>
                        )}
                      </div>
                      {mcpState?.resources && Object.keys(mcpState.resources).length > 0 && (
                        <div className="mt-3 border-t border-[#e2e5ea] pt-3">
                          <div className="mb-2 text-[13px] font-medium text-[#202124]">{t("settings.mcp.resources")}</div>
                          <div className="space-y-1 text-[12px] text-[#6f7782]">
                            {Object.entries(mcpState.resources).map(([serverName, resources]) => (
                              <div key={serverName}>
                                <span className="font-medium text-[#202124]">{serverName}</span>:{" "}
                                {t("settings.mcp.resourcesCount", { n: resources.length })}
                              </div>
                            ))}
                          </div>
                        </div>
                      )}
                    </div>
                  </Section>
                  <Section title={t("settings.ocr.title")}>
                    <ToggleRow
                      checked={form.computer_use_enabled}
                      title={t("settings.ocr.toggle")}
                      description={t("settings.ocr.toggleDesc")}
                      onChange={(checked) => set("computer_use_enabled", checked)}
                    />
                    <div className="mt-4 grid gap-3 sm:grid-cols-[minmax(0,1fr)_auto_auto]">
                      <Field label={t("settings.ocr.modelSource")} help={t(selectedOcrSource.noteKey)}>
                        <Select
                          value={form.ocr_model_source}
                          onChange={(v) => set("ocr_model_source", v as OcrModelSource)}
                          options={ocrSources.map((s) => ({ value: s.value, label: s.label }))}
                        />
                      </Field>
                      <button
                        className={`${secondaryButtonCls} mt-[22px]`}
                        disabled={ocrBusy}
                        onClick={refreshOcrStatus}
                        type="button"
                      >
                        {t("settings.ocr.refresh")}
                      </button>
                      <button
                        className="mt-[22px] inline-flex h-9 items-center justify-center rounded-md bg-[#111827] px-4 text-[12px] font-medium text-white transition hover:bg-[#2b3442] disabled:cursor-not-allowed disabled:bg-[#b8bec8]"
                        disabled={ocrBusy}
                        onClick={downloadOcrModels}
                        type="button"
                      >
                        {ocrBusy ? t("settings.ocr.downloading") : ocrStatus?.installed ? t("settings.ocr.redownload") : t("settings.ocr.download")}
                      </button>
                    </div>
                    <div className="mt-3 rounded-lg border border-[#e2e5ea] bg-[#fbfcfd] p-3 text-[12px] leading-5 text-[#6f7782]">
                      <div className="flex flex-wrap items-center justify-between gap-2">
                        <div className="font-medium text-[#202124]">
                          {ocrStatus?.installed ? t("settings.ocr.ready") : t("settings.ocr.missingModels")}
                          {ocrStatus ? ` · ${formatBytes(ocrStatus.totalBytes)}` : ""}
                        </div>
                        <a
                          className="text-[#1557b0] hover:underline"
                          href={ocrStatus?.sourceUrl || selectedOcrSource.url}
                          rel="noreferrer"
                          target="_blank"
                        >
                          {ocrStatus?.sourceLabel || selectedOcrSource.label}
                        </a>
                      </div>
                      <div className="mt-1 text-[#7a8088]">{ocrStatus?.sourceNote || t(selectedOcrSource.noteKey)}</div>
                      {ocrStatus?.modelDir && <div className="mt-2 break-all">{t("settings.ocr.pathPrefix", { path: ocrStatus.modelDir })}</div>}
                      {ocrStatus && (
                        <div className="mt-3 overflow-hidden rounded-md border border-[#e4e7eb] bg-white">
                          {ocrStatus.files.map((file) => (
                            <div
                              key={file.name}
                              className="grid grid-cols-[minmax(0,1fr)_78px_62px] items-center gap-3 border-b border-[#f0f2f5] px-3 py-2 last:border-b-0"
                            >
                              <div className="min-w-0 truncate font-mono text-[11px] text-[#3b4350]" title={file.name}>
                                {file.name}
                              </div>
                              <div className={file.present ? "text-[#177245]" : "text-[#b42318]"}>
                                {file.present ? t("settings.ocr.present") : t("settings.ocr.missing")}
                              </div>
                              <a
                                className="text-right text-[#1557b0] hover:underline"
                                href={file.downloadUrl}
                                rel="noreferrer"
                                target="_blank"
                              >
                                {t("settings.ocr.source")}
                              </a>
                            </div>
                          ))}
                        </div>
                      )}
                      {ocrStatus && !ocrStatus.installed && (
                        <div className="mt-3 rounded-md border border-[#f5d6a4] bg-[#fff8eb] px-3 py-2 text-[#7a4d00]">
                          {t("settings.ocr.missingHint", { files: ocrStatus.missing.join(", "), hint: ocrStatus.manualInstallHint })}
                        </div>
                      )}
                      {ocrProgress && (
                        <div className="mt-3 space-y-2">
                          <div className="flex flex-wrap items-center justify-between gap-2 text-[#3b4350]">
                            <span>
                              {t("settings.ocr.overall", {
                                pct: ocrOverallProgressPct ?? 0,
                                done: ocrProgress.completedFiles,
                                total: ocrProgress.totalFiles,
                              })}
                            </span>
                            <span>{formatBytes(ocrProgress.downloadedTotalBytes)}</span>
                          </div>
                          <div className="h-2 overflow-hidden rounded-full bg-[#e5e9ef]">
                            <div
                              className="h-full rounded-full bg-[#111827] transition-[width]"
                              style={{ width: `${ocrOverallProgressPct ?? 0}%` }}
                            />
                          </div>
                          <div className="flex flex-wrap items-center justify-between gap-2 text-[#6f7782]">
                            <span>
                              {t("settings.ocr.fileProgress", {
                                index: ocrProgress.index,
                                total: ocrProgress.totalFiles,
                                file: ocrProgress.file,
                                phase: ocrProgress.phase,
                              })}
                            </span>
                            <span>
                              {formatBytes(ocrProgress.downloadedBytes)}
                              {ocrProgress.totalBytes ? ` / ${formatBytes(ocrProgress.totalBytes)}` : ""}
                              {ocrFileProgressPct !== null ? ` (${ocrFileProgressPct}%)` : ""}
                            </span>
                          </div>
                          {ocrFileProgressPct !== null && (
                            <div className="h-1.5 overflow-hidden rounded-full bg-[#edf1f5]">
                              <div
                                className="h-full rounded-full bg-[#59616d] transition-[width]"
                                style={{ width: `${ocrFileProgressPct}%` }}
                              />
                            </div>
                          )}
                        </div>
                      )}
                      {ocrError && <div className="text-[#b42318]">{ocrError}</div>}
                    </div>
                  </Section>
                  <Section title={t("settings.perm.title")} description={t("settings.perm.desc")}>
                    <div className="mb-3 flex justify-between gap-3">
                      <div className="text-[12px] leading-5 text-[#7a8088]">
                        {t("settings.perm.resolutionOrder")}
                      </div>
                      <button
                        className={secondaryButtonCls}
                        type="button"
                        disabled={permissionBusy}
                        onClick={refreshPermissionState}
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
                            onChange={(e) => setPermissionDraft((draft) => ({ ...draft, tool: e.target.value }))}
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
                              setPermissionDraft((draft) => ({
                                ...draft,
                                effect: e.target.value as PermissionEffect,
                              }))
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
                              setPermissionDraft((draft) => ({
                                ...draft,
                                scope: e.target.value as Exclude<PermissionScope, "once">,
                              }))
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
                            onChange={(e) => setPermissionDraft((draft) => ({ ...draft, reason: e.target.value }))}
                          />
                        </Field>
                        <button
                          className="mt-[22px] inline-flex h-9 items-center justify-center rounded-md bg-[#111827] px-4 text-[12px] font-medium text-white transition hover:bg-[#2b3442] disabled:cursor-not-allowed disabled:bg-[#b8bec8]"
                          type="button"
                          disabled={permissionBusy || !permissionDraft.tool}
                          onClick={savePermissionRule}
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

                    <div className="space-y-2">
                      {permissionState?.rules.length ? (
                        permissionState.rules.map((rule) => (
                          <div key={`${rule.scope}:${rule.tool}`} className="rounded-lg border border-[#e2e5ea] bg-white p-3">
                            <div className="flex items-start gap-3">
                              <div className="min-w-0 flex-1">
                                <div className="truncate text-[13px] font-semibold text-[#202124]">{rule.tool}</div>
                                <div className="mt-1 text-[12px] text-[#7a8088]">
                                  {permissionEffectLabel(rule.effect, t)} / {permissionScopeLabel(rule.scope, t)} /{" "}
                                  {formatTime(rule.updated_at)}
                                </div>
                                <div className="mt-1 text-[12px] leading-5 text-[#8a9099]">{rule.reason}</div>
                              </div>
                              <button
                                className={secondaryButtonCls}
                                type="button"
                                disabled={permissionBusy || rule.scope === "once"}
                                onClick={() => editPermissionRule(rule.scope, rule.tool, rule.effect, rule.reason)}
                              >
                                {t("settings.perm.edit")}
                              </button>
                              <button
                                className={secondaryButtonCls}
                                type="button"
                                disabled={permissionBusy}
                                onClick={() => resetPermissionRule(rule.scope, rule.tool)}
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
                          </div>
                        ))
                      ) : (
                        <div>{t("settings.perm.noAudit")}</div>
                      )}
                    </div>
                  </Section>
                </>
              )}

              {activeTab === "voice" && (
                <>
                  <Section title={t("settings.voice.title")}>
                    <ToggleRow
                      checked={form.voice_enabled}
                      title={t("settings.voice.toggle")}
                      description={t("settings.voice.toggleDesc")}
                      onChange={(checked) => set("voice_enabled", checked)}
                    />
                    <div className="mt-4 grid gap-4 sm:grid-cols-2">
                      <Field label={t("settings.voice.stt")} help={t("settings.voice.sttHelp")}>
                        <input
                          className={inputCls}
                          value={form.voice_stt_backend}
                          placeholder="none / dashscope / openai"
                          onChange={(e) => set("voice_stt_backend", e.target.value)}
                        />
                      </Field>
                      <Field label={t("settings.voice.tts")}>
                        <input
                          className={inputCls}
                          value={form.voice_tts_backend}
                          placeholder="none / GPT-SoVITS / CosyVoice"
                          onChange={(e) => set("voice_tts_backend", e.target.value)}
                        />
                      </Field>
                    </div>
                    <div className="mt-4">
                      <Field label={t("settings.voice.voiceId")}>
                        <input
                          className={inputCls}
                          value={form.voice_id}
                          placeholder="default"
                          onChange={(e) => set("voice_id", e.target.value)}
                        />
                      </Field>
                    </div>
                  </Section>
                </>
              )}

              {activeTab === "advanced" && (
                <>
                  <Section title={t("settings.advanced.title")}>
                    <Field label={t("settings.advanced.maxChars")} help={t("settings.advanced.maxCharsHelp")}>
                      <input
                        className={inputCls}
                        type="number"
                        min={2000}
                        step={1000}
                        value={form.max_context_chars}
                        onChange={(e) => set("max_context_chars", Number(e.target.value) || 0)}
                      />
                    </Field>
                  </Section>
                </>
              )}
            </div>
          </div>
        </section>
    </div>
  );
}
