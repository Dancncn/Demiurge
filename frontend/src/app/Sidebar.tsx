import { useEffect, useState } from "react";
import type { SessionMeta } from "@/lib/types";
import { useI18n } from "@/lib/i18n";
import { ChatIcon, ComposeIcon, ImageIcon, PanelLeftIcon, PersonIcon, SettingsIcon, SparklesIcon, TrashIcon } from "@/shared/components/Icons";

export type AppView = "chat" | "media" | "skills" | "live2d" | "settings";

type Props = {
  open: boolean;
  activeView: AppView;
  packName: string;
  packAvatar?: string;
  sessions: SessionMeta[];
  activeId: string;
  busy: boolean;
  navigationPending: boolean;
  onToggle: () => void;
  onViewChange: (view: AppView) => void;
  onNewChat: () => void;
  onSelectSession: (id: string) => void;
  onRenameSession: (id: string, title: string) => Promise<void> | void;
  onDeleteSession: (id: string) => void;
  onOpenSettings: () => void;
};

export function Sidebar({
  open,
  activeView,
  packName,
  packAvatar,
  sessions,
  activeId,
  busy,
  navigationPending,
  onToggle,
  onViewChange,
  onNewChat,
  onSelectSession,
  onRenameSession,
  onDeleteSession,
  onOpenSettings,
}: Props) {
  const { t } = useI18n();
  const [editingId, setEditingId] = useState<string | null>(null);
  const [draftTitle, setDraftTitle] = useState("");
  const [renameError, setRenameError] = useState<string | null>(null);
  const navigationLocked = busy || navigationPending;

  useEffect(() => {
    if (!sessions.some((s) => s.id === editingId)) {
      setEditingId(null);
      setDraftTitle("");
      setRenameError(null);
    }
  }, [editingId, sessions]);

  function beginRename(session: SessionMeta) {
    if (navigationLocked) return;
    setEditingId(session.id);
    setDraftTitle(session.title);
    setRenameError(null);
  }

  async function commitRename(id: string) {
    const title = draftTitle.trim();
    if (!title) {
      setRenameError(t("sidebar.titleEmpty"));
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

  function closeCompactNavigation() {
    if (window.matchMedia("(max-width: 767px)").matches) onToggle();
  }

  function navigateTo(view: AppView) {
    onViewChange(view);
    closeCompactNavigation();
  }

  const navButton =
    "app-nav-item cf-press flex h-12 w-full items-center gap-3 rounded-full px-3 text-left text-[13px]";

  return (
    <>
      {open && <div onClick={onToggle} aria-hidden className="fixed inset-0 z-30 bg-black/20 md:hidden" />}
      <aside
        className={`app-sidebar app-navigation-rail fixed inset-y-0 left-0 z-40 flex w-[260px] flex-col border-r border-[#dfe3e8] bg-[#eef1f5] px-3 py-2 shadow-[12px_0_32px_rgba(15,23,42,0.12)] transition-transform duration-200 md:relative md:z-auto md:shrink-0 md:translate-x-0 md:shadow-none md:transition-[width] ${
          open ? "translate-x-0" : "-translate-x-full"
        } ${open ? "md:w-[248px]" : "md:w-[80px]"}`}
        aria-label={t("sidebar.chats")}
      >
        <div
          className={`app-navigation-leading mb-3 flex items-center gap-2 ${
            open ? "h-10 justify-between px-1" : "h-auto flex-col justify-center"
          }`}
        >
          <button
            onClick={onToggle}
            className="md-icon-button grid h-8 w-8 shrink-0 place-items-center rounded-md text-[#4f5661] cf-press hover:bg-[#dfe4ea]"
            aria-label={t("sidebar.toggle")}
            title={t("sidebar.toggle")}
          >
            <PanelLeftIcon size={20} />
          </button>
          <button
            onClick={() => {
              navigateTo("chat");
              onNewChat();
            }}
            disabled={navigationLocked}
            className="md-icon-button md-icon-button-tonal grid h-8 w-8 place-items-center rounded-md text-[#4f5661] cf-press hover:bg-[#dfe4ea] disabled:cursor-not-allowed disabled:opacity-50"
            aria-label={t("sidebar.newChat")}
            title={t("sidebar.newChat")}
          >
            <ComposeIcon size={19} />
          </button>
        </div>

        <nav className="mb-3 grid gap-1" aria-label={t("sidebar.chats")}>
          <button
            onClick={() => navigateTo("chat")}
            className={`${navButton} ${activeView === "chat" ? "is-active bg-white text-[#111827] shadow-sm" : "text-[#202124] hover:bg-[#dfe4ea]"} ${
              open ? "" : "justify-center px-0"
            }`}
            aria-current={activeView === "chat" ? "page" : undefined}
            title={t("nav.chat")}
          >
            <ChatIcon size={17} className="shrink-0" />
            {open && <span>{t("nav.chat")}</span>}
          </button>
          <button
            onClick={() => navigateTo("media")}
            className={`${navButton} ${activeView === "media" ? "is-active bg-white text-[#111827] shadow-sm" : "text-[#202124] hover:bg-[#dfe4ea]"} ${
              open ? "" : "justify-center px-0"
            }`}
            aria-current={activeView === "media" ? "page" : undefined}
            title={t("nav.images")}
          >
            <ImageIcon size={17} className="shrink-0" />
            {open && <span>{t("nav.images")}</span>}
          </button>
          <button
            onClick={() => navigateTo("skills")}
            className={`${navButton} ${activeView === "skills" ? "is-active bg-white text-[#111827] shadow-sm" : "text-[#202124] hover:bg-[#dfe4ea]"} ${
              open ? "" : "justify-center px-0"
            }`}
            aria-current={activeView === "skills" ? "page" : undefined}
            title={t("nav.skills")}
          >
            <SparklesIcon size={17} className="shrink-0" />
            {open && <span>{t("nav.skills")}</span>}
          </button>
          <button
            onClick={() => navigateTo("live2d")}
            className={`${navButton} ${activeView === "live2d" ? "is-active bg-white text-[#111827] shadow-sm" : "text-[#202124] hover:bg-[#dfe4ea]"} ${
              open ? "" : "justify-center px-0"
            }`}
            aria-current={activeView === "live2d" ? "page" : undefined}
            title={t("nav.live2d")}
          >
            <PersonIcon size={17} className="shrink-0" />
            {open && <span>{t("nav.live2d")}</span>}
          </button>
        </nav>

        <div className={`capsule-scrollbar min-h-0 flex-1 overflow-y-auto ${open ? "" : "hidden"}`}>
          <div className="px-2 pb-1.5 text-[11px] font-semibold uppercase tracking-wide text-[#8a9099]">{t("sidebar.chats")}</div>
          {sessions.length === 0 && <div className="px-2 py-2 text-[13px] text-[#9aa1ab]">{t("sidebar.noChats")}</div>}
          {sessions.map((s) => {
            const editing = editingId === s.id;
            return (
              <div
                key={s.id}
                className={`app-session-item group relative mb-1 rounded-lg ${
                  activeView === "chat" && s.id === activeId ? "is-active bg-white shadow-sm" : "hover:bg-[#dfe4ea]"
                }`}
              >
                <div className="flex items-center">
                  {editing ? (
                    <input
                      autoFocus
                      value={draftTitle}
                      disabled={navigationLocked}
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
                      aria-label={t("sidebar.chatTitle")}
                      aria-invalid={Boolean(renameError)}
                      aria-describedby={renameError ? `rename-error-${s.id}` : undefined}
                    />
                  ) : (
                    <button
                      onClick={() => {
                        navigateTo("chat");
                        onSelectSession(s.id);
                      }}
                      onDoubleClick={() => beginRename(s)}
                      onKeyDown={(event) => {
                        if (event.key === "F2") {
                          event.preventDefault();
                          beginRename(s);
                        }
                      }}
                      disabled={navigationLocked}
                      className="min-w-0 flex-1 px-2.5 py-2 text-left text-[13px] text-[#202124] disabled:cursor-not-allowed disabled:opacity-60"
                      title={`${s.title}\n${t("sidebar.renameHint")}`}
                      aria-current={activeView === "chat" && s.id === activeId ? "page" : undefined}
                    >
                      <span className="block truncate">{s.title}</span>
                      {s.workspace_name && (
                        <span className="mt-0.5 flex items-center gap-1 truncate text-[10px] text-[#8a9099]" title={s.workspace_path}>
                          {s.workspace_name}
                        </span>
                      )}
                    </button>
                  )}
                  {!editing && (
                    <button
                      onClick={() => beginRename(s)}
                      disabled={navigationLocked}
                      className="grid h-7 w-7 shrink-0 place-items-center rounded-md text-[#69707a] opacity-0 transition hover:bg-[#cfd5dd] hover:text-[#111827] focus:opacity-100 group-focus-within:opacity-100 group-hover:opacity-100 disabled:opacity-0"
                      aria-label={t("sidebar.rename")}
                      title={t("sidebar.rename")}
                    >
                      <ComposeIcon size={14} />
                    </button>
                  )}
                  <button
                    onClick={() => onDeleteSession(s.id)}
                    disabled={navigationLocked || editing}
                    className="mr-1 grid h-7 w-7 shrink-0 place-items-center rounded-md text-[#69707a] opacity-0 transition hover:bg-[#cfd5dd] hover:text-[#dc2626] focus:opacity-100 group-focus-within:opacity-100 group-hover:opacity-100 disabled:opacity-0"
                    aria-label={t("sidebar.deleteChat")}
                  >
                    <TrashIcon size={15} />
                  </button>
                </div>
                {editing && renameError && (
                  <div id={`rename-error-${s.id}`} role="alert" className="px-3 pb-2 text-xs text-[#dc2626]">
                    {renameError}
                  </div>
                )}
              </div>
            );
          })}
        </div>

        <div className="app-navigation-footer border-t border-[#dfe3e8] pt-2">
          <button
            onClick={() => {
              onOpenSettings();
              closeCompactNavigation();
            }}
            className={`app-settings-entry flex w-full items-center gap-2 rounded-md py-2 text-left text-[13px] transition ${
              activeView === "settings" ? "is-active bg-white shadow-sm" : "hover:bg-[#dfe4ea]"
            } ${open ? "px-2" : "justify-center px-0"}`}
            aria-label={t("sidebar.settings")}
            aria-current={activeView === "settings" ? "page" : undefined}
          >
            {open ? (
              <>
                <img
                  src={packAvatar || "/demiurge.png"}
                  alt=""
                  className="size-7 shrink-0 rounded-md border border-[#dfe3e8] bg-white object-cover"
                />
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
