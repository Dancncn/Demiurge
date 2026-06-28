import { useEffect, useMemo, useState } from "react";
import * as api from "../lib/api";
import type {
  AgentPanelState,
  ContextPanelState,
  MemoryPanelState,
  McpPanelState,
  McpServerConfig,
  OcrDownloadProgress,
  OcrModelSource,
  OcrModelStatus,
  PackManifest,
  PermissionPanelState,
  PermissionScope,
  ProviderKind,
  Settings,
  WebDavBackupFile,
  WebDavConfig,
  WebSearchProvider,
} from "../lib/types";
import { CheckIcon, CloseIcon } from "./Icons";

interface Props {
  open: boolean;
  settings: Settings;
  packs: PackManifest[];
  agentPanel: AgentPanelState;
  onClose: () => void;
  onSave: (s: Settings) => void;
}

type SettingsTab = "provider" | "media" | "web" | "files" | "context" | "tools" | "voice" | "advanced";

type ProviderOption = {
  value: ProviderKind;
  label: string;
  short: string;
  baseUrl: string;
  model: string;
  help: string;
};

const providerOptions: ProviderOption[] = [
  {
    value: "deepseek",
    label: "DeepSeek",
    short: "DS",
    baseUrl: "https://api.deepseek.com/v1",
    model: "deepseek-chat",
    help: "DeepSeek official OpenAI-compatible endpoint.",
  },
  {
    value: "dashscope",
    label: "阿里云百炼",
    short: "BL",
    baseUrl: "https://dashscope.aliyuncs.com/compatible-mode/v1",
    model: "deepseek-v4-flash",
    help: "DashScope/Bailian OpenAI-compatible chat endpoint. Media uses native DashScope APIs below.",
  },
  {
    value: "openai",
    label: "ChatGPT / OpenAI",
    short: "AI",
    baseUrl: "https://api.openai.com/v1",
    model: "gpt-4o",
    help: "OpenAI chat completions endpoint.",
  },
  {
    value: "openrouter",
    label: "OpenRouter",
    short: "OR",
    baseUrl: "https://openrouter.ai/api/v1",
    model: "openai/gpt-4o",
    help: "OpenRouter model gateway.",
  },
  {
    value: "anthropic",
    label: "Anthropic",
    short: "AN",
    baseUrl: "https://api.anthropic.com/v1",
    model: "claude-sonnet-4-5",
    help: "Claude API. The key is stored in the system credential manager.",
  },
  {
    value: "gemini",
    label: "Gemini",
    short: "GE",
    baseUrl: "https://generativelanguage.googleapis.com/v1beta",
    model: "gemini-2.5-pro",
    help: "Google AI Studio compatible Gemini endpoint.",
  },
  {
    value: "glm",
    label: "智谱 GLM",
    short: "GL",
    baseUrl: "https://open.bigmodel.cn/api/paas/v4",
    model: "glm-4-plus",
    help: "Zhipu AI OpenAI-compatible endpoint.",
  },
  {
    value: "minimax",
    label: "MiniMax",
    short: "MM",
    baseUrl: "https://api.minimax.chat/v1",
    model: "MiniMax-Text-01",
    help: "MiniMax OpenAI-compatible endpoint.",
  },
  {
    value: "custom",
    label: "Custom Provider",
    short: "CU",
    baseUrl: "",
    model: "",
    help: "Any OpenAI-compatible endpoint.",
  },
  {
    value: "open_ai_compatible",
    label: "OpenAI Compatible",
    short: "OA",
    baseUrl: "https://api.deepseek.com/v1",
    model: "deepseek-chat",
    help: "Legacy generic OpenAI-compatible profile.",
  },
  {
    value: "local",
    label: "Local Endpoint",
    short: "LO",
    baseUrl: "http://localhost:11434/v1",
    model: "llama3.1",
    help: "Ollama, LM Studio, vLLM or any local OpenAI-compatible service.",
  },
];

const webSearchProviders: { value: WebSearchProvider; label: string; help: string }[] = [
  { value: "auto", label: "Auto", help: "Try Bing first, then fall back to DuckDuckGo." },
  { value: "bing", label: "Bing", help: "Public Bing result page. No API key required." },
  { value: "duckduckgo", label: "DuckDuckGo", help: "DuckDuckGo Instant Answer API. No API key required." },
  { value: "tavily", label: "Tavily", help: "Requires a Tavily API key." },
  { value: "brave", label: "Brave", help: "Requires a Brave Search API key." },
  { value: "exa", label: "Exa", help: "Requires an Exa API key." },
];

const ocrSources: { value: OcrModelSource; label: string }[] = [
  { value: "modelscope", label: "ModelScope" },
  { value: "huggingface", label: "Hugging Face" },
];

const inputCls =
  "h-9 w-full rounded-md border border-[#d9d9d9] bg-white px-3 text-[13px] text-[#202124] outline-none transition focus:border-[#7a7f87] focus:ring-2 focus:ring-[#202124]/5";
const labelCls = "mb-1.5 block text-[12px] font-medium text-[#5f6368]";
const secondaryButtonCls =
  "inline-flex h-8 items-center justify-center rounded-md border border-[#d9d9d9] bg-white px-3 text-[12px] font-medium text-[#333] transition hover:bg-[#f5f5f5] disabled:cursor-not-allowed disabled:opacity-50";

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

function normalizeWebSearchProvider(value: string): WebSearchProvider {
  return webSearchProviders.some((p) => p.value === value) ? (value as WebSearchProvider) : "auto";
}

function normalizeMediaProvider(value: string) {
  return value.trim() || "dashscope";
}

function permissionEffectLabel(effect: string) {
  if (effect === "allow") return "Allow";
  if (effect === "deny") return "Deny";
  return "Ask";
}

function permissionScopeLabel(scope: string) {
  if (scope === "session") return "Session";
  if (scope === "project") return "Project";
  return "Once";
}

function formatTime(ms: number) {
  if (!ms) return "-";
  return new Date(ms).toLocaleString();
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

function parseEnvLines(value: string) {
  return splitLines(value).map((line) => {
    const idx = line.indexOf("=");
    if (idx === -1) {
      return { key: line.trim(), value: "", secret: false };
    }
    return {
      key: line.slice(0, idx).trim(),
      value: line.slice(idx + 1),
      secret: false,
    };
  });
}

function formatEnvLines(server: McpServerConfig) {
  return server.env.map((env) => `${env.key}=${env.value}`).join("\n");
}

function mcpStatusLabel(status?: string) {
  if (status === "connected") return "Connected";
  if (status === "failed") return "Failed";
  if (status === "pending") return "Pending";
  if (status === "disabled") return "Disabled";
  return "Not started";
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
      <div className="mt-1 text-[15px] font-semibold text-[#202124]">{value}</div>
    </div>
  );
}

export default function SettingsDialog({ open, settings, packs, agentPanel, onClose, onSave }: Props) {
  const [form, setForm] = useState<Settings>(settings);
  const [activeTab, setActiveTab] = useState<SettingsTab>("provider");
  const [ocrStatus, setOcrStatus] = useState<OcrModelStatus | null>(null);
  const [ocrProgress, setOcrProgress] = useState<OcrDownloadProgress | null>(null);
  const [ocrBusy, setOcrBusy] = useState(false);
  const [ocrError, setOcrError] = useState("");
  const [permissionState, setPermissionState] = useState<PermissionPanelState | null>(null);
  const [permissionBusy, setPermissionBusy] = useState(false);
  const [mcpState, setMcpState] = useState<McpPanelState | null>(null);
  const [mcpBusy, setMcpBusy] = useState(false);
  const [mcpError, setMcpError] = useState("");
  const [contextState, setContextState] = useState<ContextPanelState | null>(null);
  const [memoryState, setMemoryState] = useState<MemoryPanelState | null>(null);
  const [memoryBusy, setMemoryBusy] = useState(false);
  const [memoryError, setMemoryError] = useState("");
  const [webdavBusy, setWebdavBusy] = useState(false);
  const [webdavStatus, setWebdavStatus] = useState("");
  const [webdavFiles, setWebdavFiles] = useState<WebDavBackupFile[]>([]);

  useEffect(() => {
    if (open) {
      setForm(settings);
      setWebdavStatus("");
      setWebdavFiles([]);
      setMcpError("");
    }
  }, [open, settings]);

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
    void api.permissionPanelState().then(
      (state) => {
        if (!cancelled) setPermissionState(state);
      },
      (err) => {
        if (!cancelled) console.error("Failed to read permission state", err);
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
    () => providerOptions.find((p) => p.value === form.provider) ?? providerOptions[0],
    [form.provider],
  );
  const selectedWebSearchProvider = useMemo(
    () => webSearchProviders.find((p) => p.value === form.web_search_provider) ?? webSearchProviders[0],
    [form.web_search_provider],
  );

  if (!open) return null;

  const set = <K extends keyof Settings>(k: K, v: Settings[K]) => setForm((f) => ({ ...f, [k]: v }));
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
      setWebdavStatus(`Backup created: ${fileName}`);
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
      setWebdavStatus("Backup list refreshed.");
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
      setWebdavStatus(`Deleted: ${fileName}`);
    } catch (err) {
      setWebdavStatus(String(err));
    } finally {
      setWebdavBusy(false);
    }
  }

  const progressPct =
    ocrProgress?.totalBytes && ocrProgress.totalBytes > 0
      ? Math.min(100, Math.round((ocrProgress.downloadedBytes / ocrProgress.totalBytes) * 100))
      : null;
  const tokenPct = Math.min(
    100,
    Math.round(((contextState?.estimated_history_tokens ?? 0) / Math.max(1, contextState?.max_input_tokens ?? 1)) * 100),
  );

  const navItems: { id: SettingsTab; label: string; detail: string }[] = [
    { id: "provider", label: "Providers", detail: selectedProvider.label },
    { id: "media", label: "Media", detail: form.image_model || "DashScope" },
    { id: "web", label: "Web Search", detail: selectedWebSearchProvider.label },
    { id: "files", label: "Files", detail: form.webdav_enabled ? "WebDAV enabled" : "Docs and backup" },
    { id: "context", label: "Context", detail: `${form.max_input_tokens} tokens` },
    { id: "tools", label: "Tools", detail: `${form.mcp_servers.length} MCP / ${ocrStatus?.installed ? "OCR ready" : "OCR"}` },
    { id: "voice", label: "Voice", detail: form.voice_enabled ? "Enabled" : "Disabled" },
    { id: "advanced", label: "Advanced", detail: "Storage and limits" },
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
    <div
      className="fixed inset-0 z-40 flex items-center justify-center bg-[#111827]/35 p-4 backdrop-blur-[2px]"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="flex h-[min(760px,92vh)] w-[min(1040px,96vw)] overflow-hidden rounded-xl border border-[#d7dbe2] bg-[#f6f7f9] shadow-[0_24px_80px_rgba(15,23,42,0.28)]">
        <aside className="flex w-[232px] shrink-0 flex-col border-r border-[#dfe3e8] bg-[#eef1f5]">
          <div className="flex h-12 items-center border-b border-[#dfe3e8] px-4">
            <div className="text-[13px] font-semibold text-[#202124]">Settings</div>
            <button
              className="ml-auto grid size-8 place-items-center rounded-md text-[#69707a] transition hover:bg-[#e3e7ed] hover:text-[#202124]"
              onClick={onClose}
              aria-label="Close settings"
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
                  className={`mb-1 flex w-full items-center gap-3 rounded-lg px-3 py-2.5 text-left transition ${
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
            API keys are stored in the system credential manager.
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
                Cancel
              </button>
              <button
                type="button"
                className="inline-flex h-8 items-center justify-center rounded-md bg-[#111827] px-4 text-[12px] font-medium text-white transition hover:bg-[#2b3442]"
                onClick={save}
              >
                Save
              </button>
            </div>
          </header>

          <div className="min-h-0 flex-1 overflow-y-auto">
            <div className="mx-auto max-w-[760px] px-6 py-5">
              {activeTab === "provider" && (
                <>
                  <Section title="Provider" description="Select the active LLM provider and configure its endpoint.">
                    <div className="grid gap-3 md:grid-cols-[240px_minmax(0,1fr)]">
                      <div className="space-y-1 rounded-lg border border-[#e2e5ea] bg-[#f8f9fb] p-1.5">
                        {providerOptions.map((provider) => {
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
                              className={`flex w-full items-center gap-2.5 rounded-md px-2.5 py-2 text-left transition ${
                                selected ? "bg-white shadow-sm" : "hover:bg-white/70"
                              }`}
                            >
                              <ProviderMark short={provider.short} selected={selected} />
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

                      <div className="rounded-lg border border-[#e2e5ea] bg-white">
                        <div className="flex items-center gap-3 border-b border-[#eceff3] px-4 py-3">
                          <ProviderMark short={selectedProvider.short} selected />
                          <div className="min-w-0 flex-1">
                            <div className="truncate text-[14px] font-semibold text-[#202124]">
                              {selectedProvider.label}
                            </div>
                            <div className="mt-0.5 truncate text-[12px] text-[#7a8088]">{selectedProvider.help}</div>
                          </div>
                        </div>
                        <div className="grid gap-4 p-4">
                          <Field label="Base URL" help={`Default: ${selectedProvider.baseUrl}`}>
                            <input
                              className={inputCls}
                              value={form.base_url}
                              placeholder={selectedProvider.baseUrl}
                              onChange={(e) => set("base_url", e.target.value)}
                            />
                          </Field>
                          <Field label="API Key" help="Saved securely outside settings.json. Local providers may leave this empty.">
                            <input
                              className={inputCls}
                              type="password"
                              value={form.api_key}
                              placeholder="sk-..."
                              onChange={(e) => set("api_key", e.target.value)}
                            />
                          </Field>
                          <Field label="Model">
                            <input
                              className={inputCls}
                              value={form.model}
                              placeholder={selectedProvider.model}
                              onChange={(e) => set("model", e.target.value)}
                            />
                          </Field>
                          <Field label="Persona Pack">
                            <select
                              className={inputCls}
                              value={form.current_pack}
                              onChange={(e) => set("current_pack", e.target.value)}
                            >
                              {packs.map((p) => (
                                <option key={p.id} value={p.id}>
                                  {p.name} ({p.id})
                                </option>
                              ))}
                            </select>
                          </Field>
                          <div className="flex flex-wrap gap-2">
                            <button
                              type="button"
                              className={secondaryButtonCls}
                              onClick={() => {
                                set("base_url", selectedProvider.baseUrl);
                                set("model", selectedProvider.model);
                              }}
                            >
                              Use defaults
                            </button>
                          </div>
                        </div>
                      </div>
                    </div>
                  </Section>
                </>
              )}

              {activeTab === "media" && (
                <>
                  <Section
                    title="Media Provider"
                    description="Image generation and TTS use DashScope native AIGC APIs. The key is stored outside settings.json."
                  >
                    <div className="grid gap-4">
                      <Field label="Provider">
                        <select
                          className={inputCls}
                          value={form.media_provider}
                          onChange={(e) => set("media_provider", e.target.value)}
                        >
                          <option value="dashscope">阿里云百炼 / DashScope</option>
                        </select>
                      </Field>
                      <Field label="Media Base URL" help="Use the origin only. Compatible-mode suffix is stripped automatically.">
                        <input
                          className={inputCls}
                          value={form.media_base_url}
                          placeholder="https://dashscope.aliyuncs.com"
                          onChange={(e) => set("media_base_url", e.target.value)}
                        />
                      </Field>
                      <Field label="Media API Key" help="Optional when the active LLM provider is DashScope and uses the same key.">
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
                  <Section title="Image Generation" description="Defaults used by the Images workspace.">
                    <div className="grid gap-4 sm:grid-cols-2">
                      <Field label="Image model">
                        <input
                          className={inputCls}
                          value={form.image_model}
                          placeholder="qwen-image-2.0"
                          onChange={(e) => set("image_model", e.target.value)}
                        />
                      </Field>
                      <Field label="Default size">
                        <select
                          className={inputCls}
                          value={form.image_size}
                          onChange={(e) => set("image_size", e.target.value)}
                        >
                          {["512*512", "768*768", "1024*1024", "1280*720", "720*1280"].map((size) => (
                            <option key={size} value={size}>
                              {size}
                            </option>
                          ))}
                        </select>
                      </Field>
                    </div>
                  </Section>
                  <Section title="Text To Speech" description="Defaults for DashScope TTS testing in the Images workspace.">
                    <div className="grid gap-4 sm:grid-cols-2">
                      <Field label="TTS model">
                        <input
                          className={inputCls}
                          value={form.tts_model}
                          placeholder="qwen3-tts-flash"
                          onChange={(e) => set("tts_model", e.target.value)}
                        />
                      </Field>
                      <Field label="Voice">
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

              {activeTab === "web" && (
                <>
                  <Section title="Search Provider" description="Configure the search backend used by the web_search tool.">
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
                            <div className="mt-1 text-[12px] leading-5 text-[#7a8088]">{provider.help}</div>
                          </button>
                        );
                      })}
                    </div>
                  </Section>
                  <Section title="API Keys" description="Only Tavily, Brave and Exa require keys. Environment variables still work.">
                    <div className="grid gap-4">
                      <Field label="Tavily API Key">
                        <input
                          className={inputCls}
                          type="password"
                          value={form.tavily_api_key}
                          placeholder="tvly-..."
                          onChange={(e) => set("tavily_api_key", e.target.value)}
                        />
                      </Field>
                      <Field label="Brave Search API Key">
                        <input
                          className={inputCls}
                          type="password"
                          value={form.brave_search_api_key}
                          placeholder="BSA..."
                          onChange={(e) => set("brave_search_api_key", e.target.value)}
                        />
                      </Field>
                      <Field label="Exa API Key">
                        <input
                          className={inputCls}
                          type="password"
                          value={form.exa_api_key}
                          placeholder="exa-..."
                          onChange={(e) => set("exa_api_key", e.target.value)}
                        />
                      </Field>
                    </div>
                  </Section>
                </>
              )}

              {activeTab === "files" && (
                <>
                  <Section
                    title="Document Processing"
                    description="Composer attachments are converted into prompt context before the agent turn starts."
                  >
                    <div className="grid gap-2 sm:grid-cols-2">
                      {[
                        ["Text and code", "TXT, Markdown, JSON, CSV and common source files are read as text."],
                        ["Images", "Images show as attachment previews. OCR can be run separately through screen/OCR tools."],
                        ["PDF", "PDF text is extracted in the renderer, up to the first 80 pages."],
                        ["Office", "DOCX, PPTX, XLSX and XLSM are parsed from OpenXML content into plain text."],
                        ["Mermaid", "```mermaid code blocks render as diagrams in assistant messages."],
                        ["Syntax highlight", "Language-tagged code blocks use highlight.js and keep copy controls."],
                      ].map(([title, description]) => (
                        <div key={title} className="rounded-lg border border-[#e2e5ea] bg-[#fbfcfd] p-3">
                          <div className="text-[13px] font-semibold text-[#202124]">{title}</div>
                          <div className="mt-1 text-[12px] leading-5 text-[#7a8088]">{description}</div>
                        </div>
                      ))}
                    </div>
                  </Section>

                  <Section
                    title="WebDAV Backup"
                    description="Back up Demiurge settings and sessions to a WebDAV directory. Passwords are stored in the system credential manager."
                  >
                    <ToggleRow
                      checked={form.webdav_enabled}
                      title="Enable WebDAV backup controls"
                      description="Keeps the configuration visible and ready for manual backup operations."
                      onChange={(checked) => set("webdav_enabled", checked)}
                    />
                    <div className="mt-4 grid gap-4">
                      <Field label="WebDAV URL" help="Example: https://example.com/remote.php/dav/files/you">
                        <input
                          className={inputCls}
                          value={form.webdav_url}
                          placeholder="https://..."
                          onChange={(e) => set("webdav_url", e.target.value)}
                        />
                      </Field>
                      <div className="grid gap-4 sm:grid-cols-2">
                        <Field label="Username">
                          <input
                            className={inputCls}
                            value={form.webdav_username}
                            onChange={(e) => set("webdav_username", e.target.value)}
                          />
                        </Field>
                        <Field label="Password">
                          <input
                            className={inputCls}
                            type="password"
                            value={form.webdav_password}
                            onChange={(e) => set("webdav_password", e.target.value)}
                          />
                        </Field>
                      </div>
                      <Field label="Backup path" help="A collection under the WebDAV URL. Demiurge will create it if possible.">
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
                        Test connection
                      </button>
                      <button className={secondaryButtonCls} disabled={webdavBusy} type="button" onClick={backupToWebDav}>
                        Backup now
                      </button>
                      <button className={secondaryButtonCls} disabled={webdavBusy} type="button" onClick={refreshWebDavFiles}>
                        List backups
                      </button>
                    </div>
                    {webdavStatus && (
                      <div className="mt-3 rounded-lg border border-[#e2e5ea] bg-[#fbfcfd] p-3 text-[12px] leading-5 text-[#6f7782]">
                        {webdavStatus}
                      </div>
                    )}
                    <div className="mt-4 overflow-hidden rounded-lg border border-[#e2e5ea]">
                      <div className="grid grid-cols-[minmax(0,1fr)_120px_80px] gap-2 border-b border-[#eceff3] bg-[#f8f9fb] px-3 py-2 text-[11px] font-semibold text-[#7a8088]">
                        <span>File</span>
                        <span>Modified</span>
                        <span className="text-right">Action</span>
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
                              Delete
                            </button>
                          </div>
                        ))
                      ) : (
                        <div className="px-3 py-4 text-[12px] text-[#7a8088]">No backup files loaded.</div>
                      )}
                    </div>
                  </Section>
                </>
              )}

              {activeTab === "context" && (
                <>
                  <Section title="Context Budget" description="These limits control prompt assembly and history trimming.">
                    <div className="grid gap-4 sm:grid-cols-2">
                      <Field label="Max input tokens">
                        <input
                          className={inputCls}
                          type="number"
                          min={4000}
                          step={1000}
                          value={form.max_input_tokens}
                          onChange={(e) => set("max_input_tokens", Number(e.target.value) || 0)}
                        />
                      </Field>
                      <Field label="Reserved output tokens">
                        <input
                          className={inputCls}
                          type="number"
                          min={512}
                          step={256}
                          value={form.reserved_output_tokens}
                          onChange={(e) => set("reserved_output_tokens", Number(e.target.value) || 0)}
                        />
                      </Field>
                    </div>
                    <div className="mt-4 rounded-lg border border-[#e2e5ea] bg-[#fbfcfd] p-3">
                      <div className="mb-3 flex items-center justify-between gap-3">
                        <div className="text-[13px] font-medium text-[#202124]">Current session</div>
                        <button
                          className={secondaryButtonCls}
                          type="button"
                          onClick={async () => setContextState(await api.contextPanelState())}
                        >
                          Refresh
                        </button>
                      </div>
                      <div className="grid grid-cols-2 gap-2 sm:grid-cols-4">
                        <ContextMetric label="Messages" value={contextState?.message_count ?? 0} />
                        <ContextMetric label="User" value={contextState?.user_messages ?? 0} />
                        <ContextMetric label="Assistant" value={contextState?.assistant_messages ?? 0} />
                        <ContextMetric label="Tool" value={contextState?.tool_messages ?? 0} />
                      </div>
                      <div className="mt-3 h-2 overflow-hidden rounded-full bg-[#e8ebef]">
                        <div className="h-full rounded-full bg-[#111827]" style={{ width: `${tokenPct}%` }} />
                      </div>
                      <div className="mt-2 text-[12px] text-[#7a8088]">
                        Estimated history {contextState?.estimated_history_tokens ?? 0} /{" "}
                        {contextState?.max_input_tokens ?? form.max_input_tokens} tokens. Summary{" "}
                        {contextState?.summary_chars ?? 0} chars.
                      </div>
                    </div>
                  </Section>
                  <Section title="Memory">
                    <ToggleRow
                      checked={form.auto_memory_enabled}
                      title="Automatic long-term memory extraction"
                      description="Extract durable user preferences and project constraints into sandbox/.demiurge/memory.md."
                      onChange={(checked) => set("auto_memory_enabled", checked)}
                    />
                    <div className="mt-4 rounded-lg border border-[#e2e5ea] bg-[#fbfcfd] p-3">
                      <div className="flex items-start justify-between gap-3">
                        <div className="min-w-0">
                          <div className="text-[13px] font-medium text-[#202124]">Project memory audit</div>
                          <div className="mt-1 break-all text-[12px] text-[#7a8088]">
                            {memoryState?.path || ".demiurge/memory.md"}
                          </div>
                        </div>
                        <div className="flex shrink-0 gap-2">
                          <button
                            className={secondaryButtonCls}
                            type="button"
                            disabled={memoryBusy}
                            onClick={() => runMemoryAction(api.memoryPanelState)}
                          >
                            Refresh
                          </button>
                          <button
                            className={secondaryButtonCls}
                            type="button"
                            disabled={memoryBusy || !memoryState?.duplicates.length}
                            onClick={() => runMemoryAction(api.memoryDedupeApply)}
                          >
                            Dedupe {memoryState?.duplicates.length || 0}
                          </button>
                        </div>
                      </div>
                      {memoryError && (
                        <div className="mt-3 rounded-lg border border-[#ffd7d7] bg-[#fff7f7] p-3 text-[12px] text-[#b42318]">
                          {memoryError}
                        </div>
                      )}
                      <div className="mt-3 max-h-72 space-y-2 overflow-auto">
                        {memoryState?.entries.length ? (
                          memoryState.entries.map((entry) => {
                            const duplicate = memoryState.duplicates.some((group) =>
                              group.duplicate_ids.includes(entry.id),
                            );
                            return (
                              <div key={entry.id} className="rounded-lg border border-[#e2e5ea] bg-white p-3">
                                <div className="mb-2 flex flex-wrap items-center gap-2 text-[11px] text-[#7a8088]">
                                  <span className="rounded-md bg-[#eef1f5] px-1.5 py-0.5 text-[#4f5661]">
                                    {entry.kind}
                                  </span>
                                  <span>line {entry.line}</span>
                                  {duplicate && (
                                    <span className="rounded-md bg-[#fff6dd] px-1.5 py-0.5 text-[#8a5a00]">
                                      duplicate
                                    </span>
                                  )}
                                </div>
                                <textarea
                                  className="min-h-16 w-full resize-y rounded-md border border-[#d9d9d9] px-2 py-1.5 text-[12px] text-[#202124] outline-none transition focus:border-[#7a7f87] focus:ring-2 focus:ring-[#202124]/5"
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
                                    Delete
                                  </button>
                                  <button
                                    className="inline-flex h-8 items-center justify-center rounded-md bg-[#111827] px-3 text-[12px] font-medium text-white transition hover:bg-[#2b3442] disabled:cursor-not-allowed disabled:bg-[#b8bec8]"
                                    type="button"
                                    disabled={memoryBusy}
                                    onClick={() =>
                                      runMemoryAction(() => api.memoryUpdateEntry(entry.id, entry.kind, entry.text))
                                    }
                                  >
                                    Save
                                  </button>
                                </div>
                              </div>
                            );
                          })
                        ) : (
                          <div className="rounded-lg border border-dashed border-[#d8dde5] bg-white p-4 text-[12px] text-[#7a8088]">
                            No project memory entries.
                          </div>
                        )}
                      </div>
                    </div>
                  </Section>
                  <Section title="Custom Agents" description="Agent definitions loaded from the project sandbox.">
                    <div className="rounded-lg border border-[#e2e5ea] bg-[#fbfcfd] p-3">
                      <div className="flex items-center justify-between gap-3">
                        <div className="min-w-0">
                          <div className="text-[13px] font-medium text-[#202124]">
                            {agentPanel.definitions.length} definitions
                          </div>
                          <div className="mt-1 break-all text-[12px] text-[#7a8088]">
                            {agentPanel.agents_dir || ".demiurge/agents"}
                          </div>
                        </div>
                      </div>
                      <div className="mt-3 grid gap-2">
                        {agentPanel.definitions.length ? (
                          agentPanel.definitions.slice(0, 6).map((agent) => (
                            <div key={agent.name} className="rounded-md border border-[#e2e5ea] bg-white p-3">
                              <div className="truncate text-[13px] font-semibold text-[#202124]">{agent.name}</div>
                              <div className="mt-1 truncate text-[12px] text-[#7a8088]">
                                {agent.kind} / {agent.description || agent.path}
                              </div>
                              <div className="mt-1 truncate text-[12px] text-[#8a9099]">
                                tools: {agent.allowed_tools.length ? agent.allowed_tools.join(", ") : "default"}
                              </div>
                              {agent.invalid_tools.length ? (
                                <div className="mt-1 truncate text-[12px] text-[#b42318]">
                                  invalid: {agent.invalid_tools.join(", ")}
                                </div>
                              ) : null}
                            </div>
                          ))
                        ) : (
                          <div className="rounded-md border border-dashed border-[#d8dde5] bg-white p-4 text-[12px] text-[#7a8088]">
                            No custom agent definitions loaded.
                          </div>
                        )}
                      </div>
                    </div>
                  </Section>
                </>
              )}

              {activeTab === "tools" && (
                <>
                  <Section title="MCP Servers">
                    <div className="mb-3 flex items-center justify-between gap-3">
                      <div className="text-[12px] text-[#7a8088]">
                        stdio servers are started locally and exposed as `mcp__server__tool` tools.
                      </div>
                      <div className="flex shrink-0 gap-2">
                        <button className={secondaryButtonCls} type="button" disabled={mcpBusy} onClick={refreshMcp}>
                          Refresh
                        </button>
                        <button className={secondaryButtonCls} type="button" onClick={addMcpServer}>
                          Add server
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
                                  placeholder="server-name"
                                  onChange={(e) => updateMcpServer(index, { name: e.target.value })}
                                />
                                <span className="rounded-md bg-white px-2 py-1 text-[11px] text-[#5f6368]">
                                  {mcpStatusLabel(status)}
                                </span>
                                {runtime?.tool_count ? (
                                  <span className="rounded-md bg-white px-2 py-1 text-[11px] text-[#5f6368]">
                                    {runtime.tool_count} tools
                                  </span>
                                ) : null}
                                <label className="ml-auto flex items-center gap-2 text-[12px] text-[#5f6368]">
                                  <input
                                    type="checkbox"
                                    className="h-4 w-4 accent-[#111827]"
                                    checked={server.enabled}
                                    onChange={(e) => updateMcpServer(index, { enabled: e.target.checked })}
                                  />
                                  Enabled
                                </label>
                                {saved && (
                                  <button
                                    className={secondaryButtonCls}
                                    type="button"
                                    disabled={mcpBusy}
                                    onClick={() => setSavedMcpServerEnabled(server.name, !server.enabled)}
                                  >
                                    {server.enabled ? "Stop" : "Start"}
                                  </button>
                                )}
                                <button className={secondaryButtonCls} type="button" onClick={() => removeMcpServer(index)}>
                                  Remove
                                </button>
                              </div>
                              <div className="grid gap-3 sm:grid-cols-[1fr_1fr]">
                                <Field label="Command">
                                  <input
                                    className={inputCls}
                                    value={server.command}
                                    placeholder={navigator.userAgent.includes("Windows") ? "cmd" : "npx"}
                                    onChange={(e) => updateMcpServer(index, { command: e.target.value })}
                                  />
                                </Field>
                                <Field label="Transport">
                                  <input className={inputCls} value="stdio" disabled />
                                </Field>
                              </div>
                              <div className="mt-3 grid gap-3 sm:grid-cols-2">
                                <Field label="Arguments" help="One argument per line. On Windows, use cmd with /c and npx on following lines.">
                                  <textarea
                                    className="min-h-24 w-full resize-y rounded-md border border-[#d9d9d9] bg-white px-3 py-2 text-[12px] text-[#202124] outline-none transition focus:border-[#7a7f87] focus:ring-2 focus:ring-[#202124]/5"
                                    value={server.args.join("\n")}
                                    placeholder={"/c\nnpx\n-y\n@modelcontextprotocol/server-filesystem"}
                                    onChange={(e) => updateMcpServer(index, { args: splitLines(e.target.value) })}
                                  />
                                </Field>
                                <Field label="Environment" help="KEY=value, one per line. Secret storage is handled in the next pass.">
                                  <textarea
                                    className="min-h-24 w-full resize-y rounded-md border border-[#d9d9d9] bg-white px-3 py-2 text-[12px] text-[#202124] outline-none transition focus:border-[#7a7f87] focus:ring-2 focus:ring-[#202124]/5"
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
                          No MCP servers configured.
                        </div>
                      )}
                    </div>
                    <div className="mt-4 rounded-lg border border-[#e2e5ea] bg-[#fbfcfd] p-3">
                      <div className="mb-2 text-[13px] font-medium text-[#202124]">Discovered tools</div>
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
                          <div className="text-[12px] text-[#7a8088]">No MCP tools discovered.</div>
                        )}
                      </div>
                      {mcpState?.resources && Object.keys(mcpState.resources).length > 0 && (
                        <div className="mt-3 border-t border-[#e2e5ea] pt-3">
                          <div className="mb-2 text-[13px] font-medium text-[#202124]">Resources</div>
                          <div className="space-y-1 text-[12px] text-[#6f7782]">
                            {Object.entries(mcpState.resources).map(([serverName, resources]) => (
                              <div key={serverName}>
                                <span className="font-medium text-[#202124]">{serverName}</span>: {resources.length} resources
                              </div>
                            ))}
                          </div>
                        </div>
                      )}
                    </div>
                  </Section>
                  <Section title="Computer Use / OCR">
                    <ToggleRow
                      checked={form.computer_use_enabled}
                      title="Enable screen and OCR tools"
                      description="Screen capture and OCR still require explicit confirmation before reading pixels."
                      onChange={(checked) => set("computer_use_enabled", checked)}
                    />
                    <div className="mt-4 grid gap-3 sm:grid-cols-[1fr_auto]">
                      <Field label="OCR model source">
                        <select
                          className={inputCls}
                          value={form.ocr_model_source}
                          onChange={(e) => set("ocr_model_source", e.target.value as OcrModelSource)}
                        >
                          {ocrSources.map((s) => (
                            <option key={s.value} value={s.value}>
                              {s.label}
                            </option>
                          ))}
                        </select>
                      </Field>
                      <button
                        className="mt-[22px] inline-flex h-9 items-center justify-center rounded-md bg-[#111827] px-4 text-[12px] font-medium text-white transition hover:bg-[#2b3442] disabled:cursor-not-allowed disabled:bg-[#b8bec8]"
                        disabled={ocrBusy}
                        onClick={downloadOcrModels}
                        type="button"
                      >
                        {ocrBusy ? "Downloading" : ocrStatus?.installed ? "Redownload" : "Download models"}
                      </button>
                    </div>
                    <div className="mt-3 rounded-lg border border-[#e2e5ea] bg-[#fbfcfd] p-3 text-[12px] leading-5 text-[#6f7782]">
                      <div>Status: {ocrStatus?.installed ? "Installed" : "Not installed"} {ocrStatus ? `(${formatBytes(ocrStatus.totalBytes)})` : ""}</div>
                      {ocrStatus?.modelDir && <div className="break-all">Path: {ocrStatus.modelDir}</div>}
                      {ocrStatus && !ocrStatus.installed && <div>Missing: {ocrStatus.missing.join(", ")}</div>}
                      {ocrProgress && (
                        <div>
                          {ocrProgress.file}: {formatBytes(ocrProgress.downloadedBytes)}
                          {ocrProgress.totalBytes ? ` / ${formatBytes(ocrProgress.totalBytes)}` : ""}
                          {progressPct !== null ? ` (${progressPct}%)` : ""}
                        </div>
                      )}
                      {ocrError && <div className="text-[#b42318]">{ocrError}</div>}
                    </div>
                  </Section>
                  <Section title="Permission Rules" description="Rules remembered from confirmation dialogs.">
                    <div className="mb-3 flex justify-end">
                      <button
                        className={secondaryButtonCls}
                        type="button"
                        disabled={permissionBusy}
                        onClick={async () => setPermissionState(await api.permissionPanelState())}
                      >
                        Refresh
                      </button>
                    </div>
                    <div className="space-y-2">
                      {permissionState?.rules.length ? (
                        permissionState.rules.map((rule) => (
                          <div key={`${rule.scope}:${rule.tool}`} className="rounded-lg border border-[#e2e5ea] bg-white p-3">
                            <div className="flex items-start gap-3">
                              <div className="min-w-0 flex-1">
                                <div className="truncate text-[13px] font-semibold text-[#202124]">{rule.tool}</div>
                                <div className="mt-1 text-[12px] text-[#7a8088]">
                                  {permissionEffectLabel(rule.effect)} / {permissionScopeLabel(rule.scope)} /{" "}
                                  {formatTime(rule.updated_at)}
                                </div>
                                <div className="mt-1 text-[12px] leading-5 text-[#8a9099]">{rule.reason}</div>
                              </div>
                              <button
                                className={secondaryButtonCls}
                                type="button"
                                disabled={permissionBusy}
                                onClick={() => resetPermissionRule(rule.scope, rule.tool)}
                              >
                                Clear
                              </button>
                            </div>
                          </div>
                        ))
                      ) : (
                        <div className="rounded-lg border border-dashed border-[#d8dde5] bg-[#fbfcfd] p-4 text-[12px] text-[#7a8088]">
                          No remembered permission rules.
                        </div>
                      )}
                    </div>
                    <div className="mt-4 max-h-44 overflow-auto rounded-lg border border-[#e2e5ea] bg-[#fbfcfd] p-3 text-[12px] text-[#6f7782]">
                      <div className="mb-2 font-semibold text-[#202124]">Recent audit</div>
                      {permissionState?.audit.length ? (
                        permissionState.audit.slice(0, 8).map((entry) => (
                          <div key={`${entry.timestamp}:${entry.tool}:${entry.reason}`} className="border-t border-[#e8ebef] py-2 first:border-t-0">
                            <span className="font-medium text-[#202124]">{entry.tool}</span> /{" "}
                            {permissionEffectLabel(entry.effect)} / {permissionScopeLabel(entry.scope)} /{" "}
                            {formatTime(entry.timestamp)}
                            <div className="text-[#8a9099]">{entry.reason}</div>
                          </div>
                        ))
                      ) : (
                        <div>No audit entries.</div>
                      )}
                    </div>
                  </Section>
                </>
              )}

              {activeTab === "voice" && (
                <>
                  <Section title="Voice">
                    <ToggleRow
                      checked={form.voice_enabled}
                      title="Enable voice adapters"
                      description="The backend interface is reserved. Concrete STT/TTS providers can be wired later."
                      onChange={(checked) => set("voice_enabled", checked)}
                    />
                    <div className="mt-4 grid gap-4 sm:grid-cols-2">
                      <Field label="STT backend">
                        <input
                          className={inputCls}
                          value={form.voice_stt_backend}
                          placeholder="none / whisper / doubao"
                          onChange={(e) => set("voice_stt_backend", e.target.value)}
                        />
                      </Field>
                      <Field label="TTS backend">
                        <input
                          className={inputCls}
                          value={form.voice_tts_backend}
                          placeholder="none / GPT-SoVITS / CosyVoice"
                          onChange={(e) => set("voice_tts_backend", e.target.value)}
                        />
                      </Field>
                    </div>
                    <div className="mt-4">
                      <Field label="Voice ID">
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
                  <Section title="Compatibility Limits">
                    <Field label="Max context characters" help="Legacy character limit retained for older trimming paths.">
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
    </div>
  );
}
