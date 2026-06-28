import { useEffect, useState } from "react";
import type { ConfirmRequestEvent, PermissionMode, PermissionScope, ToolRisk } from "../lib/types";

interface Props {
  req: ConfirmRequestEvent | null;
  mode: PermissionMode;
  onRespond: (allow: boolean, scope: PermissionScope) => void;
}

function riskLabel(risk?: ToolRisk) {
  switch (risk) {
    case "read_only":
      return "只读";
    case "mutating":
      return "会修改文件";
    case "external":
      return "会访问外部服务";
    case "privileged":
      return "会调用系统能力";
    default:
      return "需要确认";
  }
}

export default function ConfirmDialog({ req, mode, onRespond }: Props) {
  const [scope, setScope] = useState<PermissionScope>("once");

  useEffect(() => {
    setScope("once");
  }, [req?.id]);

  if (!req) return null;
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30 p-4 backdrop-blur-[2px]">
      <div className="w-full max-w-sm rounded-3xl border border-[#ececec] bg-white p-5 shadow-[0_24px_60px_rgba(0,0,0,0.18)]">
        <div className="mb-1 text-base font-semibold text-[#171717]">需要你的确认</div>
        <p className="mb-3 text-sm text-[#6f6f6f]">
          助手想执行一个有副作用的操作：
          <span className="font-medium text-[#0b57d0]"> {req.tool}</span>
        </p>
        <div className="mb-3 space-y-1 rounded-xl border border-[#ececec] bg-[#fafafa] p-3 text-xs text-[#5f5f5f]">
          {req.summary && (
            <div className="rounded-lg bg-white px-2 py-1.5 text-[#333] shadow-sm">
              <span className="text-[#9a9a9a]">将要执行：</span>
              {req.summary}
            </div>
          )}
          <div>
            <span className="text-[#9a9a9a]">权限模式：</span>
            {mode}
          </div>
          <div>
            <span className="text-[#9a9a9a]">风险：</span>
            {riskLabel(req.risk)}
          </div>
          {req.description && (
            <div>
              <span className="text-[#9a9a9a]">说明：</span>
              {req.description}
            </div>
          )}
          {req.reason && (
            <div>
              <span className="text-[#9a9a9a]">原因：</span>
              {req.reason}
            </div>
          )}
        </div>
        <pre className="mb-3 max-h-48 overflow-auto whitespace-pre-wrap rounded-xl border border-[#ececec] bg-[#f7f7f7] p-3 text-xs text-[#3f3f3f]">
          {req.args}
        </pre>
        {req.preview && (
          <div className="mb-4">
            <div className="mb-1 text-xs font-medium text-[#6f6f6f]">执行预览</div>
            <pre className="max-h-64 overflow-auto whitespace-pre-wrap rounded-xl border border-[#e5e5e5] bg-[#111] p-3 font-mono text-xs leading-relaxed text-[#f2f2f2]">
              {req.preview}
            </pre>
          </div>
        )}
        <div className="mb-4 grid grid-cols-3 gap-2 rounded-2xl bg-[#f7f7f7] p-1 text-xs">
          {[
            ["once", "仅本次"],
            ["session", "本会话"],
            ["project", "本项目"],
          ].map(([value, label]) => (
            <button
              key={value}
              className={`rounded-xl px-3 py-2 transition ${
                scope === value ? "bg-white text-[#111] shadow-sm" : "text-[#6f6f6f] hover:text-[#333]"
              }`}
              onClick={() => setScope(value as PermissionScope)}
            >
              {label}
            </button>
          ))}
        </div>
        <div className="flex justify-end gap-2">
          <button
            className="rounded-full border border-[#e5e5e5] px-4 py-2 text-sm text-[#3f3f3f] transition hover:bg-[#f7f7f7]"
            onClick={() => onRespond(false, scope)}
          >
            拒绝
          </button>
          <button
            className="rounded-full bg-[#111] px-4 py-2 text-sm text-white transition hover:bg-[#333]"
            onClick={() => onRespond(true, scope)}
          >
            允许
          </button>
        </div>
      </div>
    </div>
  );
}
