import { RefObject, useEffect, useRef, useState } from "react";
import * as api from "../lib/api";
import {
  attachmentAccept,
  attachmentKindLabel,
  formatAttachmentSize,
  processFiles,
  releaseAttachment,
  type ProcessedAttachment,
} from "../lib/fileProcessing";
import { ArrowUpIcon, CloseIcon, FileIcon, MicIcon, PaperclipIcon, StopIcon } from "./Icons";
import { Select } from "./Select";
import { ContextMeter } from "./ContextMeter";
import { findProvider, REASONING_EFFORTS } from "../lib/providers";
import type { PermissionMode, ProviderKind, ReasoningEffort } from "../lib/types";

const PERMISSION_MODE_LABELS: Record<PermissionMode, string> = {
  plan: "Plan",
  default: "Default",
  auto: "Auto",
  bypass: "Bypass",
};

type SlashCommand = { name: string; args: string; desc: string };

// Slash commands handled by the backend send dispatcher (src-tauri/src/lib.rs).
const SLASH_COMMANDS: SlashCommand[] = [
  { name: "/goal", args: "<objective> [+budget]", desc: "Set a persistent goal the agent keeps driving" },
  { name: "/effort", args: "[auto|low|medium|high|xhigh|max]", desc: "Switch reasoning effort for supported models" },
  { name: "/compact", args: "[keep=N]", desc: "Collapse earlier context to save tokens" },
  { name: "/ultracode", args: "<task>", desc: "Multi-agent orchestration overlay" },
  { name: "/workflows", args: "", desc: "List workflow runs" },
  { name: "/workflow", args: "resume <run_id>", desc: "Resume a workflow from its journal" },
  { name: "/skills", args: "", desc: "List available skills" },
  { name: "/dream", args: "", desc: "Tidy long-term memory in the background" },
];

const MAX_TEXTAREA_HEIGHT = 200;

type Props = {
  input: string;
  canSend: boolean;
  loading: boolean;
  permissionMode: PermissionMode;
  onSetPermissionMode: (mode: PermissionMode) => void;
  provider: ProviderKind;
  model: string;
  reasoningEffort: ReasoningEffort;
  maxInputTokens: number;
  onSetModel: (model: string) => void;
  onSetEffort: (effort: ReasoningEffort) => void;
  onOpenSettings: () => void;
  textareaRef: RefObject<HTMLTextAreaElement>;
  onSubmit: (attachments: ProcessedAttachment[]) => Promise<boolean> | boolean;
  onStop: () => void;
  onInputChange: (value: string) => void;
};

const ghostChip =
  "flex h-7 max-w-[150px] items-center gap-1 rounded-md px-2 text-[12px] font-medium text-[#5f6368] outline-none transition hover:bg-[#eef1f5]";
const ghostIcon = "grid size-7 shrink-0 place-items-center rounded-md text-[#6f7782] transition hover:bg-[#eef1f5]";

export function Composer({
  input,
  canSend,
  loading,
  permissionMode,
  onSetPermissionMode,
  provider,
  model,
  reasoningEffort,
  maxInputTokens,
  onSetModel,
  onSetEffort,
  onOpenSettings,
  textareaRef,
  onSubmit,
  onStop,
  onInputChange,
}: Props) {
  const providerModels = findProvider(provider).models;
  const modelOptions = (model && !providerModels.includes(model) ? [model, ...providerModels] : providerModels).map(
    (m) => ({ value: m, label: m }),
  );
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const attachmentsRef = useRef<ProcessedAttachment[]>([]);
  const [attachments, setAttachments] = useState<ProcessedAttachment[]>([]);
  const [processingFiles, setProcessingFiles] = useState(false);
  const [cmdActive, setCmdActive] = useState(0);
  const [cmdDismissed, setCmdDismissed] = useState(false);

  // Slash-command palette: show while typing the leading "/command" token (before any space).
  const cmdToken = input.startsWith("/") && !input.includes(" ") ? input.toLowerCase() : null;
  const cmdMatches = cmdToken ? SLASH_COMMANDS.filter((c) => c.name.startsWith(cmdToken)) : [];
  const paletteOpen = !cmdDismissed && cmdMatches.length > 0;

  // Auto-grow the textarea up to a max height, then keep it fixed and scroll.
  useEffect(() => {
    const el = textareaRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, MAX_TEXTAREA_HEIGHT)}px`;
  }, [input, textareaRef]);

  function changeInput(value: string) {
    if (!value.startsWith("/")) setCmdDismissed(false);
    setCmdActive(0);
    onInputChange(value);
  }

  function applyCommand(c: SlashCommand) {
    onInputChange(`${c.name} `);
    setCmdActive(0);
    setCmdDismissed(false);
    requestAnimationFrame(() => textareaRef.current?.focus());
  }

  useEffect(() => {
    attachmentsRef.current = attachments;
  }, [attachments]);

  useEffect(() => {
    return () => {
      attachmentsRef.current.forEach(releaseAttachment);
    };
  }, []);

  const attachmentReady = attachments.some((attachment) => attachment.status === "ready");
  const readyToSend = (canSend || attachmentReady) && !processingFiles;

  async function addFiles(files: FileList | null) {
    if (!files?.length) return;
    setProcessingFiles(true);
    try {
      const next = await processFiles(Array.from(files), { extractImageText: api.ocrImageBytes });
      setAttachments((prev) => [...prev, ...next]);
    } finally {
      setProcessingFiles(false);
      if (fileInputRef.current) fileInputRef.current.value = "";
    }
  }

  function removeAttachment(id: string) {
    setAttachments((prev) => {
      const target = prev.find((attachment) => attachment.id === id);
      if (target) releaseAttachment(target);
      return prev.filter((attachment) => attachment.id !== id);
    });
  }

  async function submit() {
    if (loading) {
      onStop();
      return;
    }
    if (!readyToSend) return;
    const sent = await onSubmit(attachments);
    if (sent) {
      attachments.forEach(releaseAttachment);
      setAttachments([]);
    }
  }

  function handleKeyDown(event: React.KeyboardEvent<HTMLTextAreaElement>) {
    if (event.nativeEvent.isComposing) return;
    if (paletteOpen) {
      const len = cmdMatches.length;
      if (event.key === "ArrowDown") {
        event.preventDefault();
        setCmdActive((i) => (i + 1) % len);
        return;
      }
      if (event.key === "ArrowUp") {
        event.preventDefault();
        setCmdActive((i) => (i - 1 + len) % len);
        return;
      }
      if (event.key === "Tab" || (event.key === "Enter" && !event.shiftKey)) {
        event.preventDefault();
        applyCommand(cmdMatches[cmdActive] ?? cmdMatches[0]);
        return;
      }
      if (event.key === "Escape") {
        event.preventDefault();
        setCmdDismissed(true);
        return;
      }
    }
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      void submit();
    }
  }

  const permissionTrigger = `flex h-7 items-center gap-1 rounded-md px-2 text-[12px] font-medium outline-none transition ${
    permissionMode === "bypass"
      ? "bg-[#fff0f0] text-[#b42318] hover:bg-[#ffe6e6]"
      : permissionMode === "plan"
        ? "bg-[#eef5ff] text-[#0b57d0] hover:bg-[#e3efff]"
        : "text-[#5f6368] hover:bg-[#eef1f5]"
  }`;

  return (
    <div className="shrink-0 px-4 pb-3 pt-2">
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void submit();
        }}
        className="relative mx-auto w-full max-w-3xl"
      >
        {paletteOpen && (
          <div className="cf-menu-in absolute bottom-[calc(100%+8px)] left-0 right-0 z-30 overflow-hidden rounded-lg border border-[#e2e5ea] bg-white shadow-[0_12px_36px_rgba(15,23,42,0.16)]">
            <div className="border-b border-[#eef1f4] px-3 py-1.5 text-[11px] font-semibold uppercase tracking-wide text-[#8a9099]">
              Commands
            </div>
            <div className="max-h-64 overflow-y-auto p-1">
              {cmdMatches.map((c, i) => (
                <button
                  key={c.name}
                  type="button"
                  onMouseEnter={() => setCmdActive(i)}
                  onClick={() => applyCommand(c)}
                  className={`flex w-full items-center gap-3 rounded-md px-2.5 py-2 text-left transition ${
                    i === cmdActive ? "bg-[#eef1f5]" : "hover:bg-[#f3f4f7]"
                  }`}
                >
                  <span className="font-mono text-[13px] font-medium text-[#111827]">{c.name}</span>
                  {c.args && <span className="font-mono text-[12px] text-[#9aa1ab]">{c.args}</span>}
                  <span className="ml-auto min-w-0 truncate text-[12px] text-[#7a8088]">{c.desc}</span>
                </button>
              ))}
            </div>
          </div>
        )}

        {/* Input box — kept clean; only the textarea lives inside. */}
        <div className="rounded-2xl border border-[#dfe3e8] bg-white px-1 py-1 shadow-[0_1px_3px_rgba(15,23,42,0.06)] transition focus-within:border-[#c2c8d0] focus-within:shadow-[0_2px_10px_rgba(15,23,42,0.08)]">
          {attachments.length > 0 && (
            <div className="mb-1 flex max-h-28 flex-wrap gap-2 overflow-y-auto px-2 pt-1">
              {attachments.map((attachment) => (
                <div
                  key={attachment.id}
                  className={`group flex max-w-full items-center gap-2 rounded-md border px-2 py-1.5 text-[12px] ${
                    attachment.status === "error"
                      ? "border-[#fecaca] bg-[#fff1f2] text-[#991b1b]"
                      : "border-[#dfe3e8] bg-[#fbfcfd] text-[#3f4652]"
                  }`}
                  title={attachment.error || attachment.note || attachment.name}
                >
                  {attachment.previewUrl ? (
                    <img src={attachment.previewUrl} alt="" className="size-6 rounded border border-[#e5e8ed] object-cover" />
                  ) : (
                    <FileIcon size={16} className="shrink-0 text-[#7a8088]" />
                  )}
                  <span className="min-w-0">
                    <span className="block max-w-[220px] truncate font-medium">{attachment.name}</span>
                    <span className="block text-[11px] text-[#8a9099]">
                      {attachment.status === "error"
                        ? "Failed"
                        : `${attachmentKindLabel(attachment.kind)} · ${formatAttachmentSize(attachment.size)}`}
                    </span>
                  </span>
                  <button
                    type="button"
                    onClick={() => removeAttachment(attachment.id)}
                    className="grid size-6 shrink-0 place-items-center rounded-md text-[#7a8088] opacity-80 transition hover:bg-[#eef1f5] hover:text-[#202124] group-hover:opacity-100"
                    aria-label={`Remove ${attachment.name}`}
                  >
                    <CloseIcon size={14} />
                  </button>
                </div>
              ))}
            </div>
          )}
          <textarea
            ref={textareaRef}
            rows={1}
            value={input}
            onChange={(event) => changeInput(event.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Ask anything — type / for commands"
            style={{ maxHeight: MAX_TEXTAREA_HEIGHT }}
            className="block max-h-[200px] min-h-[28px] w-full resize-none overflow-y-auto bg-transparent px-3 py-2 text-[14px] leading-6 outline-none placeholder:text-[#8a9099]"
          />
        </div>

        {/* Control strip — separated from the input box; smaller, lighter buttons. */}
        <div className="mt-1.5 flex items-center justify-between gap-2 px-0.5">
          <div className="flex min-w-0 items-center gap-0.5">
            <input
              ref={fileInputRef}
              type="file"
              multiple
              accept={attachmentAccept}
              className="hidden"
              onChange={(event) => void addFiles(event.target.files)}
            />
            <button
              type="button"
              onClick={() => fileInputRef.current?.click()}
              disabled={loading || processingFiles}
              title={processingFiles ? "Reading files…" : "Attach files"}
              aria-label="Attach files"
              className={`${ghostIcon} disabled:cursor-not-allowed disabled:opacity-50`}
            >
              <PaperclipIcon size={16} />
            </button>
            <button
              type="button"
              onClick={onOpenSettings}
              title="Voice input (configure in Settings)"
              aria-label="Voice input"
              className={`${ghostIcon} text-[#9aa1ab]`}
            >
              <MicIcon size={16} />
            </button>

            <span className="mx-1 h-4 w-px bg-[#e5e8ed]" aria-hidden />

            <Select
              value={permissionMode}
              onChange={(v) => onSetPermissionMode(v as PermissionMode)}
              direction="up"
              options={(Object.keys(PERMISSION_MODE_LABELS) as PermissionMode[]).map((mode) => ({
                value: mode,
                label: PERMISSION_MODE_LABELS[mode],
              }))}
              triggerClassName={permissionTrigger}
            />
          </div>

          <div className="flex min-w-0 items-center gap-0.5">
            <Select
              value={model}
              onChange={onSetModel}
              direction="up"
              align="right"
              placeholder="Model"
              options={modelOptions}
              triggerClassName={ghostChip}
            />
            <Select
              value={reasoningEffort}
              onChange={(v) => onSetEffort(v as ReasoningEffort)}
              direction="up"
              align="right"
              options={REASONING_EFFORTS.map((e) => ({ value: e.value, label: e.label, hint: e.hint }))}
              triggerClassName={ghostChip}
            />
            <ContextMeter maxInputTokens={maxInputTokens} onOpenSettings={onOpenSettings} />
            <button
              type={loading ? "button" : "submit"}
              onClick={loading ? onStop : undefined}
              disabled={!loading && !readyToSend}
              className="ml-0.5 grid size-8 shrink-0 place-items-center rounded-full bg-[#111827] text-white transition hover:bg-[#2b3442] disabled:bg-[#c7ccd4] disabled:hover:bg-[#c7ccd4]"
              aria-label={loading ? "Stop" : "Send"}
            >
              {loading ? <StopIcon size={14} /> : <ArrowUpIcon size={18} />}
            </button>
          </div>
        </div>
      </form>
    </div>
  );
}
