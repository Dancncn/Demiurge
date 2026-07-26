import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import * as api from "../lib/api";
import type {
  GitChangedFile,
  WorkspaceEntry,
  WorkspaceFilePreview,
  WorkspaceState,
} from "../lib/types";
import { useI18n } from "../lib/i18n";
import { pickFolder, type FolderPickOutcome } from "../lib/folderPicker";
import { normalizeExplorerTab, RequestGeneration, type ExplorerTab } from "../lib/agentEventReducer";
import {
  ChevronDownIcon,
  CloseIcon,
  FileIcon,
  FolderIcon,
  RotateCwIcon,
} from "./Icons";
import { SegmentedControl } from "./SegmentedControl";

type Props = {
  open: boolean;
  workspace: WorkspaceState | null;
  busy: boolean;
  refreshKey: number;
  onClose: () => void;
  onWorkspaceChange: (workspace: WorkspaceState) => void;
};

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
  const [picking, setPicking] = useState(false);
  const pickingRef = useRef(false);
  const isDesktop = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
  const workspaceKey = workspace ? `${workspace.path}\u0000${workspace.is_git ? "git" : "files"}` : "";
  const workspaceKeyRef = useRef(workspaceKey);
  const requestsRef = useRef(new RequestGeneration());
  workspaceKeyRef.current = workspaceKey;

  const loadDirectory = useCallback(
    async (relativePath: string) => {
      const ticket = requestsRef.current.issue(`directory:${normalizePath(relativePath)}`, workspaceKey);
      setLoadingPaths((current) => new Set(current).add(relativePath));
      try {
        const entries = isDesktop
          ? await api.listWorkspaceDirectory(relativePath || undefined)
          : relativePath
            ? []
            : PREVIEW_ROOT;
        if (!requestsRef.current.accepts(ticket, workspaceKeyRef.current)) return;
        setChildrenByPath((current) => ({ ...current, [relativePath]: entries }));
        setError(null);
      } catch (e) {
        if (requestsRef.current.accepts(ticket, workspaceKeyRef.current)) setError(String(e));
      } finally {
        if (requestsRef.current.accepts(ticket, workspaceKeyRef.current)) {
          setLoadingPaths((current) => {
            const next = new Set(current);
            next.delete(relativePath);
            return next;
          });
        }
      }
    },
    [isDesktop, workspaceKey],
  );

  const loadChanges = useCallback(async () => {
    const ticket = requestsRef.current.issue("changes", workspaceKey);
    if (!workspace?.is_git) {
      if (requestsRef.current.accepts(ticket, workspaceKeyRef.current)) {
        setChanges([]);
        setTab((current) => normalizeExplorerTab(current, false));
      }
      return;
    }
    try {
      const next = isDesktop ? await api.gitChangedFiles() : [];
      if (!requestsRef.current.accepts(ticket, workspaceKeyRef.current)) return;
      setChanges(next);
      setError(null);
    } catch (e) {
      if (requestsRef.current.accepts(ticket, workspaceKeyRef.current)) setError(String(e));
    }
  }, [isDesktop, workspace?.is_git, workspaceKey]);

  useEffect(() => {
    requestsRef.current.invalidateAll();
    setTab((current) => normalizeExplorerTab(current, Boolean(workspace?.is_git)));
    setChildrenByPath({});
    setExpanded(new Set());
    setLoadingPaths(new Set());
    setPreview(null);
    setPreviewLoading(false);
    setChanges([]);
    setError(null);
    setRefreshing(false);
    if (!open || !workspace) return;
    void Promise.all([loadDirectory(""), loadChanges()]);
    return () => requestsRef.current.invalidateAll();
  }, [open, workspace?.path, refreshKey, loadDirectory, loadChanges]);

  const changedPathSet = useMemo(
    () => new Set(changes.map((item) => normalizePath(item.path))),
    [changes],
  );

  async function refresh() {
    const ticket = requestsRef.current.issue("refresh", workspaceKey);
    setRefreshing(true);
    setError(null);
    try {
      const paths = ["", ...expanded];
      await Promise.all([...paths.map((path) => loadDirectory(path)), loadChanges()]);
    } finally {
      if (requestsRef.current.accepts(ticket, workspaceKeyRef.current)) setRefreshing(false);
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
    const ticket = requestsRef.current.issue("preview", workspaceKey);
    setPreviewLoading(true);
    setError(null);
    try {
      if (isDesktop) {
        const next = await api.readWorkspaceFile(entry.path);
        if (requestsRef.current.accepts(ticket, workspaceKeyRef.current)) setPreview(next);
      } else {
        const next = {
          path: entry.path,
          name: entry.name,
          content: t("workspace.previewUnavailable"),
          size: entry.size,
          truncated: false,
          binary: false,
        };
        if (requestsRef.current.accepts(ticket, workspaceKeyRef.current)) setPreview(next);
      }
    } catch (e) {
      if (requestsRef.current.accepts(ticket, workspaceKeyRef.current)) setError(String(e));
    } finally {
      if (requestsRef.current.accepts(ticket, workspaceKeyRef.current)) setPreviewLoading(false);
    }
  }

  function closePreview() {
    requestsRef.current.issue("preview", workspaceKey);
    setPreview(null);
    setPreviewLoading(false);
  }

  async function chooseWorkspace() {
    // 生成中禁止切换项目：这里给出可见原因，而不是让按钮看起来坏掉。
    if (busy) {
      setError(t("workspace.busyHint"));
      return;
    }
    // 原生文件夹对话框是模态的，但它的 Promise 并不阻止第二次点击。
    // 重复调用会让第二个对话框排在第一个后面（表现为"点了没反应"），
    // 所以这里自己守住重入。
    if (pickingRef.current) {
      setError(t("workspace.pickerAlreadyOpen"));
      return;
    }
    setError(null);

    pickingRef.current = true;
    setPicking(true);
    let outcome: FolderPickOutcome;
    try {
      outcome = await pickFolder(t("workspace.chooseFolder"));
    } finally {
      pickingRef.current = false;
      setPicking(false);
    }

    if (outcome.status === "unavailable") {
      setError(t("workspace.desktopOnly"));
      return;
    }
    if (outcome.status === "failed") {
      // 插件权限缺失 / 对话框拉起失败都会走到这里，必须浮出原因，不能静默吞掉。
      setError(t("workspace.chooseFolderFailed", { error: outcome.error }));
      return;
    }
    // 用户取消选择：不是错误，安静返回。
    if (outcome.status === "cancelled") return;

    try {
      const next = await api.selectWorkspace(outcome.path);
      onWorkspaceChange(next);
      setTab("files");
    } catch (e) {
      setError(t("workspace.switchFolderFailed", { error: String(e) }));
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
            className="md-type-body-small group flex h-8 w-full min-w-0 items-center gap-1.5 rounded-md pr-2 text-left text-[#4f5661] transition hover:bg-[#eef1f5] hover:text-[#202124]"
            style={{ paddingLeft: 8 + depth * 14 }}
            title={entry.path}
            aria-expanded={directory ? isOpen : undefined}
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
              <span className="md-type-label-small hidden shrink-0 text-[#a0a6af] group-hover:inline">{formatBytes(entry.size)}</span>
            )}
          </button>
          {directory && isOpen ? renderEntries(entry.path, depth + 1) : null}
        </div>
      );
    });
  }

  if (!open || !workspace) return null;

  return (
    <aside className="workspace-explorer flex w-[360px] shrink-0 flex-col border-l border-[#dfe3e8] bg-[#fbfcfd]" aria-label={t("workspace.toggle")}>
      <div className="flex items-start gap-2 border-b border-[#eceff3] px-3 py-3">
        <div className="workspace-header-icon grid size-8 shrink-0 place-items-center rounded-lg border border-[#e2e5ea] bg-white text-[#59616d]">
          <FolderIcon size={16} />
        </div>
        <div className="min-w-0 flex-1">
          <div className="md-type-title-small truncate font-semibold text-[#202124]">{workspace.name}</div>
          <div className="md-type-label-small mt-0.5 truncate text-[#8a9099]" title={workspace.path}>{workspace.path}</div>
        </div>
        <button
          type="button"
          onClick={() => void refresh()}
          disabled={refreshing}
          className="md-icon-button grid size-7 shrink-0 place-items-center text-[#69707a] transition hover:bg-[#eef1f5] disabled:opacity-50"
          aria-label={t("workspace.refresh")}
          title={t("workspace.refresh")}
        >
          <RotateCwIcon size={14} className={refreshing ? "animate-spin" : ""} />
        </button>
        <button
          type="button"
          onClick={onClose}
          className="md-icon-button grid size-7 shrink-0 place-items-center text-[#69707a] transition hover:bg-[#eef1f5]"
          aria-label={t("workspace.close")}
        >
          <CloseIcon size={15} />
        </button>
      </div>

      <div className="workspace-actions flex items-center gap-1.5 border-b border-[#eceff3] px-3 py-2">
        <button
          type="button"
          onClick={() => void openWorkspaceInSystem()}
          className="md-button md-button-outlined flex-1 whitespace-nowrap border border-[#e2e5ea] bg-white px-2 text-[#59616d] transition hover:bg-[#eef1f5] hover:text-[#202124]"
        >
          {t("workspace.openFolder")}
        </button>
        <button
          type="button"
          onClick={() => void chooseWorkspace()}
          // 生成中刻意不用 disabled：原生 disabled 会吞掉 click，用户只会看到
          // 一个"坏掉"的按钮。这里保留可点击并在 onClick 里说明原因，
          // 视觉与语义上仍然标记为不可用。
          aria-disabled={busy || picking}
          disabled={picking}
          title={busy ? t("workspace.busyHint") : t("workspace.chooseFolder")}
          className={`md-button md-button-filled flex-1 whitespace-nowrap bg-[#111827] px-2 text-white transition hover:bg-[#2b3442] ${
            busy || picking ? "cursor-not-allowed opacity-40" : ""
          }`}
        >
          {t("workspace.changeFolder")}
        </button>
      </div>

      <div className="workspace-tabs border-b border-[#eceff3] px-3 py-2">
        <SegmentedControl<ExplorerTab>
          value={tab}
          onChange={setTab}
          ariaLabel={`${t("workspace.files")} / ${t("workspace.changes")}`}
          className="workspace-tab-control"
          showCheck={false}
          options={[
            { value: "files", label: t("workspace.files") },
            {
              value: "changes",
              label: `${t("workspace.changes")}${changes.length > 0 ? ` (${changes.length})` : ""}`,
              disabled: !workspace.is_git,
            },
          ]}
        />
      </div>

      {error && (
        <div className="md-type-body-small border-b border-[#f2d7a5] bg-[#fff8e8] px-3 py-2 text-[#8a5a00]">
          {error}
        </div>
      )}

      {tab === "files" ? (
        preview ? (
          <div className="flex min-h-0 flex-1 flex-col" role="tabpanel">
            <button
              type="button"
              onClick={closePreview}
              className="md-type-label-medium flex h-9 shrink-0 items-center gap-1.5 border-b border-[#eceff3] px-3 text-left text-[#59616d] transition hover:bg-[#eef1f5]"
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
          <div className="capsule-scrollbar min-h-0 flex-1 overflow-y-auto p-2" role="tabpanel">
            {previewLoading ? (
              <div className="px-3 py-3 text-xs text-[#8a9099]">{t("workspace.loading")}</div>
            ) : (
              renderEntries("")
            )}
          </div>
        )
      ) : (
        <div className="capsule-scrollbar min-h-0 flex-1 overflow-y-auto p-2" role="tabpanel">
          {changes.length === 0 ? (
            <div className="px-3 py-8 text-center text-xs text-[#8a9099]">{t("workspace.clean")}</div>
          ) : (
            changes.map((change) => (
              <button
                key={`${change.status}:${change.path}`}
                type="button"
                onClick={() => {
                  setTab("files");
                  void openFile({
                    name: change.path.split(/[\\/]/).pop() || change.path,
                    path: normalizePath(change.path),
                    kind: "file",
                    size: 0,
                  });
                }}
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
