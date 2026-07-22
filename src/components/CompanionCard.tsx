import { useEffect, useRef, useState } from "react";
import * as api from "../lib/api";
import { applyDoNotDisturb } from "../lib/companionGovernance";
import { useI18n } from "../lib/i18n";
import type { CompanionPanelState, Settings } from "../lib/types";
import { CloudSunIcon, RotateCwIcon, SettingsIcon } from "./Icons";

type Props = {
  settings: Settings | null;
  onOpenSettings: () => void;
};

function formatTemp(value: number) {
  if (!Number.isFinite(value)) return "-";
  return `${Math.round(value)}°C`;
}

function label(value: string) {
  return value.replaceAll("_", " ");
}

function formatCache(value: number | null | undefined, t: (key: string, vars?: Record<string, string | number>) => string) {
  if (!value) return "";
  const diff = value - Date.now();
  if (diff <= 0) return t("companion.cacheExpired");
  const minutes = Math.max(1, Math.round(diff / 60000));
  return t("companion.cacheExpires", { minutes });
}

export default function CompanionCard({ settings, onOpenSettings }: Props) {
  const { t } = useI18n();
  const [state, setState] = useState<CompanionPanelState | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [localNow, setLocalNow] = useState(() => new Date());
  const requestGenerationRef = useRef(0);

  async function loadPanel(invalidateWeather: boolean) {
    if (!settings?.companion_enabled) return;
    const generation = requestGenerationRef.current + 1;
    requestGenerationRef.current = generation;
    setLoading(true);
    setError("");
    try {
      if (invalidateWeather) {
        await api.companionClearWeatherCache();
        if (generation !== requestGenerationRef.current) return;
      }
      const next = await api.companionPanelState();
      if (generation === requestGenerationRef.current) setState(next);
    } catch (err) {
      if (generation === requestGenerationRef.current) setError(String(err));
    } finally {
      if (generation === requestGenerationRef.current) setLoading(false);
    }
  }

  const refresh = () => loadPanel(false);
  const refreshWeather = () => loadPanel(true);

  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => void refresh(), 30 * 60 * 1000);
    return () => {
      requestGenerationRef.current += 1;
      window.clearInterval(timer);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    settings?.companion_enabled,
    settings?.companion_tone,
    settings?.companion_mood,
    settings?.companion_energy,
    settings?.companion_focus,
    settings?.weather_enabled,
    settings?.weather_city,
    settings?.weather_location_mode,
    settings?.weather_provider,
  ]);

  useEffect(() => {
    const timer = window.setInterval(() => setLocalNow(new Date()), 60 * 1000);
    return () => window.clearInterval(timer);
  }, []);

  if (!settings?.companion_enabled) return null;

  const weather = state?.weather;
  const governed = applyDoNotDisturb(
    state?.suggestions ?? [],
    settings.companion_do_not_disturb,
    localNow,
  );
  const suggestions = governed.suggestions;
  const weatherSummary = weather
    ? [
        weather.city,
        weather.condition,
        formatTemp(weather.temperature_c),
        t("companion.feelsLike", { temp: formatTemp(weather.apparent_temperature_c) }),
        weather.uv_index ? `UV ${Math.round(weather.uv_index)}` : "",
        weather.air_quality_index ? `AQI ${weather.air_quality_index}` : "",
      ]
        .filter(Boolean)
        .join(" · ")
    : settings.weather_enabled
      ? settings.weather_city || settings.weather_location_mode === "auto"
        ? t("companion.weatherLoading")
        : t("companion.weatherNeedsCity")
      : t("companion.weatherOff");

  return (
    <section className="bg-white">
      <div className="flex flex-wrap items-start gap-2">
        <div className="flex min-w-0 items-center gap-2">
          <span className="grid size-7 shrink-0 place-items-center rounded-md border border-[#e2e5ea] bg-white text-[#49515c]">
            <CloudSunIcon size={15} />
          </span>
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-1.5 text-[12px] font-medium text-[#202124]">
              <span>{t("companion.title")}</span>
              <span className="rounded border border-[#e2e5ea] bg-white px-1.5 py-0.5 text-[11px] text-[#68707c]">
                {label(settings.companion_tone)}
              </span>
              <span className="rounded border border-[#e2e5ea] bg-white px-1.5 py-0.5 text-[11px] text-[#68707c]">
                {label(settings.companion_focus)}
              </span>
            </div>
            <div className="mt-0.5 truncate text-[11px] text-[#7a8088]" title={weatherSummary}>
              {weatherSummary}
            </div>
          </div>
        </div>

        <div className="ml-auto flex shrink-0 items-center gap-1">
          <button
            type="button"
            onClick={() => void refreshWeather()}
            disabled={loading}
            className="grid size-7 place-items-center rounded-md text-[#59616d] hover:bg-[#eef1f5] disabled:opacity-50"
            aria-label={t("companion.refreshWeather")}
            title={t("companion.refreshWeather")}
          >
            <RotateCwIcon size={14} className={loading ? "animate-spin" : ""} />
          </button>
          <button
            type="button"
            onClick={onOpenSettings}
            className="grid size-7 place-items-center rounded-md text-[#59616d] hover:bg-[#eef1f5]"
            aria-label={t("sidebar.settings")}
            title={t("sidebar.settings")}
          >
            <SettingsIcon size={14} />
          </button>
        </div>
      </div>

      {(suggestions.length > 0 || error || state?.weather_error || state?.privacy.note) && (
        <div className="mt-2 grid gap-2 lg:grid-cols-[minmax(0,1fr)_220px]">
          <div className="flex min-w-0 flex-wrap gap-1.5">
            {suggestions.map((item) => (
              <span
                key={`${item.kind}-${item.text}`}
                className="max-w-full truncate rounded-md border border-[#e2e5ea] bg-white px-2 py-1 text-[11px] text-[#59616d]"
                title={item.text}
              >
                {item.text}
              </span>
            ))}
            {error && (
              <span className="rounded-md border border-[#fde2e2] bg-[#fff7f7] px-2 py-1 text-[11px] text-[#b42318]">
                {error}
              </span>
            )}
            {state?.weather_error && (
              <span className="rounded-md border border-[#fde2e2] bg-[#fff7f7] px-2 py-1 text-[11px] text-[#b42318]">
                {state.weather_error}
              </span>
            )}
            {state?.weather_cache.active_cached && (
              <span className="rounded-md border border-[#e2e5ea] bg-white px-2 py-1 text-[11px] text-[#59616d]">
                {formatCache(state.weather_cache.expires_at, t)}
              </span>
            )}
          </div>
          <div className="truncate text-[11px] text-[#8a9099]" title={state?.privacy.note}>
            {state?.privacy.provider ? `${state.privacy.provider} · ` : ""}
            {state?.privacy.note}
          </div>
        </div>
      )}
    </section>
  );
}
