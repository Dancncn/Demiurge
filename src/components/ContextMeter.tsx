import { useEffect, useRef, useState } from "react";
import * as api from "../lib/api";
import type { ContextPanelState } from "../lib/types";

function fmt(n: number): string {
  if (!n || n <= 0) return "0";
  if (n >= 1000) return `${(n / 1000).toFixed(n >= 10000 ? 0 : 1)}K`;
  return String(n);
}

/** Circular context-usage ring; click opens a popover with the token breakdown. */
export function ContextMeter({
  maxInputTokens,
  onOpenSettings,
}: {
  maxInputTokens: number;
  onOpenSettings: () => void;
}) {
  const [open, setOpen] = useState(false);
  const [state, setState] = useState<ContextPanelState | null>(null);
  const ref = useRef<HTMLDivElement | null>(null);

  async function refresh() {
    try {
      setState(await api.contextPanelState());
    } catch {
      /* outside Tauri / no session */
    }
  }

  useEffect(() => {
    void refresh();
  }, []);

  useEffect(() => {
    if (!open) return;
    void refresh();
    const onDown = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && setOpen(false);
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const budget = state?.max_input_tokens || maxInputTokens || 0;
  const used = state?.projected_total_tokens ?? 0;
  const frac = budget > 0 ? Math.min(1, used / budget) : 0;
  const pct = Math.round(frac * 100);

  // ring geometry
  const r = 8.5;
  const c = 2 * Math.PI * r;
  const stroke = frac >= 0.92 ? "#dc2626" : frac >= 0.72 ? "#d97706" : "#10a37f";

  const rows: { label: string; value: string }[] = state
    ? [
        { label: "Messages", value: String(state.message_count) },
        { label: "System prompt", value: `${fmt(state.system_prompt_tokens)} tok` },
        { label: "Tools", value: `${fmt(state.tools_tokens)} tok` },
        { label: "Summary", value: `${fmt(state.summary_tokens)} tok` },
        { label: "History", value: `${fmt(state.estimated_history_tokens)} tok` },
        { label: "History budget", value: `${fmt(state.history_budget_tokens)} tok` },
        { label: "Reserved output", value: `${fmt(state.reserved_output_tokens)} tok` },
        { label: "Max input", value: `${fmt(state.max_input_tokens)} tok` },
      ]
    : [];

  return (
    <div ref={ref} className="relative">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        title={`Context usage: ${fmt(used)} / ${fmt(budget)} tokens (${pct}%)`}
        aria-label="Context usage"
        className="grid size-8 place-items-center rounded-full text-[#7a8088] transition hover:bg-[#f1f2f5]"
      >
        <svg width="22" height="22" viewBox="0 0 22 22" className="-rotate-90">
          <circle cx="11" cy="11" r={r} fill="none" stroke="#e5e8ed" strokeWidth="2.5" />
          <circle
            cx="11"
            cy="11"
            r={r}
            fill="none"
            stroke={stroke}
            strokeWidth="2.5"
            strokeLinecap="round"
            strokeDasharray={c}
            strokeDashoffset={c * (1 - frac)}
            style={{ transition: "stroke-dashoffset 0.3s ease, stroke 0.3s ease" }}
          />
        </svg>
      </button>
      {open && (
        <div className="cf-menu-in cf-dropdown absolute bottom-[calc(100%+8px)] right-0 z-30 w-64 overflow-hidden">
          <div className="border-b border-[#eef1f4] px-3 py-2.5">
            <div className="flex items-baseline justify-between">
              <span className="text-[12px] font-semibold text-[#202124]">Context</span>
              <span className="text-[11px] tabular-nums text-[#7a8088]">
                {fmt(used)} / {fmt(budget)} · {pct}%
              </span>
            </div>
            <div className="mt-2 h-1 overflow-hidden rounded-full bg-[#eef1f5]">
              <div className="h-full rounded-full transition-[width]" style={{ width: `${pct}%`, background: stroke }} />
            </div>
          </div>
          <div className="max-h-60 overflow-y-auto p-1">
            {rows.length === 0 ? (
              <div className="px-2.5 py-2 text-[12px] text-[#9aa1ab]">No active session yet.</div>
            ) : (
              rows.map((row) => (
                <div key={row.label} className="flex items-center justify-between px-2.5 py-1.5 text-[12px]">
                  <span className="text-[#5f6368]">{row.label}</span>
                  <span className="tabular-nums font-medium text-[#202124]">{row.value}</span>
                </div>
              ))
            )}
          </div>
          <button
            type="button"
            onClick={() => {
              setOpen(false);
              onOpenSettings();
            }}
            className="block w-full border-t border-[#eef1f4] px-3 py-2 text-left text-[12px] font-medium text-[#0b57d0] transition hover:bg-[#f6f9ff]"
          >
            Context settings →
          </button>
        </div>
      )}
    </div>
  );
}

export default ContextMeter;
