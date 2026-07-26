import { useEffect, useMemo, useState } from "react";
import * as api from "../lib/api";
import { findEntry, getTodayRecord } from "../lib/fortune";
import { useI18n } from "../lib/i18n";
import type { GoalPanelState, SessionList, Settings } from "../lib/types";
import CompanionCard from "./CompanionCard";
import FortuneDialog from "./FortuneDialog";
import { PanelLeftIcon, SparklesIcon } from "./Icons";
import PomodoroCard from "./PomodoroCard";

const IS_TAURI = "__TAURI_INTERNALS__" in window;

const PREVIEW_SETTINGS = {
  language: "zh",
  theme: "light",
  appearance: "material_bloom",
  companion_enabled: true,
  companion_tone: "gentle",
  companion_mood: "neutral",
  companion_energy: "steady",
  companion_focus: "available",
  companion_do_not_disturb: "off",
  weather_enabled: false,
  weather_location_mode: "off",
  weather_city: "",
  weather_provider: "open_meteo",
} as Settings;

export default function WidgetsWindowShell() {
  const { setLang, t } = useI18n();
  const [settings, setSettings] = useState<Settings | null>(IS_TAURI ? null : PREVIEW_SETTINGS);
  const [sessions, setSessions] = useState<SessionList | null>(IS_TAURI ? null : { active: "", sessions: [] });
  const [goal, setGoal] = useState<GoalPanelState | null>(null);
  const [fortuneOpen, setFortuneOpen] = useState(false);
  const [fortuneRevision, setFortuneRevision] = useState(0);
  const [status, setStatus] = useState("");

  useEffect(() => {
    document.documentElement.dataset.window = "widgets";
    return () => {
      delete document.documentElement.dataset.window;
    };
  }, []);

  useEffect(() => {
    const theme = settings?.theme ?? "system";
    const media = window.matchMedia?.("(prefers-color-scheme: dark)") ?? null;

    function applyTheme() {
      const resolved = theme === "system" ? (media?.matches ? "dark" : "light") : theme;
      document.documentElement.dataset.theme = theme;
      document.documentElement.dataset.resolvedTheme = resolved;
      document.documentElement.style.colorScheme = resolved;
    }

    applyTheme();
    if (theme !== "system" || !media) return;
    media.addEventListener("change", applyTheme);
    return () => media.removeEventListener("change", applyTheme);
  }, [settings?.theme]);

  useEffect(() => {
    document.documentElement.dataset.appearance = settings?.appearance ?? "material_bloom";
  }, [settings?.appearance]);

  useEffect(() => {
    if (!IS_TAURI) {
      setLang(PREVIEW_SETTINGS.language);
      return;
    }

    let cancelled = false;
    let unlisten: (() => void) | undefined;

    async function refresh() {
      try {
        const [nextSettings, nextSessions, nextGoal] = await Promise.all([
          api.getSettings(),
          api.listSessions(),
          api.goalPanelState(),
        ]);
        if (cancelled) return;
        setSettings(nextSettings);
        setSessions(nextSessions);
        setGoal(nextGoal);
        setLang(nextSettings.language);
        setStatus("");
      } catch (error) {
        if (!cancelled) setStatus(String(error));
      }
    }

    void refresh();
    // 窗口常驻：关闭只是隐藏，webview 一直存活。隐藏期间不轮询，重新显示时
    // 立刻刷新一次，避免展示上一次打开时的陈旧数据。
    const timer = window.setInterval(() => {
      if (document.hidden) return;
      void refresh();
    }, 60_000);
    const onVisible = () => {
      if (!document.hidden) void refresh();
    };
    document.addEventListener("visibilitychange", onVisible);
    window.addEventListener("focus", onVisible);
    void api.listenSettingsUpdated((nextSettings) => {
      setSettings(nextSettings);
      setLang(nextSettings.language);
    }).then((nextUnlisten) => {
      if (cancelled) nextUnlisten();
      else unlisten = nextUnlisten;
    });

    return () => {
      cancelled = true;
      window.clearInterval(timer);
      document.removeEventListener("visibilitychange", onVisible);
      window.removeEventListener("focus", onVisible);
      unlisten?.();
    };
  }, [setLang]);

  const activeSession = useMemo(
    () => sessions?.sessions.find((session) => session.id === sessions.active),
    [sessions],
  );
  const todayEntry = useMemo(() => {
    const record = getTodayRecord();
    return record ? findEntry(record.entryId) ?? null : null;
  }, [fortuneRevision]);

  async function showMainWindow() {
    try {
      await api.desktopCompanionShowMain();
    } catch (error) {
      if (IS_TAURI) setStatus(String(error));
    }
  }

  return (
    <main className="widgets-window-shell flex h-screen min-h-0 flex-col overflow-hidden">
      <header className="widgets-window-header flex h-16 shrink-0 items-center gap-3 border-b px-4">
        <span className="widgets-window-app-icon grid size-10 shrink-0 place-items-center rounded-lg">
          <img src="/fortune-icon.png" alt="" className="size-8 object-contain" />
        </span>
        <div className="min-w-0 flex-1">
          <h1 className="md-type-title-medium truncate font-semibold">{t("header.toys")}</h1>
          <p className="md-type-label-small truncate">Demiurge</p>
        </div>
        <button
          type="button"
          className="md-icon-button"
          onClick={() => void showMainWindow()}
          aria-label={t("desktopCompanion.openMain")}
          title={t("desktopCompanion.openMain")}
        >
          <PanelLeftIcon size={18} />
        </button>
      </header>

      <div className="widgets-window-content min-h-0 flex-1 overflow-y-auto p-4">
        {status && <div className="widgets-window-error mb-3 rounded-lg px-3 py-2 text-[12px]">{status}</div>}

        <section className="widgets-fortune-surface mb-3 flex min-h-[148px] items-center gap-4 rounded-lg p-4">
          <div className="widgets-fortune-art grid size-24 shrink-0 place-items-center rounded-lg">
            <img src="/fortune-icon.png" alt="" className="size-20 object-contain" />
          </div>
          <div className="min-w-0 flex-1">
            <h2 className="md-type-title-medium font-semibold">{t("fortune.cardTitle")}</h2>
            <p className="md-type-body-small mt-1 line-clamp-2">
              {todayEntry?.title ?? t("fortune.cardDesc")}
            </p>
            <button
              type="button"
              className="md-button md-button-filled mt-4"
              onClick={() => setFortuneOpen(true)}
            >
              <SparklesIcon size={16} />
              {todayEntry ? t("fortune.cardView") : t("fortune.cardDraw")}
            </button>
          </div>
        </section>

        <section className="widgets-tool-surface mb-3 rounded-lg p-4">
          <CompanionCard settings={settings} onOpenSettings={() => void showMainWindow()} />
        </section>

        <section className="widgets-tool-surface rounded-lg p-4">
          <PomodoroCard
            activeSessionId={sessions?.active ?? ""}
            activeSessionTitle={activeSession?.title}
            goal={goal}
          />
        </section>
      </div>

      <FortuneDialog
        open={fortuneOpen}
        onClose={() => {
          setFortuneOpen(false);
          setFortuneRevision((revision) => revision + 1);
        }}
      />
    </main>
  );
}
