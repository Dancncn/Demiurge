import { useEffect, useMemo, useState, type CSSProperties } from "react";
import * as api from "@/lib/api";
import { useI18n } from "@/lib/i18n";
import type { DayCell, StatsPanel } from "@/lib/types";
import { findEntry, getTodayRecord } from "@/lib/fortune";
import { ChevronDownIcon } from "@/shared/components/Icons";

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
      className="dashboard-metric cf-rise min-w-0 px-3 py-2.5"
      style={{ "--i": index } as CSSProperties}
    >
      <div className="md-type-label-small text-[#8a9099]">{label}</div>
      <div className="md-type-title-small truncate font-semibold tabular-nums text-[#202124]" title={value}>
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
      <div className="md-type-label-small mt-2 flex items-center gap-1.5 text-[#9aa1ab]">
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
    <main className="dashboard-root mx-auto w-full max-w-[1120px] pb-6 pt-5">
      <header className="dashboard-greeting mb-5 flex items-center gap-3">
        <img src={AVATAR} alt="" className="size-10 rounded-lg border border-[#e6e9ee] bg-[#faf8fd] object-contain" />
        <h1 className="md-type-title-large font-semibold text-[#202124]">{greeting}</h1>
      </header>

      <div className="dashboard-layout grid min-w-0 gap-4">
        {stats && (
          <section className="dashboard-panel min-w-0 overflow-hidden rounded-lg border border-[#e6e9ee] bg-white shadow-[0_1px_3px_rgba(15,23,42,0.05)]">
            <header className="flex items-center justify-between border-b border-[#eceff3] px-4 py-3">
              <h2 className="md-type-title-small font-semibold text-[#202124]">{t("dashboard.overview")}</h2>
              <span className="md-type-label-small text-[#9aa1ab]">
                {t("dashboard.lastWeeks", { n: Math.round(stats.heatmap_days / 7) })}
              </span>
            </header>
            <div className="dashboard-metrics grid grid-cols-2 sm:grid-cols-4">
              {metrics.map((metric, index) => (
                <Metric key={metric.label} index={index} label={metric.label} value={metric.value} />
              ))}
            </div>
            <div className="dashboard-heatmap overflow-x-auto border-t border-[#eceff3] px-4 py-3">
              <Heatmap cells={stats.heatmap} />
            </div>
          </section>
        )}

        {onOpenFortune && (
          <button
            type="button"
            onClick={onOpenFortune}
            className="dashboard-fortune cf-lift flex min-h-[180px] w-full flex-col items-start rounded-lg border border-[#e6e9ee] bg-white p-4 text-left shadow-[0_1px_3px_rgba(15,23,42,0.05)]"
          >
            <span className="dashboard-fortune-icon grid size-11 shrink-0 place-items-center rounded-lg bg-[#fde8f3]">
              <img src="/fortune-icon.png" alt="" className="size-9 object-contain" />
            </span>
            <span className="mt-4 min-w-0">
              <span className="md-type-title-small block font-semibold text-[#202124]">{t("fortune.cardTitle")}</span>
              <span className="md-type-body-small mt-1 block text-[#7a8088]">{fortuneDesc}</span>
            </span>
            <span className="dashboard-fortune-action md-type-label-medium mt-auto flex w-full items-center justify-between pt-4 font-semibold text-[#b91c1c]">
              {fortuneAction}
              <ChevronDownIcon size={16} className="-rotate-90" />
            </span>
          </button>
        )}
      </div>
    </main>
  );
}

export default Dashboard;
