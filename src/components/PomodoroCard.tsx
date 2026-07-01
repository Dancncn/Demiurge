import { useEffect, useMemo, useState } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import * as api from "../lib/api";
import type { PomodoroMode, PomodoroPanelState } from "../lib/types";
import { PauseIcon, PlayIcon, RotateCwIcon, TargetIcon } from "./Icons";

const modeLabels: Record<PomodoroMode, string> = {
  focus: "专注",
  short_break: "短休息",
  long_break: "长休息",
  custom: "自定义",
};

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

export default function PomodoroCard() {
  const [state, setState] = useState<PomodoroPanelState | null>(null);
  const [mode, setMode] = useState<PomodoroMode>("focus");
  const [customMinutes, setCustomMinutes] = useState(25);
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
      }),
    );

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
              onClick={() => void runAction(api.pomodoroSkip)}
              disabled={busy}
              className="grid h-8 w-8 place-items-center rounded-md border border-[#d9dfe7] text-[#4f5661] transition hover:bg-[#f5f6f8] disabled:opacity-50"
              title="跳过"
            >
              <RotateCwIcon size={15} />
            </button>
          </div>
        )}
      </div>
      {(timer?.feedback.start_message || error) && (
        <div className={`mt-2 text-[11px] ${error ? "text-[#b42318]" : "text-[#6f7782]"}`}>
          {error || timer?.feedback.start_message}
        </div>
      )}
    </section>
  );
}
