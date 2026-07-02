import { useEffect, useMemo, useState } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import * as api from "../lib/api";
import { useI18n } from "../lib/i18n";
import type {
  GoalPanelState,
  PomodoroMode,
  PomodoroPanelState,
  PomodoroTaskBinding,
  PomodoroTaskKind,
  WorkflowRunProgress,
} from "../lib/types";
import { ClockIcon, PauseIcon, PlayIcon, RotateCwIcon } from "./Icons";
import { Select } from "./Select";

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
  const { t } = useI18n();
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

  // 轮询仅在 running 时启用（1s 兜底同步剩余秒数）；idle/paused 时由
  // listenPomodoroUpdated / listenPomodoroCompleted 事件驱动刷新，避免无谓 IPC。
  useEffect(() => {
    if (!running) return;
    const poll = window.setInterval(() => void refresh(), 1000);
    return () => window.clearInterval(poll);
  }, [running]);
  const progress = useMemo(() => {
    if (!duration) return 0;
    return Math.min(100, Math.max(0, ((duration - remaining) / duration) * 100));
  }, [duration, remaining]);

  const selectedWorkflow = workflowRuns.find((run) => run.run_id === workflowRunId) ?? workflowRuns[0] ?? null;
  const preferredDuration = topRhythmEntry(state?.rhythm.focus_duration_counts);
  const efficientHour = topRhythmEntry(state?.rhythm.efficient_hour_counts);
  const commonInterruption = topRhythmEntry(state?.rhythm.interruption_reasons);
  const rhythmSummary = [
    preferredDuration ? t("pomodoro.rhythmDuration", { minutes: preferredDuration }) : "",
    efficientHour ? t("pomodoro.rhythmHour", { hour: String(efficientHour).padStart(2, "0") }) : "",
    commonInterruption ? t("pomodoro.rhythmInterrupt", { reason: commonInterruption }) : "",
  ].filter(Boolean);

  function taskBinding(): PomodoroTaskBinding {
    if (taskKind === "session") {
      return {
        kind: "session",
        title: activeSessionTitle || t("pomodoro.currentChat"),
        session_id: activeSessionId || null,
      };
    }
    if (taskKind === "goal") {
      return {
        kind: "goal",
        title: goal?.objective || t("pomodoro.currentGoal"),
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
      title: manualTitle.trim() || t("pomodoro.defaultTask"),
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
    ? `${t(`pomodoro.mode.${timer?.mode ?? "focus"}`)} · ${paused ? t("pomodoro.paused") : t("pomodoro.running")}`
    : state?.timer.feedback.completion_message || t("pomodoro.ready");
  const interruptionReasons = [
    t("pomodoro.interrupt.switchTask"),
    t("pomodoro.interrupt.message"),
    t("pomodoro.interrupt.blocked"),
    t("pomodoro.interrupt.away"),
  ];
  const compactSelectTrigger =
    "flex h-8 min-w-[120px] items-center gap-1.5 rounded-md border border-[#e4e7ec] bg-[#fbfcfd] px-2.5 text-[12px] text-[#202124] outline-none transition hover:bg-white focus:shadow-[0_0_0_3px_rgba(17,24,39,0.06)] disabled:cursor-not-allowed disabled:opacity-50";

  return (
    <section className="bg-white">
      <div className="flex flex-wrap items-center gap-3">
        <div className="flex min-w-[154px] items-center gap-2">
          <div className="grid size-8 shrink-0 place-items-center rounded-md border border-[#dfe3e8] text-[#4f5661]">
            <ClockIcon size={17} />
          </div>
          <div className="min-w-0">
            <div className="text-[12px] font-semibold text-[#202124]">{t("pomodoro.title")}</div>
            <div className="truncate text-[11px] text-[#7a8088]" title={statusLabel}>
              {statusLabel}
            </div>
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
            <Select
              value={mode}
              onChange={(value) => setMode(value as PomodoroMode)}
              triggerClassName={compactSelectTrigger}
              options={[
                { value: "focus", label: t("pomodoro.option.focus") },
                { value: "short_break", label: t("pomodoro.option.shortBreak") },
                { value: "long_break", label: t("pomodoro.option.longBreak") },
                { value: "custom", label: t("pomodoro.option.custom") },
              ]}
            />
            {mode === "custom" && (
              <input
                type="number"
                min={1}
                max={240}
                value={customMinutes}
                onChange={(e) => setCustomMinutes(Math.min(240, Math.max(1, Number(e.target.value) || 1)))}
                className="h-8 w-20 rounded-md border border-[#d9dfe7] bg-white px-2 text-[12px] outline-none"
                aria-label={t("pomodoro.customMinutes")}
              />
            )}
            <Select
              value={taskKind}
              onChange={(value) => setTaskKind(value as PomodoroTaskKind)}
              triggerClassName={compactSelectTrigger}
              options={[
                { value: "manual", label: t("pomodoro.task.manual") },
                { value: "session", label: t("pomodoro.task.session") },
                ...(goal ? [{ value: "goal", label: t("pomodoro.task.goal") }] : []),
                ...(workflowRuns.length ? [{ value: "workflow", label: "Workflow" }] : []),
              ]}
            />
            {taskKind === "manual" && (
              <input
                value={manualTitle}
                onChange={(e) => setManualTitle(e.target.value)}
                placeholder={t("pomodoro.taskPlaceholder")}
                className="h-8 w-36 rounded-md border border-[#d9dfe7] bg-white px-2 text-[12px] outline-none"
              />
            )}
            {taskKind === "workflow" && (
              <Select
                value={workflowRunId}
                onChange={setWorkflowRunId}
                triggerClassName={`${compactSelectTrigger} max-w-44`}
                options={workflowRuns.map((run) => ({
                  value: run.run_id,
                  label: run.name,
                  hint: run.status,
                }))}
              />
            )}
            <button
              type="button"
              onClick={() => void start()}
              disabled={busy}
              className="inline-flex h-8 items-center gap-1.5 rounded-md bg-[#111827] px-3 text-[12px] font-medium text-white transition hover:bg-[#2b3442] disabled:opacity-50"
            >
              <PlayIcon size={13} />
              {t("pomodoro.start")}
            </button>
          </div>
        )}

        {active && (
          <div className="flex items-center gap-2">
            {timer?.mode === "focus" && (
              <Select
                value={skipReason}
                onChange={setSkipReason}
                triggerClassName={`${compactSelectTrigger} max-w-36`}
                placeholder={t("pomodoro.interruptReason")}
                options={interruptionReasons.map((reason) => ({ value: reason, label: reason }))}
              />
            )}
            <button
              type="button"
              onClick={() => void runAction(paused ? api.pomodoroResume : api.pomodoroPause)}
              disabled={busy}
              className="grid h-8 w-8 place-items-center rounded-md border border-[#d9dfe7] text-[#4f5661] transition hover:bg-[#f5f6f8] disabled:opacity-50"
              aria-label={paused ? t("pomodoro.resume") : t("pomodoro.pause")}
              title={paused ? t("pomodoro.resume") : t("pomodoro.pause")}
            >
              {paused ? <PlayIcon size={13} /> : <PauseIcon size={15} />}
            </button>
            <button
              type="button"
              onClick={() => void skip()}
              disabled={busy}
              className="grid h-8 w-8 place-items-center rounded-md border border-[#d9dfe7] text-[#4f5661] transition hover:bg-[#f5f6f8] disabled:opacity-50"
              aria-label={t("pomodoro.skip")}
              title={t("pomodoro.skip")}
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
