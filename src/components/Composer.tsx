import { FormEvent, KeyboardEvent, RefObject } from "react";
import { ArrowUpIcon, FolderIcon, StopIcon } from "./Icons";

type Props = {
  input: string;
  sidebarOpen: boolean;
  canSend: boolean;
  loading: boolean;
  textareaRef: RefObject<HTMLTextAreaElement>;
  onSubmit: (event: FormEvent) => void;
  onStop: () => void;
  onInputChange: (value: string) => void;
  onKeyDown: (event: KeyboardEvent<HTMLTextAreaElement>) => void;
  onOpenSandbox: () => void;
};

export function Composer({
  input,
  sidebarOpen,
  canSend,
  loading,
  textareaRef,
  onSubmit,
  onStop,
  onInputChange,
  onKeyDown,
  onOpenSandbox,
}: Props) {
  return (
    <div
      className={`pointer-events-none fixed bottom-0 left-0 right-0 bg-gradient-to-t from-[#f6f0fb] via-[#f6f0fb] to-transparent pb-5 pt-14 transition-[left,right] duration-200 ${
        sidebarOpen ? "md:left-[230px]" : "md:left-[64px]"
      }`}
    >
      <form onSubmit={onSubmit} className="pointer-events-auto mx-auto w-full max-w-3xl px-4">
        <div className="rounded-[28px] border border-[#e1e1e1] bg-white p-2 shadow-[0_0_0_1px_rgba(0,0,0,0.02),0_8px_28px_rgba(0,0,0,0.08)] transition">
          <textarea
            ref={textareaRef}
            rows={1}
            value={input}
            onChange={(event) => onInputChange(event.target.value)}
            onKeyDown={onKeyDown}
            placeholder="和你的桌面伴侣聊点什么…"
            className="min-h-12 max-h-44 w-full resize-none bg-transparent px-4 py-3 text-[15px] leading-6 outline-none placeholder:text-[#8a8a8a]"
          />

          <div className="flex items-center justify-between gap-2 px-2 pb-1">
            <button
              type="button"
              onClick={onOpenSandbox}
              className="flex h-9 items-center gap-1.5 rounded-full border border-[#e5e5e5] px-3 text-sm text-[#3f3f3f] transition hover:bg-[#f7f7f7]"
              aria-label="打开沙盒目录"
            >
              <FolderIcon size={17} />
              沙盒
            </button>

            <button
              type={loading ? "button" : "submit"}
              onClick={loading ? onStop : undefined}
              disabled={!loading && !canSend}
              className="grid h-9 w-9 shrink-0 place-items-center rounded-full bg-[#111] text-white transition hover:bg-[#333] disabled:bg-[#d7d7d7] disabled:hover:bg-[#d7d7d7]"
              aria-label={loading ? "停止生成" : "发送"}
            >
              {loading ? <StopIcon size={15} /> : <ArrowUpIcon size={19} />}
            </button>
          </div>
        </div>
        <p className="mx-auto mt-2 max-w-3xl px-1 text-center text-xs text-[#9a9a9a]">
          Demiurge 会调用工具操作你的机器，重要操作前会请你确认。
        </p>
      </form>
    </div>
  );
}
