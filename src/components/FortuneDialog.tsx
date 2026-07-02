import { useEffect, useRef, useState, type CSSProperties } from "react";
import { useI18n } from "../lib/i18n";
import {
  drawFortune,
  findEntry,
  getTodayRecord,
  markDismissedToday,
  resetTodayRecord,
  LEVEL_META,
  type FortuneEntry,
} from "../lib/fortune";

interface Props {
  open: boolean;
  onClose: () => void;
}

type Phase = "guide" | "shaking" | "result";

/** 是否启用了系统级减少动效偏好。降级时缩短摇签等待、静音音效。 */
function prefersReducedMotion(): boolean {
  return typeof window !== "undefined" && !!window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
}

/** 摇签筒图标。guide 态加 cf-breathe 呼吸，shaking 态加 cf-shake 摇晃。 */
function FortuneTube({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 64 84" width="76" height="100" aria-hidden className={className}>
      <defs>
        <linearGradient id="cf-tube" x1="0" y1="0" x2="1" y2="0">
          <stop offset="0" stopColor="#a14a3a" />
          <stop offset="0.5" stopColor="#8b3a2e" />
          <stop offset="1" stopColor="#6e2a22" />
        </linearGradient>
      </defs>
      {/* 签（顶部露出，长短不一） */}
      <g fill="#3a2a1e">
        <rect x="20" y="4" width="3" height="26" rx="1.5" />
        <rect x="29" y="2" width="3" height="28" rx="1.5" />
        <rect x="38" y="6" width="3" height="24" rx="1.5" />
      </g>
      {/* 签筒主体 */}
      <path
        d="M10 30 Q10 26 14 26 L50 26 Q54 26 54 30 L50 76 Q50 80 46 80 L18 80 Q14 80 14 76 Z"
        fill="url(#cf-tube)"
      />
      {/* 筒口 */}
      <ellipse cx="32" cy="28" rx="22" ry="4.5" fill="#5e221b" />
      <ellipse cx="32" cy="27" rx="20" ry="3.5" fill="#2a1a12" />
      {/* 筒身饰带 */}
      <rect x="14" y="48" width="36" height="3" fill="#5e221b" opacity="0.55" />
    </svg>
  );
}

function CloseIcon({ size = 18 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round">
      <path d="M6 6l12 12M18 6L6 18" />
    </svg>
  );
}

export default function FortuneDialog({ open, onClose }: Props) {
  const { t } = useI18n();
  const [phase, setPhase] = useState<Phase>("guide");
  const [entry, setEntry] = useState<FortuneEntry | null>(null);
  const shakeTimer = useRef<number | null>(null);
  const dialogRef = useRef<HTMLDivElement | null>(null);
  const prevActiveRef = useRef<HTMLElement | null>(null);

  // 打开时初始化：今天已抽就直接看结果，否则进入引导态。
  useEffect(() => {
    if (!open) return;
    const rec = getTodayRecord();
    if (rec) {
      setEntry(findEntry(rec.entryId) ?? null);
      setPhase("result");
    } else {
      setEntry(null);
      setPhase("guide");
    }
    return () => {
      if (shakeTimer.current) {
        window.clearTimeout(shakeTimer.current);
        shakeTimer.current = null;
      }
    };
  }, [open]);

  // 键盘交互：ESC 关闭（摇签进行中禁用避免打断）、Tab 焦点陷阱防跳到背后输入框。
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        if (phase !== "shaking") onClose();
        return;
      }
      if (e.key === "Tab" && dialogRef.current) {
        const root = dialogRef.current;
        const focusable = Array.from(
          root.querySelectorAll<HTMLElement>(
            'button:not([disabled]), a[href], input:not([disabled]), [tabindex]:not([tabindex="-1"])',
          ),
        );
        if (focusable.length === 0) return;
        const first = focusable[0];
        const last = focusable[focusable.length - 1];
        const active = document.activeElement as HTMLElement | null;
        if (e.shiftKey) {
          if (active === first || !root.contains(active)) {
            e.preventDefault();
            last.focus();
          }
        } else if (active === last || !root.contains(active)) {
          e.preventDefault();
          first.focus();
        }
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open, phase, onClose]);

  // 焦点管理：打开时记录触发元素并聚焦弹窗主按钮，关闭时归还焦点到触发按钮。
  useEffect(() => {
    if (!open) {
      prevActiveRef.current?.focus?.();
      prevActiveRef.current = null;
      return;
    }
    if (!prevActiveRef.current) prevActiveRef.current = document.activeElement as HTMLElement | null;
    const raf = requestAnimationFrame(() => {
      const btn = dialogRef.current?.querySelector<HTMLElement>('[data-autofocus="true"]');
      btn?.focus();
    });
    return () => cancelAnimationFrame(raf);
  }, [open, phase]);

  function handleDraw() {
    if (phase !== "guide") return;
    setPhase("shaking");
    if (shakeTimer.current) window.clearTimeout(shakeTimer.current);
    // 降级动效用户：缩短等待到 300ms，避免静止签筒配"摇签中"文案干等 1.2s。
    const delay = prefersReducedMotion() ? 300 : 1200;
    shakeTimer.current = window.setTimeout(() => {
      setEntry(drawFortune());
      setPhase("result");
      shakeTimer.current = null;
    }, delay);
  }

  function handleClose() {
    if (phase === "shaking") {
      // 摇签进行中关闭=取消抽签：清掉定时器，不写记录、不标记忽略，直接关闭。
      if (shakeTimer.current) {
        window.clearTimeout(shakeTimer.current);
        shakeTimer.current = null;
      }
      onClose();
      return;
    }
    // guide 态（未抽就关）标记今日已忽略，避免每次启动强弹打扰。
    if (phase === "guide") markDismissedToday();
    onClose();
  }

  if (!open) return null;

  const meta = entry ? LEVEL_META[entry.level] : null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-[#111827]/35 p-4 backdrop-blur-[2px]"
      role="dialog"
      aria-modal="true"
      aria-label={t("fortune.title")}
    >
      <div
        ref={dialogRef}
        tabIndex={-1}
        className="cf-menu-in flex w-full max-w-md flex-col overflow-hidden rounded-2xl border border-[#d7dbe2] bg-white shadow-[0_24px_80px_rgba(15,23,42,0.28)] outline-none"
      >
        <header className="flex items-center justify-between border-b border-[#eceff3] bg-[#fbfcfd] px-5 py-3.5">
          <div className="text-[15px] font-semibold text-[#202124]">{t("fortune.title")}</div>
          <button
            type="button"
            onClick={handleClose}
            aria-label={t("fortune.close")}
            className="cf-press grid h-8 w-8 place-items-center rounded-md text-[#6f7782] transition hover:bg-[#eef1f5] hover:text-[#202124]"
          >
            <CloseIcon size={18} />
          </button>
        </header>

        <div className="flex min-h-0 flex-1 flex-col items-center px-6 py-8">
          {phase === "guide" && (
            <div className="cf-message-in flex flex-col items-center text-center">
              <FortuneTube className="cf-breathe" />
              <h2 className="mt-4 text-[20px] font-semibold tracking-tight text-[#202124]">
                {t("fortune.guideTitle")}
              </h2>
              <p className="mt-2 max-w-[280px] text-[13px] leading-[1.7] text-[#6f7782]">
                {t("fortune.guideDesc")}
              </p>
              <button
                type="button"
                data-autofocus
                onClick={handleDraw}
                className="cf-press mt-6 inline-flex h-10 items-center justify-center rounded-full bg-[#b91c1c] px-7 text-[14px] font-medium text-white shadow-[0_4px_14px_rgba(185,28,28,0.28)] transition hover:bg-[#a11616]"
              >
                {t("fortune.draw")}
              </button>
            </div>
          )}

          {phase === "shaking" && (
            <div className="flex flex-col items-center text-center">
              <FortuneTube className="cf-shake" />
              <p className="mt-6 text-[13px] text-[#8a9099]">{t("fortune.drawing")}</p>
            </div>
          )}

          {phase === "result" && entry && meta && (
            <ResultView entry={entry} t={t} onClose={handleClose} />
          )}

          {phase === "result" && !entry && (
            // 今日已抽但签文数据缺失（版本回退/签文库裁剪）：温和兜底，允许重抽。
            <div className="cf-message-in flex flex-col items-center text-center">
              <p className="text-[13px] text-[#6f7782]">{t("fortune.missingKey")}</p>
              <div className="mt-6 flex gap-2">
                <button
                  type="button"
                  data-autofocus
                  onClick={() => {
                    resetTodayRecord();
                    setEntry(null);
                    setPhase("guide");
                  }}
                  className="cf-press inline-flex h-9 items-center rounded-md bg-[#b91c1c] px-5 text-[13px] font-medium text-white hover:bg-[#a11616]"
                >
                  {t("fortune.redraw")}
                </button>
                <button
                  type="button"
                  onClick={handleClose}
                  className="cf-press inline-flex h-9 items-center rounded-md border border-[#d9d9d9] bg-white px-5 text-[13px] font-medium text-[#344054] hover:bg-[#f5f5f5]"
                >
                  {t("fortune.close")}
                </button>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function ResultView({
  entry,
  t,
  onClose,
}: {
  entry: FortuneEntry;
  t: (key: string, vars?: Record<string, string | number>) => string;
  onClose: () => void;
}) {
  const meta = LEVEL_META[entry.level];
  const levelLabel = t(`fortune.level.${entry.level}`);
  const verses = entry.verse.split("／");

  return (
    <div className="cf-fall w-full" aria-live="polite">
      {/* 顶部装饰条：等级强调色 */}
      <div className="mx-auto mb-5 h-1 w-16 rounded-full" style={{ background: meta.accent }} />

      {/* 等级标签 + 签号 */}
      <div className="cf-rise flex items-center justify-center gap-2" style={{ "--i": 0 } as CSSProperties}>
        <span
          className="rounded-full px-3 py-1 text-[12px] font-semibold"
          style={{ background: meta.chipBg, color: meta.chipText }}
        >
          {levelLabel}
        </span>
        <span className="text-[11px] text-[#9aa1ab]">{entry.id}</span>
      </div>

      {/* 签题 */}
      <h2
        className="cf-rise mt-4 text-center text-[26px] font-semibold tracking-[0.04em]"
        style={{ "--i": 1, color: meta.accent } as CSSProperties}
      >
        {entry.title}
      </h2>

      {/* 签诗 */}
      <div
        className="cf-rise mt-3 text-center text-[15px] leading-[2] text-[#3f3f3f]"
        style={{ "--i": 2 } as CSSProperties}
      >
        {verses.map((line, i) => (
          <div key={i}>{line}</div>
        ))}
      </div>

      {/* 解签 */}
      <div
        className="cf-rise mt-5 w-full rounded-lg border border-[#eceff3] bg-[#fbfcfd] px-4 py-3 text-[13px] leading-[1.75] text-[#59616d]"
        style={{ "--i": 3 } as CSSProperties}
      >
        <div className="mb-1 text-[11px] font-semibold uppercase tracking-wide text-[#9aa1ab]">
          {t("fortune.interpretation")}
        </div>
        {entry.interpretation}
      </div>

      {/* 祝福 */}
      <div
        className="cf-rise mt-4 text-center text-[14px] font-medium leading-[1.7]"
        style={{ "--i": 4, color: meta.accent } as CSSProperties}
      >
        {entry.blessing}
      </div>

      {/* 明日再来提示 + 关闭 */}
      <div className="cf-rise mt-6 flex flex-col items-center gap-3" style={{ "--i": 5 } as CSSProperties}>
        <p className="text-[12px] text-[#9aa1ab]">{t("fortune.tomorrow")}</p>
        <button
          type="button"
          data-autofocus
          onClick={onClose}
          className="cf-press inline-flex h-10 items-center justify-center rounded-full px-7 text-[14px] font-medium text-white transition"
          style={{ background: meta.accent }}
        >
          {t("fortune.close")}
        </button>
      </div>
    </div>
  );
}
