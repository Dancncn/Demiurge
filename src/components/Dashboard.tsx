import { useEffect, useMemo, useState } from "react";
import * as api from "../lib/api";
import type { DayCell, StatsPanel } from "../lib/types";

const LEVEL_BG = ["#eef1f5", "#cfe9dd", "#9fd6bf", "#52b894", "#10a37f"];
const AVATAR = "/demiurge.png";

function fmtNum(n: number): string {
  return n.toLocaleString();
}
function fmtTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}
function fmtHour(h: number | null): string {
  if (h == null) return "—";
  if (h === 0) return "12 AM";
  if (h === 12) return "12 PM";
  return h < 12 ? `${h} AM` : `${h - 12} PM`;
}

// Browser-preview mock so the dashboard renders without the Tauri backend.
function mockStats(): StatsPanel {
  const heatmap: DayCell[] = [];
  const today = new Date();
  for (let i = 125; i >= 0; i--) {
    const d = new Date(today);
    d.setDate(today.getDate() - i);
    const r = Math.random();
    const level = i < 12 && r > 0.4 ? Math.min(4, 1 + Math.floor(r * 4)) : r > 0.8 ? Math.floor(r * 3) : 0;
    heatmap.push({ date: d.toISOString().slice(0, 10), count: level, level });
  }
  return {
    sessions: 12,
    messages: 348,
    est_tokens: 184_300,
    active_days: 9,
    current_streak: 3,
    longest_streak: 6,
    peak_hour: 14,
    model: "deepseek-chat",
    heatmap_days: 126,
    heatmap,
  };
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-lg border border-[#eceff3] bg-[#fbfcfd] px-3 py-2">
      <div className="text-[11px] text-[#8a9099]">{label}</div>
      <div className="mt-0.5 truncate text-[15px] font-semibold tabular-nums text-[#202124]" title={value}>
        {value}
      </div>
    </div>
  );
}

function Heatmap({ cells }: { cells: DayCell[] }) {
  // pad the leading days so weekday rows align (week starts Sunday)
  const pad = cells.length ? new Date(`${cells[0].date}T00:00:00`).getDay() : 0;
  return (
    <div>
      <div
        className="grid w-fit gap-[3px]"
        style={{ gridTemplateRows: "repeat(7, 11px)", gridAutoFlow: "column" }}
      >
        {Array.from({ length: pad }).map((_, i) => (
          <div key={`pad-${i}`} style={{ width: 11, height: 11 }} />
        ))}
        {cells.map((c) => (
          <div
            key={c.date}
            title={`${c.date}: ${c.count} active`}
            className="rounded-[2px]"
            style={{ width: 11, height: 11, background: LEVEL_BG[c.level] ?? LEVEL_BG[0] }}
          />
        ))}
      </div>
      <div className="mt-2 flex items-center gap-1.5 text-[11px] text-[#9aa1ab]">
        <span>Less</span>
        {LEVEL_BG.map((bg) => (
          <span key={bg} className="rounded-[2px]" style={{ width: 11, height: 11, background: bg }} />
        ))}
        <span>More</span>
      </div>
    </div>
  );
}

export function Dashboard({
  greeting,
  suggestions,
  onSuggestionClick,
}: {
  greeting: string;
  suggestions: string[];
  onSuggestionClick: (text: string) => void;
}) {
  const [stats, setStats] = useState<StatsPanel | null>(null);

  useEffect(() => {
    (async () => {
      try {
        setStats(await api.sessionStats(new Date().getTimezoneOffset()));
      } catch {
        if (!("__TAURI_INTERNALS__" in window)) setStats(mockStats());
      }
    })();
  }, []);

  const metrics = useMemo(() => {
    if (!stats) return [];
    return [
      { label: "Sessions", value: fmtNum(stats.sessions) },
      { label: "Messages", value: fmtNum(stats.messages) },
      { label: "Tokens (est.)", value: fmtTokens(stats.est_tokens) },
      { label: "Active days", value: fmtNum(stats.active_days) },
      { label: "Current streak", value: `${stats.current_streak}d` },
      { label: "Longest streak", value: `${stats.longest_streak}d` },
      { label: "Peak hour", value: fmtHour(stats.peak_hour) },
      { label: "Model", value: stats.model || "—" },
    ];
  }, [stats]);

  return (
    <div className="mx-auto flex w-full max-w-2xl flex-col items-center px-4 py-10">
      <img src={AVATAR} alt="" className="mb-4 size-12 rounded-2xl border border-[#e6e9ee] bg-[#faf8fd] object-contain" />
      <h1 className="mb-6 text-[22px] font-semibold tracking-tight text-[#202124]">{greeting}</h1>

      {stats && (
        <div className="mb-6 w-full overflow-hidden rounded-xl border border-[#e6e9ee] bg-white shadow-[0_1px_3px_rgba(15,23,42,0.05)]">
          <div className="flex items-center justify-between border-b border-[#eceff3] px-4 py-2.5">
            <span className="text-[12px] font-semibold uppercase tracking-wide text-[#8a9099]">Overview</span>
            <span className="text-[11px] text-[#9aa1ab]">Last {Math.round(stats.heatmap_days / 7)} weeks</span>
          </div>
          <div className="grid grid-cols-2 gap-2 p-3 sm:grid-cols-4">
            {metrics.map((m) => (
              <Metric key={m.label} label={m.label} value={m.value} />
            ))}
          </div>
          <div className="overflow-x-auto px-3 pb-3 pt-1">
            <Heatmap cells={stats.heatmap} />
          </div>
        </div>
      )}

      <div className="grid w-full grid-cols-1 gap-2 sm:grid-cols-2">
        {suggestions.map((item) => (
          <button
            key={item}
            onClick={() => onSuggestionClick(item)}
            className="cf-lift cf-press rounded-lg border border-[#e8eaef] bg-white px-3.5 py-3 text-left text-[13px] text-[#3f4652] hover:border-[#dcdfe5] hover:bg-[#f7f8fa] hover:shadow-[0_8px_20px_rgba(15,23,42,0.06)]"
          >
            {item}
          </button>
        ))}
      </div>
    </div>
  );
}

export default Dashboard;
