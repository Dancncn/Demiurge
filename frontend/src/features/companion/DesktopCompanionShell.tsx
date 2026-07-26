import { useEffect, useMemo, useState, type ReactNode } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import * as api from "@/lib/api";
import type { CompanionPanelState, PackManifest, Settings } from "@/lib/types";
import { CloseIcon, MaximizeIcon, MinimizeIcon, MousePointerIcon, PinIcon, SettingsIcon, SparklesIcon } from "@/shared/components/Icons";
import { useI18n } from "@/lib/i18n";

const focusTone: Record<string, string> = {
  available: "desktopCompanion.focus.available",
  focusing: "desktopCompanion.focus.focusing",
  resting: "desktopCompanion.focus.resting",
};

const moodTone: Record<string, string> = {
  neutral: "desktopCompanion.mood.neutral",
  good: "desktopCompanion.mood.good",
  stressed: "desktopCompanion.mood.stressed",
  down: "desktopCompanion.mood.down",
};

function ShellButton({
  active,
  title,
  disabled,
  onClick,
  children,
}: {
  active?: boolean;
  title: string;
  disabled?: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      disabled={disabled}
      title={title}
      aria-label={title}
      onClick={(event) => {
        event.stopPropagation();
        onClick();
      }}
      className={`desktop-companion-control grid size-7 place-items-center rounded-md border text-[12px] transition ${
        active
          ? "border-[#b8d4ff] bg-[#eef5ff] text-[#0b57d0]"
          : "border-[#dfe3e8] bg-white/88 text-[#59616d] hover:bg-white hover:text-[#202124]"
      } disabled:cursor-not-allowed disabled:opacity-45`}
    >
      {children}
    </button>
  );
}

export default function DesktopCompanionShell() {
  const { t } = useI18n();
  const [settings, setSettings] = useState<Settings | null>(null);
  const [packs, setPacks] = useState<PackManifest[]>([]);
  const [panel, setPanel] = useState<CompanionPanelState | null>(null);
  const [status, setStatus] = useState("");

  useEffect(() => {
    document.documentElement.dataset.window = "desktop-companion";
    return () => {
      delete document.documentElement.dataset.window;
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    const refresh = async () => {
      try {
        const [nextSettings, nextPacks, nextPanel] = await Promise.all([
          api.getSettings(),
          api.listPacks(),
          api.companionPanelState(),
        ]);
        if (cancelled) return;
        setSettings(nextSettings);
        setPacks(nextPacks);
        setPanel(nextPanel);
      } catch (err) {
        if (!cancelled) setStatus(String(err));
      }
    };
    void refresh();
    const timer = window.setInterval(() => void refresh(), 60_000);
    let unlisten: (() => void) | undefined;
    api.listenSettingsUpdated((next) => {
      setSettings(next);
      void api.companionPanelState().then(setPanel, (err) => setStatus(String(err)));
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      cancelled = true;
      window.clearInterval(timer);
      unlisten?.();
    };
  }, []);

  const activePack = useMemo(
    () => packs.find((pack) => pack.id === settings?.current_pack),
    [packs, settings?.current_pack],
  );
  const avatar = activePack?.avatarDataUrl || "/demiurge.png";
  const title = activePack?.name || "Demiurge";
  const focus = settings?.companion_focus || "available";
  const mood = settings?.companion_mood || "neutral";
  const focusLabel = t(focusTone[focus] || "desktopCompanion.focus.available");
  const moodLabel = t(moodTone[mood] || "desktopCompanion.mood.neutral");
  const weatherLine = panel?.weather
    ? t("desktopCompanion.weather", {
        city: panel.weather.city,
        temp: Math.round(panel.weather.temperature_c),
      })
    : settings?.weather_enabled
      ? t("desktopCompanion.weatherPending")
      : t("desktopCompanion.localOnly");
  const suggestion = panel?.suggestions?.[0]?.text || t("desktopCompanion.ready");
  const collapsed = settings?.desktop_companion_collapsed ?? false;
  const boundaryLine = [
    settings?.computer_use_enabled ? t("desktopCompanion.screenToolsOn") : t("desktopCompanion.screenToolsOff"),
    settings?.voice_enabled ? t("desktopCompanion.micManual") : t("desktopCompanion.micOff"),
    settings?.weather_enabled && settings.weather_location_mode !== "off"
      ? t("desktopCompanion.locationCity")
      : t("desktopCompanion.locationOff"),
    settings?.desktop_companion_click_through
      ? t("desktopCompanion.clickThroughOn")
      : t("desktopCompanion.clickThroughOff"),
  ].join(" / ");

  async function updateSettings(patch: Partial<Settings>) {
    const base = settings ?? (await api.getSettings());
    const next = { ...base, ...patch };
    setSettings(next);
    setStatus("");
    try {
      await api.saveSettings(next);
    } catch (err) {
      setStatus(String(err));
    }
  }

  function startDragging(event: React.PointerEvent<HTMLElement>) {
    if (event.button !== 0 || (event.target as HTMLElement).closest(".desktop-companion-control")) return;
    void getCurrentWindow().startDragging();
  }

  if (!settings) {
    return (
      <main className="grid h-screen w-screen place-items-center bg-transparent p-2">
        <div className="h-10 w-36 rounded-lg border border-[#dfe3e8] bg-white/92 shadow-[0_8px_24px_rgba(15,23,42,0.12)]" />
      </main>
    );
  }

  if (collapsed) {
    return (
      <main className="desktop-companion-shell h-screen w-screen bg-transparent p-2">
        <div
          className="desktop-companion-drag flex h-full w-full items-center gap-2 rounded-lg border border-[#dfe3e8] bg-white/92 px-2.5 shadow-[0_8px_24px_rgba(15,23,42,0.12)] backdrop-blur"
          data-tauri-drag-region
          onPointerDown={startDragging}
        >
          <img src={avatar} alt="" className="size-8 shrink-0 rounded-md border border-[#dfe3e8] bg-white object-cover" />
          <div className="min-w-0 flex-1">
            <div className="truncate text-[12px] font-semibold text-[#202124]">{title}</div>
            <div className="truncate text-[11px] text-[#6f7782]">{focusLabel}</div>
          </div>
          <ShellButton
            title={t("desktopCompanion.expand")}
            onClick={() => void updateSettings({ desktop_companion_collapsed: false })}
          >
            <MaximizeIcon size={14} />
          </ShellButton>
        </div>
      </main>
    );
  }

  return (
    <main className="desktop-companion-shell h-screen w-screen bg-transparent p-2">
      <div
        className="desktop-companion-drag flex h-full w-full flex-col rounded-lg border border-[#dfe3e8] bg-white/92 shadow-[0_8px_24px_rgba(15,23,42,0.12)] backdrop-blur"
        data-tauri-drag-region
        onPointerDown={startDragging}
      >
        <header className="flex h-12 shrink-0 items-center gap-2 border-b border-[#eceff3] px-2.5">
          <img src={avatar} alt="" className="size-8 shrink-0 rounded-md border border-[#dfe3e8] bg-white object-cover" />
          <div className="min-w-0 flex-1">
            <div className="flex min-w-0 items-center gap-1.5">
              <span className="size-1.5 shrink-0 rounded-full bg-[#177245]" />
              <span className="truncate text-[12px] font-semibold text-[#202124]">{title}</span>
            </div>
            <div className="truncate text-[11px] text-[#6f7782]">
              {focusLabel} / {moodLabel}
            </div>
          </div>
          <ShellButton
            title={t("desktopCompanion.openMain")}
            onClick={() => void api.desktopCompanionShowMain()}
          >
            <SettingsIcon size={14} />
          </ShellButton>
          <ShellButton
            title={t("desktopCompanion.collapse")}
            onClick={() => void updateSettings({ desktop_companion_collapsed: true })}
          >
            <MinimizeIcon size={14} />
          </ShellButton>
        </header>

        <section className="min-h-0 flex-1 px-2.5 py-2">
          <div className="flex items-center gap-1.5 text-[11px] text-[#6f7782]">
            <SparklesIcon size={13} className="shrink-0 text-[#49515c]" />
            <span className="truncate">{weatherLine}</span>
          </div>
          <div className="mt-2 line-clamp-2 min-h-[34px] text-[12px] leading-5 text-[#202124]">{suggestion}</div>
          {status && <div className="mt-1 truncate text-[11px] text-[#b42318]">{status}</div>}
        </section>

        <footer className="flex h-10 shrink-0 items-center justify-between border-t border-[#eceff3] px-2.5">
          <div className="truncate text-[11px] text-[#7a8088]" title={boundaryLine}>
            {boundaryLine}
          </div>
          <div className="flex items-center gap-1.5">
            <ShellButton
              active={settings.desktop_companion_always_on_top}
              title={t("desktopCompanion.pin")}
              onClick={() =>
                void updateSettings({ desktop_companion_always_on_top: !settings.desktop_companion_always_on_top })
              }
            >
              <PinIcon size={14} />
            </ShellButton>
            <ShellButton
              active={settings.desktop_companion_click_through}
              title={t("desktopCompanion.clickThrough")}
              onClick={() =>
                void updateSettings({ desktop_companion_click_through: !settings.desktop_companion_click_through })
              }
            >
              <MousePointerIcon size={14} />
            </ShellButton>
            <ShellButton
              title={t("desktopCompanion.close")}
              onClick={() =>
                void updateSettings({
                  desktop_companion_enabled: false,
                  desktop_companion_click_through: false,
                })
              }
            >
              <CloseIcon size={14} />
            </ShellButton>
          </div>
        </footer>
      </div>
    </main>
  );
}
