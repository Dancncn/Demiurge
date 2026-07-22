import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { CloseIcon, StopIcon } from "./Icons";

type JsonValue = string | number | boolean | null | JsonValue[] | { [key: string]: JsonValue };
type WorkflowStatus = "running" | "stale_running" | "done" | "failed" | "killed" | "journaled";
type ValidationLevel = "error" | "warning";

interface WorkflowInputDefinition {
  key: string;
  label: string;
  description: string;
  kind: "text" | "textarea" | "number" | "boolean" | "select";
  required: boolean;
  default?: JsonValue;
  options: string[];
  min?: number;
  max?: number;
  placeholder: string;
}

interface ValidationIssue {
  level: ValidationLevel;
  path: string;
  message: string;
}

interface WorkflowDefinitionInfo {
  name: string;
  description: string;
  path: string;
  inputs?: WorkflowInputDefinition[];
  valid?: boolean;
  issues?: ValidationIssue[];
  steps_total?: number;
}

interface WorkflowTemplateInfo {
  id: string;
  name: string;
  description: string;
}

interface WorkflowAgentProgress {
  id: number;
  node_id?: string;
  label: string;
  phase?: string;
  status: WorkflowStatus;
  result?: string;
  error?: string;
}

interface WorkflowFailedNode {
  node_id: string;
  kind: string;
  label: string;
  phase?: string;
  error: string;
  retryable: boolean;
}

interface WorkflowRunProgress {
  run_id: string;
  name: string;
  definition_name?: string;
  status: WorkflowStatus;
  cancel_requested: boolean;
  current_phase?: string;
  agents: WorkflowAgentProgress[];
  logs: string[];
  journal_path: string;
  started_at: number;
  updated_at: number;
  error?: string;
  budget?: { total?: number; used_exact: number; used_estimated: number };
  steps_total: number;
  steps_done: number;
  input_values?: Record<string, JsonValue>;
  failed_node?: WorkflowFailedNode;
  parent_run_id?: string;
}

interface WorkflowPanelState {
  definitions: WorkflowDefinitionInfo[];
  runs: WorkflowRunProgress[];
  templates?: WorkflowTemplateInfo[];
}

interface DryRunNode {
  node_id: string;
  kind: string;
  label: string;
  preview: string;
  children: DryRunNode[];
}

interface WorkflowDryRun {
  name: string;
  valid: boolean;
  issues: ValidationIssue[];
  normalized_inputs: Record<string, JsonValue>;
  steps_total: number;
  nodes: DryRunNode[];
}

interface Props {
  open: boolean;
  busy: boolean;
  onClose: () => void;
  onResume: (command: string) => void;
}

const EMPTY_STATE: WorkflowPanelState = { definitions: [], runs: [], templates: [] };

const workflowApi = {
  panelState: () => invoke<WorkflowPanelState>("workflow_panel_state"),
  run: (name: string, inputs: Record<string, JsonValue>) =>
    invoke<string>("workflow_run_with_inputs", { name, inputs }),
  stop: (runId: string) => invoke<void>("workflow_stop", { runId }),
  dryRun: (name: string, inputs: Record<string, JsonValue>) =>
    invoke<WorkflowDryRun>("workflow_dry_run", { name, inputs }),
  retryNode: (runId: string) => invoke<string>("workflow_retry_failed_node", { runId }),
  installTemplate: (templateId: string) =>
    invoke<WorkflowDefinitionInfo>("workflow_install_template", { templateId, name: null }),
};

function statusLabel(status: WorkflowStatus) {
  return status === "stale_running" ? "stale" : status === "journaled" ? "journal" : status;
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

function defaultInputs(definition?: WorkflowDefinitionInfo) {
  return Object.fromEntries(
    (definition?.inputs ?? []).flatMap((field) => {
      if (field.default !== undefined) return [[field.key, field.default]];
      if (field.kind === "boolean") return [[field.key, false]];
      return [];
    }),
  ) as Record<string, JsonValue>;
}

function budgetLabel(budget?: { total?: number; used_exact: number; used_estimated: number }) {
  if (!budget?.total) return "unlimited";
  const used = budget.used_exact + budget.used_estimated;
  return `${used}/${budget.total} · ${Math.max(0, budget.total - used)} left`;
}

export default function WorkflowsPanel({ open, busy, onClose, onResume }: Props) {
  const [state, setState] = useState<WorkflowPanelState>(EMPTY_STATE);
  const [selectedDefinitionName, setSelectedDefinitionName] = useState("");
  const [selectedRunId, setSelectedRunId] = useState("");
  const [inputs, setInputs] = useState<Record<string, JsonValue>>({});
  const [dryRun, setDryRun] = useState<WorkflowDryRun | null>(null);
  const [working, setWorking] = useState("");
  const [error, setError] = useState("");

  async function refresh() {
    const next = await workflowApi.panelState();
    setState(next);
    setSelectedRunId((current) => current || next.runs[0]?.run_id || "");
    setSelectedDefinitionName((current) => current || next.definitions[0]?.name || "");
  }

  useEffect(() => {
    if (!open) return;
    let unlisten: UnlistenFn | undefined;
    let disposed = false;
    refresh().catch((reason) => !disposed && setError(String(reason)));
    listen<WorkflowPanelState>("workflow-updated", (event) => {
      if (disposed) return;
      setState(event.payload);
      setSelectedRunId((current) => current || event.payload.runs[0]?.run_id || "");
    })
      .then((next) => {
        if (disposed) next();
        else unlisten = next;
      })
      .catch((reason) => !disposed && setError(String(reason)));
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [open]);

  const selectedDefinition = useMemo(
    () => state.definitions.find((definition) => definition.name === selectedDefinitionName),
    [selectedDefinitionName, state.definitions],
  );
  const selectedRun = useMemo(
    () => state.runs.find((run) => run.run_id === selectedRunId) ?? state.runs[0],
    [selectedRunId, state.runs],
  );

  useEffect(() => {
    setInputs(defaultInputs(selectedDefinition));
    setDryRun(null);
  }, [selectedDefinition?.name]);

  async function withWork(label: string, action: () => Promise<void>) {
    setError("");
    setWorking(label);
    try {
      await action();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setWorking("");
    }
  }

  function selectDefinition(definition: WorkflowDefinitionInfo) {
    setSelectedDefinitionName(definition.name);
  }

  async function previewWorkflow() {
    if (!selectedDefinition) return;
    await withWork("dry-run", async () => {
      setDryRun(await workflowApi.dryRun(selectedDefinition.name, inputs));
    });
  }

  async function runWorkflow() {
    if (!selectedDefinition) return;
    await withWork("run", async () => {
      const plan = await workflowApi.dryRun(selectedDefinition.name, inputs);
      setDryRun(plan);
      if (!plan.valid) return;
      const runId = await workflowApi.run(selectedDefinition.name, inputs);
      await refresh();
      setSelectedRunId(runId);
    });
  }

  async function stopWorkflow(runId: string) {
    await withWork("stop", async () => {
      await workflowApi.stop(runId);
      await refresh();
    });
  }

  async function retryFailedNode(runId: string) {
    await withWork("retry-node", async () => {
      const nextRunId = await workflowApi.retryNode(runId);
      await refresh();
      setSelectedRunId(nextRunId);
    });
  }

  async function installTemplate(templateId: string) {
    await withWork(`template:${templateId}`, async () => {
      const definition = await workflowApi.installTemplate(templateId);
      await refresh();
      setSelectedDefinitionName(definition.name);
    });
  }

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-40 bg-black/20 p-3 backdrop-blur-sm md:p-5" role="dialog" aria-modal="true">
      <div className="mx-auto flex h-full max-w-7xl flex-col overflow-hidden rounded-lg border border-[#e2e5e9] bg-[#fbfbfc] shadow-[0_24px_80px_rgba(35,25,45,0.22)]">
        <header className="flex min-h-14 shrink-0 items-center justify-between border-b border-[#e7e9ed] px-4">
          <div className="min-w-0">
            <h2 className="truncate text-base font-semibold text-[#232323]">Workflows</h2>
            <p className="truncate text-xs text-[#777]">Validated inputs, dry-run plans, and node-level recovery</p>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="Close Workflows"
            className="grid size-9 shrink-0 place-items-center rounded-md text-[#555] transition hover:bg-[#f0edf2]"
          >
            <CloseIcon size={19} />
          </button>
        </header>

        <div className="grid min-h-0 flex-1 grid-cols-1 lg:grid-cols-[280px_minmax(360px,0.9fr)_minmax(420px,1.1fr)]">
          <aside className="min-h-0 overflow-y-auto border-b border-[#e7e9ed] p-3 lg:border-b-0 lg:border-r">
            <SectionTitle title="Definitions" count={state.definitions.length} />
            <div className="space-y-1">
              {state.definitions.map((definition) => (
                <button
                  key={definition.name}
                  type="button"
                  onClick={() => selectDefinition(definition)}
                  className={`flex min-h-14 w-full items-center gap-3 rounded-md px-3 py-2 text-left transition ${
                    selectedDefinition?.name === definition.name ? "bg-white shadow-sm" : "hover:bg-white"
                  }`}
                >
                  <span className={`size-2 shrink-0 rounded-full ${definition.valid === false ? "bg-[#d64545]" : "bg-[#2f9e44]"}`} />
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-sm font-medium text-[#262626]">{definition.name}</span>
                    <span className="block truncate text-xs text-[#777]">
                      {definition.description || `${definition.steps_total ?? 0} steps`}
                    </span>
                  </span>
                </button>
              ))}
              {state.definitions.length === 0 && (
                <div className="border-l-2 border-[#d8dde4] px-3 py-2 text-sm text-[#777]">No workflow files yet.</div>
              )}
            </div>

            <SectionTitle title="Starter templates" count={state.templates?.length ?? 0} className="mt-5" />
            <div className="space-y-2">
              {(state.templates ?? []).map((template) => (
                <div key={template.id} className="border-l-2 border-[#d8dde4] px-3 py-1.5">
                  <div className="text-sm font-medium text-[#2f343b]">{template.name}</div>
                  <div className="mt-0.5 text-xs leading-5 text-[#747b85]">{template.description}</div>
                  <button
                    type="button"
                    disabled={Boolean(working)}
                    onClick={() => void installTemplate(template.id)}
                    className="mt-1.5 text-xs font-medium text-[#315f9b] hover:underline disabled:opacity-50"
                  >
                    {working === `template:${template.id}` ? "Installing..." : "Install"}
                  </button>
                </div>
              ))}
            </div>
          </aside>

          <section className="min-h-0 overflow-y-auto border-b border-[#e7e9ed] p-4 lg:border-b-0 lg:border-r">
            <WorkflowForm
              definition={selectedDefinition}
              values={inputs}
              dryRun={dryRun}
              disabled={Boolean(working)}
              onChange={(key, value) => {
                setInputs((current) => ({ ...current, [key]: value }));
                setDryRun(null);
              }}
              onPreview={() => void previewWorkflow()}
              onRun={() => void runWorkflow()}
              working={working}
            />
          </section>

          <section className="min-h-0 overflow-y-auto p-4">
            {error && <div className="mb-3 border-l-2 border-[#d64545] bg-[#fff4f4] px-3 py-2 text-sm text-[#9f1d1d]">{error}</div>}
            <SectionTitle title="Runs" count={state.runs.length} />
            <div className="mb-4 flex gap-1 overflow-x-auto pb-1">
              {state.runs.map((run) => (
                <button
                  key={run.run_id}
                  type="button"
                  onClick={() => setSelectedRunId(run.run_id)}
                  className={`flex h-9 max-w-48 shrink-0 items-center gap-2 rounded-md px-2.5 text-left text-xs transition ${
                    selectedRun?.run_id === run.run_id ? "bg-[#eef1f5] text-[#202124]" : "text-[#69717c] hover:bg-[#f2f4f6]"
                  }`}
                >
                  <span className={`size-2 shrink-0 rounded-full ${statusClass(run.status)}`} />
                  <span className="truncate">{run.name}</span>
                </button>
              ))}
            </div>
            {selectedRun ? (
              <RunDetail
                run={selectedRun}
                busy={busy || Boolean(working)}
                onStop={stopWorkflow}
                onRetryNode={retryFailedNode}
                onResume={onResume}
                onClose={onClose}
              />
            ) : (
              <div className="grid min-h-48 place-items-center text-sm text-[#777]">No workflow run selected.</div>
            )}
          </section>
        </div>
      </div>
    </div>
  );
}

function WorkflowForm({
  definition,
  values,
  dryRun,
  disabled,
  onChange,
  onPreview,
  onRun,
  working,
}: {
  definition?: WorkflowDefinitionInfo;
  values: Record<string, JsonValue>;
  dryRun: WorkflowDryRun | null;
  disabled: boolean;
  onChange: (key: string, value: JsonValue) => void;
  onPreview: () => void;
  onRun: () => void;
  working: string;
}) {
  if (!definition) return <div className="grid min-h-48 place-items-center text-sm text-[#777]">Select a definition.</div>;
  const fields = definition.inputs ?? [];
  return (
    <div className="space-y-5">
      <div>
        <div className="flex items-start justify-between gap-3">
          <div className="min-w-0">
            <h3 className="truncate text-lg font-semibold text-[#242424]">{definition.name}</h3>
            <p className="mt-1 break-all text-xs leading-5 text-[#777]">{definition.description || definition.path}</p>
          </div>
          <span className="shrink-0 rounded bg-[#eef1f5] px-2 py-1 text-xs text-[#59616d]">{definition.steps_total ?? 0} steps</span>
        </div>
        {(definition.issues ?? []).map((issue) => (
          <IssueLine key={`${issue.path}:${issue.message}`} issue={issue} />
        ))}
      </div>

      <div className="space-y-3">
        {fields.map((field) => (
          <WorkflowField key={field.key} field={field} value={values[field.key]} onChange={(value) => onChange(field.key, value)} />
        ))}
        {fields.length === 0 && <div className="text-sm text-[#737a84]">This workflow has no inputs.</div>}
      </div>

      <div className="flex flex-wrap gap-2 border-t border-[#e7e9ed] pt-4">
        <button
          type="button"
          disabled={disabled}
          onClick={onPreview}
          className="h-9 rounded-md border border-[#d9dde3] bg-white px-3 text-sm font-medium text-[#3f4751] transition hover:bg-[#f6f7f9] disabled:opacity-50"
        >
          {working === "dry-run" ? "Checking..." : "Dry run"}
        </button>
        <button
          type="button"
          disabled={disabled || definition.valid === false}
          onClick={onRun}
          className="h-9 rounded-md bg-[#24272b] px-3 text-sm font-medium text-white transition hover:bg-[#34383d] disabled:opacity-50"
        >
          {working === "run" ? "Starting..." : "Run workflow"}
        </button>
      </div>

      {dryRun && <DryRunView plan={dryRun} />}
    </div>
  );
}

function WorkflowField({
  field,
  value,
  onChange,
}: {
  field: WorkflowInputDefinition;
  value: JsonValue | undefined;
  onChange: (value: JsonValue) => void;
}) {
  const label = (
    <span className="text-xs font-medium text-[#434a54]">
      {field.label}
      {field.required && <span className="ml-1 text-[#b42318]">*</span>}
    </span>
  );
  const controlClass =
    "mt-1.5 w-full rounded-md border border-[#d9dde3] bg-white px-3 text-sm text-[#202124] outline-none transition focus:border-[#7a9dca] focus:ring-2 focus:ring-[#dce9f8]";

  if (field.kind === "boolean") {
    return (
      <label className="flex items-start gap-2.5">
        <input type="checkbox" checked={Boolean(value)} onChange={(event) => onChange(event.target.checked)} className="mt-0.5 size-4" />
        <span>
          {label}
          {field.description && <span className="mt-0.5 block text-xs leading-5 text-[#777]">{field.description}</span>}
        </span>
      </label>
    );
  }

  return (
    <label className="block">
      {label}
      {field.kind === "textarea" ? (
        <textarea
          value={typeof value === "string" ? value : ""}
          rows={4}
          placeholder={field.placeholder}
          onChange={(event) => onChange(event.target.value)}
          className={`${controlClass} min-h-24 py-2`}
        />
      ) : field.kind === "select" ? (
        <select value={typeof value === "string" ? value : ""} onChange={(event) => onChange(event.target.value)} className={`${controlClass} h-10`}>
          <option value="" disabled>
            Select...
          </option>
          {field.options.map((option) => (
            <option key={option} value={option}>
              {option}
            </option>
          ))}
        </select>
      ) : (
        <input
          type={field.kind === "number" ? "number" : "text"}
          value={typeof value === "string" || typeof value === "number" ? value : ""}
          min={field.min}
          max={field.max}
          placeholder={field.placeholder}
          onChange={(event) => onChange(field.kind === "number" ? Number(event.target.value) : event.target.value)}
          className={`${controlClass} h-10`}
        />
      )}
      {field.description && <span className="mt-1 block text-xs leading-5 text-[#777]">{field.description}</span>}
    </label>
  );
}

function DryRunView({ plan }: { plan: WorkflowDryRun }) {
  return (
    <div className="border-t border-[#e7e9ed] pt-4">
      <div className="mb-2 flex items-center justify-between gap-3">
        <h4 className="text-sm font-semibold text-[#333]">Dry-run plan</h4>
        <span className={`text-xs font-medium ${plan.valid ? "text-[#177245]" : "text-[#b42318]"}`}>
          {plan.valid ? `${plan.steps_total} nodes validated` : "Blocked by validation"}
        </span>
      </div>
      {plan.issues.map((issue) => (
        <IssueLine key={`${issue.path}:${issue.message}`} issue={issue} />
      ))}
      {plan.valid && (
        <div className="mt-3 space-y-1">
          {plan.nodes.map((node) => (
            <DryRunNodeLine key={node.node_id} node={node} depth={0} />
          ))}
        </div>
      )}
    </div>
  );
}

function DryRunNodeLine({ node, depth }: { node: DryRunNode; depth: number }) {
  return (
    <div>
      <div className="border-l-2 border-[#d8dde4] py-1 pl-2.5" style={{ marginLeft: depth * 14 }}>
        <div className="flex min-w-0 items-center gap-2">
          <span className="shrink-0 rounded bg-[#eef1f5] px-1.5 py-0.5 font-mono text-[10px] text-[#59616d]">{node.kind}</span>
          <span className="truncate text-xs font-medium text-[#333]">{node.label}</span>
        </div>
        <div className="mt-0.5 line-clamp-2 text-xs leading-5 text-[#747b85]">{node.preview}</div>
        <div className="truncate font-mono text-[10px] text-[#9aa0a8]">{node.node_id}</div>
      </div>
      {node.children.map((child) => (
        <DryRunNodeLine key={child.node_id} node={child} depth={depth + 1} />
      ))}
    </div>
  );
}

function IssueLine({ issue }: { issue: ValidationIssue }) {
  return (
    <div className={`mt-1.5 border-l-2 px-2.5 py-1 text-xs leading-5 ${issue.level === "error" ? "border-[#d64545] bg-[#fff4f4] text-[#9f1d1d]" : "border-[#c68a24] bg-[#fff9ec] text-[#765115]"}`}>
      <span className="font-mono">{issue.path}</span>: {issue.message}
    </div>
  );
}

function SectionTitle({ title, count, className = "" }: { title: string; count: number; className?: string }) {
  return (
    <div className={`mb-2 flex items-center justify-between px-1 ${className}`}>
      <h3 className="text-xs font-semibold uppercase tracking-[0.08em] text-[#7f8791]">{title}</h3>
      <span className="text-xs text-[#9a9a9a]">{count}</span>
    </div>
  );
}

function RunDetail({
  run,
  busy,
  onStop,
  onRetryNode,
  onResume,
  onClose,
}: {
  run: WorkflowRunProgress;
  busy: boolean;
  onStop: (runId: string) => Promise<void>;
  onRetryNode: (runId: string) => Promise<void>;
  onResume: (command: string) => void;
  onClose: () => void;
}) {
  const doneAgents = run.agents.filter((agent) => agent.status === "done").length;
  const failedAgents = run.agents.filter((agent) => agent.status === "failed").length;
  const progressPct = run.steps_total ? Math.round((run.steps_done / run.steps_total) * 100) : run.status === "done" ? 100 : 0;
  return (
    <div className="space-y-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex min-w-0 items-center gap-2">
            <span className={`size-2.5 shrink-0 rounded-full ${statusClass(run.status)}`} />
            <h3 className="truncate text-base font-semibold text-[#242424]">{run.name}</h3>
            <span className="shrink-0 rounded bg-[#eef1f5] px-2 py-1 text-xs text-[#5f6670]">{statusLabel(run.status)}</span>
          </div>
          <p className="mt-1 truncate font-mono text-[10px] text-[#858b94]" title={run.run_id}>{run.run_id}</p>
        </div>
        <div className="flex shrink-0 items-center gap-2">
          {run.status === "running" && (
            <button type="button" onClick={() => void onStop(run.run_id)} className="inline-flex h-9 items-center gap-2 rounded-md border border-[#d9dde3] px-3 text-sm text-[#4a515a] hover:bg-white">
              <StopIcon size={15} /> Stop
            </button>
          )}
          {run.failed_node?.retryable && (
            <button type="button" disabled={busy} onClick={() => void onRetryNode(run.run_id)} className="h-9 rounded-md border border-[#d9dde3] px-3 text-sm font-medium text-[#3f4751] hover:bg-white disabled:opacity-50">
              Retry failed node
            </button>
          )}
          <button
            type="button"
            disabled={busy}
            onClick={() => {
              onResume(`/workflow resume ${run.run_id}`);
              onClose();
            }}
            className="h-9 rounded-md bg-[#24272b] px-3 text-sm font-medium text-white hover:bg-[#34383d] disabled:opacity-50"
          >
            Resume in chat
          </button>
        </div>
      </div>

      {run.failed_node && (
        <div className="border-l-2 border-[#d64545] bg-[#fff4f4] px-3 py-2 text-sm text-[#9f1d1d]">
          <div className="font-medium">Failed node: {run.failed_node.label}</div>
          <div className="mt-1 font-mono text-xs">{run.failed_node.node_id}</div>
          <div className="mt-1 whitespace-pre-wrap text-xs leading-5">{run.failed_node.error}</div>
        </div>
      )}
      {run.error && !run.failed_node && <div className="border-l-2 border-[#d64545] bg-[#fff4f4] px-3 py-2 text-xs leading-5 text-[#9f1d1d]">{run.error}</div>}

      <div className="grid gap-2 sm:grid-cols-4">
        <Metric label="Phase" value={run.current_phase || "-"} />
        <Metric label="Steps" value={run.steps_total ? `${run.steps_done}/${run.steps_total}` : "-"} />
        <Metric label="Agents" value={`${doneAgents}/${run.agents.length} · ${failedAgents} failed`} />
        <Metric label="Budget" value={budgetLabel(run.budget)} />
      </div>

      <div>
        <div className="mb-1.5 flex items-center justify-between text-xs text-[#777]"><span>Progress</span><span>{progressPct}%</span></div>
        <div className="h-2 overflow-hidden rounded-full bg-[#edf0f3]">
          <div className={`h-full rounded-full transition-all ${run.status === "failed" ? "bg-[#d64545]" : run.status === "killed" ? "bg-[#8a8a8a]" : "bg-[#2f9e44]"}`} style={{ width: `${progressPct}%` }} />
        </div>
      </div>

      <div>
        <h4 className="mb-2 text-sm font-semibold text-[#333]">Agents</h4>
        <div className="divide-y divide-[#eceff2] border-y border-[#eceff2]">
          {run.agents.map((agent) => (
            <div key={agent.id} className="py-2.5">
              <div className="flex min-w-0 items-center gap-2">
                <span className={`size-2 shrink-0 rounded-full ${statusClass(agent.status)}`} />
                <span className="truncate text-sm font-medium text-[#2b2b2b]">{agent.label}</span>
                <span className="truncate font-mono text-[10px] text-[#8a9099]">{agent.node_id || `#${agent.id}`}</span>
              </div>
              {agent.result && <pre className="mt-1.5 max-h-40 overflow-auto whitespace-pre-wrap text-xs leading-5 text-[#555]">{agent.result}</pre>}
              {agent.error && <p className="mt-1.5 text-xs text-[#9f1d1d]">{agent.error}</p>}
            </div>
          ))}
          {run.agents.length === 0 && <div className="py-3 text-sm text-[#777]">No agents recorded.</div>}
        </div>
      </div>

      <div>
        <h4 className="mb-2 text-sm font-semibold text-[#333]">Logs</h4>
        <pre className="max-h-44 overflow-auto border-y border-[#eceff2] py-2.5 text-xs leading-5 text-[#555]">
          {run.logs.length ? run.logs.join("\n") : "No logs."}
        </pre>
      </div>
    </div>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0 border-l-2 border-[#d8dde4] px-2.5 py-1">
      <div className="text-xs text-[#777]">{label}</div>
      <div className="mt-1 truncate text-sm font-medium text-[#262626]" title={value}>{value}</div>
    </div>
  );
}
