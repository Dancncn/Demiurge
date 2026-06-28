import { useEffect, useState } from "react";
import type { SessionMeta } from "../lib/types";
import { ComposeIcon, FolderIcon, PanelLeftIcon, SettingsIcon, TrashIcon } from "./Icons";

type Props = {
  open: boolean;
  packName: string;
  sessions: SessionMeta[];
  activeId: string;
  busy: boolean;
  onToggle: () => void;
  onNewChat: () => void;
  onSelectSession: (id: string) => void;
  onRenameSession: (id: string, title: string) => Promise<void> | void;
  onDeleteSession: (id: string) => void;
  onOpenSandbox: () => void;
  onOpenSettings: () => void;
};

export function Sidebar({
  open,
  packName,
  sessions,
  activeId,
  busy,
  onToggle,
  onNewChat,
  onSelectSession,
  onRenameSession,
  onDeleteSession,
  onOpenSandbox,
  onOpenSettings,
}: Props) {
  const [editingId, setEditingId] = useState<string | null>(null);
  const [draftTitle, setDraftTitle] = useState("");
  const [renameError, setRenameError] = useState<string | null>(null);

  useEffect(() => {
    if (!sessions.some((s) => s.id === editingId)) {
      setEditingId(null);
      setDraftTitle("");
      setRenameError(null);
    }
  }, [editingId, sessions]);

  function beginRename(session: SessionMeta) {
    if (busy) return;
    setEditingId(session.id);
    setDraftTitle(session.title);
    setRenameError(null);
  }

  async function commitRename(id: string) {
    const title = draftTitle.trim();
    if (!title) {
      setRenameError("标题不能为空");
      return;
    }
    const current = sessions.find((s) => s.id === id);
    if (current && current.title === title) {
      setEditingId(null);
      setRenameError(null);
      return;
    }
    try {
      await onRenameSession(id, title);
      setEditingId(null);
      setRenameError(null);
    } catch (e) {
      setRenameError(String(e));
    }
  }

  return (
    <>
      {open && <div onClick={onToggle} aria-hidden className="fixed inset-0 z-30 bg-black/20 md:hidden" />}
      <aside
        className={`fixed inset-y-0 left-0 z-40 flex w-[260px] flex-col border-r border-[#ededed] bg-[#f9f9f7] px-3 py-3 shadow-[12px_0_32px_rgba(0,0,0,0.08)] transition-transform duration-200 md:relative md:z-auto md:shrink-0 md:translate-x-0 md:shadow-none md:transition-[width] ${
          open ? "translate-x-0" : "-translate-x-full"
        } ${open ? "md:w-[230px]" : "md:w-[64px]"}`}
      >
        <div className={`mb-4 flex items-center ${open ? "justify-between px-1" : "justify-center"}`}>
          <button
            onClick={onToggle}
            className="grid h-9 w-9 shrink-0 place-items-center rounded-lg text-[#3f3f3f] transition hover:bg-[#ececea]"
            aria-label="折叠侧边栏"
          >
            <PanelLeftIcon size={20} />
          </button>
          <button
            onClick={onNewChat}
            className={`grid h-9 w-9 place-items-center rounded-lg text-[#3f3f3f] transition hover:bg-[#ececea] ${open ? "" : "hidden"}`}
            aria-label="新建对话"
          >
            <ComposeIcon size={19} />
          </button>
        </div>

        <button
          onClick={onNewChat}
          className={`mb-5 flex h-10 items-center gap-3 rounded-lg text-left text-sm hover:bg-[#ececea] ${open ? "px-3" : "justify-center px-0"}`}
        >
          <img src="/demiurge.png" alt="Demiurge" className="size-8 shrink-0 rounded-full bg-[#faf8fd] object-contain" />
          {open && <span>新对话</span>}
        </button>

        <div className={`min-h-0 flex-1 overflow-y-auto ${open ? "" : "hidden"}`}>
          <div className="px-3 pb-2 text-xs font-medium text-[#8a8a8a]">对话</div>
          {sessions.length === 0 && <div className="px-3 py-2 text-sm text-[#b4b4b4]">还没有对话</div>}
          {sessions.map((s) => {
            const editing = editingId === s.id;
            return (
              <div
                key={s.id}
                className={`group relative mb-1 rounded-lg ${s.id === activeId ? "bg-[#ececea]" : "hover:bg-[#ececea]"}`}
              >
                <div className="flex items-center">
                  {editing ? (
                    <input
                      autoFocus
                      value={draftTitle}
                      disabled={busy}
                      onChange={(e) => {
                        setDraftTitle(e.target.value);
                        setRenameError(null);
                      }}
                      onBlur={() => void commitRename(s.id)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") {
                          e.preventDefault();
                          void commitRename(s.id);
                        } else if (e.key === "Escape") {
                          setEditingId(null);
                          setRenameError(null);
                        }
                      }}
                      className="mx-1 min-w-0 flex-1 rounded-md border border-[#d8d8d6] bg-white px-2 py-1.5 text-sm outline-none focus:border-[#171717] disabled:opacity-60"
                      aria-label="会话标题"
                    />
                  ) : (
                    <button
                      onClick={() => onSelectSession(s.id)}
                      onDoubleClick={() => beginRename(s)}
                      disabled={busy}
                      className="min-w-0 flex-1 truncate px-3 py-2 text-left text-sm disabled:cursor-not-allowed disabled:opacity-60"
                      title={`${s.title}\n双击重命名`}
                    >
                      {s.title}
                    </button>
                  )}
                  {!editing && (
                    <button
                      onClick={() => beginRename(s)}
                      disabled={busy}
                      className="grid h-7 w-7 shrink-0 place-items-center rounded-md text-[#6f6f6f] opacity-0 transition hover:bg-[#dededc] hover:text-[#111] group-hover:opacity-100 disabled:opacity-0"
                      aria-label="重命名对话"
                      title="重命名"
                    >
                      <ComposeIcon size={14} />
                    </button>
                  )}
                  <button
                    onClick={() => onDeleteSession(s.id)}
                    disabled={busy || editing}
                    className="mr-1 grid h-7 w-7 shrink-0 place-items-center rounded-md text-[#6f6f6f] opacity-0 transition hover:bg-[#dededc] hover:text-[#dc2626] group-hover:opacity-100 disabled:opacity-0"
                    aria-label="删除对话"
                  >
                    <TrashIcon size={15} />
                  </button>
                </div>
                {editing && renameError && <div className="px-3 pb-2 text-xs text-[#dc2626]">{renameError}</div>}
              </div>
            );
          })}

          <div className="mt-4 px-3 pb-2 text-xs font-medium text-[#8a8a8a]">工具</div>
          <button
            onClick={onOpenSandbox}
            className="flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-left text-sm transition hover:bg-[#ececea]"
          >
            <FolderIcon size={16} className="text-[#8a8a8a]" />
            打开沙盒目录
          </button>
        </div>

        <div className="border-t border-[#ededed] pt-3">
          <button
            onClick={onOpenSettings}
            className={`flex w-full items-center gap-3 rounded-lg py-2 text-left text-sm transition hover:bg-[#ececea] ${open ? "px-2" : "justify-center px-0"}`}
            aria-label="设置"
          >
            {open ? (
              <>
                <img src="/demiurge.png" alt="Demiurge" className="size-8 shrink-0 rounded-full bg-[#faf8fd] object-contain" />
                <span className="min-w-0 flex-1 truncate text-[#3f3f3f]">{packName}</span>
                <SettingsIcon size={17} className="shrink-0 text-[#8a8a8a]" />
              </>
            ) : (
              <SettingsIcon size={19} className="text-[#3f3f3f]" />
            )}
          </button>
        </div>
      </aside>
    </>
  );
}
