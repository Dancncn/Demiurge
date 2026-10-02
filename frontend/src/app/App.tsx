import { Suspense, lazy, useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow, PhysicalSize } from "@tauri-apps/api/window";
import * as api from "@/lib/api";
import type {
  AgentPanelState,
  GoalPanelState,
  GoalProgressEvent,
  NavigationSnapshot,
  PackManifest,
  PermissionMode,
  PermissionScope,
  PlanState,
  ReasoningEffort,
  SessionEnginePanelState,
  SessionMeta,
  Settings,
  AppAppearance,
  AppTheme,
  WorkspaceState,
} from "@/lib/types";
import { MessageList } from "@/features/chat/MessageList";
import { Sidebar, type AppView } from "@/app/Sidebar";
import { Composer } from "@/features/chat/Composer";
import { WorkspaceExplorer } from "@/features/workspace/WorkspaceExplorer";
import GoalBar, { type GoalAction } from "@/features/agent/GoalBar";
import ConfirmDialog from "@/features/agent/ConfirmDialog";
import SettingsDialog, { type SettingsTab } from "@/features/settings/SettingsDialog";
import MediaStudio from "@/features/media/MediaStudio";
import IntegrationCenter from "@/features/agent/IntegrationCenter";
import FortuneDialog from "@/features/companion/FortuneDialog";
import VoiceCallPanel from "@/features/voice/VoiceCallPanel";
import WorkflowsPanel from "@/features/workflow/WorkflowsPanel";
import {
  CheckIcon,
  ChevronDownIcon,
  CloseIcon,
  FolderIcon,
  MaximizeIcon,
  MinimizeIcon,
  PanelLeftIcon,
  PhoneIcon,
  PinIcon,
  VolumeIcon,
} from "@/shared/components/Icons";
import { attachmentKindLabel, buildAttachmentPrompt, formatAttachmentSize, type ProcessedAttachment } from "@/lib/fileProcessing";
import { autoContextBudget } from "@/lib/providers";
import { canDrawToday, isAutoPromptEnabled, isDismissedToday } from "@/lib/fortune";
import { useI18n } from "@/lib/i18n";
import { useClickOutside } from "@/lib/hooks";
import { usePomodoroNotifications } from "@/lib/usePomodoroNotifications";
import { useStreamingTtsQueue } from "@/lib/useStreamingTtsQueue";
import {
  NavigationEpoch,
  type NavigationTicket,
} from "@/lib/navigationEpoch";
import { MessageProjection } from "@/lib/messageProjection";
import { ToolConfirmationState } from "@/lib/confirmationState";

const Live2DPanel = lazy(() => import("@/features/live2d/Live2DPanel"));

const DEFAULT_WINDOW_SIZE = { width: 1811, height: 1213 };

// Browser-preview fallback so the UI renders without the Tauri backend (dev/screenshots only).
const PREVIEW_SETTINGS: Settings = {
  provider: "deepseek",
  permission_mode: "default",
  base_url: "https://api.deepseek.com/v1",
  api_key: "",
  model: "deepseek-chat",
  reasoning_effort: "auto",
  model_routing: {
    enabled: false,
    subagent_tier: "haiku",
    docs_agent_tier: "sonnet",
    haiku_model: "",
    sonnet_model: "",
    opus_model: "",
    fallback_models: [],
    auto_failover: true,
    max_failover_attempts: 2,
    failure_threshold: 2,
    cooldown_seconds: 60,
  },
  current_pack: "default",
  current_pet: "",
  max_context_chars: 512000,
  max_input_tokens: 128000,
  reserved_output_tokens: 16000,
  context_budget_auto: true,
  language: "zh",
  theme: "system",
  appearance: "material_bloom",
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
  desktop_companion_enabled: false,
  desktop_companion_always_on_top: true,
  desktop_companion_click_through: false,
  desktop_companion_collapsed: false,
  weather_enabled: false,
  weather_location_mode: "manual",
  weather_city: "",
  weather_provider: "open_meteo",
  voice_enabled: false,
  voice_stt_backend: "",
  voice_tts_backend: "",
  voice_id: "",
  voice_speed: 1,
  voice_emotion: "",
  voice_streaming: false,
  voice_tts_fallback: true,
  voice_hotkey_enabled: true,
  voice_hotkey: "Ctrl+Shift+Space",
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

function matchesHotkey(event: KeyboardEvent, hotkey: string) {
  const parts = hotkey
    .split("+")
    .map((part) => part.trim().toLowerCase())
    .filter(Boolean);
  if (!parts.length) return false;
  const wantsCtrl = parts.includes("ctrl") || parts.includes("control");
  const wantsShift = parts.includes("shift");
  const wantsAlt = parts.includes("alt") || parts.includes("option");
  const wantsMeta = parts.includes("meta") || parts.includes("cmd") || parts.includes("command");
  const keyPart = parts.find((part) => !["ctrl", "control", "shift", "alt", "option", "meta", "cmd", "command"].includes(part));
  const key = event.key === " " ? "space" : event.key.toLowerCase();
  return (
    event.ctrlKey === wantsCtrl &&
    event.shiftKey === wantsShift &&
    event.altKey === wantsAlt &&
    event.metaKey === wantsMeta &&
    key === (keyPart || "space")
  );
}

function waitForNextPaint() {
  return new Promise<void>((resolve) => {
    requestAnimationFrame(() => requestAnimationFrame(() => resolve()));
  });
}

export default function App() {
  const { t, setLang } = useI18n();
  usePomodoroNotifications();
  const [projection] = useState(() => new MessageProjection({
    request: (callback) => requestAnimationFrame(callback),
    cancel: (frame) => cancelAnimationFrame(frame),
  }));
  const items = useSyncExternalStore(projection.subscribe, projection.getSnapshot);
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [packs, setPacks] = useState<PackManifest[]>([]);
  const [agentPanel, setAgentPanel] = useState<AgentPanelState>({ definitions: [], agents_dir: "" });
  const [workspace, setWorkspace] = useState<WorkspaceState | null>(null);
  const [workspacePanelOpen, setWorkspacePanelOpen] = useState(false);
  const [workspaceRefreshKey, setWorkspaceRefreshKey] = useState(0);
  const [goalPanel, setGoalPanel] = useState<GoalPanelState | null>(null);
  const [goalProgress, setGoalProgress] = useState<GoalProgressEvent | null>(null);
  const [sessionEngine, setSessionEngine] = useState<SessionEnginePanelState | null>(null);
  const [selectedAgentNames, setSelectedAgentNames] = useState<string[]>([]);
  const [sessions, setSessions] = useState<SessionMeta[]>([]);
  const [activeId, setActiveId] = useState("");
  const [navigationPending, setNavigationPending] = useState(true);
  const [activeView, setActiveView] = useState<AppView>("chat");
  const [live2dMounted, setLive2dMounted] = useState(false);
  const [settingsInitialTab, setSettingsInitialTab] = useState<SettingsTab>("general");
  const [previewTheme, setPreviewTheme] = useState<AppTheme | null>(null);
  const [previewAppearance, setPreviewAppearance] = useState<AppAppearance | null>(null);
  const [sidebarOpen, setSidebarOpen] = useState(
    () => typeof window === "undefined" || !window.matchMedia("(max-width: 767px)").matches,
  );
  const [packMenuOpen, setPackMenuOpen] = useState(false);
  const [agentMenuOpen, setAgentMenuOpen] = useState(false);
  const [titleMenuOpen, setTitleMenuOpen] = useState<"file" | "edit" | "view" | "persona" | "help" | null>(null);
  const [confirmations] = useState(() => new ToolConfirmationState());
  const confirmReq = useSyncExternalStore(confirmations.subscribe, confirmations.getSnapshot);
  const [planState, setPlanState] = useState<PlanState>({ active: false, approved: false });
  const [fortuneOpen, setFortuneOpen] = useState(false);
  const [workflowOpen, setWorkflowOpen] = useState(false);
  const [spokenRepliesEnabled, setSpokenRepliesEnabled] = useState(false);
  const [voicePanelOpen, setVoicePanelOpen] = useState(false);
  const [voiceCallActive, setVoiceCallActive] = useState(false);
  const [voiceCallStartedAt, setVoiceCallStartedAt] = useState<number | null>(null);
  const [voiceCallMuted, setVoiceCallMuted] = useState(false);
  const ttsQueue = useStreamingTtsQueue(settings);

  const packMenuRef = useRef<HTMLDivElement | null>(null);
  const agentMenuRef = useRef<HTMLDivElement | null>(null);
  const titleMenuRef = useRef<HTMLDivElement | null>(null);
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);
  const spokenRepliesEnabledRef = useRef(spokenRepliesEnabled);
  const voiceCallActiveRef = useRef(voiceCallActive);
  const ttsQueueRef = useRef(ttsQueue);
  const activeIdRef = useRef(activeId);
  const navigationEpochRef = useRef(new NavigationEpoch());
  const navigationPendingRef = useRef(true);

  useEffect(() => {
    activeIdRef.current = activeId;
  }, [activeId]);

  useEffect(() => {
    spokenRepliesEnabledRef.current = spokenRepliesEnabled;
  }, [spokenRepliesEnabled]);

  useEffect(() => {
    voiceCallActiveRef.current = voiceCallActive;
  }, [voiceCallActive]);

  useEffect(() => {
    ttsQueueRef.current = ttsQueue;
  }, [ttsQueue]);

  useEffect(() => {
    ttsQueue.setMuted(voiceCallMuted);
  }, [voiceCallMuted]);

  useEffect(() => {
    if (spokenRepliesEnabled && !ttsQueue.available) {
      setSpokenRepliesEnabled(false);
      ttsQueue.stop();
    }
  }, [spokenRepliesEnabled, ttsQueue.available]);

  useEffect(() => {
    if (!settings?.voice_hotkey_enabled) return;
    const hotkey = settings.voice_hotkey || "Ctrl+Shift+Space";
    const onKeyDown = (event: KeyboardEvent) => {
      if (!matchesHotkey(event, hotkey)) return;
      event.preventDefault();
      setVoicePanelOpen(true);
      if (!voiceCallActiveRef.current) {
        startVoiceCall();
      } else {
        window.dispatchEvent(new Event("demiurge-voice-hotkey"));
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [settings?.voice_hotkey_enabled, settings?.voice_hotkey]);

  const activeSession = useMemo(() => sessions.find((s) => s.id === activeId) ?? null, [activeId, sessions]);
  const agentsDir = agentPanel.agents_dir || ".demiurge/agents";
  const appBusy = busy || sessionEngine?.busy === true;
  const interactionBusy = appBusy || navigationPending;
  const runtimeStatus = sessionEngine?.cancel_requested
    ? t("status.cancelling")
    : sessionEngine?.active_turn
      ? sessionEngine.active_turn.status === "cancelling"
        ? t("status.cancelling")
        : t("status.processing")
      : t("status.ready");

  function setNavigationPendingValue(value: boolean) {
    navigationPendingRef.current = value;
    setNavigationPending(value);
  }

  function beginNavigation(expectedSessionId?: string) {
    const ticket = navigationEpochRef.current.begin(expectedSessionId);
    confirmations.clear();
    setNavigationPendingValue(true);
    return ticket;
  }

  function finishNavigation(ticket: NavigationTicket) {
    if (navigationEpochRef.current.accepts(ticket, ticket.expectedSessionId ?? activeIdRef.current)) {
      setNavigationPendingValue(false);
    }
  }

  function applyNavigationSnapshot(snapshot: NavigationSnapshot, ticket: NavigationTicket, replaceHistory: boolean) {
    if (!navigationEpochRef.current.accepts(ticket, snapshot.session_id)) return false;
    if (!snapshot.sessions.some((session) => session.id === snapshot.session_id)) return false;

    activeIdRef.current = snapshot.session_id;
    confirmations.selectSession(snapshot.session_id);
    setActiveId(snapshot.session_id);
    setSessions(snapshot.sessions);
    setGoalPanel(snapshot.goal);
    setWorkspace(snapshot.workspace);
    setWorkspaceRefreshKey((value) => value + 1);
    if (replaceHistory) {
      projection.replaceHistory(snapshot.session_id, snapshot.history);
      setGoalProgress(null);
      confirmations.clear();
    }
    return true;
  }

  async function refreshNavigationSnapshot(replaceHistory = false) {
    if (navigationPendingRef.current) return false;
    const expectedSessionId = activeIdRef.current;
    if (!expectedSessionId) return false;
    const ticket = navigationEpochRef.current.capture(expectedSessionId);
    try {
      const snapshot = await api.navigationSnapshot(expectedSessionId);
      return applyNavigationSnapshot(snapshot, ticket, replaceHistory);
    } catch (e) {
      if (navigationEpochRef.current.accepts(ticket, expectedSessionId)) {
        console.error("Failed to refresh navigation snapshot", e);
      }
      return false;
    }
  }

  async function recoverNavigation(ticket: NavigationTicket) {
    try {
      const snapshot = await api.navigationSnapshot(ticket.expectedSessionId);
      return applyNavigationSnapshot(snapshot, ticket, true);
    } catch {
      return false;
    }
  }

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
    document.documentElement.dataset.appearance = previewAppearance ?? settings?.appearance ?? "material_bloom";
  }, [previewAppearance, settings?.appearance]);

  useEffect(() => {
    const compactWindow = window.matchMedia("(max-width: 767px)");
    const collapseForCompactWindow = () => {
      if (compactWindow.matches) setSidebarOpen(false);
    };

    collapseForCompactWindow();
    compactWindow.addEventListener("change", collapseForCompactWindow);
    return () => compactWindow.removeEventListener("change", collapseForCompactWindow);
  }, []);

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

  useEffect(() => {
    const currentPack = packs.find((pack) => pack.id === settings?.current_pack);
    if (!currentPack?.live2d) return;
    let cancelled = false;
    const preload = () => {
      void import("@/lib/live2d").then((module) => {
        if (!cancelled) {
          // Preloading is opportunistic — old engine bundles may not export it.
          try { module.preloadLive2DEngine?.(); } catch { /* optional */ }
        }
      });
    };
    let cancelPreload: () => void;
    if (typeof window.requestIdleCallback === "function") {
      const idleId = window.requestIdleCallback(preload, { timeout: 4_000 });
      cancelPreload = () => window.cancelIdleCallback(idleId);
    } else {
      const timeoutId = globalThis.setTimeout(preload, 1_500);
      cancelPreload = () => globalThis.clearTimeout(timeoutId);
    }
    return () => {
      cancelled = true;
      cancelPreload();
    };
  }, [packs, settings?.current_pack]);

  // 每日吉签：应用启动时若启用自动弹窗、今日尚未抽签且未主动忽略，弹出引导抽签。
  useEffect(() => {
    if (!isAutoPromptEnabled()) return;
    if (canDrawToday() && !isDismissedToday()) setFortuneOpen(true);
  }, []);

  useEffect(() => {
    const navigationTicket = beginNavigation();
    (async () => {
      try {
        const [s, ps, agents, snapshot, plan, engine] = await Promise.all([
          api.getSettings(),
          api.listPacks(),
          api.agentPanelState(),
          api.navigationSnapshot(),
          api.planState(),
          api.sessionEngineState(),
        ]);
        setSettings(s);
        if (s.language === "zh" || s.language === "en") setLang(s.language);
        setPacks(ps);
        setAgentPanel(agents);
        applyNavigationSnapshot(snapshot, navigationTicket, true);
        setPlanState(plan);
        confirmations.updateEngine(engine);
        setSessionEngine(engine);
        setBusy(engine.busy);
      } catch (e) {
        console.error("Failed to initialize Demiurge", e);
        // Outside Tauri (browser preview), seed defaults so the UI is still browsable.
        if (!("__TAURI_INTERNALS__" in window)) {
          setSettings((prev) => prev ?? PREVIEW_SETTINGS);
          setWorkspace({ path: "D:\\Project\\Project-1\\Demiurge", name: "Demiurge", is_git: true, branch: "main", dirty: false });
        }
      } finally {
        finishNavigation(navigationTicket);
      }
    })();
  }, []);

  async function refreshSessions() {
    await refreshNavigationSnapshot(false);
  }

  async function refreshWorkspaceState() {
    await refreshNavigationSnapshot(false);
  }

  function handleWorkspaceChange(_next: WorkspaceState) {
    void refreshNavigationSnapshot(false);
  }

  async function refreshGoalPanel() {
    await refreshNavigationSnapshot(false);
  }

  useEffect(() => {
    let un: UnlistenFn | undefined;
    let disposed = false;

    api
      .listenAgentEvents({
        onAssistantStart: (turn, responseId) => {
          if (!projection.consume({ kind: "start", turn, responseId })) return;
          ttsQueueRef.current.beginTurn(spokenRepliesEnabledRef.current || voiceCallActiveRef.current);
        },
        onAssistantDelta: (text, turn, responseId) => {
          if (!projection.consume({ kind: "delta", text, turn, responseId })) return;
          if (spokenRepliesEnabledRef.current || voiceCallActiveRef.current) ttsQueueRef.current.pushText(text);
        },
        onAssistantReasoning: (text, turn, responseId) => {
          projection.consume({ kind: "reasoning", text, turn, responseId });
        },
        onAssistantDone: (text, turn, responseId) => {
          if (!projection.consume({ kind: "done", text, turn, responseId })) return;
          confirmations.clear();
          if (spokenRepliesEnabledRef.current || voiceCallActiveRef.current) ttsQueueRef.current.flush();
          setBusy(false);
          void refreshGoalPanel();
        },
        onAssistantError: (error, turn, responseId) => {
          if (!projection.consume({ kind: "error", error, turn, responseId })) return;
          confirmations.clear();
          ttsQueueRef.current.stop();
          setBusy(false);
          void refreshGoalPanel();
        },
        onAssistantInterrupted: (turn, responseId) => {
          if (!projection.consume({ kind: "interrupted", turn, responseId })) return;
          confirmations.clear();
          ttsQueueRef.current.stop();
          setBusy(false);
          void refreshGoalPanel();
        },
        onToolStart: (tool, turn, responseId) => {
          projection.consume({ kind: "tool_start", tool, turn, responseId });
        },
        onToolEnd: (tool, turn, responseId) => {
          if (!projection.consume({ kind: "tool_end", tool, turn, responseId })) return;
          if (["write_file", "edit_file", "multi_edit", "apply_patch", "undo_edit"].includes(tool.name)) {
            void refreshWorkspaceState();
          }
        },
        onConfirmRequest: (event) => {
          confirmations.receive(event);
        },
        onGoalProgress: (event) => {
          if (!projection.appendGoalProgress(event)) return;
          setGoalProgress(event);
          void refreshGoalPanel();
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
      confirmations.updateEngine(next);
      setSessionEngine(next);
      setBusy(next.busy);
    }).then((u) => {
      if (disposed) u();
      else unSessionEngine = u;
    }).catch((e) => console.warn("subscribe failed", e));

    return () => {
      disposed = true;
      projection.dispose();
      un?.();
      unPlan?.();
      unMode?.();
      unSettings?.();
      unSessionEngine?.();
    };
  }, []);

  // 顶部菜单 / pack 菜单 / agent 菜单的「外点 + Escape 关闭」逻辑统一收敛到 useClickOutside。
  useClickOutside(packMenuRef, () => setPackMenuOpen(false), { enabled: packMenuOpen });
  useClickOutside(titleMenuRef, () => setTitleMenuOpen(null), { escape: true, enabled: !!titleMenuOpen });
  useClickOutside(agentMenuRef, () => setAgentMenuOpen(false), { enabled: agentMenuOpen });

  async function handleSend(textArg?: string, attachments: ProcessedAttachment[] = []) {
    const text = (textArg ?? input).trim();
    const attachmentPrompt = buildAttachmentPrompt(attachments);
    if ((!text && !attachmentPrompt) || appBusy || navigationPendingRef.current) return false;
    const turnSessionId = activeIdRef.current;
    let completed = false;
    setInput("");
    setActiveView("chat");
    const prompt = `${text || "Please review the attached files."}${attachmentPrompt}`;
    const submission = projection.beginSubmission(buildUserDisplayText(text, attachments), prompt);
    confirmations.clear();
    setBusy(true);
    try {
      if (selectedAgentNames.length) {
        await api.sendWithAgents(prompt, selectedAgentNames);
      } else {
        await api.send(prompt);
      }
      completed = true;
    } catch (err) {
      confirmations.clear();
      projection.failSubmission(submission, err);
    } finally {
      setBusy(false);
      if (completed) await syncHistoryIfMissingAssistant(turnSessionId);
      void refreshSessions();
      void refreshGoalPanel();
    }
    return true;
  }

  function handleInterrupt() {
    confirmations.cancel();
    ttsQueue.stop();
    void api.interrupt();
  }

  async function handleRespondConfirm(allow: boolean, scope: PermissionScope) {
    const request = confirmations.getSnapshot();
    if (!request) return;
    const id = request.id;
    confirmations.clear();
    try {
      await api.respondConfirm(id, allow, scope);
    } catch (e) {
      console.error("Failed to respond to confirmation", e);
    }
  }

  async function handleGoalAction(action: GoalAction) {
    if (navigationPendingRef.current || ((action === "resume" || action === "continue") && appBusy)) return;
    const actionSessionId = activeIdRef.current;
    const ticket = navigationEpochRef.current.capture(actionSessionId);
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
      if (navigationEpochRef.current.accepts(ticket, activeIdRef.current)) {
        setGoalPanel(next);
        await refreshSessions();
      }
    } catch (err) {
      if (!navigationEpochRef.current.accepts(ticket, activeIdRef.current)) return;
      projection.appendWarning(`Warning: ${String(err)}`);
    } finally {
      if (action === "resume" || action === "continue") setBusy(false);
      if (navigationEpochRef.current.accepts(ticket, activeIdRef.current)) void refreshGoalPanel();
    }
  }

  async function syncHistoryIfMissingAssistant(sessionId: string) {
    const ticket = navigationEpochRef.current.capture(sessionId);
    await waitForNextPaint();
    if (!navigationEpochRef.current.accepts(ticket, activeIdRef.current)) return;
    if (!projection.needsHistory(sessionId)) return;

    try {
      const snapshot = await api.navigationSnapshot(sessionId);
      if (!projection.needsHistory(sessionId)) return;
      applyNavigationSnapshot(snapshot, ticket, true);
    } catch (e) {
      if (navigationEpochRef.current.accepts(ticket, sessionId)) {
        console.error("Failed to sync chat history after turn", e);
      }
    }
  }

  async function handleNewChat() {
    if (appBusy || navigationPendingRef.current) return;
    const ticket = beginNavigation();
    try {
      const snapshot = await api.newSession();
      if (applyNavigationSnapshot(snapshot, ticket, true)) {
        requestAnimationFrame(() => textareaRef.current?.focus());
      }
    } catch (e) {
      console.error(e);
      await recoverNavigation(ticket);
    } finally {
      finishNavigation(ticket);
    }
  }

  async function handleSelectSession(id: string) {
    if (appBusy || navigationPendingRef.current || id === activeIdRef.current) return;
    const ticket = beginNavigation(id);
    try {
      const snapshot = await api.selectSession(id);
      applyNavigationSnapshot(snapshot, ticket, true);
    } catch (e) {
      console.error(e);
      await recoverNavigation(ticket);
    } finally {
      finishNavigation(ticket);
    }
  }

  async function handleRenameSession(id: string, title: string) {
    if (appBusy || navigationPendingRef.current) return;
    const renamed = await api.renameSession(id, title);
    setSessions((prev) =>
      prev
        .map((s) => (s.id === id ? { ...s, title: renamed, updated_at: Date.now() } : s))
        .sort((a, b) => b.updated_at - a.updated_at),
    );
    await refreshSessions();
  }

  async function handleDeleteSession(id: string) {
    if (appBusy || navigationPendingRef.current) return;
    const ticket = beginNavigation();
    try {
      const snapshot = await api.deleteSession(id);
      applyNavigationSnapshot(snapshot, ticket, true);
    } catch (e) {
      console.error(e);
      await recoverNavigation(ticket);
    } finally {
      finishNavigation(ticket);
    }
  }

  async function handleArchiveSession(id: string, archived: boolean) {
    if (appBusy || navigationPendingRef.current) return;
    try {
      await api.setSessionArchived(id, archived);
      await refreshSessions();
    } catch (e) {
      console.error("Failed to update session archive state", e);
      throw e;
    }
  }

  async function handleSaveSettings(s: Settings) {
    try {
      await api.saveSettings(s);
      setSettings(s);
      setPreviewTheme(null);
      setPreviewAppearance(null);
    } catch (e) {
      console.error("Failed to save settings", e);
    }
  }

  function handleCloseSettings() {
    setPreviewTheme(null);
    setPreviewAppearance(null);
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

  function navigateToView(view: AppView) {
    if (view === "live2d") setLive2dMounted(true);
    setActiveView(view);
  }

  function openSettings(tab: SettingsTab = "general") {
    setSettingsInitialTab(tab);
    setActiveView("settings");
  }

  async function openWidgets() {
    try {
      await api.openWidgetsWindow();
    } catch (e) {
      console.error("Failed to open widgets window", e);
    }
  }

  const last = items[items.length - 1];
  const tailStreaming = last?.kind === "assistant" && last.streaming;
  const tailToolRunning = last?.kind === "tool" && last.status === "running";
  const thinking = appBusy && !tailStreaming && !tailToolRunning;
  const canSend = input.trim().length > 0 && !interactionBusy;
  const titleMenuButtonClass = (menu: typeof titleMenuOpen) =>
    `app-title-menu-button ${titleMenuOpen === menu ? "is-active" : ""}`;

  function toggleSpokenReplies() {
    setSpokenRepliesEnabled((enabled) => {
      const next = !enabled;
      if (!next) ttsQueue.stop();
      return next;
    });
  }

  /**
   * 主工具栏按钮的语义是"呼出"而不是开关：已启用时只把陪伴壳恢复到前台，
   * 不会因为窗口被隐藏/最小化而反向停用。停用只走设置页和陪伴壳自身的关闭按钮。
   */
  async function summonDesktopCompanion() {
    if (!settings) return;
    if (settings.desktop_companion_enabled) {
      try {
        await api.desktopCompanionRestore();
      } catch (e) {
        console.error("Failed to summon desktop companion", e);
      }
      return;
    }
    const next = {
      ...settings,
      desktop_companion_enabled: true,
    };
    setSettings(next);
    try {
      await api.saveSettings(next);
    } catch (e) {
      console.error("Failed to enable desktop companion", e);
      setSettings(settings);
    }
  }

  function startVoiceCall() {
    setVoicePanelOpen(true);
    setVoiceCallActive(true);
    setVoiceCallStartedAt((value) => value ?? Date.now());
    setSpokenRepliesEnabled(false);
  }

  function endVoiceCall() {
    setVoiceCallActive(false);
    setVoiceCallStartedAt(null);
    ttsQueue.stop();
  }

  function closeVoicePanel() {
    endVoiceCall();
    setVoicePanelOpen(false);
  }

  async function handleVoiceTranscript(text: string) {
    if (!text.trim() || interactionBusy) return false;
    startVoiceCall();
    return handleSend(text);
  }

  async function handleWindowMinimize() {
    if (!("__TAURI_INTERNALS__" in window)) return;
    await api.mainWindowMinimize();
  }

  async function handleWindowToggleMaximize() {
    if (!("__TAURI_INTERNALS__" in window)) return;
    await api.mainWindowToggleMaximize();
  }

  async function handleWindowClose() {
    if (!("__TAURI_INTERNALS__" in window)) return;
    await api.mainWindowClose();
  }

  return (
    <main className="app-shell flex h-[100dvh] flex-col overflow-hidden bg-[#eef1f5] text-[#202124]">
      <div ref={titleMenuRef} className="app-titlebar">
        <div
          className="app-titlebar-brand app-titlebar-drag"
          data-tauri-drag-region
          onDoubleClick={() => void handleWindowToggleMaximize()}
        >
          <img src="/demiurge.png" alt="" className="size-4 rounded-[4px]" />
          <span>Demiurge</span>
        </div>

        {!sidebarOpen && (
          <button
            type="button"
            onClick={() => setSidebarOpen(true)}
            className="app-mobile-navigation-toggle"
            aria-label={t("header.openSidebar")}
            title={t("header.openSidebar")}
          >
            <PanelLeftIcon size={17} />
          </button>
        )}

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
                    handleInterrupt();
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
                  ["skills", t("nav.resources")],
                  ["live2d", t("nav.live2d")],
                ].map(([view, label]) => (
                  <button
                    key={view}
                    type="button"
                    onClick={() => {
                      setTitleMenuOpen(null);
                      navigateToView(view as AppView);
                    }}
                    className={`cf-menu-item flex w-full items-center justify-between gap-2 ${
                      activeView === view ? "is-active" : ""
                    }`}
                  >
                    <span>{label}</span>
                    {activeView === view && <CheckIcon size={14} />}
                  </button>
                ))}
                <button
                  type="button"
                  onClick={() => {
                    setTitleMenuOpen(null);
                    setWorkflowOpen(true);
                  }}
                  className="cf-menu-item flex w-full items-center gap-2"
                >
                  Workflows
                </button>
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
          navigationPending={navigationPending}
          onToggle={() => setSidebarOpen((v) => !v)}
          onViewChange={navigateToView}
          onNewChat={handleNewChat}
          onSelectSession={handleSelectSession}
          onRenameSession={handleRenameSession}
          onArchiveSession={handleArchiveSession}
          onDeleteSession={handleDeleteSession}
          onOpenSettings={() => openSettings("general")}
        />

        <section className="app-content-frame flex min-h-0 min-w-0 flex-1 flex-col p-2 pl-0">
        <div className="app-workspace-surface flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-lg border border-[#dfe3e8] bg-white shadow-[0_1px_2px_rgba(15,23,42,0.06)]">
          {activeView === "chat" ? (
            <>
              <header className="app-chat-toolbar flex h-12 shrink-0 items-center gap-2 border-b border-[#eceff3] bg-[#fbfcfd] px-3">
                <div ref={packMenuRef} className="relative">
                  <button
                    onClick={() => setPackMenuOpen((v) => !v)}
                    className="app-chat-toolbar-pack md-button md-button-text flex h-8 max-w-[240px] items-center gap-2 rounded-md px-2 text-[14px] font-semibold text-[#202124] transition hover:bg-[#eef1f5]"
                    aria-haspopup="menu"
                    aria-expanded={packMenuOpen}
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
                    className={`app-chat-toolbar-agent md-button md-button-text flex h-8 items-center gap-1 rounded-md px-2 text-[13px] font-medium transition hover:bg-[#eef1f5] ${
                      selectedAgentNames.length ? "text-[#171717]" : "text-[#6f7782]"
                    }`}
                    title={t("header.agents")}
                    aria-haspopup="menu"
                    aria-expanded={agentMenuOpen}
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
                              {(agent.model_tier || agent.scope) && (
                                <span className="mt-1 block truncate text-xs text-[#6b7280]">
                                  {agent.model_tier ? `tier: ${agent.model_tier}` : "tier: auto"}
                                  {agent.scope ? ` · scope: ${agent.scope}` : " · scope: read_only"}
                                </span>
                              )}
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

                <div className="app-session-heading hidden min-w-0 flex-col border-l border-[#dfe3e8] pl-3 text-[14px] text-[#8a9099] sm:flex">
                  <span className="max-w-[28vw] truncate font-medium text-[#3f3f3f]" title={activeSession?.title ?? t("chat.newChat")}>
                    {activeSession?.title ?? t("chat.newChat")}
                  </span>
                  <span>{runtimeStatus}</span>
                </div>


                <div className="app-toolbar-actions ml-auto flex items-center gap-1">
                  {planState.path && !planState.approved && (
                    <div className="hidden items-center gap-1 rounded-md border border-[#b8d4ff] bg-[#eef5ff] px-2 py-1 text-xs text-[#0b57d0] lg:flex">
                      <span className="max-w-[18vw] truncate" title={planState.path}>
                        {t("header.planReady")}{planState.path}
                      </span>
                      <button className="md-button md-button-filled rounded bg-[#0b57d0] px-2 py-1 text-white" onClick={() => void handleApprovePlan()}>
                        {t("header.approve")}
                      </button>
                      <button className="md-button md-button-text rounded px-2 py-1 text-[#5f6368] hover:bg-white" onClick={() => void handleRejectPlan()}>
                        {t("header.reject")}
                      </button>
                    </div>
                  )}

                  <button
                    type="button"
                    onClick={() => setWorkspacePanelOpen((value) => !value)}
                    disabled={!workspace}
                    className={`app-toolbar-secondary md-icon-button grid h-8 w-8 place-items-center rounded-md transition ${
                      workspacePanelOpen
                        ? "bg-[#eef5ff] text-[#0b57d0]"
                        : "text-[#59616d] hover:bg-[#eef1f5]"
                    } disabled:cursor-not-allowed disabled:opacity-40`}
                    aria-label={t("workspace.toggle")}
                    aria-pressed={workspacePanelOpen}
                    title={workspace?.path || t("workspace.toggle")}
                  >
                    <FolderIcon size={17} />
                  </button>

                  <button
                    type="button"
                    onClick={() => void summonDesktopCompanion()}
                    disabled={!settings}
                    className={`app-toolbar-secondary md-icon-button grid h-8 w-8 place-items-center rounded-md transition ${
                      settings?.desktop_companion_enabled
                        ? "bg-[#eef5ff] text-[#0b57d0]"
                        : "text-[#59616d] hover:bg-[#eef1f5]"
                    } disabled:cursor-not-allowed disabled:opacity-40`}
                    aria-label={t("desktopCompanion.summon")}
                    title={t("desktopCompanion.summon")}
                  >
                    <PinIcon size={16} />
                  </button>

                  <button
                    type="button"
                    onClick={() => {
                      setVoicePanelOpen(true);
                      if (!voiceCallActive) startVoiceCall();
                    }}
                    disabled={!settings?.voice_enabled}
                    className={`md-icon-button grid h-8 w-8 place-items-center rounded-md transition ${
                      voicePanelOpen
                        ? "bg-[#eef5ff] text-[#0b57d0]"
                        : "text-[#59616d] hover:bg-[#eef1f5]"
                    } disabled:cursor-not-allowed disabled:opacity-40`}
                    aria-label={t("voice.call.title")}
                    aria-pressed={voicePanelOpen}
                    title={t("voice.call.title")}
                  >
                    <PhoneIcon size={16} />
                  </button>

                  <button
                    type="button"
                    onClick={toggleSpokenReplies}
                    disabled={!ttsQueue.available}
                    className={`app-toolbar-secondary md-icon-button grid h-8 w-8 place-items-center rounded-md transition ${
                      spokenRepliesEnabled
                        ? "bg-[#eef5ff] text-[#0b57d0]"
                        : "text-[#59616d] hover:bg-[#eef1f5]"
                    } disabled:cursor-not-allowed disabled:opacity-40`}
                    aria-label={spokenRepliesEnabled ? t("voice.stopSpokenReplies") : t("voice.startSpokenReplies")}
                    aria-pressed={spokenRepliesEnabled}
                    title={spokenRepliesEnabled ? t("voice.stopSpokenReplies") : t("voice.startSpokenReplies")}
                  >
                    <VolumeIcon size={17} />
                  </button>

                  {(spokenRepliesEnabled || ttsQueue.status.speaking || ttsQueue.status.queued > 0) && (
                    <div className="hidden max-w-[180px] items-center gap-1 rounded-md border border-[#e2e5ea] bg-white px-2 py-1 text-[11px] text-[#6f7782] lg:flex">
                      <span
                        className={`size-1.5 rounded-full ${
                          ttsQueue.status.speaking ? "bg-[#177245]" : "bg-[#c7ccd4]"
                        }`}
                      />
                      <span className="truncate">
                        {ttsQueue.status.speaking
                          ? t("voice.speaking")
                          : t("voice.queue", { n: ttsQueue.status.queued })}
                      </span>
                    </div>
                  )}

                  <button
                    type="button"
                    onClick={() => void openWidgets()}
                    className="md-icon-button grid h-8 w-8 place-items-center rounded-md text-[#59616d] transition hover:bg-[#eef1f5]"
                    aria-label={t("header.toys")}
                    title={t("header.toys")}
                  >
                    <img src="/fortune-icon.png" alt="" className="size-5 object-contain" />
                  </button>
                </div>
              </header>

              <GoalBar goal={goalPanel} busy={interactionBusy} progress={goalProgress} onAction={handleGoalAction} />

              <VoiceCallPanel
                open={voicePanelOpen}
                active={voiceCallActive}
                muted={voiceCallMuted}
                busy={interactionBusy}
                startedAt={voiceCallStartedAt}
                settings={settings}
                ttsStatus={ttsQueue.status}
                characterName={packName}
                avatarUrl={packAvatar}
                onStart={startVoiceCall}
                onClose={closeVoicePanel}
                onMutedChange={setVoiceCallMuted}
                onTranscript={handleVoiceTranscript}
                onStopAudio={ttsQueue.stop}
              />

              <div className="app-chat-body flex min-h-0 min-w-0 flex-1">
                <div className="flex min-h-0 min-w-0 flex-1 flex-col">
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
                    loading={interactionBusy}
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
                    onOpenWorkspace={() => setWorkspacePanelOpen(true)}
                    onWorkspaceChange={handleWorkspaceChange}
                    onRefreshWorkspace={() => setWorkspaceRefreshKey((value) => value + 1)}
                    textareaRef={textareaRef}
                    onSubmit={(attachments) => handleSend(undefined, attachments)}
                    onStop={() => {
                      handleInterrupt();
                    }}
                    onInputChange={setInput}
                  />
                </div>

                <WorkspaceExplorer
                  open={workspacePanelOpen}
                  workspace={workspace}
                  busy={interactionBusy}
                  refreshKey={workspaceRefreshKey}
                  onClose={() => setWorkspacePanelOpen(false)}
                  onWorkspaceChange={handleWorkspaceChange}
                />
              </div>
            </>
          ) : activeView === "media" ? (
            <MediaStudio settings={settings} onOpenSettings={() => openSettings("media")} />
          ) : activeView === "skills" ? (
            <IntegrationCenter onSessionImported={() => refreshSessions()} />
          ) : activeView === "live2d" ? (
            null
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
              onPreviewAppearance={setPreviewAppearance}
              onPacksChange={setPacks}
              onAgentPanelChange={setAgentPanel}
            />
          ) : null}
          {settings && live2dMounted && (
            <div
              className={`live2d-keep-alive min-h-0 flex-1 ${
                activeView === "live2d" ? "flex" : "hidden"
              }`}
            >
              <Suspense
                fallback={
                  <div className="grid flex-1 place-items-center text-[13px] text-[#8a9099]">
                    {t("live2d.loading")}
                  </div>
                }
              >
                <Live2DPanel
                  packId={settings.current_pack}
                  active={activeView === "live2d"}
                  onOpenSettings={() => openSettings("persona")}
                />
              </Suspense>
            </div>
          )}
        </div>
      </section>
      </div>

      <ConfirmDialog req={confirmReq} mode={settings?.permission_mode ?? "default"} onRespond={handleRespondConfirm} />
      <FortuneDialog open={fortuneOpen} onClose={() => setFortuneOpen(false)} />
      <WorkflowsPanel
        open={workflowOpen}
        busy={interactionBusy}
        onClose={() => setWorkflowOpen(false)}
        onResume={(command) => {
          setWorkflowOpen(false);
          void handleSend(command);
        }}
      />
    </main>
  );
}
