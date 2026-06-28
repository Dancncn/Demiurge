import { RefObject, useEffect, useRef, useState } from "react";
import {
  attachmentAccept,
  attachmentKindLabel,
  formatAttachmentSize,
  processFiles,
  releaseAttachment,
  type ProcessedAttachment,
} from "../lib/fileProcessing";
import { ArrowUpIcon, CloseIcon, FileIcon, FolderIcon, PaperclipIcon, StopIcon } from "./Icons";

type Props = {
  input: string;
  canSend: boolean;
  loading: boolean;
  textareaRef: RefObject<HTMLTextAreaElement>;
  onSubmit: (attachments: ProcessedAttachment[]) => Promise<boolean> | boolean;
  onStop: () => void;
  onInputChange: (value: string) => void;
  onOpenSandbox: () => void;
};

export function Composer({
  input,
  canSend,
  loading,
  textareaRef,
  onSubmit,
  onStop,
  onInputChange,
  onOpenSandbox,
}: Props) {
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const attachmentsRef = useRef<ProcessedAttachment[]>([]);
  const [attachments, setAttachments] = useState<ProcessedAttachment[]>([]);
  const [processingFiles, setProcessingFiles] = useState(false);

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
      const next = await processFiles(Array.from(files));
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
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      void submit();
    }
  }

  return (
    <div className="shrink-0 border-t border-[#eceff3] bg-white/95 px-4 pb-4 pt-3">
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void submit();
        }}
        className="mx-auto w-full max-w-3xl"
      >
        <div className="rounded-lg border border-[#dfe3e8] bg-[#fbfcfd] p-2 shadow-[0_1px_2px_rgba(15,23,42,0.06)] transition">
          {attachments.length > 0 && (
            <div className="mb-2 flex max-h-28 flex-wrap gap-2 overflow-y-auto px-2 pt-1">
              {attachments.map((attachment) => (
                <div
                  key={attachment.id}
                  className={`group flex max-w-full items-center gap-2 rounded-md border px-2 py-1.5 text-[12px] ${
                    attachment.status === "error"
                      ? "border-[#fecaca] bg-[#fff1f2] text-[#991b1b]"
                      : "border-[#dfe3e8] bg-white text-[#3f4652]"
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
            onChange={(event) => onInputChange(event.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Ask anything, or attach text, images, PDF, DOCX, PPTX, XLSX..."
            className="min-h-11 max-h-40 w-full resize-none bg-transparent px-3 py-2.5 text-[14px] leading-6 outline-none placeholder:text-[#8a9099]"
          />

          <div className="flex items-center justify-between gap-2 px-2 pb-1">
            <div className="flex items-center gap-2">
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
                className="flex h-8 items-center gap-1.5 rounded-md border border-[#d9dfe7] bg-white px-2.5 text-[12px] font-medium text-[#4f5661] transition hover:bg-[#f5f6f8] disabled:cursor-not-allowed disabled:opacity-60"
                aria-label="Attach files"
              >
                <PaperclipIcon size={16} />
                {processingFiles ? "Reading" : "Attach"}
              </button>
              <button
                type="button"
                onClick={onOpenSandbox}
                className="flex h-8 items-center gap-1.5 rounded-md border border-[#d9dfe7] bg-white px-2.5 text-[12px] font-medium text-[#4f5661] transition hover:bg-[#f5f6f8]"
                aria-label="打开沙盒目录"
              >
                <FolderIcon size={17} />
                沙盒
              </button>
            </div>

            <button
              type={loading ? "button" : "submit"}
              onClick={loading ? onStop : undefined}
              disabled={!loading && !readyToSend}
              className="grid h-8 w-8 shrink-0 place-items-center rounded-md bg-[#111827] text-white transition hover:bg-[#2b3442] disabled:bg-[#c7ccd4] disabled:hover:bg-[#c7ccd4]"
              aria-label={loading ? "停止生成" : "发送"}
            >
              {loading ? <StopIcon size={15} /> : <ArrowUpIcon size={19} />}
            </button>
          </div>
        </div>
        <p className="mx-auto mt-2 max-w-3xl px-1 text-center text-[11px] text-[#8a9099]">
          Demiurge 会调用工具操作你的机器，重要操作前会请你确认。
        </p>
      </form>
    </div>
  );
}
