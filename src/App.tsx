import { Suspense, lazy, useEffect, useMemo, useRef, useState } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow, PhysicalSize } from "@tauri-apps/api/window";
import * as api from "./lib/api";
import type {
  AgentPanelState,
  AssistantErrorEvent,
  ConfirmRequestEvent,
  DisplayItem,
  GoalPanelState,
  GoalProgressEvent,
  Message,
  PackManifest,
  PermissionMode,
  PermissionScope,
  PlanState,
  ReasoningEffort,
  SessionEnginePanelState,
  SessionMeta,
  Settings,
  AppTheme,
  WorkspaceState,
} from "./lib/types";
import { MessageList } from "./components/MessageList";
import { Sidebar, type AppView } from "./components/Sidebar";
import { Composer } from "./components/Composer";
import GoalBar, { type GoalAction } from "./components/GoalBar";
import ConfirmDialog from "./components/ConfirmDialog";
import SettingsDialog, { type SettingsTab } from "./components/SettingsDialog";
import MediaStudio from "./components/MediaStudio";
import SkillsPanel from "./components/SkillsPanel";
import FortuneDialog from "./components/FortuneDialog";
import CompanionCard from "./components/CompanionCard";
import PomodoroCard from "./components/PomodoroCard";
import {
  CheckIcon,
  ChevronDownIcon,
  CloseIcon,
  MaximizeIcon,
  MinimizeIcon,
  PanelLeftIcon,
  SettingsIcon,
  SparklesIcon,
} from "./components/Icons";
import { attachmentKindLabel, buildAttachmentPrompt, formatAttachmentSize, type ProcessedAttachment } from "./lib/fileProcessing";
import { autoContextBudget } from "./lib/providers";
import { canDrawToday, isAutoPromptEnabled, isDismissedToday } from "./lib/fortune";
import { useI18n } from "./lib/i18n";
import { useClickOutside } from "./lib/hooks";

const Live2DPanel = lazy(() => import("./components/Live2DPanel"));

const DEFAULT_WINDOW_SIZE = { width: 1811, height: 1213 };

// Browser-preview fallback so the UI renders without the Tauri backend (dev/screenshots only).
const PREVIEW_SETTINGS: Settings = {
  provider: "deepseek",
  permission_mode: "default",
  base_url: "https://api.deepseek.com/v1",
  api_key: "",
  model: "deepseek-chat",
  reasoning_effort: "auto",
  current_pack: "default",
  max_context_chars: 24000,
  max_input_tokens: 32000,
  reserved_output_tokens: 4000,
  context_budget_auto: true,
  language: "zh",
  theme: "system",
  launch_on_startup: false,
  auto_memory_enabled: true,
  embedding_enabled: false,
  embedding_provider: "none",
  embedding_base_url: "",
  embedding_api_key: "",
  embedding_model: "",
  embedding_dims: 1024,
  hybrid_weight: 0.5,
  companion_enabled: true,
  companion_memory_extraction_enabled: false,
  companion_memory_extraction_scope: "recent_turn",
  companion_tone: "gentle",
  companion_mood: "neutral",
  companion_energy: "normal",
  companion_focus: "available",
  companion_do_not_disturb: "",
  weather_enabled: false,
  weather_location_mode: "manual",
  weather_city: "",
  weather_provider: "open_meteo",
  voice_enabled: false,
  voice_stt_backend: "",
  voice_tts_backend: "",
  voice_id: "",
  computer_use_enabled: false,
  ocr_model_source: "modelscope",
  web_search_provider: "auto",
  tavily_api_key: "",
  brave_search_api_key: "",
  exa_api_key: "",
  webdav_enabled: false,
  webdav_url: "",
  webdav_username: "",
  webdav_password: "",
  webdav_path: "",
  media_provider: "dashscope",
  media_base_url: "",
  media_api_key: "",
  image_model: "",
  image_size: "",
  tts_model: "",
  tts_voice: "",
  mcp_servers: [],
};

function friendlyAssistantError(err: unknown, event?: AssistantErrorEvent) {
  const raw = event?.message || String(err);
  const lower = raw.toLowerCase();
  let title = "Request failed";
  let hint = event?.hint || "Check the provider settings and try again.";

  if (event?.kind === "llm" || lower.includes("llm") || lower.includes("model")) {
    title = "Model request failed";
    hint = event?.hint || "Verify the model name, base URL, API key, and provider capability settings.";
  }
  if (lower.includes("401") || lower.includes("403") || lower.includes("unauthorized") || lower.includes("api key")) {
    title = "Provider authentication failed";
    hint = event?.hint || "Re-save the provider API key in Settings, then retry the same request.";
  } else if (lower.includes("timeout") || lower.includes("timed out")) {
    title = "Request timed out";
    hint = event?.hint || "The provider or network was slow. Retry once; if it repeats, lower context size or switch endpoint.";
  } else if (
    lower.includes("network") ||
    lower.includes("connection") ||
    lower.includes("dns") ||
    lower.includes("econn") ||
    lower.includes("fetch")
  ) {
    title = "Network request failed";
    hint = event?.hint || "Check the endpoint and local network path. If you use a proxy, confirm the app can reach it.";
  }

  return { title, message: raw.replace(/^Error:\s*/i, ""), hint, retryable: event?.retryable ?? true };
}

function buildHistory(msgs: Message[]): DisplayItem[] {
  const out: DisplayItem[] = [];
  const results = new Map<string, string>();
  for (const m of msgs) {
    if (m.role === "tool" && m.tool_call_id) results.set(m.tool_call_id, m.content ?? "");
  }
  let seq = 0;
  const id = () => `h_${++seq}`;
  for (const m of msgs) {
    if (m.role === "user") {
      const text = m.content ?? "";
      if (!text.startsWith("[Goal ")) {
        out.push({ id: id(), kind: "user", text });
      }
    } else if (m.role === "assistant") {
      if (m.content) out.push({ id: id(), kind: "assistant", text: m.content, streaming: false });
      for (const tc of m.tool_calls ?? []) {
        let args: unknown = {};
        try {
          args = JSON.parse(tc.function.arguments || "{}");
        } catch {
          args = tc.function.arguments;
        }
        out.push({
          id: id(),
          kind: "tool",
          tool_call_id: tc.id,
          name: tc.function.name,
          args,
          status: "done",
          result: results.get(tc.id),
        });
      }
    }
  }
  return out;
}

function buildUserDisplayText(text: string, attachments: ProcessedAttachment[]) {
  if (attachments.length === 0) return text;
  const lines = text ? [text] : ["Attached files"];
  lines.push("");
  lines.push("Attachments:");
  for (const attachment of attachments) {
    const status = attachment.status === "error" ? `failed: ${attachment.error ?? "unable to read"}` : "ready";
    lines.push(
      `- ${attachment.name} (${attachmentKindLabel(attachment.kind)}, ${formatAttachmentSize(attachment.size)}, ${status})`,
    );
  }
  return lines.join("\n");
}

export default function App() {
  const { t, setLang } = useI18n();
  const [items, setItems] = useState<DisplayItem[]>([]);
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [packs, setPacks] = useState<PackManifest[]>([]);
  const [agentPanel, setAgentPanel] = useState<AgentPanelState>({ definitions: [], agents_dir: "" });
  const [workspace, setWorkspace] = useState<WorkspaceState | null>(null);
  const [goalPanel, setGoalPanel] = useState<GoalPanelState | null>(null);
  const [goalProgress, setGoalProgress] = useState<GoalProgressEvent | null>(null);
  const [sessionEngine, setSessionEngine] = useState<SessionEnginePanelState | null>(null);
  const [selectedAgentNames, setSelectedAgentNames] = useState<string[]>([]);
  const [sessions, setSessions] = useState<SessionMeta[]>([]);
  const [activeId, setActiveId] = useState("");
  const [activeView, setActiveView] = useState<AppView>("chat");
  const [settingsInitialTab, setSettingsInitialTab] = useState<SettingsTab>("general");
  const [previewTheme, setPreviewTheme] = useState<AppTheme | null>(null);
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [packMenuOpen, setPackMenuOpen] = useState(false);
  const [agentMenuOpen, setAgentMenuOpen] = useState(false);
  const [toyMenuOpen, setToyMenuOpen] = useState(false);
  const [titleMenuOpen, setTitleMenuOpen] = useState<"file" | "edit" | "view" | "persona" | "help" | null>(null);
  const [confirmReq, setConfirmReq] = useState<ConfirmRequestEvent | null>(null);
  const [planState, setPlanState] = useState<PlanState>({ active: false, approved: false });
  const [fortuneOpen, setFortuneOpen] = useState(false);

  const seq = useRef(0);
  const genId = () => `it_${++seq.current}`;
  const curAssistantId = useRef<string | null>(null);
  const toolItemIds = useRef<Map<string, string>>(new Map());
  const lastRetryText = useRef<string>("");
  const assistantErrorDelivered = useRef(false);
  // 流式增量缓冲：把每个 token 的 setState 合并到「每帧一次」（requestAnimationFrame），
  // 避免逐 token 触发 setItems + markdown 全量重解析造成的卡顿（长回复尤甚）。
  const pendingStream = useRef<{ content: string; reasoning: string; raf: number }>({
    content: "",
    reasoning: "",
    raf: 0,
  });
  const packMenuRef = useRef<HTMLDivElement | null>(null);
  const agentMenuRef = useRef<HTMLDivElement | null>(null);
  const toyMenuRef = useRef<HTMLDivElement | null>(null);
  const titleMenuRef = useRef<HTMLDivElement | null>(null);
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);

  const activeSession = useMemo(() => sessions.find((s) => s.id === activeId) ?? null, [activeId, sessions]);
  const agentsDir = agentPanel.agents_dir || ".demiurge/agents";
  const appBusy = busy || sessionEngine?.busy === true;
  const runtimeStatus = sessionEngine?.cancel_requested
    ? t("status.cancelling")
    : sessionEngine?.active_turn
      ? sessionEngine.active_turn.status === "cancelling"
        ? t("status.cancelling")
        : t("status.processing")
      : t("status.ready");

  const currentPack = useMemo(
    () => packs.find((x) => x.id === settings?.current_pack) ?? null,
    [packs, settings?.current_pack],
  );
  const packName = currentPack?.name ?? settings?.current_pack ?? "Demiurge";
  const packAvatar = currentPack?.avatarDataUrl;

  const selectedAgentLabel = useMemo(() => {
    if (selectedAgentNames.length === 0) return t("header.agents");
    if (selectedAgentNames.length === 1) return selectedAgentNames[0];
    return t("header.agentsCount", { n: selectedAgentNames.length });
  }, [selectedAgentNames, t]);
  const showAgentMenu = agentPanel.definitions.length > 0 || selectedAgentNames.length > 0;

  useEffect(() => {
    const theme = previewTheme ?? settings?.theme ?? "system";
    const media =
      typeof window !== "undefined" && typeof window.matchMedia === "function"
        ? window.matchMedia("(prefers-color-scheme: dark)")
        : null;

    function applyTheme() {
      const resolved = theme === "system" ? (media?.matches ? "dark" : "light") : theme;
      document.documentElement.dataset.theme = theme;
      document.documentElement.dataset.resolvedTheme = resolved;
      document.documentElement.style.colorScheme = resolved;
    }

    applyTheme();
    if (theme !== "system" || !media) return;

    media.addEventListener("change", applyTheme);
    return () => media.removeEventListener("change", applyTheme);
  }, [previewTheme, settings?.theme]);

  useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window)) return;
    const appWindow = getCurrentWindow();
    void appWindow
      .setSize(new PhysicalSize(DEFAULT_WINDOW_SIZE.width, DEFAULT_WINDOW_SIZE.height))
      .then(() => appWindow.center())
      .catch((e) => console.warn("Failed to apply default window size", e));
  }, []);

  useEffect(() => {
    const preventContextMenu = (event: MouseEvent) => event.preventDefault();
    document.addEventListener("contextmenu", preventContextMenu);
    return () => document.removeEventListener("contextmenu", preventContextMenu);
  }, []);

  // 每日吉签：应用启动时若启用自动弹窗、今日尚未抽签且未主动忽略，弹出引导抽签。
  useEffect(() => {
    if (!isAutoPromptEnabled()) return;
    if (canDrawToday() && !isDismissedToday()) setFortuneOpen(true);
  }, []);

  useEffect(() => {
    (async () => {
      try {
        const [s, ps, agents, goal, list, hist, plan, engine, workspaceState] = await Promise.all([
          api.getSettings(),
          api.listPacks(),
          api.agentPanelState(),
          api.goalPanelState(),
          api.listSessions(),
          api.getHistory(),
          api.planState(),
          api.sessionEngineState(),
          api.workspaceState(),
        ]);
        setSettings(s);
        if (s.language === "zh" || s.language === "en") setLang(s.language);
        setPacks(ps);
        setAgentPanel(agents);
        setGoalPanel(goal);
        setSessions(list.sessions);
        setActiveId(list.active);
        setItems(buildHistory(hist));
        setPlanState(plan);
        setSessionEngine(engine);
        setWorkspace(workspaceState);
        setBusy(engine.busy);
      } catch (e) {
        console.error("Failed to initialize Demiurge", e);
        // Outside Tauri (browser preview), seed defaults so the UI is still browsable.
        if (!("__TAURI_INTERNALS__" in window)) {
          setSettings((prev) => prev ?? PREVIEW_SETTINGS);
          setWorkspace({ path: "D:\\Project\\Project-1\\Demiurge", name: "Demiurge", is_git: true, branch: "main", dirty: false });
        }
      }
    })();
  }, []);

  async function refreshSessions() {
    try {
      const list = await api.listSessions();
      setSessions(list.sessions);
      setActiveId(list.active);
    } catch (e) {
      console.error(e);
    }
  }

  async function refreshGoalPanel() {
    try {
      setGoalPanel(await api.goalPanelState());
    } catch (e) {
      console.error(e);
    }
  }

  useEffect(() => {
    let un: UnlistenFn | undefined;
    let disposed = false;

    // 把累积的增量一次性写入当前 assistant 项（必要时创建）；推理与正文分开累积。
    const flushPending = () => {
      if (pendingStream.current.raf) {
        cancelAnimationFrame(pendingStream.current.raf);
        pendingStream.current.raf = 0;
      }
      const { content, reasoning } = pendingStream.current;
      if (!content && !reasoning) return;
      pendingStream.current.content = "";
      pendingStream.current.reasoning = "";
      setItems((p) => {
        let id = curAssistantId.current;
        let arr = p;
        if (!id) {
          id = genId();
          curAssistantId.current = id;
          arr = [...p, { id, kind: "assistant", text: "", reasoning: "", streaming: true }];
        }
        return arr.map((it) =>
          it.id === id && it.kind === "assistant"
            ? { ...it, text: it.text + content, reasoning: (it.reasoning ?? "") + reasoning }
            : it,
        );
      });
    };

    const scheduleFlush = () => {
      if (pendingStream.current.raf) return;
      pendingStream.current.raf = requestAnimationFrame(() => {
        pendingStream.current.raf = 0;
        flushPending();
      });
    };

    const finalizeAssistant = () => {
      flushPending();
      const id = curAssistantId.current;
      if (id) {
        setItems((p) => p.map((it) => (it.id === id && it.kind === "assistant" ? { ...it, streaming: false } : it)));
        curAssistantId.current = null;
      }
    };

    api
      .listenAgentEvents({
        onAssistantStart: () => finalizeAssistant(),
        onAssistantDelta: (text) => {
          pendingStream.current.content += text;
          scheduleFlush();
        },
        onAssistantReasoning: (text) => {
          pendingStream.current.reasoning += text;
          scheduleFlush();
        },
        onAssistantDone: (text) => {
          flushPending();
          const id = curAssistantId.current;
          if (id) {
            setItems((p) =>
              p.map((it) =>
                it.id === id && it.kind === "assistant" ? { ...it, streaming: false, text: it.text || text } : it,
              ),
            );
          } else if (text) {
            const nid = genId();
            setItems((p) => [...p, { id: nid, kind: "assistant", text, streaming: false }]);
          }
          curAssistantId.current = null;
          setBusy(false);
          void refreshGoalPanel();
        },
        onAssistantError: (e) => {
          finalizeAssistant();
          assistantErrorDelivered.current = true;
          const friendly = friendlyAssistantError(e.message, e);
          setItems((p) => [
            ...p,
            {
              id: genId(),
              kind: "assistant",
              text: friendly.message,
              streaming: false,
              error: true,
              errorTitle: friendly.title,
              errorHint: friendly.hint,
              retryText: friendly.retryable ? lastRetryText.current : undefined,
            },
          ]);
          setBusy(false);
          void refreshGoalPanel();
        },
        onAssistantInterrupted: () => {
          finalizeAssistant();
          setBusy(false);
          void refreshGoalPanel();
        },
        onToolStart: (e) => {
          finalizeAssistant();
          const nid = genId();
          toolItemIds.current.set(e.tool_call_id, nid);
          setItems((p) => [
            ...p,
            {
              id: nid,
              kind: "tool",
              tool_call_id: e.tool_call_id,
              name: e.name,
              args: e.args,
              status: "running",
              preview: e.preview,
              description: e.description,
              risk: e.risk,
              permission_effect: e.permission_effect,
            },
          ]);
        },
        onToolEnd: (e) => {
          const id = toolItemIds.current.get(e.tool_call_id);
          if (id) toolItemIds.current.delete(e.tool_call_id);
          setItems((p) =>
            p.map((it) =>
              it.kind === "tool" && (it.id === id || it.tool_call_id === e.tool_call_id)
                ? {
                    ...it,
                    status: e.denied ? "denied" : e.ok ? "done" : "failed",
                    result: e.result,
                    duration_ms: e.duration_ms,
                    error_hint: e.error_hint,
                    source_quality: e.source_quality,
                  }
                : it,
            ),
          );
        },
        onConfirmRequest: (e) => setConfirmReq(e),
        onGoalProgress: (e) => {
          setGoalProgress(e);
          void refreshGoalPanel();
          setItems((p) => [
            ...p,
            {
              id: genId(),
              kind: "tool",
              name: "goal",
              args: { turns_executed: e.turns_executed, tokens_used: e.tokens_used, token_budget: e.token_budget },
              status: e.status === "active" ? "running" : "done",
              result: e.message,
              description: "Goal progress",
            },
          ]);
        },
      })
      .then((u) => {
        if (disposed) u();
        else un = u;
      })
      .catch((e) => console.warn("subscribe failed", e));

    let unPlan: UnlistenFn | undefined;
    let unMode: UnlistenFn | undefined;
    let unSettings: UnlistenFn | undefined;
    let unSessionEngine: UnlistenFn | undefined;
    api.listenPlanUpdated(setPlanState).then((u) => {
      if (disposed) u();
      else unPlan = u;
    }).catch((e) => console.warn("subscribe failed", e));
    api.listenPermissionModeUpdated((mode) => {
      setSettings((prev) => (prev ? { ...prev, permission_mode: mode } : prev));
    }).then((u) => {
      if (disposed) u();
      else unMode = u;
    }).catch((e) => console.warn("subscribe failed", e));
    api.listenSettingsUpdated((s) => {
      setSettings(s);
      if (s.language === "zh" || s.language === "en") setLang(s.language);
    }).then((u) => {
      if (disposed) u();
      else unSettings = u;
    }).catch((e) => console.warn("subscribe failed", e));
    api.listenSessionEngineUpdated((next) => {
      setSessionEngine(next);
      setBusy(next.busy);
    }).then((u) => {
      if (disposed) u();
      else unSessionEngine = u;
    }).catch((e) => console.warn("subscribe failed", e));

    return () => {
      disposed = true;
      if (pendingStream.current.raf) {
        cancelAnimationFrame(pendingStream.current.raf);
        pendingStream.current.raf = 0;
      }
      un?.();
      unPlan?.();
      unMode?.();
      unSettings?.();
      unSessionEngine?.();
    };
  }, []);

  // 顶部菜单 / pack 菜单 / agent 菜单 / 玩具菜单的「外点 + Escape 关闭」逻辑统一收敛到 useClickOutside。
  useClickOutside(packMenuRef, () => setPackMenuOpen(false), { enabled: packMenuOpen });
  useClickOutside(titleMenuRef, () => setTitleMenuOpen(null), { escape: true, enabled: !!titleMenuOpen });
  useClickOutside(agentMenuRef, () => setAgentMenuOpen(false), { enabled: agentMenuOpen });
  useClickOutside(toyMenuRef, () => setToyMenuOpen(false), { escape: true, enabled: toyMenuOpen });

  async function handleSend(textArg?: string, attachments: ProcessedAttachment[] = []) {
    const text = (textArg ?? input).trim();
    const attachmentPrompt = buildAttachmentPrompt(attachments);
    if ((!text && !attachmentPrompt) || appBusy) return false;
    setInput("");
    setActiveView("chat");
    assistantErrorDelivered.current = false;
    const uid = genId();
    setItems((p) => [...p, { id: uid, kind: "user", text: buildUserDisplayText(text, attachments) }]);
    setBusy(true);
    try {
      const prompt = `${text || "Please review the attached files."}${attachmentPrompt}`;
      lastRetryText.current = prompt;
      if (selectedAgentNames.length) {
        await api.sendWithAgents(prompt, selectedAgentNames);
      } else {
        await api.send(prompt);
      }
    } catch (err) {
      const id = curAssistantId.current;
      if (id) {
        setItems((p) => p.map((it) => (it.id === id && it.kind === "assistant" ? { ...it, streaming: false } : it)));
        curAssistantId.current = null;
      }
      if (!assistantErrorDelivered.current) {
        const friendly = friendlyAssistantError(err);
        const nid = genId();
        setItems((p) => [
          ...p,
          {
            id: nid,
            kind: "assistant",
            text: friendly.message,
            streaming: false,
            error: true,
            errorTitle: friendly.title,
            errorHint: friendly.hint,
            retryText: friendly.retryable ? lastRetryText.current : undefined,
          },
        ]);
      }
    } finally {
      setBusy(false);
      void refreshSessions();
      void refreshGoalPanel();
    }
    return true;
  }

  async function handleRespondConfirm(allow: boolean, scope: PermissionScope) {
    if (!confirmReq) return;
    const id = confirmReq.id;
    setConfirmReq(null);
    try {
      await api.respondConfirm(id, allow, scope);
    } catch (e) {
      console.error("Failed to respond to confirmation", e);
    }
  }

  async function handleGoalAction(action: GoalAction) {
    if ((action === "resume" || action === "continue") && appBusy) return;
    setGoalProgress(null);
    if (action === "resume" || action === "continue") {
      setActiveView("chat");
      setBusy(true);
    }
    try {
      const next =
        action === "pause"
          ? await api.goalPause()
          : action === "resume"
            ? await api.goalResume()
            : action === "continue"
              ? await api.goalContinue()
              : await api.goalClear();
      setGoalPanel(next);
      await refreshSessions();
    } catch (err) {
      const nid = genId();
      setItems((p) => [
        ...p,
        { id: nid, kind: "assistant", text: `Warning: ${String(err)}`, streaming: false, error: true },
      ]);
    } finally {
      if (action === "resume" || action === "continue") setBusy(false);
      void refreshGoalPanel();
    }
  }

  function resetTurnRefs() {
    curAssistantId.current = null;
    toolItemIds.current.clear();
  }

  async function handleNewChat() {
    if (appBusy) return;
    try {
      await api.newSession();
    } catch (e) {
      console.error(e);
    }
    resetTurnRefs();
    setItems([]);
    setGoalPanel(null);
    setGoalProgress(null);
    await refreshSessions();
    await refreshGoalPanel();
    requestAnimationFrame(() => textareaRef.current?.focus());
  }

  async function loadActiveHistory() {
    try {
      const hist = await api.getHistory();
      resetTurnRefs();
      setItems(buildHistory(hist));
      await refreshGoalPanel();
    } catch (e) {
      console.error(e);
    }
  }

  async function handleSelectSession(id: string) {
    if (appBusy || id === activeId) return;
    try {
      await api.selectSession(id);
      setActiveId(id);
      setGoalProgress(null);
      await loadActiveHistory();
    } catch (e) {
      console.error(e);
    }
  }

  async function handleRenameSession(id: string, title: string) {
    if (appBusy) return;
    const renamed = await api.renameSession(id, title);
    setSessions((prev) =>
      prev
        .map((s) => (s.id === id ? { ...s, title: renamed, updated_at: Date.now() } : s))
        .sort((a, b) => b.updated_at - a.updated_at),
    );
    await refreshSessions();
  }

  async function handleDeleteSession(id: string) {
    if (appBusy) return;
    try {
      await api.deleteSession(id);
      await refreshSessions();
      await loadActiveHistory();
      setGoalProgress(null);
    } catch (e) {
      console.error(e);
    }
  }

  async function handleSaveSettings(s: Settings) {
    try {
      await api.saveSettings(s);
      setSettings(s);
      setPreviewTheme(null);
    } catch (e) {
      console.error("Failed to save settings", e);
    }
  }

  function handleCloseSettings() {
    setPreviewTheme(null);
    setActiveView("chat");
  }

  async function handleSelectPack(id: string) {
    setPackMenuOpen(false);
    if (!settings || settings.current_pack === id) return;
    const next = { ...settings, current_pack: id };
    setSettings(next);
    try {
      await api.saveSettings(next);
    } catch (e) {
      console.error(e);
    }
  }


  async function handleSetModel(model: string) {
    if (!settings) return;
    let next: Settings = { ...settings, model };
    // When the input budget follows the model, re-size it for the new model.
    if (settings.context_budget_auto) {
      const budget = autoContextBudget(settings.provider, model);
      if (budget) {
        next = { ...next, max_input_tokens: budget.maxInput, reserved_output_tokens: budget.reservedOutput };
      }
    }
    setSettings(next);
    try {
      await api.saveSettings(next);
    } catch (e) {
      console.error("Failed to save model", e);
    }
  }

  async function handleSetEffort(reasoning_effort: ReasoningEffort) {
    if (!settings) return;
    const next = { ...settings, reasoning_effort };
    setSettings(next);
    try {
      await api.saveSettings(next);
    } catch (e) {
      console.error("Failed to save reasoning effort", e);
    }
  }

  async function handleSetPermissionMode(mode: PermissionMode) {
    try {
      const next = await api.setPermissionMode(mode);
      setSettings(next);
      setPlanState(await api.planState());
    } catch (e) {
      console.error("Failed to set permission mode", e);
    }
  }

  async function handleApprovePlan() {
    try {
      setPlanState(await api.approvePlan());
      const next = await api.getSettings();
      setSettings(next);
    } catch (e) {
      console.error("Failed to approve plan", e);
    }
  }

  async function handleRejectPlan() {
    try {
      setPlanState(await api.rejectPlan());
    } catch (e) {
      console.error("Failed to reject plan", e);
    }
  }

  function toggleAgent(name: string) {
    setSelectedAgentNames((prev) =>
      prev.includes(name) ? prev.filter((item) => item !== name) : [...prev, name],
    );
  }

  function openSettings(tab: SettingsTab = "general") {
    setSettingsInitialTab(tab);
    setActiveView("settings");
  }

  const last = items[items.length - 1];
  const tailStreaming = last?.kind === "assistant" && last.streaming;
  const tailToolRunning = last?.kind === "tool" && last.status === "running";
  const thinking = appBusy && !tailStreaming && !tailToolRunning;
  const canSend = input.trim().length > 0 && !appBusy;
  const titleMenuButtonClass = (menu: typeof titleMenuOpen) =>
    `app-title-menu-button ${titleMenuOpen === menu ? "is-active" : ""}`;

  async function handleWindowMinimize() {
    if (!("__TAURI_INTERNALS__" in window)) return;
    await getCurrentWindow().minimize();
  }

  async function handleWindowToggleMaximize() {
    if (!("__TAURI_INTERNALS__" in window)) return;
    await getCurrentWindow().toggleMaximize();
  }

  async function handleWindowClose() {
    if (!("__TAURI_INTERNALS__" in window)) return;
    await getCurrentWindow().close();
  }

  return (
    <main className="flex h-[100dvh] flex-col overflow-hidden bg-[#eef1f5] text-[#202124]">
      <div ref={titleMenuRef} className="app-titlebar">
        <div
          className="app-titlebar-brand app-titlebar-drag"
          data-tauri-drag-region
          onDoubleClick={() => void handleWindowToggleMaximize()}
        >
          <img src="/demiurge.png" alt="" className="size-4 rounded-[4px]" />
          <span>Demiurge</span>
        </div>

        <nav className="app-titlebar-menus" aria-label="Application menu">
          <div className="relative">
            <button
              type="button"
              onClick={() => setTitleMenuOpen((v) => (v === "file" ? null : "file"))}
              className={titleMenuButtonClass("file")}
            >
              {t("header.menu.file")}
            </button>
            {titleMenuOpen === "file" && (
              <div className="cf-pop cf-pop-down cf-dropdown app-titlebar-menu w-52 overflow-hidden p-1">
                <button
                  type="button"
                  onClick={() => {
                    setTitleMenuOpen(null);
                    setActiveView("chat");
                    void handleNewChat();
                  }}
                  className="cf-menu-item flex w-full items-center gap-2"
                >
                  {t("sidebar.newChat")}
                </button>
                <button
                  type="button"
                  onClick={() => {
                    setTitleMenuOpen(null);
                    void api.openSandbox();
                  }}
                  className="cf-menu-item flex w-full items-center gap-2"
                >
                  {t("header.openLocation")}
                </button>
                <button
                  type="button"
                  onClick={() => {
                    setTitleMenuOpen(null);
                    openSettings("general");
                  }}
                  className="cf-menu-item flex w-full items-center gap-2"
                >
                  {t("sidebar.settings")}
                </button>
              </div>
            )}
          </div>

          <div className="relative">
            <button
              type="button"
              onClick={() => setTitleMenuOpen((v) => (v === "edit" ? null : "edit"))}
              className={titleMenuButtonClass("edit")}
            >
              {t("header.menu.edit")}
            </button>
            {titleMenuOpen === "edit" && (
              <div className="cf-pop cf-pop-down cf-dropdown app-titlebar-menu w-56 overflow-hidden p-1">
                <button
                  type="button"
                  onClick={() => {
                    setTitleMenuOpen(null);
                    void api.interrupt();
                    setConfirmReq(null);
                  }}
                  className="cf-menu-item flex w-full items-center justify-between gap-2 disabled:cursor-default disabled:opacity-45"
                  disabled={!appBusy}
                >
                  <span>{t("header.menu.stop")}</span>
                  {appBusy && <span className="text-[11px] text-[#8a9099]">{runtimeStatus}</span>}
                </button>
                <button
                  type="button"
                  onClick={() => {
                    setTitleMenuOpen(null);
                    setSelectedAgentNames([]);
                  }}
                  className="cf-menu-item flex w-full items-center gap-2 disabled:cursor-default disabled:opacity-45"
                  disabled={selectedAgentNames.length === 0}
                >
                  {t("header.clearSelection")}
                </button>
                <button
                  type="button"
                  onClick={() => {
                    setTitleMenuOpen(null);
                    openSettings("context");
                  }}
                  className="cf-menu-item flex w-full items-center gap-2"
                >
                  {t("settings.nav.context")}
                </button>
              </div>
            )}
          </div>

          <div className="relative">
            <button
              type="button"
              onClick={() => setTitleMenuOpen((v) => (v === "view" ? null : "view"))}
              className={titleMenuButtonClass("view")}
            >
              {t("header.menu.view")}
            </button>
            {titleMenuOpen === "view" && (
              <div className="cf-pop cf-pop-down cf-dropdown app-titlebar-menu w-44 overflow-hidden p-1">
                {[
                  ["chat", t("nav.chat")],
                  ["media", t("nav.images")],
                  ["skills", t("nav.skills")],
                  ["live2d", t("nav.live2d")],
                ].map(([view, label]) => (
                  <button
                    key={view}
                    type="button"
                    onClick={() => {
                      setTitleMenuOpen(null);
                      setActiveView(view as AppView);
                    }}
                    className={`cf-menu-item flex w-full items-center justify-between gap-2 ${
                      activeView === view ? "is-active" : ""
                    }`}
                  >
                    <span>{label}</span>
                    {activeView === view && <CheckIcon size={14} />}
                  </button>
                ))}
              </div>
            )}
          </div>

          <div className="relative">
            <button
              type="button"
              onClick={() => setTitleMenuOpen((v) => (v === "persona" ? null : "persona"))}
              className={titleMenuButtonClass("persona")}
            >
              {t("header.menu.persona")}
            </button>
            {titleMenuOpen === "persona" && (
              <div className="cf-pop cf-pop-down cf-dropdown app-titlebar-menu max-h-[70vh] w-64 overflow-y-auto p-1.5">
                {packs.length === 0 && <div className="px-3 py-2 text-sm text-[#9a9a9a]">No persona packs found</div>}
                {packs.map((p) => (
                  <button
                    key={p.id}
                    onClick={() => {
                      setTitleMenuOpen(null);
                      void handleSelectPack(p.id);
                    }}
                    className={`cf-menu-item flex w-full items-center justify-between gap-2 ${
                      settings?.current_pack === p.id ? "is-active" : ""
                    }`}
                  >
                    <span className="flex min-w-0 items-center gap-2">
                      <img
                        src={p.avatarDataUrl || "/demiurge.png"}
                        alt=""
                        className="size-7 shrink-0 rounded-md border border-[#dfe3e8] bg-white object-cover"
                      />
                      <span className="min-w-0 truncate">{p.name}</span>
                    </span>
                    {settings?.current_pack === p.id && <CheckIcon size={15} className="shrink-0 text-[#171717]" />}
                  </button>
                ))}
                <button
                  type="button"
                  onClick={() => {
                    setTitleMenuOpen(null);
                    openSettings("persona");
                  }}
                  className="mt-1 w-full rounded-md px-2.5 py-2 text-left text-xs text-[#8a8a8a] transition hover:bg-[#f6f7f9]"
                >
                  {t("settings.nav.persona")}
                </button>
              </div>
            )}
          </div>

          <div className="relative">
            <button
              type="button"
              onClick={() => setTitleMenuOpen((v) => (v === "help" ? null : "help"))}
              className={titleMenuButtonClass("help")}
            >
              {t("header.menu.help")}
            </button>
            {titleMenuOpen === "help" && (
              <div className="cf-pop cf-pop-down cf-dropdown app-titlebar-menu w-48 overflow-hidden p-1">
                <button
                  type="button"
                  onClick={() => {
                    setTitleMenuOpen(null);
                    openSettings("advanced");
                  }}
                  className="cf-menu-item flex w-full items-center gap-2"
                >
                  {t("settings.nav.advanced")}
                </button>
                <button
                  type="button"
                  onClick={() => {
                    setTitleMenuOpen(null);
                    openSettings("context");
                  }}
                  className="cf-menu-item flex w-full items-center gap-2"
                >
                  {t("settings.nav.context")}
                </button>
              </div>
            )}
          </div>
        </nav>

        <div
          className="app-titlebar-spacer app-titlebar-drag"
          data-tauri-drag-region
          onDoubleClick={() => void handleWindowToggleMaximize()}
        />
        <div className="app-window-controls">
          <button type="button" onClick={() => void handleWindowMinimize()} title={t("window.minimize")}>
            <MinimizeIcon size={14} />
          </button>
          <button type="button" onClick={() => void handleWindowToggleMaximize()} title={t("window.maximize")}>
            <MaximizeIcon size={13} />
          </button>
          <button type="button" className="app-window-close" onClick={() => void handleWindowClose()} title={t("window.close")}>
            <CloseIcon size={14} />
          </button>
        </div>
      </div>

      <div className="flex min-h-0 min-w-0 flex-1 overflow-hidden">
        <Sidebar
          open={sidebarOpen}
          activeView={activeView}
          packName={packName}
          packAvatar={packAvatar}
          sessions={sessions}
          activeId={activeId}
          busy={appBusy}
          onToggle={() => setSidebarOpen((v) => !v)}
          onViewChange={setActiveView}
          onNewChat={handleNewChat}
          onSelectSession={handleSelectSession}
          onRenameSession={handleRenameSession}
          onDeleteSession={handleDeleteSession}
          onOpenSettings={() => openSettings("general")}
        />

        <section className="flex min-h-0 min-w-0 flex-1 flex-col p-2 pl-0">
        <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-lg border border-[#dfe3e8] bg-white shadow-[0_1px_2px_rgba(15,23,42,0.06)]">
          {activeView === "chat" ? (
            <>
              <header className="flex h-12 shrink-0 items-center gap-2 border-b border-[#eceff3] bg-[#fbfcfd] px-3">
                <button
                  onClick={() => setSidebarOpen(true)}
                  aria-label={t("header.openSidebar")}
                  className="grid h-8 w-8 shrink-0 place-items-center rounded-md text-[#4f5661] transition hover:bg-[#eef1f5] md:hidden"
                >
                  <PanelLeftIcon size={20} />
                </button>

                <div ref={packMenuRef} className="relative">
                  <button
                    onClick={() => setPackMenuOpen((v) => !v)}
                    className="flex h-8 max-w-[240px] items-center gap-2 rounded-md px-2 text-[14px] font-semibold text-[#202124] transition hover:bg-[#eef1f5]"
                  >
                    <img
                      src={packAvatar || "/demiurge.png"}
                      alt=""
                      className="size-6 shrink-0 rounded-md border border-[#dfe3e8] bg-white object-cover"
                    />
                    <span className="min-w-0 truncate">{packName}</span>
                    <ChevronDownIcon
                      size={18}
                      className={`shrink-0 text-[#9a9a9a] transition-transform duration-200 ${packMenuOpen ? "rotate-180" : ""}`}
                    />
                  </button>
                  {packMenuOpen && (
                    <div className="cf-pop cf-pop-down cf-dropdown absolute left-0 top-10 z-20 max-h-[70vh] w-64 overflow-y-auto p-1.5">
                      {packs.length === 0 && <div className="px-3 py-2 text-sm text-[#9a9a9a]">No persona packs found</div>}
                      {packs.map((p) => (
                        <button
                          key={p.id}
                          onClick={() => handleSelectPack(p.id)}
                          className={`cf-menu-item flex w-full items-center justify-between gap-2 ${
                            settings?.current_pack === p.id ? "is-active" : ""
                          }`}
                        >
                          <span className="flex min-w-0 items-center gap-2">
                            <img
                              src={p.avatarDataUrl || "/demiurge.png"}
                              alt=""
                              className="size-8 shrink-0 rounded-md border border-[#dfe3e8] bg-white object-cover"
                            />
                            <span className="min-w-0">
                              <span className="block truncate font-medium">{p.name}</span>
                              <span className="block truncate text-xs text-[#8a8a8a]">Pack / {p.id}</span>
                            </span>
                          </span>
                          {settings?.current_pack === p.id && <CheckIcon size={17} className="shrink-0 text-[#171717]" />}
                        </button>
                      ))}
                    </div>
                  )}
                </div>

                {showAgentMenu && (
                  <div ref={agentMenuRef} className="relative">
                  <button
                    onClick={() => setAgentMenuOpen((v) => !v)}
                    className={`flex h-8 items-center gap-1 rounded-md px-2 text-[13px] font-medium transition hover:bg-[#eef1f5] ${
                      selectedAgentNames.length ? "text-[#171717]" : "text-[#6f7782]"
                    }`}
                    title={t("header.agents")}
                  >
                    {selectedAgentLabel}
                    <ChevronDownIcon
                      size={15}
                      className={`text-[#9a9a9a] transition-transform duration-200 ${agentMenuOpen ? "rotate-180" : ""}`}
                    />
                  </button>
                  {agentMenuOpen && (
                    <div className="cf-pop cf-pop-down cf-dropdown absolute left-0 top-10 z-20 max-h-[70vh] w-80 overflow-y-auto p-1.5">
                      <div className="border-b border-[#eef1f4] px-2.5 py-2">
                        <div className="text-[11px] font-semibold uppercase tracking-wide text-[#8a9099]">
                          Agents folder
                        </div>
                        <div className="mt-1 truncate text-xs text-[#6f7782]" title={`${agentsDir}\\*.json`}>
                          {agentsDir}\*.json
                        </div>
                      </div>
                      {agentPanel.definitions.map((agent) => {
                        const selected = selectedAgentNames.includes(agent.name);
                        return (
                          <button
                            key={agent.name}
                            onClick={() => toggleAgent(agent.name)}
                            className={`cf-menu-item flex w-full items-start justify-between gap-2 ${
                              selected ? "is-active" : ""
                            }`}
                          >
                            <span className="min-w-0">
                              <span className="block truncate font-medium">{agent.name}</span>
                              <span className="mt-0.5 block truncate text-xs text-[#8a8a8a]">
                                {agent.kind} / {agent.description || agent.path}
                              </span>
                              {agent.allowed_tools.length ? (
                                <span className="mt-1 block truncate text-xs text-[#9a9a9a]">
                                  tools: {agent.allowed_tools.join(", ")}
                                </span>
                              ) : null}
                            </span>
                            {selected && <CheckIcon size={17} className="mt-0.5 shrink-0 text-[#171717]" />}
                          </button>
                        );
                      })}
                      {selectedAgentNames.length > 0 && (
                        <button
                          onClick={() => setSelectedAgentNames([])}
                          className="mt-1 w-full rounded-md px-2.5 py-2 text-left text-xs text-[#8a8a8a] transition hover:bg-[#f6f7f9]"
                        >
                          {t("header.clearSelection")}
                        </button>
                      )}
                    </div>
                  )}
                  </div>
                )}

                <div className="hidden min-w-0 flex-col border-l border-[#dfe3e8] pl-3 text-[11px] text-[#8a9099] sm:flex">
                  <span className="max-w-[28vw] truncate font-medium text-[#3f3f3f]" title={activeSession?.title ?? t("chat.newChat")}>
                    {activeSession?.title ?? t("chat.newChat")}
                  </span>
                  <span>{runtimeStatus}</span>
                </div>


                <div className="ml-auto flex items-center gap-2">
                  {planState.path && !planState.approved && (
                    <div className="hidden items-center gap-1 rounded-md border border-[#b8d4ff] bg-[#eef5ff] px-2 py-1 text-xs text-[#0b57d0] lg:flex">
                      <span className="max-w-[18vw] truncate" title={planState.path}>
                        {t("header.planReady")}{planState.path}
                      </span>
                      <button className="rounded bg-[#0b57d0] px-2 py-1 text-white" onClick={() => void handleApprovePlan()}>
                        {t("header.approve")}
                      </button>
                      <button className="rounded px-2 py-1 text-[#5f6368] hover:bg-white" onClick={() => void handleRejectPlan()}>
                        {t("header.reject")}
                      </button>
                    </div>
                  )}

                  <div ref={toyMenuRef} className="relative">
                    <button
                      type="button"
                      onClick={() => setToyMenuOpen((v) => !v)}
                      className={`grid h-8 w-8 place-items-center rounded-md transition ${
                        toyMenuOpen ? "bg-[#eef1f5] text-[#111827]" : "text-[#59616d] hover:bg-[#eef1f5]"
                      }`}
                      aria-label={t("header.toys")}
                      title={t("header.toys")}
                    >
                      <SparklesIcon size={17} />
                    </button>
                    {toyMenuOpen && (
                      <div className="cf-pop cf-pop-down cf-dropdown absolute right-0 top-10 z-30 w-[min(720px,calc(100vw-2rem))] overflow-hidden">
                        <div className="flex items-center justify-between border-b border-[#eceff3] bg-[#fbfcfd] px-3 py-2.5">
                          <div className="flex items-center gap-2 text-[13px] font-semibold text-[#202124]">
                            <SparklesIcon size={16} />
                            {t("header.toys")}
                          </div>
                          <button
                            type="button"
                            onClick={() => {
                              setToyMenuOpen(false);
                              openSettings("companion");
                            }}
                            className="inline-flex h-7 items-center gap-1.5 rounded-md px-2 text-[12px] font-medium text-[#59616d] transition hover:bg-[#eef1f5] hover:text-[#202124]"
                          >
                            <SettingsIcon size={13} />
                            {t("sidebar.settings")}
                          </button>
                        </div>
                        <div className="toy-panel max-h-[min(72vh,680px)] overflow-y-auto p-2">
                          <button
                            type="button"
                            onClick={() => {
                              setToyMenuOpen(false);
                              setFortuneOpen(true);
                            }}
                            className="flex w-full items-center gap-2 rounded-[10px] border border-[#eceff3] bg-white p-2.5 text-left transition hover:bg-[#fbfcfd]"
                          >
                            <span className="grid size-7 shrink-0 place-items-center rounded-md border border-[#e2e5ea] text-[#49515c]">
                              <SparklesIcon size={15} />
                            </span>
                            <span className="min-w-0 flex-1">
                              <span className="block text-[12px] font-semibold text-[#202124]">{t("fortune.cardTitle")}</span>
                              <span className="block truncate text-[11px] text-[#7a8088]">{t("fortune.cardDesc")}</span>
                            </span>
                            <span className="text-[12px] font-medium text-[#59616d]">{t("fortune.cardDraw")}</span>
                          </button>
                          <CompanionCard
                            settings={settings}
                            onOpenSettings={() => {
                              setToyMenuOpen(false);
                              openSettings("companion");
                            }}
                          />
                          <PomodoroCard activeSessionId={activeId} activeSessionTitle={activeSession?.title} goal={goalPanel} />
                        </div>
                      </div>
                    )}
                  </div>
                </div>
              </header>

              <GoalBar goal={goalPanel} busy={appBusy} progress={goalProgress} onAction={handleGoalAction} />

              <MessageList
                items={items}
                thinking={thinking}
                greeting={t("chat.greeting")}
                onRetry={(text) => void handleSend(text)}
                onOpenFortune={() => setFortuneOpen(true)}
              />

              <Composer
                input={input}
                canSend={canSend}
                loading={appBusy}
                permissionMode={settings?.permission_mode ?? "default"}
                onSetPermissionMode={(m) => void handleSetPermissionMode(m)}
                provider={settings?.provider ?? "deepseek"}
                model={settings?.model ?? ""}
                reasoningEffort={settings?.reasoning_effort ?? "auto"}
                maxInputTokens={settings?.max_input_tokens ?? 0}
                onSetModel={(m) => void handleSetModel(m)}
                onSetEffort={(e) => void handleSetEffort(e)}
                onOpenSettings={() => openSettings("context")}
                workspace={workspace}
                onOpenWorkspace={() => void api.openSandbox()}
                textareaRef={textareaRef}
                onSubmit={(attachments) => handleSend(undefined, attachments)}
                onStop={() => {
                  void api.interrupt();
                  setConfirmReq(null);
                }}
                onInputChange={setInput}
              />
            </>
          ) : activeView === "media" ? (
            <MediaStudio settings={settings} onOpenSettings={() => openSettings("media")} />
          ) : activeView === "skills" ? (
            <SkillsPanel />
          ) : activeView === "live2d" ? (
            settings ? (
              <Suspense
                fallback={
                  <div className="grid flex-1 place-items-center text-[13px] text-[#8a9099]">
                    {t("live2d.loading")}
                  </div>
                }
              >
                <Live2DPanel
                  packId={settings.current_pack}
                  onOpenSettings={() => openSettings("persona")}
                />
              </Suspense>
            ) : null
          ) : settings ? (
            <SettingsDialog
              open
              settings={settings}
              packs={packs}
              agentPanel={agentPanel}
              initialTab={settingsInitialTab}
              onClose={handleCloseSettings}
              onSave={handleSaveSettings}
              onPreviewTheme={setPreviewTheme}
              onPacksChange={setPacks}
              onAgentPanelChange={setAgentPanel}
            />
          ) : null}
        </div>
      </section>
      </div>

      <ConfirmDialog req={confirmReq} mode={settings?.permission_mode ?? "default"} onRespond={handleRespondConfirm} />
      <FortuneDialog open={fortuneOpen} onClose={() => setFortuneOpen(false)} />
    </main>
  );
}
