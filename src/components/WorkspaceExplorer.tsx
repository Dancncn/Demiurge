import { useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import * as api from "../lib/api";
import type {
  GitChangedFile,
  WorkspaceEntry,
  WorkspaceFilePreview,
  WorkspaceState,
} from "../lib/types";
import { useI18n } from "../lib/i18n";
import {
  ChevronDownIcon,
  CloseIcon,
  FileIcon,
  FolderIcon,
  RotateCwIcon,
} from "./Icons";

type Props = {
  open: boolean;
  workspace: WorkspaceState | null;
  busy: boolean;
  refreshKey: number;
  onClose: () => void;
  onWorkspaceChange: (workspace: WorkspaceState) => void;
};

type ExplorerTab = "files" | "changes";

const PREVIEW_ROOT: WorkspaceEntry[] = [
  { name: "src", path: "src", kind: "directory", size: 0 },
  { name: "src-tauri", path: "src-tauri", kind: "directory", size: 0 },
  { name: "package.json", path: "package.json", kind: "file", size: 1383 },
  { name: "README.md", path: "README.md", kind: "file", size: 13411 },
];

function normalizePath(path: string) {
  return path.replace(/\\/g, "/");
}

function formatBytes(size: number) {
  if (!Number.isFinite(size) || size <= 0) return "";
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(size < 10 * 1024 ? 1 : 0)} KB`;
  return `${(size / (1024 * 1024)).toFixed(1)} MB`;
}

function changeTone(status: string) {
  if (status.includes("?") || status.includes("A")) return "bg-[#eef5ff] text-[#0b57d0]";
  if (status.includes("D")) return "bg-[#fff1f1] text-[#b42318]";
  if (status.includes("R")) return "bg-[#f4f0ff] text-[#6941c6]";
  return "bg-[#fff8e8] text-[#8a5a00]";
}

export function WorkspaceExplorer({
  open,
  workspace,
  busy,
  refreshKey,
  onClose,
  onWorkspaceChange,
}: Props) {
  const { t } = useI18n();
  const [tab, setTab] = useState<ExplorerTab>("files");
  const [childrenByPath, setChildrenByPath] = useState<Record<string, WorkspaceEntry[]>>({});
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [loadingPaths, setLoadingPaths] = useState<Set<string>>(new Set());
  const [preview, setPreview] = useState<WorkspaceFilePreview | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [changes, setChanges] = useState<GitChangedFile[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const isDesktop = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

  const loadDirectory = useCallback(
    async (relativePath: string) => {
      setLoadingPaths((current) => new Set(current).add(relativePath));
      try {
        const entries = isDesktop
          ? await api.listWorkspaceDirectory(relativePath || undefined)
          : relativePath
            ? []
            : PREVIEW_ROOT;
        setChildrenByPath((current) => ({ ...current, [relativePath]: entries }));
        setError(null);
      } catch (e) {
        setError(String(e));
      } finally {
        setLoadingPaths((current) => {
          const next = new Set(current);
          next.delete(relativePath);
          return next;
        });
      }
    },
    [isDesktop],
  );

  const loadChanges = useCallback(async () => {
    if (!workspace?.is_git) {
      setChanges([]);
      return;
    }
    try {
      setChanges(isDesktop ? await api.gitChangedFiles() : []);
    } catch (e) {
      setError(String(e));
    }
  }, [isDesktop, workspace?.is_git]);

  useEffect(() => {
    if (!open || !workspace) return;
    setChildrenByPath({});
    setExpanded(new Set());
    setPreview(null);
    setError(null);
    void Promise.all([loadDirectory(""), loadChanges()]);
  }, [open, workspace?.path, refreshKey, loadDirectory, loadChanges]);

  const changedPathSet = useMemo(
    () => new Set(changes.map((item) => normalizePath(item.path))),
    [changes],
  );

  async function refresh() {
    setRefreshing(true);
    setError(null);
    try {
      const paths = ["", ...expanded];
      await Promise.all([...paths.map((path) => loadDirectory(path)), loadChanges()]);
    } finally {
      setRefreshing(false);
    }
  }

  async function toggleDirectory(entry: WorkspaceEntry) {
    const isOpen = expanded.has(entry.path);
    setExpanded((current) => {
      const next = new Set(current);
      if (isOpen) next.delete(entry.path);
      else next.add(entry.path);
      return next;
    });
    if (!isOpen && !childrenByPath[entry.path]) await loadDirectory(entry.path);
  }

  async function openFile(entry: WorkspaceEntry) {
    setPreviewLoading(true);
    setError(null);
    try {
      if (isDesktop) {
        setPreview(await api.readWorkspaceFile(entry.path));
      } else {
        setPreview({
          path: entry.path,
          name: entry.name,
          content: t("workspace.previewUnavailable"),
          size: entry.size,
          truncated: false,
          binary: false,
        });
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setPreviewLoading(false);
    }
  }

  async function chooseWorkspace() {
    if (busy) return;
    setError(null);
    try {
      if (!isDesktop) {
        setError(t("workspace.desktopOnly"));
        return;
      }
      const { open: openDialog } = await import("@tauri-apps/plugin-dialog");
      const selected = await openDialog({
        directory: true,
        multiple: false,
        title: t("workspace.chooseFolder"),
      });
      const path = Array.isArray(selected) ? selected[0] : selected;
      if (!path) return;
      const next = await api.selectWorkspace(path);
      onWorkspaceChange(next);
      setTab("files");
    } catch (e) {
      setError(String(e));
    }
  }

  async function openWorkspaceInSystem() {
    setError(null);
    if (!isDesktop) {
      setError(t("workspace.desktopOnly"));
      return;
    }
    try {
      await api.openSandbox();
    } catch (e) {
      setError(String(e));
    }
  }

  function renderEntries(parentPath: string, depth = 0): ReactNode {
    const entries = childrenByPath[parentPath] ?? [];
    if (loadingPaths.has(parentPath) && entries.length === 0) {
      return <div className="px-3 py-2 text-xs text-[#8a9099]">{t("workspace.loading")}</div>;
    }
    if (entries.length === 0) {
      return depth === 0 ? (
        <div className="px-3 py-8 text-center text-xs text-[#8a9099]">{t("workspace.empty")}</div>
      ) : null;
    }
    return entries.map((entry) => {
      const directory = entry.kind === "directory";
      const isOpen = directory && expanded.has(entry.path);
      const changed = changedPathSet.has(normalizePath(entry.path));
      return (
        <div key={entry.path}>
          <button
            type="button"
            onClick={() => void (directory ? toggleDirectory(entry) : openFile(entry))}
            className="group flex h-8 w-full min-w-0 items-center gap-1.5 rounded-md pr-2 text-left text-[12px] text-[#4f5661] transition hover:bg-[#eef1f5] hover:text-[#202124]"
            style={{ paddingLeft: 8 + depth * 14 }}
            title={entry.path}
          >
            {directory ? (
              <ChevronDownIcon
                size={13}
                className={`shrink-0 text-[#9aa1ab] transition-transform ${isOpen ? "" : "-rotate-90"}`}
              />
            ) : (
              <span className="w-[13px] shrink-0" />
            )}
            {directory ? (
              <FolderIcon size={15} className="shrink-0 text-[#7a8088]" />
            ) : (
              <FileIcon size={14} className="shrink-0 text-[#8a9099]" />
            )}
            <span className="min-w-0 flex-1 truncate">{entry.name}</span>
            {changed && <span className="size-1.5 shrink-0 rounded-full bg-[#d97706]" />}
            {!directory && entry.size > 0 && (
              <span className="hidden shrink-0 text-[10px] text-[#a0a6af] group-hover:inline">{formatBytes(entry.size)}</span>
            )}
          </button>
          {directory && isOpen ? renderEntries(entry.path, depth + 1) : null}
        </div>
      );
    });
  }

  if (!open || !workspace) return null;

  return (
    <aside className="workspace-explorer flex w-[360px] shrink-0 flex-col border-l border-[#dfe3e8] bg-[#fbfcfd]">
      <div className="flex items-start gap-2 border-b border-[#eceff3] px-3 py-3">
        <div className="grid size-8 shrink-0 place-items-center rounded-lg border border-[#e2e5ea] bg-white text-[#59616d]">
          <FolderIcon size={16} />
        </div>
        <div className="min-w-0 flex-1">
          <div className="truncate text-[13px] font-semibold text-[#202124]">{workspace.name}</div>
          <div className="mt-0.5 truncate text-[10px] text-[#8a9099]" title={workspace.path}>{workspace.path}</div>
        </div>
        <button
          type="button"
          onClick={() => void refresh()}
          disabled={refreshing}
          className="grid size-7 shrink-0 place-items-center rounded-md text-[#69707a] transition hover:bg-[#eef1f5] disabled:opacity-50"
          aria-label={t("workspace.refresh")}
          title={t("workspace.refresh")}
        >
          <RotateCwIcon size={14} className={refreshing ? "animate-spin" : ""} />
        </button>
        <button
          type="button"
          onClick={onClose}
          className="grid size-7 shrink-0 place-items-center rounded-md text-[#69707a] transition hover:bg-[#eef1f5]"
          aria-label={t("workspace.close")}
        >
          <CloseIcon size={15} />
        </button>
      </div>

      <div className="flex items-center gap-1.5 border-b border-[#eceff3] px-3 py-2">
        <button
          type="button"
          onClick={() => void openWorkspaceInSystem()}
          className="flex-1 whitespace-nowrap rounded-md border border-[#e2e5ea] bg-white px-2 py-1.5 text-[11px] text-[#59616d] transition hover:bg-[#eef1f5] hover:text-[#202124]"
        >
          {t("workspace.openFolder")}
        </button>
        <button
          type="button"
          onClick={() => void chooseWorkspace()}
          disabled={busy}
          className="flex-1 whitespace-nowrap rounded-md bg-[#111827] px-2 py-1.5 text-[11px] font-medium text-white transition hover:bg-[#2b3442] disabled:cursor-not-allowed disabled:opacity-40"
        >
          {t("workspace.changeFolder")}
        </button>
      </div>

      <div className="flex items-center gap-1 border-b border-[#eceff3] px-3 py-2">
        <button
          type="button"
          onClick={() => setTab("files")}
          className={`whitespace-nowrap rounded-md px-2.5 py-1.5 text-[12px] font-medium transition ${
            tab === "files" ? "bg-white text-[#202124] shadow-sm" : "text-[#7a8088] hover:bg-[#eef1f5]"
          }`}
        >
          {t("workspace.files")}
        </button>
        <button
          type="button"
          onClick={() => setTab("changes")}
          disabled={!workspace.is_git}
          className={`whitespace-nowrap rounded-md px-2.5 py-1.5 text-[12px] font-medium transition disabled:opacity-40 ${
            tab === "changes" ? "bg-white text-[#202124] shadow-sm" : "text-[#7a8088] hover:bg-[#eef1f5]"
          }`}
        >
          {t("workspace.changes")} {changes.length > 0 ? `(${changes.length})` : ""}
        </button>
      </div>

      {error && (
        <div className="border-b border-[#f2d7a5] bg-[#fff8e8] px-3 py-2 text-[11px] leading-relaxed text-[#8a5a00]">
          {error}
        </div>
      )}

      {tab === "files" ? (
        preview ? (
          <div className="flex min-h-0 flex-1 flex-col">
            <button
              type="button"
              onClick={() => setPreview(null)}
              className="flex h-9 shrink-0 items-center gap-1.5 border-b border-[#eceff3] px-3 text-left text-[12px] text-[#59616d] transition hover:bg-[#eef1f5]"
            >
              <ChevronDownIcon size={14} className="rotate-90" />
              <span className="min-w-0 flex-1 truncate">{preview.path}</span>
              <span className="shrink-0 text-[10px] text-[#9aa1ab]">{formatBytes(preview.size)}</span>
            </button>
            {preview.binary ? (
              <div className="grid flex-1 place-items-center px-6 text-center text-xs text-[#8a9099]">
                {t("workspace.binaryFile")}
              </div>
            ) : (
              <pre className="min-h-0 flex-1 overflow-auto whitespace-pre p-3 font-mono text-[11px] leading-[1.65] text-[#344054]">
                {preview.content}
              </pre>
            )}
            {preview.truncated && (
              <div className="shrink-0 border-t border-[#f2d7a5] bg-[#fff8e8] px-3 py-2 text-[10px] text-[#8a5a00]">
                {t("workspace.previewTruncated")}
              </div>
            )}
          </div>
        ) : (
          <div className="capsule-scrollbar min-h-0 flex-1 overflow-y-auto p-2">
            {previewLoading ? (
              <div className="px-3 py-3 text-xs text-[#8a9099]">{t("workspace.loading")}</div>
            ) : (
              renderEntries("")
            )}
          </div>
        )
      ) : (
        <div className="capsule-scrollbar min-h-0 flex-1 overflow-y-auto p-2">
          {changes.length === 0 ? (
            <div className="px-3 py-8 text-center text-xs text-[#8a9099]">{t("workspace.clean")}</div>
          ) : (
            changes.map((change) => (
              <button
                key={`${change.status}:${change.path}`}
                type="button"
                onClick={() =>
                  void openFile({
                    name: change.path.split(/[\\/]/).pop() || change.path,
                    path: normalizePath(change.path),
                    kind: "file",
                    size: 0,
                  }).then(() => setTab("files"))
                }
                className="mb-1 flex w-full min-w-0 items-center gap-2 rounded-md px-2 py-2 text-left transition hover:bg-[#eef1f5]"
              >
                <span className={`rounded px-1.5 py-0.5 font-mono text-[10px] font-semibold ${changeTone(change.status)}`}>
                  {change.status.trim() || "M"}
                </span>
                <span className="min-w-0 flex-1 truncate font-mono text-[11px] text-[#4f5661]">{change.path}</span>
                {change.staged && <span className="shrink-0 text-[9px] uppercase text-[#177245]">staged</span>}
              </button>
            ))
          )}
        </div>
      )}
    </aside>
  );
}
