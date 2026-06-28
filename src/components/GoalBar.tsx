import type { ReactNode } from "react";
import type { GoalPanelState, GoalProgressEvent, GoalStatus } from "../lib/types";
import { CloseIcon, PauseIcon, PlayIcon, RotateCwIcon, TargetIcon } from "./Icons";

export type GoalAction = "pause" | "resume" | "continue" | "clear";

interface GoalBarProps {
  goal: GoalPanelState | null;
  busy: boolean;
  progress: GoalProgressEvent | null;
  onAction: (action: GoalAction) => void;
}

const STATUS_TONES: Record<GoalStatus, string> = {
  active: "border-[#bfe3cf] bg-[#eef9f3] text-[#1f7a4d]",
  paused: "border-[#f2d7a5] bg-[#fff8e8] text-[#8a5a00]",
  blocked: "border-[#f1b8b8] bg-[#fff1f1] text-[#b42318]",
  budget_limited: "border-[#d8c7f1] bg-[#f6f1ff] text-[#6941c6]",
  usage_limited: "border-[#c8d7f3] bg-[#eff5ff] text-[#2559a8]",
  max_turns: "border-[#f2d7a5] bg-[#fff8e8] text-[#8a5a00]",
  complete: "border-[#cfd6df] bg-[#f6f7f9] text-[#59616d]",
};

function formatNumber(value: number) {
  return new Intl.NumberFormat().format(value);
}

function formatTokens(goal: GoalPanelState) {
  if (typeof goal.token_budget === "number") {
    return `${formatNumber(goal.tokens_used)} / ${formatNumber(goal.token_budget)}`;
  }
  return `${formatNumber(goal.tokens_used)} tokens`;
}

function IconButton({
  label,
  disabled,
  onClick,
  children,
}: {
  label: string;
  disabled?: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      disabled={disabled}
      onClick={onClick}
      className="grid h-7 w-7 shrink-0 place-items-center rounded-md text-[#59616d] transition hover:bg-[#eef1f5] hover:text-[#202124] disabled:cursor-not-allowed disabled:opacity-35 disabled:hover:bg-transparent disabled:hover:text-[#59616d]"
    >
      {children}
    </button>
  );
}

export default function GoalBar({ goal, busy, progress, onAction }: GoalBarProps) {
  if (!goal) return null;

  const statusTone = STATUS_TONES[goal.status] ?? STATUS_TONES.active;
  const detail =
    goal.status === "blocked" && goal.last_block_reason
      ? goal.last_block_reason
      : progress?.message ?? (goal.status === "active" ? "Ready for continuation" : goal.status_label);

  return (
    <section className="flex min-h-[46px] shrink-0 items-center gap-3 border-b border-[#eceff3] bg-white px-3 py-2">
      <div className="grid h-8 w-8 shrink-0 place-items-center rounded-md border border-[#dfe3e8] bg-[#fbfcfd] text-[#4f5661]">
        <TargetIcon size={17} />
      </div>

      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-2">
          <span className={`shrink-0 rounded-full border px-2 py-0.5 text-[11px] font-semibold ${statusTone}`}>
            {goal.status_label}
          </span>
          <span className="min-w-0 truncate text-[13px] font-medium text-[#202124]" title={goal.objective}>
            {goal.objective}
          </span>
        </div>
        <div className="mt-0.5 flex min-w-0 flex-wrap items-center gap-x-3 gap-y-0.5 text-[11px] text-[#7b8490]">
          <span>Tokens {formatTokens(goal)}</span>
          <span>Turns {goal.turns_executed}/{goal.max_turns}</span>
          <span>Active {goal.elapsed}</span>
          <span className="min-w-0 truncate" title={detail}>
            {detail}
          </span>
        </div>
      </div>

      <div className="flex shrink-0 items-center gap-1">
        <IconButton label="Pause goal" disabled={!goal.can_pause} onClick={() => onAction("pause")}>
          <PauseIcon size={16} />
        </IconButton>
        <IconButton label="Resume goal" disabled={!goal.can_resume || busy} onClick={() => onAction("resume")}>
          <PlayIcon size={15} />
        </IconButton>
        <IconButton label="Continue goal" disabled={!goal.can_continue || busy} onClick={() => onAction("continue")}>
          <RotateCwIcon size={16} />
        </IconButton>
        <IconButton label="Clear goal" disabled={!goal.can_clear} onClick={() => onAction("clear")}>
          <CloseIcon size={16} />
        </IconButton>
      </div>
    </section>
  );
}
