import { useEffect, useState } from "react";
import type { ToolRisk, ToolSourceQuality } from "../lib/types";
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
  duration_ms?: number;
  error_hint?: string;
  source_quality?: ToolSourceQuality;
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

function formatDuration(ms?: number) {
  if (typeof ms !== "number" || ms < 0) return null;
  if (ms < 1000) return `${ms} ms`;
  return `${(ms / 1000).toFixed(ms < 10_000 ? 1 : 0)} s`;
}

function qualityClass(level: ToolSourceQuality["level"]) {
  switch (level) {
    case "strong":
      return "border-[#bfe3cf] bg-[#eef9f3] text-[#1f7a4d]";
    case "limited":
      return "border-[#f2d7a5] bg-[#fff8e8] text-[#8a5a00]";
    case "none":
      return "border-[#f1b8b8] bg-[#fff1f1] text-[#b42318]";
  }
}

export default function ToolCard({
  name,
  args,
  status,
  result,
  preview,
  description,
  risk,
  duration_ms,
  error_hint,
  source_quality,
}: Props) {
  const [open, setOpen] = useState(status === "failed");
  const b = badge(status);
  const riskText = riskLabel(risk);
  const progressText = progressSummary(name, status, args, result);
  const durationText = formatDuration(duration_ms);
  let argsText = "";
  try {
    argsText = JSON.stringify(args, null, 2);
  } catch {
    argsText = String(args);
  }

  useEffect(() => {
    if (status === "failed") setOpen(true);
  }, [status]);

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
        {durationText && <span className="rounded-full bg-white px-2 py-0.5 text-xs text-[#6f7782]">{durationText}</span>}
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
          {source_quality && (
            <div className={`mt-2 rounded-md border px-2.5 py-2 ${qualityClass(source_quality.level)}`}>
              <div className="font-medium">
                Source quality: {source_quality.level} / {source_quality.source_count} links
              </div>
              <div className="mt-0.5">{source_quality.hint}</div>
            </div>
          )}
          {error_hint && (
            <div className="mt-2 rounded-md border border-[#f2d7a5] bg-[#fff8e8] px-2.5 py-2 text-[#8a5a00]">
              {error_hint}
            </div>
          )}
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
