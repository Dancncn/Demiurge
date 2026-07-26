import { useEffect, useId, useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import type { ConfirmRequestEvent, PermissionMode, PermissionScope, ToolRisk } from "@/lib/types";
import DiffPreview from "@/features/workspace/DiffPreview";
import { SegmentedControl } from "@/shared/components/SegmentedControl";

interface Props {
  req: ConfirmRequestEvent | null;
  mode: PermissionMode;
  onRespond: (allow: boolean, scope: PermissionScope) => void;
}

function riskLabel(risk?: ToolRisk) {
  switch (risk) {
    case "read_only":
      return "Read only";
    case "mutating":
      return "Can modify files";
    case "external":
      return "Network access";
    case "privileged":
      return "System capability";
    default:
      return "Needs approval";
  }
}

const scopeOptions: { value: PermissionScope; label: string; detail: string }[] = [
  { value: "once", label: "Once", detail: "Only this call" },
  { value: "session", label: "Session", detail: "Remember in this chat" },
  { value: "project", label: "Project", detail: "Remember for this project" },
  { value: "user", label: "User", detail: "Remember globally" },
];

function sourceLabel(source?: string) {
  if (source === "user_override") return "Saved rule";
  if (source === "unknown_tool") return "Unknown tool";
  return "Tool default";
}

const editTools = new Set(["edit_file", "multi_edit", "apply_patch", "undo_edit"]);

function approvalLabel(tool: string) {
  if (tool === "undo_edit") return "Undo";
  if (editTools.has(tool)) return "Apply";
  return "Allow";
}

export default function ConfirmDialog({ req, mode, onRespond }: Props) {
  const [scope, setScope] = useState<PermissionScope>("once");
  const dialogRef = useRef<HTMLDivElement | null>(null);
  const rejectRef = useRef<HTMLButtonElement | null>(null);
  const titleId = useId();
  const summaryId = useId();

  useEffect(() => {
    setScope("once");
  }, [req?.id]);

  useEffect(() => {
    if (!req) return;
    const previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const frame = requestAnimationFrame(() => rejectRef.current?.focus());
    return () => {
      cancelAnimationFrame(frame);
      previousFocus?.focus();
    };
  }, [req?.id]);

  function handleDialogKeyDown(event: ReactKeyboardEvent<HTMLDivElement>) {
    if (event.key === "Escape") {
      event.preventDefault();
      onRespond(false, scope);
      return;
    }
    if (event.key !== "Tab") return;
    const focusable = Array.from(
      dialogRef.current?.querySelectorAll<HTMLElement>(
        'button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
      ) ?? [],
    );
    if (focusable.length === 0) {
      event.preventDefault();
      dialogRef.current?.focus();
      return;
    }
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (!dialogRef.current?.contains(document.activeElement)) {
      event.preventDefault();
      (event.shiftKey ? last : first).focus();
    } else if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }

  if (!req) return null;
  const isEditTool = editTools.has(req.tool);
  const confirmLabel = approvalLabel(req.tool);

  return (
    <div className="permission-dialog-backdrop fixed inset-0 z-50 flex items-center justify-center bg-[#111827]/35 p-4 backdrop-blur-[2px]">
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={req.summary ? summaryId : undefined}
        tabIndex={-1}
        onKeyDown={handleDialogKeyDown}
        className="permission-dialog-surface flex max-h-[88vh] w-full max-w-3xl flex-col overflow-hidden rounded-xl border border-[#d7dbe2] bg-white shadow-[0_24px_80px_rgba(15,23,42,0.28)]"
      >
        <header className="border-b border-[#eceff3] bg-[#fbfcfd] px-5 py-4">
          <div className="flex items-start justify-between gap-4">
            <div className="min-w-0">
              <div id={titleId} className="text-[15px] font-semibold text-[#202124]">
                {isEditTool ? "Confirm File Change" : "Approve Tool Call"}
              </div>
              <div className="mt-1 flex flex-wrap items-center gap-2 text-[12px] text-[#6f7782]">
                <span className="rounded-md bg-white px-2 py-1 font-medium text-[#202124] shadow-sm">{req.tool}</span>
                <span className="rounded-md bg-[#eef1f5] px-2 py-1">{riskLabel(req.risk)}</span>
                {req.effect && <span className="rounded-md bg-[#eef1f5] px-2 py-1">Policy: {req.effect}</span>}
                {req.scope && <span className="rounded-md bg-[#eef1f5] px-2 py-1">Default: {req.scope}</span>}
                <span className="rounded-md bg-[#eef1f5] px-2 py-1">{sourceLabel(req.source)}</span>
                <span className="rounded-md bg-[#eef1f5] px-2 py-1">Mode: {mode}</span>
              </div>
            </div>
          </div>
          {req.summary && (
            <div id={summaryId} className="mt-3 rounded-lg border border-[#e2e5ea] bg-white px-3 py-2 text-[13px] text-[#344054]">
              {req.summary}
            </div>
          )}
        </header>

        <div className="min-h-0 flex-1 overflow-y-auto px-5 py-4">
          <div className="grid gap-4 md:grid-cols-[minmax(0,0.9fr)_minmax(0,1.1fr)]">
            <section className="min-w-0">
              <div className="mb-2 text-[12px] font-semibold uppercase tracking-wide text-[#8a9099]">Request</div>
              <div className="space-y-2 rounded-lg border border-[#e2e5ea] bg-[#fbfcfd] p-3 text-[12px] leading-5 text-[#59616d]">
                {req.description && (
                  <div>
                    <span className="font-medium text-[#344054]">Description: </span>
                    {req.description}
                  </div>
                )}
                {req.reason && (
                  <div>
                    <span className="font-medium text-[#344054]">Rule: </span>
                    {req.reason}
                  </div>
                )}
                {req.affected_paths?.length ? (
                  <div>
                    <span className="font-medium text-[#344054]">Affected paths: </span>
                    <div className="mt-1 flex flex-wrap gap-1">
                      {req.affected_paths.map((path) => (
                        <span key={path} className="rounded-md bg-white px-2 py-0.5 font-mono text-[11px] text-[#344054]">
                          {path}
                        </span>
                      ))}
                    </div>
                  </div>
                ) : null}
              </div>
              <pre className="mt-3 max-h-72 overflow-auto whitespace-pre-wrap rounded-lg border border-[#e2e5ea] bg-white p-3 text-[12px] text-[#344054]">
                {req.args}
              </pre>
            </section>

            <section className="min-w-0">
              <div className="mb-2 text-[12px] font-semibold uppercase tracking-wide text-[#8a9099]">Preview</div>
              {req.preview ? (
                <DiffPreview text={req.preview} maxHeightClass="max-h-[25rem]" />
              ) : (
                <div className="rounded-lg border border-dashed border-[#d8dde5] bg-[#fbfcfd] p-4 text-[12px] text-[#7a8088]">
                  This tool does not provide a structured preview.
                </div>
              )}
            </section>
          </div>
        </div>

        <footer className="border-t border-[#eceff3] bg-[#fbfcfd] px-5 py-4">
          <SegmentedControl
            value={scope}
            options={scopeOptions.map((option) => ({ ...option, hint: option.detail }))}
            onChange={setScope}
            ariaLabel="Permission duration"
            className="permission-scope-control"
          />
          <div className="mt-4 flex justify-end gap-2">
            <button
              ref={rejectRef}
              type="button"
              className="md-button md-button-outlined cf-press inline-flex h-9 items-center justify-center rounded-md border border-[#d9d9d9] bg-white px-4 text-[13px] font-medium text-[#344054] hover:bg-[#f5f5f5]"
              onClick={() => onRespond(false, scope)}
            >
              Reject
            </button>
            <button
              type="button"
              className="md-button md-button-filled cf-press inline-flex h-9 items-center justify-center rounded-md bg-[#111827] px-4 text-[13px] font-medium text-white hover:bg-[#2b3442]"
              onClick={() => onRespond(true, scope)}
            >
              {confirmLabel}
            </button>
          </div>
        </footer>
      </div>
    </div>
  );
}
