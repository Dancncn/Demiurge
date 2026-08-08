import { useEffect, useRef, useState, type CSSProperties } from "react";
import { useI18n } from "@/lib/i18n";
import { CloseIcon, SparklesIcon } from "@/shared/components/Icons";
import {
  drawFortune,
  findEntry,
  getTodayRecord,
  markDismissedToday,
  resetTodayRecord,
  LEVEL_META,
  type FortuneEntry,
} from "@/lib/fortune";

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
    <img
      src="/fortune-icon.png"
      alt=""
      aria-hidden
      className={`fortune-tube h-[82px] w-[82px] object-contain ${className ?? ""}`}
    />
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
      className="fortune-backdrop fixed inset-0 z-50 flex items-center justify-center bg-[#111827]/30 p-4"
      role="dialog"
      aria-modal="true"
      aria-label={t("fortune.title")}
    >
      <div
        ref={dialogRef}
        tabIndex={-1}
        className="fortune-surface cf-menu-in flex max-h-[calc(100vh-32px)] w-full max-w-[470px] flex-col overflow-hidden rounded-lg border border-[#cfd4dc] bg-white shadow-[0_18px_48px_rgba(15,23,42,0.24)] outline-none"
      >
        <header className="fortune-header flex h-11 shrink-0 items-center justify-between border-b border-[#dde1e7] bg-[#f2f4f7] px-3">
          <div className="md-type-title-small flex min-w-0 items-center gap-2 font-semibold text-[#252a31]">
            <SparklesIcon size={15} className="fortune-accent text-[#8b3a2e]" />
            <span className="truncate">{t("fortune.title")}</span>
          </div>
          <button
            type="button"
            onClick={handleClose}
            aria-label={t("fortune.close")}
            className="md-icon-button cf-press grid size-7 place-items-center rounded text-[#66707d] transition hover:bg-[#e3e7ec] hover:text-[#202124]"
          >
            <CloseIcon size={16} />
          </button>
        </header>

        <div className="min-h-0 flex-1 overflow-y-auto">
          {phase === "guide" && (
            <div className="cf-message-in">
              <div className="grid min-h-[190px] grid-cols-[82px_minmax(0,1fr)] items-center gap-5 px-6 py-6">
                <div className="grid h-[112px] place-items-center border-r border-[#e2e5ea] pr-5">
                  <FortuneTube className="cf-breathe" />
                </div>
                <div className="min-w-0 text-left">
                  <h2 className="md-type-title-medium font-semibold text-[#202124]">{t("fortune.guideTitle")}</h2>
                  <p className="md-type-body-medium mt-2 text-[#66707d]">{t("fortune.guideDesc")}</p>
                </div>
              </div>
              <footer className="flex justify-end border-t border-[#dde1e7] bg-[#f8f9fb] px-4 py-3">
                <button
                  type="button"
                  data-autofocus
                  onClick={handleDraw}
                  className="fortune-primary-action md-button md-button-filled cf-press inline-flex items-center justify-center gap-1.5 rounded-md border border-[#8f2f25] bg-[#9f3b30] px-4 text-white transition hover:bg-[#8d3027]"
                >
                  <SparklesIcon size={14} />
                  {t("fortune.draw")}
                </button>
              </footer>
            </div>
          )}

          {phase === "shaking" && (
            <div className="flex min-h-[246px] flex-col items-center justify-center px-6 py-8 text-center">
              <FortuneTube className="cf-shake" />
              <p className="md-type-body-medium mt-5 text-[#737b86]">{t("fortune.drawing")}</p>
            </div>
          )}

          {phase === "result" && entry && meta && (
            <ResultView entry={entry} t={t} onClose={handleClose} />
          )}

          {phase === "result" && !entry && (
            // 今日已抽但签文数据缺失（版本回退/签文库裁剪）：温和兜底，允许重抽。
            <div className="cf-message-in">
              <div className="md-type-body-medium px-6 py-8 text-[#66707d]">{t("fortune.missingKey")}</div>
              <div className="flex justify-end gap-2 border-t border-[#dde1e7] bg-[#f8f9fb] px-4 py-3">
                <button
                  type="button"
                  data-autofocus
                  onClick={() => {
                    resetTodayRecord();
                    setEntry(null);
                    setPhase("guide");
                  }}
                  className="fortune-primary-action md-button md-button-filled cf-press inline-flex items-center rounded-md border border-[#8f2f25] bg-[#9f3b30] px-4 text-white hover:bg-[#8d3027]"
                >
                  {t("fortune.redraw")}
                </button>
                <button
                  type="button"
                  onClick={handleClose}
                  className="md-button md-button-outlined cf-press inline-flex items-center rounded-md border border-[#cfd5dd] bg-white px-4 text-[#344054] hover:bg-[#eef1f4]"
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
    <div className="cf-fall flex min-h-0 w-full flex-col" aria-live="polite">
      <div className="px-6 pb-5 pt-5">
        {/* 等级标签 + 签号 */}
        <div className="cf-rise flex items-center gap-2" style={{ "--i": 0 } as CSSProperties}>
          <span
            className="fortune-level-chip md-type-label-small rounded-[3px] border px-2 py-0.5 font-semibold"
            style={{ background: meta.chipBg, borderColor: `${meta.accent}55`, color: meta.chipText }}
          >
            {levelLabel}
          </span>
          <span className="md-type-label-small text-[#8a929d]">{entry.id}</span>
        </div>

        {/* 签题 */}
        <h2
          className="fortune-title md-type-title-large cf-rise mt-3 font-semibold"
          style={{ "--i": 1, color: meta.accent } as CSSProperties}
        >
          {entry.title}
        </h2>

        {/* 签诗 */}
        <div
          className="fortune-verse md-type-body-large cf-rise mt-3 border-l-2 pl-4 leading-7 text-[#3f454d]"
          style={{ "--i": 2, borderColor: `${meta.accent}88` } as CSSProperties}
        >
          {verses.map((line, i) => (
            <div key={i}>{line}</div>
          ))}
        </div>
      </div>

      {/* 解签 */}
      <div
        className="md-type-body-medium cf-rise border-y border-[#e1e5ea] bg-[#f7f8fa] px-6 py-4 text-[#59616d]"
        style={{ "--i": 3 } as CSSProperties}
      >
        <div className="md-type-label-small mb-1 font-semibold text-[#737b86]">{t("fortune.interpretation")}</div>
        {entry.interpretation}
      </div>

      {/* 祝福 */}
      <div
        className="fortune-blessing md-type-body-medium cf-rise px-6 py-4 font-medium"
        style={{ "--i": 4, color: meta.accent } as CSSProperties}
      >
        {entry.blessing}
      </div>

      {/* 明日再来提示 + 关闭 */}
      <footer
        className="cf-rise flex items-center justify-between gap-4 border-t border-[#dde1e7] bg-[#f8f9fb] px-4 py-3"
        style={{ "--i": 5 } as CSSProperties}
      >
        <p className="md-type-body-small min-w-0 text-[#8a929d]">{t("fortune.tomorrow")}</p>
        <button
          type="button"
          data-autofocus
          onClick={onClose}
          className="fortune-primary-action md-button md-button-filled cf-press inline-flex shrink-0 items-center justify-center rounded-md border px-4 text-white transition"
          style={{ background: meta.accent, borderColor: meta.accent }}
        >
          {t("fortune.close")}
        </button>
      </footer>
    </div>
  );
}
