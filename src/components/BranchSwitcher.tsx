import { useEffect, useMemo, useRef, useState } from "react";
import * as api from "../lib/api";
import type { GitBranch, WorkspaceState } from "../lib/types";
import { useI18n } from "../lib/i18n";
import { CheckIcon, ChevronDownIcon, GitBranchIcon, RotateCwIcon } from "./Icons";

type Props = {
  workspace: WorkspaceState;
  busy: boolean;
  onWorkspaceChange: (workspace: WorkspaceState) => void;
  onRefreshWorkspace: () => void;
};

export function BranchSwitcher({ workspace, busy, onWorkspaceChange, onRefreshWorkspace }: Props) {
  const { t } = useI18n();
  const rootRef = useRef<HTMLDivElement | null>(null);
  const searchRef = useRef<HTMLInputElement | null>(null);
  const [open, setOpen] = useState(false);
  const [branches, setBranches] = useState<GitBranch[]>([]);
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(false);
  const [switching, setSwitching] = useState<string | null>(null);
  const [pendingBranch, setPendingBranch] = useState<GitBranch | null>(null);
  const [error, setError] = useState<string | null>(null);
  const isDesktop = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

  useEffect(() => {
    if (!open) return;
    const close = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", close);
    requestAnimationFrame(() => searchRef.current?.focus());
    return () => document.removeEventListener("pointerdown", close);
  }, [open]);

  async function loadBranches() {
    setLoading(true);
    setError(null);
    try {
      setBranches(
        isDesktop
          ? await api.gitBranches()
          : workspace.branch
            ? [{ name: workspace.branch, current: true, remote: false, upstream: null }]
            : [],
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  function toggleOpen() {
    if (busy) return;
    setOpen((current) => {
      const next = !current;
      if (next) {
        setQuery("");
        setPendingBranch(null);
        void loadBranches();
      }
      return next;
    });
  }

  async function switchBranch(branch: GitBranch) {
    if (busy || switching) return;
    if (branch.current || branch.name === workspace.branch) {
      setOpen(false);
      return;
    }
    if (workspace.dirty && pendingBranch?.name !== branch.name) {
      setPendingBranch(branch);
      return;
    }

    setSwitching(branch.name);
    setError(null);
    try {
      const next = await api.switchGitBranch(branch.name);
      onWorkspaceChange(next);
      onRefreshWorkspace();
      setOpen(false);
      setPendingBranch(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setSwitching(null);
    }
  }

  const filtered = useMemo(() => {
    const needle = query.trim().toLocaleLowerCase();
    return branches.filter((branch) => !needle || branch.name.toLocaleLowerCase().includes(needle));
  }, [branches, query]);
  const localBranches = filtered.filter((branch) => !branch.remote);
  const remoteBranches = filtered.filter((branch) => branch.remote);
  const currentLabel = workspace.branch || t("branch.detached");

  function renderBranch(branch: GitBranch) {
    const pending = switching === branch.name;
    return (
      <button
        key={`${branch.remote ? "remote" : "local"}:${branch.name}`}
        type="button"
        onClick={() => void switchBranch(branch)}
        disabled={Boolean(switching)}
        className={`cf-menu-item flex w-full min-w-0 items-center gap-2 disabled:cursor-wait disabled:opacity-60 ${
          branch.current || branch.name === workspace.branch ? "is-active" : ""
        }`}
      >
        <GitBranchIcon size={14} className="shrink-0 text-[#7a8088]" />
        <span className="min-w-0 flex-1 truncate font-mono text-[12px]">{branch.name}</span>
        {pending ? (
          <RotateCwIcon size={13} className="shrink-0 animate-spin text-[#0b57d0]" />
        ) : branch.current || branch.name === workspace.branch ? (
          <CheckIcon size={14} className="shrink-0 text-[#177245]" />
        ) : null}
      </button>
    );
  }

  return (
    <div ref={rootRef} className="relative">
      <button
        type="button"
        onClick={toggleOpen}
        disabled={busy}
        className="cf-press inline-flex h-7 max-w-[190px] items-center gap-1.5 rounded-md border border-[#e2e5ea] bg-white px-2 text-[12px] text-[#4f5661] shadow-[0_1px_2px_rgba(15,23,42,0.04)] outline-none transition hover:bg-[#fbfcfd] focus-visible:ring-2 focus-visible:ring-[#b8d4ff] disabled:cursor-not-allowed disabled:opacity-50"
        title={t("branch.switch")}
        aria-expanded={open}
      >
        <GitBranchIcon size={14} className="shrink-0 text-[#7a8088]" />
        <span className="truncate">{currentLabel}</span>
        {workspace.dirty && <span className="size-1.5 shrink-0 rounded-full bg-[#d97706]" title={t("branch.dirty")} />}
        <ChevronDownIcon
          size={12}
          className={`shrink-0 text-[#9aa1ab] transition-transform ${open ? "rotate-180" : ""}`}
        />
      </button>

      {open && (
        <div className="cf-menu-in cf-dropdown absolute bottom-[calc(100%+7px)] left-0 z-40 w-[320px] overflow-hidden">
          <div className="border-b border-[#eceff3] p-2">
            <div className="flex items-center gap-2 rounded-md border border-[#dfe3e8] bg-white px-2.5">
              <GitBranchIcon size={14} className="shrink-0 text-[#8a9099]" />
              <input
                ref={searchRef}
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder={t("branch.search")}
                className="h-8 min-w-0 flex-1 border-0 bg-transparent text-[12px] text-[#202124] outline-none placeholder:text-[#9aa1ab]"
              />
              {loading && <RotateCwIcon size={13} className="animate-spin text-[#8a9099]" />}
            </div>
          </div>

          {workspace.dirty && (
            <div className="border-b border-[#f2d7a5] bg-[#fff8e8] px-3 py-2 text-[11px] leading-relaxed text-[#8a5a00]">
              {t("branch.dirtyWarning")}
            </div>
          )}
          {error && (
            <div className="border-b border-[#f1b8b8] bg-[#fff1f1] px-3 py-2 text-[11px] leading-relaxed text-[#b42318]">
              {error}
            </div>
          )}

          {pendingBranch && workspace.dirty && (
            <div className="border-b border-[#f2d7a5] bg-white px-3 py-3">
              <div className="text-[12px] font-medium text-[#202124]">
                {t("branch.confirmTitle", { branch: pendingBranch.name })}
              </div>
              <div className="mt-1 text-[11px] leading-relaxed text-[#7a8088]">{t("branch.confirmBody")}</div>
              <div className="mt-2 flex justify-end gap-2">
                <button
                  type="button"
                  onClick={() => setPendingBranch(null)}
                  className="rounded-md px-2.5 py-1.5 text-[11px] text-[#59616d] transition hover:bg-[#eef1f5]"
                >
                  {t("branch.cancel")}
                </button>
                <button
                  type="button"
                  onClick={() => void switchBranch(pendingBranch)}
                  className="rounded-md bg-[#111827] px-2.5 py-1.5 text-[11px] font-medium text-white transition hover:bg-[#2b3442]"
                >
                  {t("branch.continue")}
                </button>
              </div>
            </div>
          )}

          <div className="capsule-scrollbar max-h-72 overflow-y-auto p-1.5">
            {!loading && localBranches.length === 0 && remoteBranches.length === 0 && (
              <div className="px-2.5 py-5 text-center text-xs text-[#8a9099]">{t("branch.empty")}</div>
            )}
            {localBranches.length > 0 && (
              <>
                <div className="px-2.5 pb-1 pt-1 text-[10px] font-semibold uppercase tracking-wide text-[#9aa1ab]">
                  {t("branch.local")}
                </div>
                {localBranches.map(renderBranch)}
              </>
            )}
            {remoteBranches.length > 0 && (
              <>
                <div className="mt-1 border-t border-[#eceff3] px-2.5 pb-1 pt-2 text-[10px] font-semibold uppercase tracking-wide text-[#9aa1ab]">
                  {t("branch.remote")}
                </div>
                {remoteBranches.map(renderBranch)}
              </>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
