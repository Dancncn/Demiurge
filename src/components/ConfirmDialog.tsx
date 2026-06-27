import type { ConfirmRequestEvent } from "../lib/types";

interface Props {
  req: ConfirmRequestEvent | null;
  onRespond: (allow: boolean) => void;
}

export default function ConfirmDialog({ req, onRespond }: Props) {
  if (!req) return null;
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30 p-4 backdrop-blur-[2px]">
      <div className="w-full max-w-sm rounded-3xl border border-[#ececec] bg-white p-5 shadow-[0_24px_60px_rgba(0,0,0,0.18)]">
        <div className="mb-1 text-base font-semibold text-[#171717]">需要你的确认</div>
        <p className="mb-3 text-sm text-[#6f6f6f]">
          助手想执行一个有副作用的操作：
          <span className="font-medium text-[#0b57d0]"> {req.tool}</span>
        </p>
        <pre className="mb-4 max-h-48 overflow-auto whitespace-pre-wrap rounded-xl border border-[#ececec] bg-[#f7f7f7] p-3 text-xs text-[#3f3f3f]">
          {req.args}
        </pre>
        <div className="flex justify-end gap-2">
          <button
            className="rounded-full border border-[#e5e5e5] px-4 py-2 text-sm text-[#3f3f3f] transition hover:bg-[#f7f7f7]"
            onClick={() => onRespond(false)}
          >
            拒绝
          </button>
          <button
            className="rounded-full bg-[#111] px-4 py-2 text-sm text-white transition hover:bg-[#333]"
            onClick={() => onRespond(true)}
          >
            允许
          </button>
        </div>
      </div>
    </div>
  );
}
