import { useEffect, useState } from "react";
import * as api from "../lib/api";
import type { CompanionPanelState, Settings } from "../lib/types";
import { RotateCwIcon, SparklesIcon } from "./Icons";

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

export default function CompanionCard({ settings, onOpenSettings }: Props) {
  const [state, setState] = useState<CompanionPanelState | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  async function refresh() {
    if (!settings?.companion_enabled) return;
    setLoading(true);
    setError("");
    try {
      setState(await api.companionPanelState());
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }

  async function refreshWeatherCache() {
    if (!settings?.companion_enabled) return;
    setLoading(true);
    setError("");
    try {
      setState(await api.companionClearWeatherCache());
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => void refresh(), 30 * 60 * 1000);
    return () => window.clearInterval(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [
    settings?.companion_enabled,
    settings?.companion_tone,
    settings?.companion_mood,
    settings?.companion_energy,
    settings?.companion_focus,
    settings?.weather_enabled,
    settings?.weather_city,
  ]);

  if (!settings?.companion_enabled) return null;

  const weather = state?.weather;
  const suggestions = state?.suggestions ?? [];

  return (
    <div className="border-b border-[#eceff3] bg-[#fbfcfd] px-3 py-2">
      <div className="flex flex-wrap items-center gap-2">
        <div className="flex min-w-0 items-center gap-2">
          <span className="grid size-7 shrink-0 place-items-center rounded-md border border-[#e2e5ea] bg-white text-[#49515c]">
            <SparklesIcon size={15} />
          </span>
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-1.5 text-[12px] font-medium text-[#202124]">
              <span>Companion</span>
              <span className="rounded border border-[#e2e5ea] bg-white px-1.5 py-0.5 text-[11px] text-[#68707c]">
                {label(settings.companion_tone)}
              </span>
              <span className="rounded border border-[#e2e5ea] bg-white px-1.5 py-0.5 text-[11px] text-[#68707c]">
                {label(settings.companion_focus)}
              </span>
            </div>
            <div className="mt-0.5 truncate text-[11px] text-[#7a8088]">
              {weather
                ? `${weather.city} · ${weather.condition} · ${formatTemp(weather.temperature_c)} · 体感 ${formatTemp(
                    weather.apparent_temperature_c,
                  )}`
                : settings.weather_enabled
                  ? settings.weather_city
                    ? "天气加载中，失败时会安静降级。"
                    : "填写城市后启用天气陪伴。"
                  : "天气陪伴关闭，仅使用本地陪伴状态。"}
            </div>
          </div>
        </div>

        <div className="ml-auto flex shrink-0 items-center gap-1">
          <button
            type="button"
            onClick={() => void refreshWeatherCache()}
            disabled={loading}
            className="grid size-7 place-items-center rounded-md text-[#59616d] hover:bg-[#eef1f5] disabled:opacity-50"
            aria-label="Refresh weather"
            title="Refresh weather"
          >
            <RotateCwIcon size={14} className={loading ? "animate-spin" : ""} />
          </button>
          <button
            type="button"
            onClick={onOpenSettings}
            className="h-7 rounded-md px-2 text-[12px] text-[#59616d] hover:bg-[#eef1f5]"
          >
            设置
          </button>
        </div>
      </div>

      {(suggestions.length > 0 || error || state?.privacy.note) && (
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
          </div>
          <div className="truncate text-[11px] text-[#8a9099]" title={state?.privacy.note}>
            {state?.privacy.note}
          </div>
        </div>
      )}
    </div>
  );
}
