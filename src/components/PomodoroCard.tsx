import { useEffect, useMemo, useState } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import * as api from "../lib/api";
import type {
  GoalPanelState,
  PomodoroMode,
  PomodoroPanelState,
  PomodoroTaskBinding,
  PomodoroTaskKind,
  WorkflowRunProgress,
} from "../lib/types";
import { PauseIcon, PlayIcon, RotateCwIcon, TargetIcon } from "./Icons";

const modeLabels: Record<PomodoroMode, string> = {
  focus: "专注",
  short_break: "短休息",
  long_break: "长休息",
  custom: "自定义",
};

const interruptionReasons = ["切换任务", "消息打断", "卡住了", "临时离开"];

function formatClock(totalSeconds: number) {
  const safe = Math.max(0, Math.floor(totalSeconds));
  const minutes = Math.floor(safe / 60);
  const seconds = safe % 60;
  return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}

function notify(title: string, body: string) {
  if (!("Notification" in window)) return;
  if (Notification.permission === "granted") {
    new Notification(title, { body });
    return;
  }
  if (Notification.permission === "default") {
    void Notification.requestPermission().then((permission) => {
      if (permission === "granted") new Notification(title, { body });
    });
  }
}

function topRhythmEntry(values?: Record<string, number>) {
  const entries = Object.entries(values ?? {}).filter(([, count]) => count > 0);
  entries.sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]));
  return entries[0]?.[0] ?? "";
}

type Props = {
  activeSessionId: string;
  activeSessionTitle?: string;
  goal: GoalPanelState | null;
};

export default function PomodoroCard({ activeSessionId, activeSessionTitle, goal }: Props) {
  const [state, setState] = useState<PomodoroPanelState | null>(null);
  const [mode, setMode] = useState<PomodoroMode>("focus");
  const [customMinutes, setCustomMinutes] = useState(25);
  const [taskKind, setTaskKind] = useState<PomodoroTaskKind>("manual");
  const [manualTitle, setManualTitle] = useState("");
  const [skipReason, setSkipReason] = useState("");
  const [workflowRuns, setWorkflowRuns] = useState<WorkflowRunProgress[]>([]);
  const [workflowRunId, setWorkflowRunId] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  async function refresh() {
    try {
      setState(await api.pomodoroState());
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    void refresh();
    void api.workflowPanelState().then((panel) => {
      setWorkflowRuns(panel.runs);
      setWorkflowRunId((current) => current || panel.runs[0]?.run_id || "");
    });
    const timer = window.setInterval(() => void refresh(), 1000);
    let unlistenUpdated: UnlistenFn | undefined;
    let unlistenCompleted: UnlistenFn | undefined;
    void api.listenPomodoroUpdated(setState).then((fn) => {
      unlistenUpdated = fn;
    });
    void api.listenPomodoroCompleted((event) => {
      setState(event.state);
      notify(event.title, event.body);
    }).then((fn) => {
      unlistenCompleted = fn;
    });
    return () => {
      window.clearInterval(timer);
      unlistenUpdated?.();
      unlistenCompleted?.();
    };
  }, []);

  const timer = state?.timer;
  const running = timer?.status === "running";
  const paused = timer?.status === "paused";
  const active = running || paused;
  const remaining = state?.remaining_secs ?? 0;
  const duration = timer?.duration_secs ?? 0;
  const progress = useMemo(() => {
    if (!duration) return 0;
    return Math.min(100, Math.max(0, ((duration - remaining) / duration) * 100));
  }, [duration, remaining]);

  const selectedWorkflow = workflowRuns.find((run) => run.run_id === workflowRunId) ?? workflowRuns[0] ?? null;
  const preferredDuration = topRhythmEntry(state?.rhythm.focus_duration_counts);
  const efficientHour = topRhythmEntry(state?.rhythm.efficient_hour_counts);
  const commonInterruption = topRhythmEntry(state?.rhythm.interruption_reasons);
  const rhythmSummary = [
    preferredDuration ? `偏好 ${preferredDuration}m` : "",
    efficientHour ? `高效 ${String(efficientHour).padStart(2, "0")}:00` : "",
    commonInterruption ? `常见中断 ${commonInterruption}` : "",
  ].filter(Boolean);

  function taskBinding(): PomodoroTaskBinding {
    if (taskKind === "session") {
      return {
        kind: "session",
        title: activeSessionTitle || "Current chat",
        session_id: activeSessionId || null,
      };
    }
    if (taskKind === "goal") {
      return {
        kind: "goal",
        title: goal?.objective || "Current goal",
        goal_objective: goal?.objective || null,
      };
    }
    if (taskKind === "workflow") {
      return {
        kind: "workflow",
        title: selectedWorkflow ? `${selectedWorkflow.name} · ${selectedWorkflow.status}` : "Workflow",
        workflow_run_id: selectedWorkflow?.run_id || null,
      };
    }
    return {
      kind: "manual",
      title: manualTitle.trim() || "Focus session",
    };
  }

  async function runAction(action: () => Promise<PomodoroPanelState>) {
    if (busy) return;
    setBusy(true);
    setError("");
    try {
      setState(await action());
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const start = () =>
    runAction(() =>
      api.pomodoroStart({
        mode,
        duration_minutes: mode === "custom" ? customMinutes : undefined,
        task: taskBinding(),
        local_hour: new Date().getHours(),
      }),
    );

  const skip = async () => {
    await runAction(() => api.pomodoroSkip({ reason: skipReason.trim() || null }));
    setSkipReason("");
  };

  const statusLabel = active
    ? `${modeLabels[timer?.mode ?? "focus"]} · ${timer?.status === "paused" ? "已暂停" : "进行中"}`
    : state?.timer.feedback.completion_message || "准备开始一轮节奏";

  return (
    <section className="border-b border-[#eceff3] bg-white px-3 py-2">
      <div className="flex flex-wrap items-center gap-3">
        <div className="flex min-w-[154px] items-center gap-2">
          <div className="grid size-8 shrink-0 place-items-center rounded-md border border-[#dfe3e8] text-[#4f5661]">
            <TargetIcon size={17} />
          </div>
          <div className="min-w-0">
            <div className="text-[12px] font-semibold text-[#202124]">番茄钟</div>
            <div className="truncate text-[11px] text-[#7a8088]">{statusLabel}</div>
          </div>
        </div>

        <div className="flex min-w-[170px] flex-1 items-center gap-3">
          <div className="w-[72px] shrink-0 font-mono text-[22px] leading-none text-[#202124]">
            {formatClock(remaining)}
          </div>
          <div className="h-2 min-w-[120px] flex-1 overflow-hidden rounded-full bg-[#edf0f4]">
            <div className="h-full rounded-full bg-[#202124]" style={{ width: `${progress}%` }} />
          </div>
        </div>

        {!active && (
          <div className="flex flex-wrap items-center gap-2">
            <select
              value={mode}
              onChange={(e) => setMode(e.target.value as PomodoroMode)}
              className="h-8 rounded-md border border-[#d9dfe7] bg-white px-2 text-[12px] outline-none"
            >
              <option value="focus">专注 25</option>
              <option value="short_break">短休息 5</option>
              <option value="long_break">长休息 15</option>
              <option value="custom">自定义</option>
            </select>
            {mode === "custom" && (
              <input
                type="number"
                min={1}
                max={240}
                value={customMinutes}
                onChange={(e) => setCustomMinutes(Math.min(240, Math.max(1, Number(e.target.value) || 1)))}
                className="h-8 w-20 rounded-md border border-[#d9dfe7] bg-white px-2 text-[12px] outline-none"
              />
            )}
            <select
              value={taskKind}
              onChange={(e) => setTaskKind(e.target.value as PomodoroTaskKind)}
              className="h-8 rounded-md border border-[#d9dfe7] bg-white px-2 text-[12px] outline-none"
            >
              <option value="manual">手动任务</option>
              <option value="session">当前会话</option>
              <option value="goal" disabled={!goal}>
                当前 Goal
              </option>
              <option value="workflow" disabled={!workflowRuns.length}>
                Workflow
              </option>
            </select>
            {taskKind === "manual" && (
              <input
                value={manualTitle}
                onChange={(e) => setManualTitle(e.target.value)}
                placeholder="任务标题"
                className="h-8 w-36 rounded-md border border-[#d9dfe7] bg-white px-2 text-[12px] outline-none"
              />
            )}
            {taskKind === "workflow" && (
              <select
                value={workflowRunId}
                onChange={(e) => setWorkflowRunId(e.target.value)}
                className="h-8 max-w-44 rounded-md border border-[#d9dfe7] bg-white px-2 text-[12px] outline-none"
              >
                {workflowRuns.map((run) => (
                  <option key={run.run_id} value={run.run_id}>
                    {run.name} · {run.status}
                  </option>
                ))}
              </select>
            )}
            <button
              type="button"
              onClick={() => void start()}
              disabled={busy}
              className="inline-flex h-8 items-center gap-1.5 rounded-md bg-[#111827] px-3 text-[12px] font-medium text-white transition hover:bg-[#2b3442] disabled:opacity-50"
            >
              <PlayIcon size={13} />
              开始
            </button>
          </div>
        )}

        {active && (
          <div className="flex items-center gap-2">
            {timer?.mode === "focus" && (
              <select
                value={skipReason}
                onChange={(e) => setSkipReason(e.target.value)}
                className="h-8 max-w-32 rounded-md border border-[#d9dfe7] bg-white px-2 text-[12px] outline-none"
                title="中断原因"
              >
                <option value="">中断原因</option>
                {interruptionReasons.map((reason) => (
                  <option key={reason} value={reason}>
                    {reason}
                  </option>
                ))}
              </select>
            )}
            <button
              type="button"
              onClick={() => void runAction(paused ? api.pomodoroResume : api.pomodoroPause)}
              disabled={busy}
              className="grid h-8 w-8 place-items-center rounded-md border border-[#d9dfe7] text-[#4f5661] transition hover:bg-[#f5f6f8] disabled:opacity-50"
              title={paused ? "继续" : "暂停"}
            >
              {paused ? <PlayIcon size={13} /> : <PauseIcon size={15} />}
            </button>
            <button
              type="button"
              onClick={() => void skip()}
              disabled={busy}
              className="grid h-8 w-8 place-items-center rounded-md border border-[#d9dfe7] text-[#4f5661] transition hover:bg-[#f5f6f8] disabled:opacity-50"
              title="跳过"
            >
              <RotateCwIcon size={15} />
            </button>
          </div>
        )}
      </div>
      {(timer?.feedback.start_message || timer?.feedback.recap_prompt || error) && (
        <div className={`mt-2 grid gap-1 text-[11px] ${error ? "text-[#b42318]" : "text-[#6f7782]"}`}>
          <div>{error || timer?.feedback.recap_prompt || timer?.feedback.start_message}</div>
          {timer?.feedback.encouragement && <div className="text-[#3f4652]">{timer.feedback.encouragement}</div>}
          {timer?.feedback.plan_steps?.length ? (
            <div className="flex flex-wrap gap-1.5">
              {timer.feedback.plan_steps.map((step, index) => (
                <span key={`${index}-${step}`} className="rounded-md bg-[#f6f7f9] px-2 py-1 text-[#59616d]">
                  {index + 1}. {step}
                </span>
              ))}
            </div>
          ) : null}
        </div>
      )}
      {rhythmSummary.length ? (
        <div className="mt-2 flex flex-wrap gap-1.5 text-[11px] text-[#6f7782]">
          {rhythmSummary.map((item) => (
            <span key={item} className="rounded-md bg-[#f6f7f9] px-2 py-1">
              {item}
            </span>
          ))}
        </div>
      ) : null}
    </section>
  );
}
