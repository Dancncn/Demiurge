import { useState } from "react";
import type { ToolRisk } from "../lib/types";
import DiffPreview from "./DiffPreview";
import { WrenchIcon } from "./Icons";

interface Props {
  name: string;
  args: unknown;
  status: "running" | "done" | "denied" | "failed";
  result?: string;
  preview?: string;
  description?: string;
  risk?: ToolRisk;
}

function badge(status: Props["status"]) {
  switch (status) {
    case "running":
      return { label: "Running", cls: "bg-[#eaf2ff] text-[#0b57d0]" };
    case "denied":
      return { label: "Denied", cls: "bg-[#fef2f2] text-[#dc2626]" };
    case "failed":
      return { label: "Failed", cls: "bg-[#fff4e5] text-[#b54708]" };
    default:
      return { label: "Done", cls: "bg-[#eafaf1] text-[#0a7d4d]" };
  }
}

function riskLabel(risk?: ToolRisk) {
  switch (risk) {
    case "read_only":
      return "Read only";
    case "mutating":
      return "Writes";
    case "external":
      return "Network";
    case "privileged":
      return "System";
    default:
      return null;
  }
}

function argString(args: unknown, key: string) {
  if (!args || typeof args !== "object" || !(key in args)) return "";
  const value = (args as Record<string, unknown>)[key];
  return typeof value === "string" ? value : "";
}

function progressSummary(name: string, status: Props["status"], args: unknown, result?: string) {
  if (status === "failed") return result ? "Tool returned an error." : "Tool failed before returning output.";
  if (status === "denied") return "Operation was not executed.";
  if (name === "web_search") {
    const query = argString(args, "query");
    if (status === "running") return `Searching${query ? `: ${query}` : ""}`;
    const count = result?.match(/^\d+\. \[/gm)?.length ?? 0;
    return count > 0 ? `Returned ${count} source links.` : "Search finished without extractable source links.";
  }
  if (name === "web_fetch") {
    const url = argString(args, "url");
    return status === "running" ? `Fetching${url ? `: ${url}` : ""}` : "Fetch finished.";
  }
  if (status === "running") return "Waiting for tool result.";
  return null;
}

export default function ToolCard({ name, args, status, result, preview, description, risk }: Props) {
  const [open, setOpen] = useState(status === "failed");
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
    <div className="cf-message-in rounded-lg border border-[#e2e5ea] bg-[#fbfcfd] text-sm text-[#4f5661]">
      <button
        type="button"
        className="flex w-full items-center gap-2 px-4 py-3 text-left"
        onClick={() => setOpen((v) => !v)}
      >
        <WrenchIcon size={16} className="shrink-0 text-[#0b57d0]" />
        <span className="font-medium text-[#202124]">{name}</span>
        <span className={`rounded-full px-2 py-0.5 text-xs ${b.cls}`}>{b.label}</span>
        {riskText && <span className="rounded-full bg-white px-2 py-0.5 text-xs text-[#6f7782]">{riskText}</span>}
        {status === "running" && (
          <span className="cf-dots shrink-0 text-[#b4b4b4]">
            <span />
            <span />
            <span />
          </span>
        )}
        <span className="ml-auto text-xs text-[#8a9099]">{open ? "Hide" : "Details"}</span>
      </button>

      {progressText && (
        <div className="border-t border-[#eceff3] px-4 py-2 text-xs text-[#6f7782]">
          <div className="h-1.5 overflow-hidden rounded-full bg-white">
            <div
              className={`h-full rounded-full ${
                status === "running"
                  ? "w-1/2 animate-pulse bg-[#0b57d0]"
                  : status === "failed"
                    ? "w-full bg-[#f79009]"
                    : status === "denied"
                      ? "w-full bg-[#dc2626]"
                      : "w-full bg-[#10a37f]"
              }`}
            />
          </div>
          <div className="mt-1.5">{progressText}</div>
        </div>
      )}

      {open && (
        <div className="space-y-3 border-t border-[#eceff3] px-4 py-3">
          {description && <div className="text-xs leading-relaxed text-[#6f7782]">{description}</div>}
          {preview && (
            <div>
              <div className="mb-1 text-xs font-medium text-[#7a8088]">Preview</div>
              <DiffPreview text={preview} />
            </div>
          )}
          <div>
            <div className="mb-1 text-xs font-medium text-[#7a8088]">Arguments</div>
            <pre className="overflow-x-auto rounded-lg border border-[#eceff3] bg-white p-2.5 text-xs text-[#344054]">
              {argsText}
            </pre>
          </div>
          {result && (
            <div>
              <div className="mb-1 text-xs font-medium text-[#7a8088]">Result</div>
              {status === "failed" ? (
                <DiffPreview text={result} maxHeightClass="max-h-60" />
              ) : (
                <pre className="max-h-60 overflow-auto whitespace-pre-wrap rounded-lg border border-[#eceff3] bg-white p-2.5 text-xs text-[#344054]">
                  {result}
                </pre>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
