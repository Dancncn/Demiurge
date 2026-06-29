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
import {
  ArrowUpIcon,
  CheckIcon,
  ChevronDownIcon,
  CloseIcon,
  FileIcon,
  MicIcon,
  PaperclipIcon,
  StopIcon,
} from "./Icons";
import { Select } from "./Select";
import { ContextMeter } from "./ContextMeter";
import { findProvider, REASONING_EFFORTS } from "../lib/providers";
import { useI18n } from "../lib/i18n";
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

// 选中麦克风设备的持久化 key（仅前端，不进 Settings/后端）。
const VOICE_DEVICE_STORAGE_KEY = "demiurge.voiceInputDeviceId";

type MicDevice = { deviceId: string; label: string };

// 浏览器是否具备录音能力（非 Tauri/预览环境或无权限 API 时为 false）。
function mediaRecordingSupported() {
  return (
    typeof navigator !== "undefined" &&
    !!navigator.mediaDevices &&
    typeof navigator.mediaDevices.getUserMedia === "function" &&
    typeof window !== "undefined" &&
    typeof window.MediaRecorder !== "undefined"
  );
}

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
const ghostIcon = "cf-press grid size-7 shrink-0 place-items-center rounded-md text-[#6f7782] hover:bg-[#eef1f5]";

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
  const { t } = useI18n();
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

  // —— 语音输入 ——
  const voiceSupported = mediaRecordingSupported();
  const [recording, setRecording] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const [devices, setDevices] = useState<MicDevice[]>([]);
  const [permissionDenied, setPermissionDenied] = useState(false);
  const [voiceToast, setVoiceToast] = useState<string | null>(null);
  const [selectedDeviceId, setSelectedDeviceId] = useState<string>(() => {
    if (typeof localStorage === "undefined") return "";
    return localStorage.getItem(VOICE_DEVICE_STORAGE_KEY) ?? "";
  });

  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  const mediaStreamRef = useRef<MediaStream | null>(null);
  const chunksRef = useRef<Blob[]>([]);
  const voiceMenuRef = useRef<HTMLDivElement | null>(null);
  const voiceToastTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const inputRef = useRef(input);
  useEffect(() => {
    inputRef.current = input;
  }, [input]);

  function showVoiceToast(message: string) {
    setVoiceToast(message);
    if (voiceToastTimerRef.current) clearTimeout(voiceToastTimerRef.current);
    voiceToastTimerRef.current = setTimeout(() => setVoiceToast(null), 2600);
  }

  // 释放当前占用的麦克风流（停止所有 track）。
  function releaseStream() {
    mediaStreamRef.current?.getTracks().forEach((track) => track.stop());
    mediaStreamRef.current = null;
  }

  // 枚举音频输入设备；label 为空时回退为「麦克风 N」。
  async function refreshDevices() {
    if (!voiceSupported || !navigator.mediaDevices.enumerateDevices) return;
    try {
      const all = await navigator.mediaDevices.enumerateDevices();
      const mics = all
        .filter((d) => d.kind === "audioinput")
        .map((d, i) => ({ deviceId: d.deviceId, label: d.label || t("composer.voiceMicN", { n: i + 1 }) }));
      setDevices(mics);
    } catch {
      // 枚举失败时保持现有列表，避免清空。
    }
  }

  // 设备插拔时刷新列表（仅在浏览器支持时挂载监听）。
  useEffect(() => {
    if (!voiceSupported) return;
    const md = navigator.mediaDevices;
    const onChange = () => void refreshDevices();
    md.addEventListener?.("devicechange", onChange);
    return () => md.removeEventListener?.("devicechange", onChange);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [voiceSupported]);

  // 卸载时停止录音并释放麦克风，清除 toast 计时器。
  useEffect(() => {
    return () => {
      try {
        mediaRecorderRef.current?.stop();
      } catch {
        /* ignore */
      }
      releaseStream();
      if (voiceToastTimerRef.current) clearTimeout(voiceToastTimerRef.current);
    };
  }, []);

  // 打开设备菜单：先触发一次授权以拿到 label，再枚举。
  async function openVoiceMenu() {
    setMenuOpen(true);
    if (!voiceSupported) return;
    try {
      const probe = await navigator.mediaDevices.getUserMedia({ audio: true });
      probe.getTracks().forEach((t) => t.stop()); // 立即释放探测流
      setPermissionDenied(false);
    } catch {
      setPermissionDenied(true);
    }
    await refreshDevices();
  }

  function pickDevice(deviceId: string) {
    setSelectedDeviceId(deviceId);
    try {
      localStorage.setItem(VOICE_DEVICE_STORAGE_KEY, deviceId);
    } catch {
      /* localStorage 不可用时忽略持久化 */
    }
    setMenuOpen(false);
  }

  // 点击外部 / Esc 关闭设备菜单。
  useEffect(() => {
    if (!menuOpen) return;
    const onDown = (e: MouseEvent) => {
      if (!voiceMenuRef.current?.contains(e.target as Node)) setMenuOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setMenuOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [menuOpen]);

  // 把录音 Blob 通过后端 STT 转写为文本并追加到输入框。
  async function transcribe(blob: Blob) {
    try {
      const status = await api.voiceStatus();
      if (!status.ready) {
        showVoiceToast(status.reason || t("composer.voiceBackendMissing"));
        return;
      }
      showVoiceToast(t("composer.voiceTranscribing"));
      const audio = Array.from(new Uint8Array(await blob.arrayBuffer()));
      const text = await api.voiceTranscribe(audio, blob.type || "audio/webm");
      if (text && text.trim()) appendTranscript(text);
      else showVoiceToast(t("composer.voiceBackendMissing"));
    } catch (e) {
      showVoiceToast(String(e) || t("composer.voiceBackendMissing"));
    }
  }

  // 将转写文本追加到当前输入（保留用户已有内容）。供将来转写成功时调用。
  function appendTranscript(text: string) {
    const trimmed = text.trim();
    if (!trimmed) return;
    const current = inputRef.current;
    onInputChange(current ? `${current}${current.endsWith(" ") ? "" : " "}${trimmed}` : trimmed);
  }

  async function startRecording() {
    if (!voiceSupported) {
      showVoiceToast(t("composer.voiceUnsupported"));
      return;
    }
    try {
      const constraints: MediaStreamConstraints = {
        audio: selectedDeviceId ? { deviceId: { exact: selectedDeviceId } } : true,
      };
      const stream = await navigator.mediaDevices.getUserMedia(constraints);
      mediaStreamRef.current = stream;
      setPermissionDenied(false);
      chunksRef.current = [];
      const recorder = new MediaRecorder(stream);
      mediaRecorderRef.current = recorder;
      recorder.ondataavailable = (e) => {
        if (e.data && e.data.size > 0) chunksRef.current.push(e.data);
      };
      recorder.onstop = () => {
        const blob = new Blob(chunksRef.current, { type: recorder.mimeType || "audio/webm" });
        chunksRef.current = [];
        releaseStream();
        setRecording(false);
        if (blob.size > 0) void transcribe(blob);
      };
      recorder.start();
      setRecording(true);
      // 拿到真实授权后刷新设备 label，方便菜单展示。
      void refreshDevices();
    } catch {
      releaseStream();
      setRecording(false);
      setPermissionDenied(true);
      showVoiceToast(t("composer.voiceMicDenied"));
    }
  }

  function stopRecording() {
    const recorder = mediaRecorderRef.current;
    if (recorder && recorder.state !== "inactive") {
      recorder.stop(); // onstop 中收集 blob 并释放流
    } else {
      releaseStream();
      setRecording(false);
    }
  }

  function toggleRecording() {
    if (recording) stopRecording();
    else void startRecording();
  }

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
              {t("composer.commands")}
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
                        ? t("composer.failed")
                        : `${attachmentKindLabel(attachment.kind)} · ${formatAttachmentSize(attachment.size)}`}
                    </span>
                  </span>
                  <button
                    type="button"
                    onClick={() => removeAttachment(attachment.id)}
                    className="grid size-6 shrink-0 place-items-center rounded-md text-[#7a8088] opacity-80 transition hover:bg-[#eef1f5] hover:text-[#202124] group-hover:opacity-100"
                    aria-label={t("composer.removeAttachment", { name: attachment.name })}
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
            placeholder={t("composer.placeholder")}
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
              title={processingFiles ? t("composer.reading") : t("composer.attach")}
              aria-label={t("composer.attach")}
              className={`${ghostIcon} disabled:cursor-not-allowed disabled:opacity-50`}
            >
              <PaperclipIcon size={16} />
            </button>
            {/* 语音输入：录音按钮 + 设备选择箭头 + 向上弹出的设备菜单 */}
            <div ref={voiceMenuRef} className="relative flex items-center">
              <button
                type="button"
                onClick={toggleRecording}
                disabled={!voiceSupported}
                title={
                  !voiceSupported
                    ? t("composer.voiceUnsupported")
                    : recording
                      ? t("composer.voiceStop")
                      : t("composer.voiceStart")
                }
                aria-label={recording ? t("composer.voiceStop") : t("composer.voiceStart")}
                className={`${ghostIcon} ${
                  recording ? "animate-pulse text-[#dc2626] hover:bg-[#fdecec]" : "text-[#9aa1ab]"
                } disabled:cursor-not-allowed disabled:opacity-50`}
              >
                <MicIcon size={16} />
              </button>
              <button
                type="button"
                onClick={() => (menuOpen ? setMenuOpen(false) : void openVoiceMenu())}
                title={t("composer.voicePick")}
                aria-label={t("composer.voicePick")}
                aria-haspopup="menu"
                aria-expanded={menuOpen}
                className="cf-press -ml-1 grid h-7 w-4 shrink-0 place-items-center rounded-md text-[#9aa1ab] hover:bg-[#eef1f5]"
              >
                <ChevronDownIcon
                  size={12}
                  className={`transition-transform duration-150 ${menuOpen ? "rotate-180" : ""}`}
                />
              </button>

              {menuOpen && (
                <div className="cf-menu-in absolute bottom-[calc(100%+6px)] left-0 z-30 max-h-[280px] w-56 overflow-y-auto rounded-lg border border-[#dfe3e8] bg-white p-1 shadow-[0_12px_36px_rgba(15,23,42,0.16)]">
                  <div className="px-2.5 py-1.5 text-[11px] font-semibold uppercase tracking-wide text-[#8a9099]">
                    {t("composer.voiceDevices")}
                  </div>
                  {permissionDenied ? (
                    <div className="px-2.5 py-2 text-[12px] text-[#b42318]">{t("composer.voicePermission")}</div>
                  ) : devices.length === 0 ? (
                    <div className="px-2.5 py-2 text-[12px] text-[#8a9099]">{t("composer.voiceNoDevices")}</div>
                  ) : (
                    devices.map((d) => {
                      const active = d.deviceId === selectedDeviceId;
                      return (
                        <button
                          key={d.deviceId || d.label}
                          type="button"
                          onClick={() => pickDevice(d.deviceId)}
                          className={`flex w-full items-center gap-2 rounded-md px-2.5 py-2 text-left transition ${
                            active ? "bg-[#eef1f5] text-[#111827]" : "text-[#3f444c] hover:bg-[#f3f4f7]"
                          }`}
                        >
                          <span className="min-w-0 flex-1 truncate text-[13px] font-medium">{d.label}</span>
                          {active && <CheckIcon size={15} className="shrink-0 text-[#111827]" />}
                        </button>
                      );
                    })
                  )}
                </div>
              )}

              {voiceToast && !menuOpen && (
                <div className="cf-menu-in absolute bottom-[calc(100%+6px)] left-0 z-30 w-60 rounded-lg border border-[#dfe3e8] bg-white px-3 py-2 text-[12px] text-[#3f444c] shadow-[0_12px_36px_rgba(15,23,42,0.16)]">
                  {voiceToast}
                </div>
              )}
            </div>

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
              placeholder={t("composer.model")}
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
              className="cf-press ml-0.5 grid size-8 shrink-0 place-items-center rounded-full bg-[#111827] text-white hover:scale-105 hover:bg-[#2b3442] disabled:scale-100 disabled:bg-[#c7ccd4] disabled:hover:bg-[#c7ccd4]"
              aria-label={loading ? t("composer.stop") : t("composer.send")}
            >
              {loading ? <StopIcon size={14} /> : <ArrowUpIcon size={18} />}
            </button>
          </div>
        </div>
      </form>
    </div>
  );
}
