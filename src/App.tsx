import { useEffect, useMemo, useRef, useState } from "react";
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
  SessionEnginePanelState,
  SessionMeta,
  Settings,
} from "./lib/types";
import { MessageList } from "./components/MessageList";
import { Sidebar, type AppView } from "./components/Sidebar";
import { Composer } from "./components/Composer";
import GoalBar, { type GoalAction } from "./components/GoalBar";
import ConfirmDialog from "./components/ConfirmDialog";
import SettingsDialog from "./components/SettingsDialog";
import WorkflowsPanel from "./components/WorkflowsPanel";
import MediaStudio from "./components/MediaStudio";
import { CheckIcon, ChevronDownIcon, PanelLeftIcon, WrenchIcon } from "./components/Icons";
import Select from "./components/Select";
import { attachmentKindLabel, buildAttachmentPrompt, formatAttachmentSize, type ProcessedAttachment } from "./lib/fileProcessing";

const SUGGESTIONS = [
  "What time is it? Then suggest what I should do today.",
  "Create a notes.txt file in the sandbox and write a few notes.",
  "Search for recent AI news worth paying attention to.",
  "Introduce yourself and explain what you can do.",
];

const DEFAULT_WINDOW_SIZE = { width: 1811, height: 1213 };

// Browser-preview fallback so the UI renders without the Tauri backend (dev/screenshots only).
const PREVIEW_SETTINGS: Settings = {
  provider: "deepseek",
  permission_mode: "default",
  base_url: "https://api.deepseek.com/v1",
  api_key: "",
  model: "deepseek-chat",
  current_pack: "default",
  max_context_chars: 24000,
  max_input_tokens: 32000,
  reserved_output_tokens: 4000,
  auto_memory_enabled: true,
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

const PERMISSION_MODE_LABELS: Record<PermissionMode, string> = {
  plan: "Plan",
  default: "Default",
  auto: "Auto",
  bypass: "Bypass",
};

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
  const [items, setItems] = useState<DisplayItem[]>([]);
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [packs, setPacks] = useState<PackManifest[]>([]);
  const [agentPanel, setAgentPanel] = useState<AgentPanelState>({ definitions: [], agents_dir: "" });
  const [goalPanel, setGoalPanel] = useState<GoalPanelState | null>(null);
  const [goalProgress, setGoalProgress] = useState<GoalProgressEvent | null>(null);
  const [sessionEngine, setSessionEngine] = useState<SessionEnginePanelState | null>(null);
  const [selectedAgentNames, setSelectedAgentNames] = useState<string[]>([]);
  const [sessions, setSessions] = useState<SessionMeta[]>([]);
  const [activeId, setActiveId] = useState("");
  const [activeView, setActiveView] = useState<AppView>("chat");
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [packMenuOpen, setPackMenuOpen] = useState(false);
  const [agentMenuOpen, setAgentMenuOpen] = useState(false);
  const [workflowsOpen, setWorkflowsOpen] = useState(false);
  const [confirmReq, setConfirmReq] = useState<ConfirmRequestEvent | null>(null);
  const [planState, setPlanState] = useState<PlanState>({ active: false, approved: false });

  const seq = useRef(0);
  const genId = () => `it_${++seq.current}`;
  const curAssistantId = useRef<string | null>(null);
  const toolItemIds = useRef<Map<string, string>>(new Map());
  const lastRetryText = useRef<string>("");
  const assistantErrorDelivered = useRef(false);
  const packMenuRef = useRef<HTMLDivElement | null>(null);
  const agentMenuRef = useRef<HTMLDivElement | null>(null);
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);

  const activeSession = useMemo(() => sessions.find((s) => s.id === activeId) ?? null, [activeId, sessions]);
  const agentsDir = agentPanel.agents_dir || ".demiurge/agents";
  const appBusy = busy || sessionEngine?.busy === true;
  const runtimeStatus = sessionEngine?.cancel_requested
    ? "Cancelling current turn"
    : sessionEngine?.active_turn
      ? sessionEngine.active_turn.status === "cancelling"
        ? "Cancelling current turn"
        : "Processing current turn"
      : "Ready";

  const currentPack = useMemo(
    () => packs.find((x) => x.id === settings?.current_pack) ?? null,
    [packs, settings?.current_pack],
  );
  const packName = currentPack?.name ?? settings?.current_pack ?? "Demiurge";
  const packAvatar = currentPack?.avatarDataUrl;

  const selectedAgentLabel = useMemo(() => {
    if (selectedAgentNames.length === 0) return "Agents";
    if (selectedAgentNames.length === 1) return selectedAgentNames[0];
    return `${selectedAgentNames.length} Agents`;
  }, [selectedAgentNames]);

  useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window)) return;
    const appWindow = getCurrentWindow();
    void appWindow
      .setSize(new PhysicalSize(DEFAULT_WINDOW_SIZE.width, DEFAULT_WINDOW_SIZE.height))
      .then(() => appWindow.center())
      .catch((e) => console.warn("Failed to apply default window size", e));
  }, []);

  useEffect(() => {
    (async () => {
      try {
        const [s, ps, agents, goal, list, hist, plan, engine] = await Promise.all([
          api.getSettings(),
          api.listPacks(),
          api.agentPanelState(),
          api.goalPanelState(),
          api.listSessions(),
          api.getHistory(),
          api.planState(),
          api.sessionEngineState(),
        ]);
        setSettings(s);
        setPacks(ps);
        setAgentPanel(agents);
        setGoalPanel(goal);
        setSessions(list.sessions);
        setActiveId(list.active);
        setItems(buildHistory(hist));
        setPlanState(plan);
        setSessionEngine(engine);
        setBusy(engine.busy);
      } catch (e) {
        console.error("Failed to initialize Demiurge", e);
        // Outside Tauri (browser preview), seed defaults so the UI is still browsable.
        if (!("__TAURI_INTERNALS__" in window)) {
          setSettings((prev) => prev ?? PREVIEW_SETTINGS);
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

    const finalizeAssistant = () => {
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
          const id = curAssistantId.current;
          if (!id) {
            const nid = genId();
            curAssistantId.current = nid;
            setItems((p) => [...p, { id: nid, kind: "assistant", text, streaming: true }]);
          } else {
            setItems((p) =>
              p.map((it) => (it.id === id && it.kind === "assistant" ? { ...it, text: it.text + text } : it)),
            );
          }
        },
        onAssistantDone: (text) => {
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
      });

    let unPlan: UnlistenFn | undefined;
    let unMode: UnlistenFn | undefined;
    let unSessionEngine: UnlistenFn | undefined;
    api.listenPlanUpdated(setPlanState).then((u) => {
      if (disposed) u();
      else unPlan = u;
    });
    api.listenPermissionModeUpdated((mode) => {
      setSettings((prev) => (prev ? { ...prev, permission_mode: mode } : prev));
    }).then((u) => {
      if (disposed) u();
      else unMode = u;
    });
    api.listenSessionEngineUpdated((next) => {
      setSessionEngine(next);
      setBusy(next.busy);
    }).then((u) => {
      if (disposed) u();
      else unSessionEngine = u;
    });

    return () => {
      disposed = true;
      un?.();
      unPlan?.();
      unMode?.();
      unSessionEngine?.();
    };
  }, []);

  useEffect(() => {
    if (!packMenuOpen) return;
    const onDown = (e: MouseEvent) => {
      if (!packMenuRef.current?.contains(e.target as Node)) setPackMenuOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [packMenuOpen]);

  useEffect(() => {
    if (!agentMenuOpen) return;
    const onDown = (e: MouseEvent) => {
      if (!agentMenuRef.current?.contains(e.target as Node)) setAgentMenuOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [agentMenuOpen]);

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
    } catch (e) {
      console.error("Failed to save settings", e);
    }
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

  const last = items[items.length - 1];
  const tailStreaming = last?.kind === "assistant" && last.streaming;
  const tailToolRunning = last?.kind === "tool" && last.status === "running";
  const thinking = appBusy && !tailStreaming && !tailToolRunning;
  const canSend = input.trim().length > 0 && !appBusy;

  return (
    <main className="flex h-[100dvh] overflow-hidden bg-[#eef1f5] text-[#202124]">
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
        onOpenSandbox={() => void api.openSandbox()}
        onOpenSettings={() => setActiveView("settings")}
      />

      <section className="flex min-h-0 min-w-0 flex-1 flex-col p-2 pl-0">
        <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden rounded-lg border border-[#dfe3e8] bg-white shadow-[0_1px_2px_rgba(15,23,42,0.06)]">
          {activeView === "chat" ? (
            <>
              <header className="flex h-12 shrink-0 items-center gap-2 border-b border-[#eceff3] bg-[#fbfcfd] px-3">
                <button
                  onClick={() => setSidebarOpen(true)}
                  aria-label="Open sidebar"
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
                    <div className="cf-pop cf-pop-down absolute left-0 top-10 z-20 max-h-[70vh] w-64 overflow-y-auto rounded-lg border border-[#dfe3e8] bg-white p-1.5 shadow-[0_16px_48px_rgba(15,23,42,0.16)]">
                      {packs.length === 0 && <div className="px-3 py-2 text-sm text-[#9a9a9a]">No persona packs found</div>}
                      {packs.map((p) => (
                        <button
                          key={p.id}
                          onClick={() => handleSelectPack(p.id)}
                          className={`flex w-full items-center justify-between gap-2 rounded-md px-2.5 py-2 text-left text-[13px] transition hover:bg-[#f6f7f9] ${
                            settings?.current_pack === p.id ? "bg-[#eef1f5]" : ""
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

                <div ref={agentMenuRef} className="relative">
                  <button
                    onClick={() => setAgentMenuOpen((v) => !v)}
                    className={`flex items-center gap-1 rounded-md px-2.5 py-2 text-sm font-medium transition hover:bg-[#eef1f5] ${
                      selectedAgentNames.length ? "text-[#171717]" : "text-[#6f7782]"
                    }`}
                  >
                    {selectedAgentLabel}
                    <ChevronDownIcon
                      size={16}
                      className={`text-[#9a9a9a] transition-transform duration-200 ${agentMenuOpen ? "rotate-180" : ""}`}
                    />
                  </button>
                  {agentMenuOpen && (
                    <div className="cf-pop cf-pop-down absolute left-0 top-11 z-20 max-h-[70vh] w-80 overflow-y-auto rounded-lg border border-[#dfe3e8] bg-white p-2 shadow-[0_16px_48px_rgba(15,23,42,0.16)]">
                      <div className="border-b border-[#eef1f4] px-3 py-2">
                        <div className="text-[11px] font-semibold uppercase tracking-wide text-[#8a9099]">
                          Agents folder
                        </div>
                        <div className="mt-1 truncate text-xs text-[#6f7782]" title={`${agentsDir}\\*.json`}>
                          {agentsDir}\*.json
                        </div>
                      </div>
                      {agentPanel.definitions.length === 0 && (
                        <div className="px-3 py-3 text-sm text-[#9a9a9a]">
                          No custom agents found. Add JSON files in the agents folder.
                        </div>
                      )}
                      {agentPanel.definitions.map((agent) => {
                        const selected = selectedAgentNames.includes(agent.name);
                        return (
                          <button
                            key={agent.name}
                            onClick={() => toggleAgent(agent.name)}
                            className={`flex w-full items-start justify-between gap-2 rounded-md px-3 py-3 text-left text-sm transition hover:bg-[#f6f7f9] ${
                              selected ? "bg-[#eef1f5]" : ""
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
                          className="mt-1 w-full rounded-md px-3 py-2 text-left text-xs text-[#8a8a8a] transition hover:bg-[#f6f7f9]"
                        >
                          Clear selection
                        </button>
                      )}
                    </div>
                  )}
                </div>

                <div className="hidden min-w-0 flex-col border-l border-[#dfe3e8] pl-3 text-[11px] text-[#8a9099] sm:flex">
                  <span className="max-w-[28vw] truncate font-medium text-[#3f3f3f]" title={activeSession?.title ?? "New chat"}>
                    {activeSession?.title ?? "New chat"}
                  </span>
                  <span>{runtimeStatus}</span>
                </div>


                <div className="ml-auto flex items-center gap-2">
                  <Select
                    value={settings?.permission_mode ?? "default"}
                    onChange={(v) => void handleSetPermissionMode(v as PermissionMode)}
                    align="right"
                    options={(Object.keys(PERMISSION_MODE_LABELS) as PermissionMode[]).map((mode) => ({
                      value: mode,
                      label: PERMISSION_MODE_LABELS[mode],
                    }))}
                    triggerClassName={`flex h-8 items-center gap-1.5 rounded-md border px-2.5 text-xs font-medium outline-none transition ${
                      settings?.permission_mode === "bypass"
                        ? "border-[#f4b4b4] bg-[#fff0f0] text-[#b42318]"
                        : settings?.permission_mode === "plan"
                          ? "border-[#b8d4ff] bg-[#eef5ff] text-[#0b57d0]"
                          : "border-[#dfe3e8] bg-white text-[#4f5661] hover:bg-[#f6f7f9]"
                    }`}
                  />
                  {planState.path && !planState.approved && (
                    <div className="hidden items-center gap-1 rounded-md border border-[#b8d4ff] bg-[#eef5ff] px-2 py-1 text-xs text-[#0b57d0] lg:flex">
                      <span className="max-w-[18vw] truncate" title={planState.path}>
                        Plan ready: {planState.path}
                      </span>
                      <button className="rounded bg-[#0b57d0] px-2 py-1 text-white" onClick={() => void handleApprovePlan()}>
                        Approve
                      </button>
                      <button className="rounded px-2 py-1 text-[#5f6368] hover:bg-white" onClick={() => void handleRejectPlan()}>
                        Reject
                      </button>
                    </div>
                  )}
                </div>

                <button
                  onClick={() => setWorkflowsOpen(true)}
                  title="Workflows"
                  className="inline-flex h-8 shrink-0 items-center gap-2 rounded-md px-2.5 text-[12px] font-medium text-[#4f5661] transition hover:bg-[#eef1f5]"
                >
                  <WrenchIcon size={17} />
                  <span className="hidden sm:inline">Workflows</span>
                </button>
              </header>

              <GoalBar goal={goalPanel} busy={appBusy} progress={goalProgress} onAction={handleGoalAction} />

              <MessageList
                items={items}
                thinking={thinking}
                greeting="How can I help?"
                suggestions={SUGGESTIONS}
                onSuggestionClick={(t) => void handleSend(t)}
                onRetry={(t) => void handleSend(t)}
              />

              <Composer
                input={input}
                canSend={canSend}
                loading={appBusy}
                textareaRef={textareaRef}
                onSubmit={(attachments) => handleSend(undefined, attachments)}
                onStop={() => {
                  void api.interrupt();
                  setConfirmReq(null);
                }}
                onInputChange={setInput}
                onOpenSandbox={() => void api.openSandbox()}
              />
            </>
          ) : activeView === "media" ? (
            <MediaStudio settings={settings} onOpenSettings={() => setActiveView("settings")} />
          ) : settings ? (
            <SettingsDialog
              open
              settings={settings}
              packs={packs}
              agentPanel={agentPanel}
              onClose={() => setActiveView("chat")}
              onSave={handleSaveSettings}
              onPacksChange={setPacks}
              onAgentPanelChange={setAgentPanel}
            />
          ) : null}
        </div>
      </section>

      <WorkflowsPanel
        open={workflowsOpen}
        busy={appBusy}
        onClose={() => setWorkflowsOpen(false)}
        onResume={(command) => void handleSend(command)}
      />
      <ConfirmDialog req={confirmReq} mode={settings?.permission_mode ?? "default"} onRespond={handleRespondConfirm} />
    </main>
  );
}
