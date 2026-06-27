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
  onDeleteSession,
  onOpenSandbox,
  onOpenSettings,
}: Props) {
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
          {sessions.map((s) => (
            <div
              key={s.id}
              className={`group relative mb-1 flex items-center rounded-lg ${
                s.id === activeId ? "bg-[#ececea]" : "hover:bg-[#ececea]"
              }`}
            >
              <button
                onClick={() => onSelectSession(s.id)}
                disabled={busy}
                className="min-w-0 flex-1 truncate px-3 py-2 text-left text-sm disabled:cursor-not-allowed disabled:opacity-60"
                title={s.title}
              >
                {s.title}
              </button>
              <button
                onClick={() => onDeleteSession(s.id)}
                disabled={busy}
                className="mr-1 grid h-7 w-7 shrink-0 place-items-center rounded-md text-[#6f6f6f] opacity-0 transition hover:bg-[#dededc] hover:text-[#dc2626] group-hover:opacity-100 disabled:opacity-0"
                aria-label="删除对话"
              >
                <TrashIcon size={15} />
              </button>
            </div>
          ))}

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
