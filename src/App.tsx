import { useEffect, useMemo, useRef, useState } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import * as api from "./lib/api";
import type {
  ConfirmRequestEvent,
  DisplayItem,
  Message,
  PackManifest,
  PermissionScope,
  SessionMeta,
  Settings,
} from "./lib/types";
import { MessageList } from "./components/MessageList";
import { Sidebar } from "./components/Sidebar";
import { Composer } from "./components/Composer";
import ConfirmDialog from "./components/ConfirmDialog";
import SettingsDialog from "./components/SettingsDialog";
import WorkflowsPanel from "./components/WorkflowsPanel";
import { CheckIcon, ChevronDownIcon, PanelLeftIcon, WrenchIcon } from "./components/Icons";

const SUGGESTIONS = [
  "现在几点了？顺便说说今天该做点什么",
  "帮我在沙盒里建一个 notes.txt 记点东西",
  "搜索一下最近有什么值得关注的 AI 新闻",
  "介绍一下你自己吧",
];

// 把持久化的消息历史还原成展示项（恢复上次会话）
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
      out.push({ id: id(), kind: "user", text: m.content ?? "" });
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

export default function App() {
  const [items, setItems] = useState<DisplayItem[]>([]);
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [packs, setPacks] = useState<PackManifest[]>([]);
  const [sessions, setSessions] = useState<SessionMeta[]>([]);
  const [activeId, setActiveId] = useState("");
  const [sidebarOpen, setSidebarOpen] = useState(true);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [packMenuOpen, setPackMenuOpen] = useState(false);
  const [workflowsOpen, setWorkflowsOpen] = useState(false);
  const [confirmReq, setConfirmReq] = useState<ConfirmRequestEvent | null>(null);

  const seq = useRef(0);
  const genId = () => `it_${++seq.current}`;
  const curAssistantId = useRef<string | null>(null);
  const toolItemIds = useRef<Map<string, string>>(new Map());
  const packMenuRef = useRef<HTMLDivElement | null>(null);
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);

  const packName = useMemo(() => {
    const p = packs.find((x) => x.id === settings?.current_pack);
    return p?.name ?? settings?.current_pack ?? "Demiurge";
  }, [packs, settings]);

  // 初次加载
  useEffect(() => {
    (async () => {
      try {
        const [s, ps, list, hist] = await Promise.all([
          api.getSettings(),
          api.listPacks(),
          api.listSessions(),
          api.getHistory(),
        ]);
        setSettings(s);
        setPacks(ps);
        setSessions(list.sessions);
        setActiveId(list.active);
        setItems(buildHistory(hist));
      } catch (e) {
        console.error("初始化失败", e);
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

  // 注册 agent 事件（兼容 StrictMode 双挂载）
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
        },
        onAssistantInterrupted: () => {
          finalizeAssistant();
          setBusy(false);
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
                ? { ...it, status: e.ok ? "done" : "denied", result: e.result }
                : it,
            ),
          );
        },
        onConfirmRequest: (e) => setConfirmReq(e),
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

  // 角色包下拉的外部点击关闭
  useEffect(() => {
    if (!packMenuOpen) return;
    const onDown = (e: MouseEvent) => {
      if (!packMenuRef.current?.contains(e.target as Node)) setPackMenuOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [packMenuOpen]);

  async function handleSend(textArg?: string) {
    const text = (textArg ?? input).trim();
    if (!text || busy) return;
    setInput("");
    const uid = genId();
    setItems((p) => [...p, { id: uid, kind: "user", text }]);
    setBusy(true);
    try {
      await api.send(text);
    } catch (err) {
      const id = curAssistantId.current;
      if (id) {
        setItems((p) => p.map((it) => (it.id === id && it.kind === "assistant" ? { ...it, streaming: false } : it)));
        curAssistantId.current = null;
      }
      const nid = genId();
      setItems((p) => [...p, { id: nid, kind: "assistant", text: `⚠️ ${String(err)}`, streaming: false, error: true }]);
    } finally {
      setBusy(false);
      void refreshSessions(); // 标题/排序可能因本轮更新
    }
  }

  async function handleRespondConfirm(allow: boolean, scope: PermissionScope) {
    if (!confirmReq) return;
    const id = confirmReq.id;
    setConfirmReq(null);
    try {
      await api.respondConfirm(id, allow, scope);
    } catch (e) {
      console.error("确认回执失败", e);
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
    await refreshSessions();
    requestAnimationFrame(() => textareaRef.current?.focus());
  }

  async function loadActiveHistory() {
    try {
      const hist = await api.getHistory();
      resetTurnRefs();
      setItems(buildHistory(hist));
    } catch (e) {
      console.error(e);
    }
  }

  async function handleSelectSession(id: string) {
    if (busy || id === activeId) return;
    try {
      await api.selectSession(id);
      setActiveId(id);
      await loadActiveHistory();
    } catch (e) {
      console.error(e);
    }
  }

  async function handleDeleteSession(id: string) {
    if (busy) return;
    try {
      await api.deleteSession(id);
      await refreshSessions();
      await loadActiveHistory();
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
      console.error("保存设置失败", e);
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

  function onKeyDown(e: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (e.nativeEvent.isComposing) return;
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      void handleSend();
    }
  }

  const last = items[items.length - 1];
  const tailStreaming = last?.kind === "assistant" && last.streaming;
  const tailToolRunning = last?.kind === "tool" && last.status === "running";
  const thinking = busy && !tailStreaming && !tailToolRunning;
  const canSend = input.trim().length > 0 && !busy;

  return (
    <main className="flex h-[100dvh] overflow-hidden bg-gradient-to-b from-[#fdfbff] to-[#f6f0fb] text-[#171717]">
      <Sidebar
        open={sidebarOpen}
        packName={packName}
        sessions={sessions}
        activeId={activeId}
        busy={busy}
        onToggle={() => setSidebarOpen((v) => !v)}
        onNewChat={handleNewChat}
        onSelectSession={handleSelectSession}
        onDeleteSession={handleDeleteSession}
        onOpenSandbox={() => void api.openSandbox()}
        onOpenSettings={() => setSettingsOpen(true)}
      />

      <section className="flex min-h-0 min-w-0 flex-1 flex-col">
        <header className="flex h-14 shrink-0 items-center gap-2 px-4">
          <button
            onClick={() => setSidebarOpen(true)}
            aria-label="打开侧边栏"
            className="grid h-9 w-9 shrink-0 place-items-center rounded-lg text-[#3f3f3f] transition hover:bg-[#f5f5f5] md:hidden"
          >
            <PanelLeftIcon size={20} />
          </button>

          <div ref={packMenuRef} className="relative">
            <button
              onClick={() => setPackMenuOpen((v) => !v)}
              className="flex items-center gap-1 rounded-lg px-3 py-2 text-lg font-semibold text-[#2b2b2b] transition hover:bg-[#f5f5f5]"
            >
              {packName}
              <ChevronDownIcon
                size={18}
                className={`text-[#9a9a9a] transition-transform duration-200 ${packMenuOpen ? "rotate-180" : ""}`}
              />
            </button>
            {packMenuOpen && (
              <div className="cf-pop cf-pop-down absolute left-0 top-12 z-20 max-h-[70vh] w-64 overflow-y-auto rounded-2xl border border-[#ececec] bg-white p-2 shadow-[0_16px_48px_rgba(0,0,0,0.16)]">
                {packs.length === 0 && <div className="px-3 py-2 text-sm text-[#9a9a9a]">未找到角色包</div>}
                {packs.map((p) => (
                  <button
                    key={p.id}
                    onClick={() => handleSelectPack(p.id)}
                    className={`flex w-full items-center justify-between gap-2 rounded-xl px-3 py-3 text-left text-sm transition hover:bg-[#f7f7f7] ${
                      settings?.current_pack === p.id ? "bg-[#f7f7f7]" : ""
                    }`}
                  >
                    <span>
                      <span className="block font-medium">{p.name}</span>
                      <span className="block text-xs text-[#8a8a8a]">角色包 · {p.id}</span>
                    </span>
                    {settings?.current_pack === p.id && <CheckIcon size={17} className="shrink-0 text-[#171717]" />}
                  </button>
                ))}
              </div>
            )}
          </div>

          <button
            onClick={() => setWorkflowsOpen(true)}
            title="Workflows"
            className="ml-auto inline-flex h-9 shrink-0 items-center gap-2 rounded-lg px-3 text-sm font-medium text-[#3f3f3f] transition hover:bg-[#f5f5f5]"
          >
            <WrenchIcon size={17} />
            <span className="hidden sm:inline">Workflows</span>
          </button>
        </header>

        <MessageList
          items={items}
          thinking={thinking}
          greeting="有什么可以帮忙的？"
          suggestions={SUGGESTIONS}
          onSuggestionClick={(t) => void handleSend(t)}
        />

        <Composer
          input={input}
          sidebarOpen={sidebarOpen}
          canSend={canSend}
          loading={busy}
          textareaRef={textareaRef}
          onSubmit={(e) => {
            e.preventDefault();
            void handleSend();
          }}
          onStop={() => {
            void api.interrupt();
            setConfirmReq(null);
          }}
          onInputChange={setInput}
          onKeyDown={onKeyDown}
          onOpenSandbox={() => void api.openSandbox()}
        />
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
          onClose={() => setSettingsOpen(false)}
          onSave={handleSaveSettings}
        />
      )}
    </main>
  );
}
