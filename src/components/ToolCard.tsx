import { useState } from "react";
import type { ToolRisk } from "../lib/types";
import { WrenchIcon } from "./Icons";

interface Props {
  name: string;
  args: unknown;
  status: "running" | "done" | "denied";
  result?: string;
  description?: string;
  risk?: ToolRisk;
}

function badge(status: Props["status"]) {
  switch (status) {
    case "running":
      return { label: "执行中…", cls: "bg-[#eaf2ff] text-[#0b57d0]" };
    case "denied":
      return { label: "已拒绝", cls: "bg-[#fef2f2] text-[#dc2626]" };
    default:
      return { label: "完成", cls: "bg-[#eafaf1] text-[#0a7d4d]" };
  }
}

function riskLabel(risk?: ToolRisk) {
  switch (risk) {
    case "read_only":
      return "只读";
    case "mutating":
      return "会修改";
    case "external":
      return "外部访问";
    case "privileged":
      return "系统操作";
    default:
      return null;
  }
}

function progressSummary(name: string, status: Props["status"], args: unknown, result?: string) {
  if (name.startsWith("mcp__")) {
    const parts = name.split("__");
    const server = parts[1] || "server";
    const tool = parts.slice(2).join("__") || name;
    if (status === "running") return `正在调用 MCP ${server} / ${tool}`;
    if (status === "done") {
      const chars = result?.length ?? 0;
      return chars > 0 ? `MCP 工具完成，返回 ${chars} 字符` : "MCP 工具完成";
    }
  }
  if (name === "web_search") {
    const query = typeof args === "object" && args && "query" in args ? String((args as { query?: unknown }).query ?? "") : "";
    if (status === "running") return `正在搜索${query ? `：${query}` : ""}`;
    if (status === "done") {
      const count = result?.match(/^\d+\. \[/gm)?.length ?? 0;
      return count > 0 ? `已返回 ${count} 条来源链接` : "搜索完成，未提取到来源链接";
    }
  }
  if (status === "running") return "等待工具返回结果";
  return null;
}

export default function ToolCard({ name, args, status, result, description, risk }: Props) {
  const [open, setOpen] = useState(false);
  const b = badge(status);
  const riskText = riskLabel(risk);
  const progressText = progressSummary(name, status, args, result);
  let argsText = "";
  try {
    argsText = JSON.stringify(args, null, 2);
  } catch {
    argsText = String(args);
  }

  return (
    <div className="cf-message-in rounded-xl bg-[#f7f7f7] text-sm text-[#5f5f5f]">
      <button
        type="button"
        className="flex w-full items-center gap-2 px-4 py-3 text-left"
        onClick={() => setOpen((v) => !v)}
      >
        <WrenchIcon size={16} className="shrink-0 text-[#0b57d0]" />
        <span className="font-medium text-[#3f3f3f]">{name}</span>
        <span className={`rounded-full px-2 py-0.5 text-xs ${b.cls}`}>{b.label}</span>
        {riskText && <span className="rounded-full bg-white px-2 py-0.5 text-xs text-[#6f6f6f]">{riskText}</span>}
        {status === "running" && (
          <span className="cf-dots shrink-0 text-[#b4b4b4]">
            <span />
            <span />
            <span />
          </span>
        )}
        <span className="ml-auto text-xs text-[#9a9a9a]">{open ? "收起" : "详情"}</span>
      </button>
      {progressText && (
        <div className="border-t border-[#ececec] px-4 py-2 text-xs text-[#6f6f6f]">
          <div className="h-1.5 overflow-hidden rounded-full bg-white">
            <div className={`h-full rounded-full ${status === "running" ? "w-1/2 animate-pulse bg-[#0b57d0]" : "w-full bg-[#10a37f]"}`} />
          </div>
          <div className="mt-1.5">{progressText}</div>
        </div>
      )}
      {open && (
        <div className="space-y-2 border-t border-[#ececec] px-4 py-3">
          {description && <div className="text-xs leading-relaxed text-[#6f6f6f]">{description}</div>}
          <div>
            <div className="mb-1 text-xs text-[#9a9a9a]">参数</div>
            <pre className="overflow-x-auto rounded-lg border border-[#ececec] bg-white p-2.5 text-xs text-[#3f3f3f]">
              {argsText}
            </pre>
          </div>
          {result && (
            <div>
              <div className="mb-1 text-xs text-[#9a9a9a]">结果</div>
              <pre className="max-h-60 overflow-auto whitespace-pre-wrap rounded-lg border border-[#ececec] bg-white p-2.5 text-xs text-[#3f3f3f]">
                {result}
              </pre>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
