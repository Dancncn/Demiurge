import { useEffect, useMemo, useState, type CSSProperties } from "react";
import * as api from "../lib/api";
import { useI18n } from "../lib/i18n";
import type { DayCell, StatsPanel } from "../lib/types";
import { findEntry, getTodayRecord } from "../lib/fortune";

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

function Metric({ label, value, index }: { label: string; value: string; index: number }) {
  return (
    <div
      className="cf-rise rounded-md border border-[#eceff3] bg-[#fbfcfd] px-2.5 py-1.5"
      style={{ "--i": index } as CSSProperties}
    >
      <div className="text-[11px] text-[#8a9099]">{label}</div>
      <div className="truncate text-[14px] font-semibold tabular-nums text-[#202124]" title={value}>
        {value}
      </div>
    </div>
  );
}

function Heatmap({ cells }: { cells: DayCell[] }) {
  const { t } = useI18n();
  // pad the leading days so weekday rows align (week starts Sunday)
  const pad = cells.length ? new Date(`${cells[0].date}T00:00:00`).getDay() : 0;
  return (
    <div>
      <div
        className="grid w-fit gap-[3px]"
        style={{ gridTemplateRows: "repeat(7, 9px)", gridAutoFlow: "column" }}
      >
        {Array.from({ length: pad }).map((_, i) => (
          <div key={`pad-${i}`} style={{ width: 9, height: 9 }} />
        ))}
        {cells.map((c) => (
          <div
            key={c.date}
            title={`${c.date}: ${c.count} active`}
            className="rounded-[2px]"
            style={{ width: 9, height: 9, background: LEVEL_BG[c.level] ?? LEVEL_BG[0] }}
          />
        ))}
      </div>
      <div className="mt-2 flex items-center gap-1.5 text-[11px] text-[#9aa1ab]">
        <span>{t("dashboard.less")}</span>
        {LEVEL_BG.map((bg) => (
          <span key={bg} className="rounded-[2px]" style={{ width: 9, height: 9, background: bg }} />
        ))}
        <span>{t("dashboard.more")}</span>
      </div>
    </div>
  );
}

export function Dashboard({ greeting, onOpenFortune }: { greeting: string; onOpenFortune?: () => void }) {
  const { t } = useI18n();
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
      { label: t("dashboard.sessions"), value: fmtNum(stats.sessions) },
      { label: t("dashboard.messages"), value: fmtNum(stats.messages) },
      { label: t("dashboard.tokens"), value: fmtTokens(stats.est_tokens) },
      { label: t("dashboard.activeDays"), value: fmtNum(stats.active_days) },
      { label: t("dashboard.currentStreak"), value: t("unit.days", { n: stats.current_streak }) },
      { label: t("dashboard.longestStreak"), value: t("unit.days", { n: stats.longest_streak }) },
      { label: t("dashboard.peakHour"), value: fmtHour(stats.peak_hour) },
      { label: t("dashboard.model"), value: stats.model || "—" },
    ];
  }, [stats, t]);

  // 今日吉签：若已抽则回看签题，否则展示引导文案。
  const todayEntry = (() => {
    const rec = getTodayRecord();
    return rec ? findEntry(rec.entryId) ?? null : null;
  })();
  const fortuneDesc = todayEntry ? todayEntry.title : t("fortune.cardDesc");
  const fortuneAction = todayEntry ? t("fortune.cardView") : t("fortune.cardDraw");

  return (
    <div className="mx-auto flex w-full max-w-2xl flex-col items-center px-4 pb-5 pt-4">
      <div className="mb-4 flex items-center justify-center gap-3">
        <img src={AVATAR} alt="" className="size-9 rounded-xl border border-[#e6e9ee] bg-[#faf8fd] object-contain" />
        <h1 className="text-[20px] font-semibold tracking-tight text-[#202124]">{greeting}</h1>
      </div>

      {stats && (
        <div className="mb-4 w-full overflow-hidden rounded-xl border border-[#e6e9ee] bg-white shadow-[0_1px_3px_rgba(15,23,42,0.05)]">
          <div className="flex items-center justify-between border-b border-[#eceff3] px-4 py-2">
            <span className="text-[12px] font-semibold uppercase tracking-wide text-[#8a9099]">{t("dashboard.overview")}</span>
            <span className="text-[11px] text-[#9aa1ab]">{t("dashboard.lastWeeks", { n: Math.round(stats.heatmap_days / 7) })}</span>
          </div>
          <div className="grid grid-cols-2 gap-2 p-2.5 sm:grid-cols-4">
            {metrics.map((m, i) => (
              <Metric key={m.label} index={i} label={m.label} value={m.value} />
            ))}
          </div>
          <div className="overflow-x-auto px-3 pb-2.5 pt-0.5">
            <Heatmap cells={stats.heatmap} />
          </div>
        </div>
      )}

      {onOpenFortune && (
        <button
          type="button"
          onClick={onOpenFortune}
          className="cf-lift mb-4 flex w-full items-center gap-3 rounded-xl border border-[#e6e9ee] bg-white px-4 py-3 text-left shadow-[0_1px_3px_rgba(15,23,42,0.05)]"
        >
          <span className="grid size-10 shrink-0 place-items-center rounded-lg bg-[#fff1e6] text-[#b91c1c]">
            <svg viewBox="0 0 64 84" width="22" height="28" aria-hidden>
              <path d="M10 30 Q10 26 14 26 L50 26 Q54 26 54 30 L50 76 Q50 80 46 80 L18 80 Q14 80 14 76 Z" fill="#8b3a2e" />
              <ellipse cx="32" cy="28" rx="22" ry="4.5" fill="#5e221b" />
              <g fill="#3a2a1e">
                <rect x="20" y="4" width="3" height="26" rx="1.5" />
                <rect x="29" y="2" width="3" height="28" rx="1.5" />
                <rect x="38" y="6" width="3" height="24" rx="1.5" />
              </g>
            </svg>
          </span>
          <span className="min-w-0 flex-1">
            <span className="block text-[13px] font-semibold text-[#202124]">{t("fortune.cardTitle")}</span>
            <span className="block truncate text-[11px] text-[#7a8088]">{fortuneDesc}</span>
          </span>
          <span className="shrink-0 text-[12px] font-medium text-[#b91c1c]">{fortuneAction}</span>
        </button>
      )}
    </div>
  );
}

export default Dashboard;
