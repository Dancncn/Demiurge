import { useMemo, useState } from "react";
import * as api from "@/lib/api";
import type { ImageGenerationResult, Settings, SpeechSynthesisResult } from "@/lib/types";
import { DownloadIcon, ImageIcon, SparklesIcon, VolumeIcon } from "@/shared/components/Icons";
import { Select } from "@/shared/components/Select";
import { useI18n } from "@/lib/i18n";

type Props = {
  settings: Settings | null;
  onOpenSettings: () => void;
};

type HistoryItem = {
  id: string;
  prompt: string;
  model: string;
  size: string;
  url: string;
  createdAt: number;
};

const sizeOptions = ["512*512", "768*768", "1024*1024", "1280*720", "720*1280"];

function usageSummary(result?: ImageGenerationResult | SpeechSynthesisResult | null) {
  if (!result?.usage) return "";
  return Object.entries(result.usage)
    .map(([key, value]) => `${key}: ${String(value)}`)
    .join(" / ");
}

export default function MediaStudio({ settings, onOpenSettings }: Props) {
  const { t } = useI18n();
  const [prompt, setPrompt] = useState("");
  const [negativePrompt, setNegativePrompt] = useState("");
  const [model, setModel] = useState(settings?.image_model || "qwen-image-2.0");
  const [size, setSize] = useState(settings?.image_size || "1024*1024");
  const [seed, setSeed] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [lastResult, setLastResult] = useState<ImageGenerationResult | null>(null);
  const [history, setHistory] = useState<HistoryItem[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [ttsBusy, setTtsBusy] = useState(false);
  const [ttsResult, setTtsResult] = useState<SpeechSynthesisResult | null>(null);

  const selected = useMemo(
    () => history.find((item) => item.id === selectedId) ?? history[0] ?? null,
    [history, selectedId],
  );

  async function generate() {
    const text = prompt.trim();
    if (!text || busy) return;
    setBusy(true);
    setError("");
    setLastResult(null);
    try {
      const result = await api.mediaGenerateImage({
        prompt: text,
        model,
        size,
        negative_prompt: negativePrompt,
        seed: seed.trim() ? Number(seed) : undefined,
        prompt_extend: false,
        watermark: false,
      });
      setLastResult(result);
      const created = result.images.map((image, index) => ({
        id: `${Date.now()}-${index}`,
        prompt: text,
        model,
        size,
        url: image.url,
        createdAt: Date.now(),
      }));
      setHistory((items) => [...created, ...items].slice(0, 48));
      setSelectedId(created[0]?.id ?? "");
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function speakPrompt() {
    const text = prompt.trim();
    if (!text || ttsBusy) return;
    setTtsBusy(true);
    setError("");
    setTtsResult(null);
    try {
      const voiceBackend = settings?.voice_tts_backend?.trim().toLowerCase() || "";
      if (["gpt-sovits", "gpt_sovits", "gptsovits"].includes(voiceBackend)) {
        const url = await api.voiceSynthesize(text, settings?.voice_id || undefined);
        setTtsResult({ request_id: "gpt-sovits", url, usage: {} });
      } else {
        setTtsResult(
          await api.mediaSynthesizeSpeech({
            text,
            model: settings?.tts_model || "qwen3-tts-flash",
            voice: settings?.tts_voice || "Cherry",
            language_type: "Chinese",
          }),
        );
      }
    } catch (err) {
      setError(String(err));
    } finally {
      setTtsBusy(false);
    }
  }

  return (
    <div className="media-studio flex min-h-0 flex-1 bg-white">
      <aside className="media-history-rail flex w-[72px] shrink-0 flex-col gap-2 overflow-y-auto border-r border-[#e5e8ed] bg-[#fbfcfd] p-2">
        <button
          type="button"
          onClick={() => {
            setSelectedId("");
            setPrompt("");
          }}
          className="media-new-action md-icon-button md-icon-button-tonal grid h-12 w-12 shrink-0 place-items-center border border-dashed border-[#cfd5dd] text-[#687180] transition hover:bg-[#eef1f5] hover:text-[#111827]"
          title={t("mediaStudio.newImage")}
        >
          <SparklesIcon size={18} />
        </button>
        {history.map((item) => (
          <button
            key={item.id}
            type="button"
            onClick={() => setSelectedId(item.id)}
            className={`relative h-12 w-12 shrink-0 overflow-hidden rounded-lg border transition ${
              item.id === selected?.id ? "border-[#111827]" : "border-[#dfe3e8] hover:border-[#aeb6c2]"
            }`}
            title={item.prompt}
          >
            <img src={item.url} alt="" className="h-full w-full object-cover" />
          </button>
        ))}
      </aside>

      <section className="media-main flex min-w-0 flex-1 flex-col bg-[#f6f7f9]">
        <div className="media-toolbar flex h-12 shrink-0 items-center border-b border-[#e5e8ed] bg-white px-4">
          <div className="md-type-title-medium flex items-center gap-2 font-semibold text-[#202124]">
            <ImageIcon size={17} />
            {t("mediaStudio.title")}
          </div>
          <div className="media-toolbar-subtitle md-type-body-medium ml-3 truncate text-[#7a8088]">
            {t("mediaStudio.subtitle")}
          </div>
          <button
            type="button"
            onClick={onOpenSettings}
            className="md-button md-button-outlined ml-auto border border-[#d9dfe7] bg-white px-3 text-[#4f5661] transition hover:bg-[#f5f6f8]"
          >
            {t("mediaStudio.settings")}
          </button>
        </div>

        <div className="media-stage relative flex min-h-0 flex-1 items-center justify-center overflow-hidden p-4">
          {selected ? (
            <>
              <img src={selected.url} alt="" className="max-h-full max-w-full rounded-md object-contain shadow-sm" />
              <div className="md-type-body-small absolute left-5 top-5 max-w-[44rem] rounded-lg border border-[#dfe3e8] bg-white/90 px-3 py-2 text-[#4f5661] shadow-sm backdrop-blur">
                <div className="font-medium text-[#202124]">{selected.model} / {selected.size}</div>
                <div className="mt-1 line-clamp-2">{selected.prompt}</div>
              </div>
              <a
                href={selected.url}
                target="_blank"
                rel="noreferrer"
                className="md-icon-button absolute right-5 top-5 grid h-9 w-9 place-items-center border border-[#dfe3e8] bg-white/90 text-[#4f5661] shadow-sm transition hover:bg-white hover:text-[#111827]"
                title={t("mediaStudio.openGeneratedImage")}
              >
                <DownloadIcon size={17} />
              </a>
            </>
          ) : (
            <div className="flex flex-col items-center text-center text-[#8a9099]">
              <div className="grid size-16 place-items-center rounded-lg border border-[#dfe3e8] bg-white text-[#687180]">
                <ImageIcon size={28} />
              </div>
              <div className="md-type-title-medium mt-4 text-[#3f4652]">{t("mediaStudio.emptyTitle")}</div>
              <div className="md-type-body-small mt-1">{t("mediaStudio.emptyDesc")}</div>
            </div>
          )}
          {busy && (
            <div className="absolute inset-0 grid place-items-center bg-white/45 backdrop-blur-[1px]">
              <div className="md-type-label-large rounded-lg border border-[#dfe3e8] bg-white px-5 py-4 text-[#3f4652] shadow-lg">
                {t("mediaStudio.generating")}
              </div>
            </div>
          )}
        </div>

        <div className="media-controls shrink-0 border-t border-[#e5e8ed] bg-white px-4 py-3">
          <div className="media-controls-inner mx-auto w-full max-w-[1120px]">
            <textarea
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              rows={3}
              placeholder={t("mediaStudio.promptPlaceholder")}
              className="md-text-field md-type-body-medium min-h-20 w-full resize-none rounded-lg border border-[#dfe3e8] bg-[#fbfcfd] px-3 py-2.5 outline-none transition focus:border-[#aeb6c2] focus:bg-white"
            />
            <div className="media-control-grid mt-2 grid gap-2">
              <input
                value={model}
                onChange={(e) => setModel(e.target.value)}
                className="md-text-field md-type-body-medium h-10 rounded-md border border-[#dfe3e8] px-2.5 outline-none"
                placeholder="qwen-image-2.0"
              />
              <Select
                value={size}
                onChange={setSize}
                triggerClassName="md-type-label-large flex h-10 items-center gap-1.5 rounded-md border border-[#e4e7ec] bg-[#fbfcfd] px-2.5 text-[#202124] outline-none transition hover:bg-white focus:shadow-[0_0_0_3px_rgba(17,24,39,0.06)]"
                options={sizeOptions.map((option) => ({ value: option, label: option }))}
              />
              <input
                value={seed}
                onChange={(e) => setSeed(e.target.value.replace(/[^\d]/g, ""))}
                className="md-text-field md-type-body-medium h-10 rounded-md border border-[#dfe3e8] px-2.5 outline-none"
                placeholder={t("mediaStudio.seed")}
              />
              <input
                value={negativePrompt}
                onChange={(e) => setNegativePrompt(e.target.value)}
                className="md-text-field md-type-body-medium h-10 rounded-md border border-[#dfe3e8] px-2.5 outline-none"
                placeholder={t("mediaStudio.negativePrompt")}
              />
              <button
                type="button"
                onClick={speakPrompt}
                disabled={ttsBusy || !prompt.trim()}
                className="md-button md-button-outlined inline-flex items-center justify-center gap-1.5 border border-[#d9dfe7] bg-white px-3 text-[#4f5661] transition hover:bg-[#f5f6f8] disabled:cursor-not-allowed disabled:opacity-50"
              >
                <VolumeIcon size={15} />
                {ttsBusy ? t("mediaStudio.speaking") : t("mediaStudio.tts")}
              </button>
              <button
                type="button"
                onClick={generate}
                disabled={busy || !prompt.trim()}
                className="md-button md-button-filled inline-flex items-center justify-center gap-1.5 bg-[#111827] px-4 text-white transition hover:bg-[#2b3442] disabled:cursor-not-allowed disabled:bg-[#b8bec8]"
              >
                <SparklesIcon size={15} />
                {t("mediaStudio.generate")}
              </button>
            </div>
            {(error || lastResult || ttsResult) && (
              <div className={`md-type-body-small mt-2 rounded-md px-3 py-2 ${error ? "bg-[#fff1f2] text-[#b42318]" : "bg-[#f6f7f9] text-[#687180]"}`}>
                {error || usageSummary(lastResult) || usageSummary(ttsResult)}
                {ttsResult?.url && (
                  <audio className="mt-2 w-full" controls src={ttsResult.url} />
                )}
              </div>
            )}
          </div>
        </div>
      </section>
    </div>
  );
}
