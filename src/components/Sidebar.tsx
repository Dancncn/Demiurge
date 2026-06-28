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
        className={`fixed inset-y-0 left-0 z-40 flex w-[260px] flex-col border-r border-[#dfe3e8] bg-[#eef1f5] px-2 py-2 shadow-[12px_0_32px_rgba(15,23,42,0.12)] transition-transform duration-200 md:relative md:z-auto md:shrink-0 md:translate-x-0 md:shadow-none md:transition-[width] ${
          open ? "translate-x-0" : "-translate-x-full"
        } ${open ? "md:w-[230px]" : "md:w-[64px]"}`}
      >
        <div className={`mb-3 flex h-9 items-center ${open ? "justify-between px-1" : "justify-center"}`}>
          <button
            onClick={onToggle}
            className="grid h-8 w-8 shrink-0 place-items-center rounded-md text-[#4f5661] transition hover:bg-[#dfe4ea]"
            aria-label="折叠侧边栏"
          >
            <PanelLeftIcon size={20} />
          </button>
          <button
            onClick={onNewChat}
            className={`grid h-8 w-8 place-items-center rounded-md text-[#4f5661] transition hover:bg-[#dfe4ea] ${open ? "" : "hidden"}`}
            aria-label="新建对话"
          >
            <ComposeIcon size={19} />
          </button>
        </div>

        <button
          onClick={onNewChat}
          className={`mb-4 flex h-9 items-center gap-2 rounded-md text-left text-[13px] text-[#202124] hover:bg-[#dfe4ea] ${open ? "px-2" : "justify-center px-0"}`}
        >
          <img src="/demiurge.png" alt="Demiurge" className="size-7 shrink-0 rounded-md border border-[#dfe3e8] bg-white object-contain" />
          {open && <span>新对话</span>}
        </button>

        <div className={`min-h-0 flex-1 overflow-y-auto ${open ? "" : "hidden"}`}>
          <div className="px-2 pb-1.5 text-[11px] font-semibold uppercase tracking-wide text-[#8a9099]">Chats</div>
          {sessions.length === 0 && <div className="px-2 py-2 text-[13px] text-[#9aa1ab]">还没有对话</div>}
          {sessions.map((s) => {
            const editing = editingId === s.id;
            return (
              <div
                key={s.id}
                className={`group relative mb-1 rounded-md ${s.id === activeId ? "bg-white shadow-sm" : "hover:bg-[#dfe4ea]"}`}
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
                      className="mx-1 min-w-0 flex-1 rounded-md border border-[#cfd5dd] bg-white px-2 py-1.5 text-[13px] outline-none focus:border-[#111827] disabled:opacity-60"
                      aria-label="会话标题"
                    />
                  ) : (
                    <button
                      onClick={() => onSelectSession(s.id)}
                      onDoubleClick={() => beginRename(s)}
                      disabled={busy}
                      className="min-w-0 flex-1 truncate px-2.5 py-2 text-left text-[13px] text-[#202124] disabled:cursor-not-allowed disabled:opacity-60"
                      title={`${s.title}\n双击重命名`}
                    >
                      {s.title}
                    </button>
                  )}
                  {!editing && (
                    <button
                      onClick={() => beginRename(s)}
                      disabled={busy}
                      className="grid h-7 w-7 shrink-0 place-items-center rounded-md text-[#69707a] opacity-0 transition hover:bg-[#cfd5dd] hover:text-[#111827] group-hover:opacity-100 disabled:opacity-0"
                      aria-label="重命名对话"
                      title="重命名"
                    >
                      <ComposeIcon size={14} />
                    </button>
                  )}
                  <button
                    onClick={() => onDeleteSession(s.id)}
                    disabled={busy || editing}
                    className="mr-1 grid h-7 w-7 shrink-0 place-items-center rounded-md text-[#69707a] opacity-0 transition hover:bg-[#cfd5dd] hover:text-[#dc2626] group-hover:opacity-100 disabled:opacity-0"
                    aria-label="删除对话"
                  >
                    <TrashIcon size={15} />
                  </button>
                </div>
                {editing && renameError && <div className="px-3 pb-2 text-xs text-[#dc2626]">{renameError}</div>}
              </div>
            );
          })}

          <div className="mt-4 px-2 pb-1.5 text-[11px] font-semibold uppercase tracking-wide text-[#8a9099]">Tools</div>
          <button
            onClick={onOpenSandbox}
            className="flex w-full items-center gap-2.5 rounded-md px-2.5 py-2 text-left text-[13px] text-[#202124] transition hover:bg-[#dfe4ea]"
          >
            <FolderIcon size={16} className="text-[#8a8a8a]" />
            打开沙盒目录
          </button>
        </div>

        <div className="border-t border-[#dfe3e8] pt-2">
          <button
            onClick={onOpenSettings}
            className={`flex w-full items-center gap-2 rounded-md py-2 text-left text-[13px] transition hover:bg-[#dfe4ea] ${open ? "px-2" : "justify-center px-0"}`}
            aria-label="设置"
          >
            {open ? (
              <>
                <img src="/demiurge.png" alt="Demiurge" className="size-7 shrink-0 rounded-md border border-[#dfe3e8] bg-white object-contain" />
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
