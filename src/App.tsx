import { useEffect, useMemo, useRef, useState } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow, PhysicalSize } from "@tauri-apps/api/window";
import * as api from "./lib/api";
import type {
  AgentPanelState,
  ConfirmRequestEvent,
  DisplayItem,
  GoalPanelState,
  GoalProgressEvent,
  Message,
  PackManifest,
  PermissionScope,
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
import { attachmentKindLabel, buildAttachmentPrompt, formatAttachmentSize, type ProcessedAttachment } from "./lib/fileProcessing";

const SUGGESTIONS = [
  "What time is it? Then suggest what I should do today.",
  "Create a notes.txt file in the sandbox and write a few notes.",
  "Search for recent AI news worth paying attention to.",
  "Introduce yourself and explain what you can do.",
];

const DEFAULT_WINDOW_SIZE = { width: 1811, height: 1213 };

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
  const [selectedAgentNames, setSelectedAgentNames] = useState<string[]>([]);
  const [sessions, setSessions] = useState<SessionMeta[]>([]);
  const [activeId, setActiveId] = useState("");
  const [activeView, setActiveView] = useState<AppView>("chat");
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [packMenuOpen, setPackMenuOpen] = useState(false);
  const [agentMenuOpen, setAgentMenuOpen] = useState(false);
  const [workflowsOpen, setWorkflowsOpen] = useState(false);
  const [confirmReq, setConfirmReq] = useState<ConfirmRequestEvent | null>(null);

  const seq = useRef(0);
  const genId = () => `it_${++seq.current}`;
  const curAssistantId = useRef<string | null>(null);
  const toolItemIds = useRef<Map<string, string>>(new Map());
  const packMenuRef = useRef<HTMLDivElement | null>(null);
  const agentMenuRef = useRef<HTMLDivElement | null>(null);
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);

  const activeSession = useMemo(() => sessions.find((s) => s.id === activeId) ?? null, [activeId, sessions]);
  const agentsDir = agentPanel.agents_dir || ".demiurge/agents";

  const packName = useMemo(() => {
    const p = packs.find((x) => x.id === settings?.current_pack);
    return p?.name ?? settings?.current_pack ?? "Demiurge";
  }, [packs, settings]);

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
        const [s, ps, agents, goal, list, hist] = await Promise.all([
          api.getSettings(),
          api.listPacks(),
          api.agentPanelState(),
          api.goalPanelState(),
          api.listSessions(),
          api.getHistory(),
        ]);
        setSettings(s);
        setPacks(ps);
        setAgentPanel(agents);
        setGoalPanel(goal);
        setSessions(list.sessions);
        setActiveId(list.active);
        setItems(buildHistory(hist));
      } catch (e) {
        console.error("Failed to initialize Demiurge", e);
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
                ? { ...it, status: e.denied ? "denied" : e.ok ? "done" : "failed", result: e.result }
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

    return () => {
      disposed = true;
      un?.();
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
    if ((!text && !attachmentPrompt) || busy) return false;
    setInput("");
    setActiveView("chat");
    const uid = genId();
    setItems((p) => [...p, { id: uid, kind: "user", text: buildUserDisplayText(text, attachments) }]);
    setBusy(true);
    try {
      const prompt = `${text || "Please review the attached files."}${attachmentPrompt}`;
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
      const nid = genId();
      setItems((p) => [...p, { id: nid, kind: "assistant", text: `Warning: ${String(err)}`, streaming: false, error: true }]);
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
    if ((action === "resume" || action === "continue") && busy) return;
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
    if (busy) return;
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
    if (busy || id === activeId) return;
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
    if (busy) return;
    const renamed = await api.renameSession(id, title);
    setSessions((prev) =>
      prev
        .map((s) => (s.id === id ? { ...s, title: renamed, updated_at: Date.now() } : s))
        .sort((a, b) => b.updated_at - a.updated_at),
    );
    await refreshSessions();
  }

  async function handleDeleteSession(id: string) {
    if (busy) return;
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
      setSettingsOpen(false);
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

  function toggleAgent(name: string) {
    setSelectedAgentNames((prev) =>
      prev.includes(name) ? prev.filter((item) => item !== name) : [...prev, name],
    );
  }

  const last = items[items.length - 1];
  const tailStreaming = last?.kind === "assistant" && last.streaming;
  const tailToolRunning = last?.kind === "tool" && last.status === "running";
  const thinking = busy && !tailStreaming && !tailToolRunning;
  const canSend = input.trim().length > 0 && !busy;

  return (
    <main className="flex h-[100dvh] overflow-hidden bg-[#eef1f5] text-[#202124]">
      <Sidebar
        open={sidebarOpen}
        activeView={activeView}
        packName={packName}
        sessions={sessions}
        activeId={activeId}
        busy={busy}
        onToggle={() => setSidebarOpen((v) => !v)}
        onViewChange={setActiveView}
        onNewChat={handleNewChat}
        onSelectSession={handleSelectSession}
        onRenameSession={handleRenameSession}
        onDeleteSession={handleDeleteSession}
        onOpenSandbox={() => void api.openSandbox()}
        onOpenSettings={() => setSettingsOpen(true)}
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
                    className="flex h-8 items-center gap-1 rounded-md px-2 text-[14px] font-semibold text-[#202124] transition hover:bg-[#eef1f5]"
                  >
                    {packName}
                    <ChevronDownIcon
                      size={18}
                      className={`text-[#9a9a9a] transition-transform duration-200 ${packMenuOpen ? "rotate-180" : ""}`}
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
                          <span>
                            <span className="block font-medium">{p.name}</span>
                            <span className="block text-xs text-[#8a8a8a]">Pack / {p.id}</span>
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
                  <span>{busy ? "Processing current turn" : "Ready"}</span>
                </div>

                <button
                  onClick={() => setWorkflowsOpen(true)}
                  title="Workflows"
                  className="ml-auto inline-flex h-8 shrink-0 items-center gap-2 rounded-md px-2.5 text-[12px] font-medium text-[#4f5661] transition hover:bg-[#eef1f5]"
                >
                  <WrenchIcon size={17} />
                  <span className="hidden sm:inline">Workflows</span>
                </button>
              </header>

              <GoalBar goal={goalPanel} busy={busy} progress={goalProgress} onAction={handleGoalAction} />

              <MessageList
                items={items}
                thinking={thinking}
                greeting="How can I help?"
                suggestions={SUGGESTIONS}
                onSuggestionClick={(t) => void handleSend(t)}
              />

              <Composer
                input={input}
                canSend={canSend}
                loading={busy}
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
          ) : (
            <MediaStudio settings={settings} onOpenSettings={() => setSettingsOpen(true)} />
          )}
        </div>
      </section>

      <WorkflowsPanel
        open={workflowsOpen}
        busy={busy}
        onClose={() => setWorkflowsOpen(false)}
        onResume={(command) => void handleSend(command)}
      />
      <ConfirmDialog req={confirmReq} onRespond={handleRespondConfirm} />
      {settings && (
        <SettingsDialog
          open={settingsOpen}
          settings={settings}
          packs={packs}
          agentPanel={agentPanel}
          onClose={() => setSettingsOpen(false)}
          onSave={handleSaveSettings}
          onAgentPanelChange={setAgentPanel}
        />
      )}
    </main>
  );
}
