import { useEffect, useMemo, useState } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import * as api from "../lib/api";
import type { WorkflowPanelState, WorkflowRunProgress, WorkflowStatus } from "../lib/types";
import { CloseIcon, StopIcon } from "./Icons";

interface Props {
  open: boolean;
  busy: boolean;
  onClose: () => void;
  onResume: (command: string) => void;
}

const EMPTY_STATE: WorkflowPanelState = { definitions: [], runs: [] };

function statusLabel(status: WorkflowStatus) {
  switch (status) {
    case "running":
      return "running";
    case "stale_running":
      return "stale";
    case "done":
      return "done";
    case "failed":
      return "failed";
    case "killed":
      return "killed";
    case "journaled":
      return "journal";
  }
}

function statusClass(status: WorkflowStatus) {
  switch (status) {
    case "running":
      return "bg-[#D77757]";
    case "stale_running":
      return "bg-[#b7791f]";
    case "done":
      return "bg-[#2f9e44]";
    case "failed":
      return "bg-[#d64545]";
    case "killed":
      return "bg-[#8a8a8a]";
    case "journaled":
      return "bg-[#6b7280]";
  }
}

function budgetLabel(budget?: { total?: number; used_exact: number; used_estimated: number }) {
  if (!budget?.total) return "unlimited";
  const used = budget.used_exact + budget.used_estimated;
  const remaining = Math.max(0, budget.total - used);
  return `${used}/${budget.total} · ${remaining} left`;
}

export default function WorkflowsPanel({ open, busy, onClose, onResume }: Props) {
  const [state, setState] = useState<WorkflowPanelState>(EMPTY_STATE);
  const [selectedRunId, setSelectedRunId] = useState<string>("");
  const [runningName, setRunningName] = useState<string>("");
  const [error, setError] = useState<string>("");

  useEffect(() => {
    if (!open) return;
    let unlisten: UnlistenFn | undefined;
    let disposed = false;
    api.workflowPanelState().then((next) => {
      if (!disposed) {
        setState(next);
        setSelectedRunId((cur) => cur || next.runs[0]?.run_id || "");
      }
    }).catch((e) => setError(String(e)));
    api.listenWorkflowUpdated((next) => {
      setState(next);
      setSelectedRunId((cur) => cur || next.runs[0]?.run_id || "");
    }).then((un) => {
      if (disposed) un();
      else unlisten = un;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [open]);

  const selectedRun = useMemo(
    () => state.runs.find((run) => run.run_id === selectedRunId) ?? state.runs[0],
    [state.runs, selectedRunId],
  );

  async function runWorkflow(name: string) {
    setError("");
    setRunningName(name);
    try {
      const runId = await api.workflowRun(name);
      const next = await api.workflowPanelState();
      setState(next);
      setSelectedRunId(runId);
    } catch (e) {
      setError(String(e));
    } finally {
      setRunningName("");
    }
  }

  async function stopWorkflow(runId: string) {
    setError("");
    try {
      await api.workflowStop(runId);
      setState(await api.workflowPanelState());
    } catch (e) {
      setError(String(e));
    }
  }

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-40 bg-black/20 p-3 backdrop-blur-sm md:p-5" role="dialog" aria-modal="true">
      <div className="mx-auto flex h-full max-w-6xl flex-col overflow-hidden rounded-2xl border border-[#e7e1ea] bg-[#fbfbfc] shadow-[0_24px_80px_rgba(35,25,45,0.22)]">
        <header className="flex h-14 shrink-0 items-center justify-between border-b border-[#ece7ef] px-4">
          <div>
            <h2 className="text-base font-semibold text-[#232323]">Workflows</h2>
            <p className="text-xs text-[#777]">.demiurge/workflows/*.json</p>
          </div>
          <button
            onClick={onClose}
            aria-label="关闭 Workflows"
            className="grid h-9 w-9 place-items-center rounded-lg text-[#555] transition hover:bg-[#f0edf2]"
          >
            <CloseIcon size={19} />
          </button>
        </header>

        <div className="grid min-h-0 flex-1 grid-cols-1 md:grid-cols-[320px_1fr]">
          <aside className="min-h-0 border-b border-[#ece7ef] md:border-b-0 md:border-r">
            <div className="h-full overflow-y-auto p-3">
              <SectionTitle title="Definitions" count={state.definitions.length} />
              <div className="space-y-1">
                {state.definitions.map((def) => (
                  <button
                    key={def.name}
                    disabled={!!runningName}
                    onClick={() => void runWorkflow(def.name)}
                    className="group flex min-h-14 w-full items-center justify-between gap-3 rounded-lg px-3 py-2 text-left transition hover:bg-white disabled:opacity-60"
                  >
                    <span className="min-w-0">
                      <span className="block truncate text-sm font-medium text-[#262626]">{def.name}</span>
                      <span className="block truncate text-xs text-[#777]">{def.description || def.path}</span>
                    </span>
                    <span className="shrink-0 rounded-md border border-[#ddd7e2] px-2 py-1 text-xs text-[#555] group-hover:bg-[#f6f2f8]">
                      {runningName === def.name ? "..." : "Run"}
                    </span>
                  </button>
                ))}
                {state.definitions.length === 0 && (
                  <div className="rounded-lg border border-dashed border-[#ddd7e2] p-3 text-sm text-[#777]">
                    No workflow JSON files yet.
                  </div>
                )}
              </div>

              <SectionTitle title="Runs" count={state.runs.length} className="mt-5" />
              <div className="space-y-1">
                {state.runs.map((run) => (
                  <button
                    key={run.run_id}
                    onClick={() => setSelectedRunId(run.run_id)}
                    className={`flex min-h-14 w-full items-center gap-3 rounded-lg px-3 py-2 text-left transition ${
                      selectedRun?.run_id === run.run_id ? "bg-white shadow-sm" : "hover:bg-white"
                    }`}
                  >
                    <span className={`h-2.5 w-2.5 shrink-0 rounded-full ${statusClass(run.status)}`} />
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-sm font-medium text-[#262626]">{run.name}</span>
                      <span className="block truncate text-xs text-[#777]">{run.current_phase || run.run_id}</span>
                    </span>
                    <span className="shrink-0 text-xs text-[#777]">{run.agents.length}</span>
                  </button>
                ))}
              </div>
            </div>
          </aside>

          <section className="min-h-0 overflow-y-auto p-4">
            {error && <div className="mb-3 rounded-lg bg-[#fff1f1] px-3 py-2 text-sm text-[#9f1d1d]">{error}</div>}
            {selectedRun ? (
              <RunDetail
                run={selectedRun}
                busy={busy}
                onStop={stopWorkflow}
                onResume={onResume}
                onRerun={runWorkflow}
                onClose={onClose}
              />
            ) : (
              <div className="flex h-full items-center justify-center text-sm text-[#777]">
                Select or run a workflow.
              </div>
            )}
          </section>
        </div>
      </div>
    </div>
  );
}

function SectionTitle({ title, count, className = "" }: { title: string; count: number; className?: string }) {
  return (
    <div className={`mb-2 flex items-center justify-between px-1 ${className}`}>
      <h3 className="text-xs font-semibold uppercase tracking-[0.08em] text-[#8a7f91]">{title}</h3>
      <span className="text-xs text-[#9a9a9a]">{count}</span>
    </div>
  );
}

function RunDetail({
  run,
  busy,
  onStop,
  onResume,
  onRerun,
  onClose,
}: {
  run: WorkflowRunProgress;
  busy: boolean;
  onStop: (runId: string) => Promise<void>;
  onResume: (command: string) => void;
  onRerun: (name: string) => Promise<void>;
  onClose: () => void;
}) {
  const doneAgents = run.agents.filter((agent) => agent.status === "done").length;
  const runningAgents = run.agents.filter((agent) => agent.status === "running").length;
  const failedAgents = run.agents.filter((agent) => agent.status === "failed").length;
  const isStale = run.status === "stale_running";
  const progressPct = run.steps_total
    ? Math.round((run.steps_done / run.steps_total) * 100)
    : run.agents.length
      ? Math.round((doneAgents / run.agents.length) * 100)
      : run.status === "done"
        ? 100
        : 0;
  const canRetry = run.status === "failed" || run.status === "killed" || isStale;
  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <span className={`h-2.5 w-2.5 rounded-full ${statusClass(run.status)}`} />
            <h3 className="truncate text-lg font-semibold text-[#242424]">{run.name}</h3>
            <span className="rounded-md bg-[#eee8f2] px-2 py-1 text-xs text-[#5f5666]">{statusLabel(run.status)}</span>
          </div>
          <p className="mt-1 break-all text-xs text-[#777]">{run.run_id}</p>
          <p className="mt-1 break-all text-xs text-[#777]">{run.journal_path}</p>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          {run.status === "running" && (
            <button
              onClick={() => void onStop(run.run_id)}
              className="inline-flex h-9 items-center gap-2 rounded-lg border border-[#e1dbe6] px-3 text-sm text-[#4a414f] transition hover:bg-white"
            >
              <StopIcon size={15} />
              Stop
            </button>
          )}
          {canRetry && (
            <button
              disabled={busy}
              onClick={() => void onRerun(run.name)}
              className="h-9 rounded-lg border border-[#e1dbe6] px-3 text-sm font-medium text-[#4a414f] transition hover:bg-white disabled:opacity-50"
            >
              Retry run
            </button>
          )}
          <button
            disabled={busy}
            onClick={() => {
              onResume(`/workflow resume ${run.run_id}`);
              onClose();
            }}
            className="h-9 rounded-lg bg-[#242226] px-3 text-sm font-medium text-white transition hover:bg-[#343139] disabled:opacity-50"
          >
            {isStale ? "Resume in chat" : "Resume"}
          </button>
        </div>
      </div>

      {run.error && !isStale && (
        <div className="rounded-lg border border-[#f1b8b8] bg-[#fff1f1] px-3 py-2 text-sm text-[#9f1d1d]">
          <div className="font-medium">Workflow stopped with an error</div>
          <div className="mt-1 whitespace-pre-wrap text-xs">{run.error}</div>
        </div>
      )}

      {isStale && (
        <div className="rounded-lg border border-[#efcf91] bg-[#fff8e8] px-3 py-2 text-sm text-[#7a5311]">
          <div className="font-medium">Restored from durable state</div>
          <div className="mt-1 text-xs">
            This run was active in a previous process. Its progress, budget, and cancellation state were restored, but no live task is attached.
          </div>
        </div>
      )}

      <div className="grid gap-3 sm:grid-cols-6">
        <Metric label="Phase" value={run.current_phase || "-"} />
        <Metric label="Steps" value={run.steps_total ? `${run.steps_done}/${run.steps_total}` : "-"} />
        <Metric label="Agents" value={`${doneAgents}/${run.agents.length}`} />
        <Metric label="Token budget" value={budgetLabel(run.budget)} />
        <Metric label="Cancel" value={run.cancel_requested ? "requested" : "-"} />
        <Metric label="Updated" value={String(run.updated_at)} />
      </div>

      <div className="rounded-lg border border-[#ece7ef] bg-white p-3">
        <div className="mb-2 flex items-center justify-between text-xs text-[#777]">
          <span>Workflow progress</span>
          <span>{progressPct}% / running {runningAgents} / failed {failedAgents}</span>
        </div>
        <div className="h-2 overflow-hidden rounded-full bg-[#f0edf2]">
          <div
            className={`h-full rounded-full transition-all ${
              run.status === "failed"
                ? "bg-[#d64545]"
                : run.status === "killed"
                  ? "bg-[#8a8a8a]"
                  : run.status === "stale_running"
                    ? "bg-[#b7791f]"
                    : "bg-[#2f9e44]"
            }`}
            style={{ width: `${progressPct}%` }}
          />
        </div>
        {run.logs[0] && <div className="mt-2 truncate text-xs text-[#777]">Latest: {run.logs[run.logs.length - 1]}</div>}
      </div>

      <div>
        <h4 className="mb-2 text-sm font-semibold text-[#333]">Agents</h4>
        <div className="grid gap-2">
          {run.agents.map((agent) => (
            <div key={agent.id} className="rounded-lg border border-[#ece7ef] bg-white px-3 py-2">
              <div className="flex flex-wrap items-center gap-2">
                <span className={`h-2 w-2 rounded-full ${statusClass(agent.status)}`} />
                <span className="font-mono text-xs text-[#777]">#{agent.id}</span>
                <span className="text-sm font-medium text-[#2b2b2b]">{agent.label}</span>
                {agent.phase && <span className="rounded bg-[#f3eef6] px-1.5 py-0.5 text-xs text-[#6c6073]">{agent.phase}</span>}
              </div>
              {agent.result && <pre className="mt-2 whitespace-pre-wrap text-xs leading-5 text-[#555]">{agent.result}</pre>}
              {agent.error && <p className="mt-2 text-xs text-[#9f1d1d]">{agent.error}</p>}
            </div>
          ))}
          {run.agents.length === 0 && <div className="text-sm text-[#777]">No agents recorded.</div>}
        </div>
      </div>

      <div>
        <h4 className="mb-2 text-sm font-semibold text-[#333]">Logs</h4>
        <div className="rounded-lg border border-[#ece7ef] bg-white p-3">
          {run.logs.length > 0 ? (
            <pre className="max-h-52 overflow-auto whitespace-pre-wrap text-xs leading-5 text-[#555]">
              {run.logs.join("\n")}
            </pre>
          ) : (
            <div className="text-sm text-[#777]">No logs.</div>
          )}
        </div>
      </div>
    </div>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-lg border border-[#ece7ef] bg-white px-3 py-2">
      <div className="text-xs text-[#777]">{label}</div>
      <div className="mt-1 truncate text-sm font-medium text-[#262626]">{value}</div>
    </div>
  );
}
