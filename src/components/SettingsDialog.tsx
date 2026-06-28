import { useEffect, useState } from "react";
import * as api from "../lib/api";
import type { OcrDownloadProgress, OcrModelSource, OcrModelStatus, PackManifest, ProviderKind, Settings } from "../lib/types";
import { CloseIcon } from "./Icons";

interface Props {
  open: boolean;
  settings: Settings;
  packs: PackManifest[];
  onClose: () => void;
  onSave: (s: Settings) => void;
}

const inputCls =
  "w-full rounded-xl border border-[#e5e5e5] bg-white px-3 py-2.5 text-[#171717] outline-none transition focus:border-[#10a37f]";
const labelCls = "mb-1.5 block text-sm font-medium text-[#3f3f3f]";

const providerOptions: { value: ProviderKind; label: string; baseUrl: string; apiKeyHelp: string }[] = [
  {
    value: "open_ai_compatible",
    label: "OpenAI-compatible / DeepSeek",
    baseUrl: "例如：https://api.deepseek.com/v1",
    apiKeyHelp: "保存到系统凭据管理器；settings.json 不落明文密钥。",
  },
  {
    value: "local",
    label: "Local OpenAI-compatible",
    baseUrl: "例如：http://localhost:11434/v1 或 LM Studio endpoint",
    apiKeyHelp: "本地服务通常可留空；如服务要求 token，会保存到系统凭据管理器。",
  },
  {
    value: "anthropic",
    label: "Anthropic",
    baseUrl: "例如：https://api.anthropic.com/v1",
    apiKeyHelp: "用于 Anthropic x-api-key header，保存到系统凭据管理器。",
  },
  {
    value: "gemini",
    label: "Gemini",
    baseUrl: "例如：https://generativelanguage.googleapis.com/v1beta",
    apiKeyHelp: "用于 Google AI Studio API key，保存到系统凭据管理器。",
  },
];

const ocrSources: { value: OcrModelSource; label: string }[] = [
  { value: "modelscope", label: "ModelScope 国内源" },
  { value: "huggingface", label: "Hugging Face 国际源" },
];

function formatBytes(n: number) {
  if (!Number.isFinite(n) || n <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  let value = n;
  let idx = 0;
  while (value >= 1024 && idx < units.length - 1) {
    value /= 1024;
    idx += 1;
  }
  return `${value.toFixed(idx === 0 ? 0 : 1)} ${units[idx]}`;
}

export default function SettingsDialog({ open, settings, packs, onClose, onSave }: Props) {
  const [form, setForm] = useState<Settings>(settings);
  const [ocrStatus, setOcrStatus] = useState<OcrModelStatus | null>(null);
  const [ocrProgress, setOcrProgress] = useState<OcrDownloadProgress | null>(null);
  const [ocrBusy, setOcrBusy] = useState(false);
  const [ocrError, setOcrError] = useState("");

  useEffect(() => {
    if (open) setForm(settings);
  }, [open, settings]);

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    api
      .ocrModelStatus()
      .then((status) => {
        if (!cancelled) setOcrStatus(status);
      })
      .catch((err) => {
        if (!cancelled) setOcrError(String(err));
      });
    return () => {
      cancelled = true;
    };
  }, [open]);

  useEffect(() => {
    if (!open) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    api.listenOcrDownloadProgress((event) => {
      if (!disposed) setOcrProgress(event);
    }).then((fn) => {
      if (disposed) fn();
      else unlisten = fn;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [open]);

  if (!open) return null;

  const set = <K extends keyof Settings>(k: K, v: Settings[K]) => setForm((f) => ({ ...f, [k]: v }));
  const selectedProvider = providerOptions.find((p) => p.value === form.provider) ?? providerOptions[0];
  const downloadOcrModels = async () => {
    setOcrBusy(true);
    setOcrError("");
    setOcrProgress(null);
    try {
      const status = await api.ocrDownloadModels(form.ocr_model_source);
      setOcrStatus(status);
    } catch (err) {
      setOcrError(String(err));
    } finally {
      setOcrBusy(false);
    }
  };
  const progressPct =
    ocrProgress?.totalBytes && ocrProgress.totalBytes > 0
      ? Math.min(100, Math.round((ocrProgress.downloadedBytes / ocrProgress.totalBytes) * 100))
      : null;

  return (
    <div
      className="fixed inset-0 z-40 flex items-center justify-center bg-black/30 p-4 backdrop-blur-[2px]"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="max-h-[92vh] w-full max-w-lg overflow-y-auto rounded-3xl border border-[#ececec] bg-white p-6 shadow-[0_24px_60px_rgba(0,0,0,0.18)]">
        <div className="mb-5 flex items-center">
          <h2 className="text-lg font-semibold text-[#171717]">设置</h2>
          <button
            className="ml-auto grid h-8 w-8 place-items-center rounded-lg text-[#8a8a8a] transition hover:bg-[#f5f5f5] hover:text-[#3f3f3f]"
            onClick={onClose}
            aria-label="关闭"
          >
            <CloseIcon size={18} />
          </button>
        </div>

        <div className="space-y-4">
          <label className="block">
            <span className={labelCls}>Provider</span>
            <select className={inputCls} value={form.provider} onChange={(e) => set("provider", e.target.value as ProviderKind)}>
              {providerOptions.map((p) => (
                <option key={p.value} value={p.value}>
                  {p.label}
                </option>
              ))}
            </select>
          </label>

          <label className="block">
            <span className={labelCls}>LLM 接口地址 (base_url)</span>
            <input
              className={inputCls}
              value={form.base_url}
              placeholder="https://api.deepseek.com/v1"
              onChange={(e) => set("base_url", e.target.value)}
            />
            <span className="mt-1.5 block text-xs text-[#9a9a9a]">{selectedProvider.baseUrl}</span>
          </label>

          <label className="block">
            <span className={labelCls}>API Key</span>
            <input
              className={inputCls}
              type="password"
              value={form.api_key}
              placeholder="sk-..."
              onChange={(e) => set("api_key", e.target.value)}
            />
            <span className="mt-1.5 block text-xs text-[#9a9a9a]">{selectedProvider.apiKeyHelp}</span>
          </label>

          <label className="block">
            <span className={labelCls}>模型 (model)</span>
            <input
              className={inputCls}
              value={form.model}
              placeholder="deepseek-chat"
              onChange={(e) => set("model", e.target.value)}
            />
          </label>

          <label className="block">
            <span className={labelCls}>角色包</span>
            <select className={inputCls} value={form.current_pack} onChange={(e) => set("current_pack", e.target.value)}>
              {packs.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name} ({p.id})
                </option>
              ))}
            </select>
          </label>

          <label className="block">
            <span className={labelCls}>最大输入 Token 预算</span>
            <input
              className={inputCls}
              type="number"
              min={4000}
              step={1000}
              value={form.max_input_tokens}
              onChange={(e) => set("max_input_tokens", Number(e.target.value) || 0)}
            />
            <span className="mt-1.5 block text-xs text-[#9a9a9a]">用于估算 system prompt、工具 schema 与历史消息的总输入上限。</span>
          </label>

          <label className="block">
            <span className={labelCls}>保留输出 Token</span>
            <input
              className={inputCls}
              type="number"
              min={512}
              step={256}
              value={form.reserved_output_tokens}
              onChange={(e) => set("reserved_output_tokens", Number(e.target.value) || 0)}
            />
          </label>

          <label className="flex items-start gap-3 rounded-2xl border border-[#eeeeee] bg-[#fafafa] p-3">
            <input
              className="mt-1 h-4 w-4 accent-[#10a37f]"
              type="checkbox"
              checked={form.auto_memory_enabled}
              onChange={(e) => set("auto_memory_enabled", e.target.checked)}
            />
            <span>
              <span className="block text-sm font-medium text-[#3f3f3f]">自动提取长期记忆</span>
              <span className="mt-1 block text-xs text-[#9a9a9a]">保守提取用户偏好和项目长期约束，写入沙盒 .demiurge/memory.md。</span>
            </span>
          </label>

          <div className="rounded-2xl border border-[#eeeeee] bg-[#fafafa] p-3">
            <label className="flex items-start gap-3">
              <input
                className="mt-1 h-4 w-4 accent-[#10a37f]"
                type="checkbox"
                checked={form.voice_enabled}
                onChange={(e) => set("voice_enabled", e.target.checked)}
              />
              <span>
                <span className="block text-sm font-medium text-[#3f3f3f]">Voice API 预留</span>
                <span className="mt-1 block text-xs text-[#9a9a9a]">先保存 STT/TTS 后端选择；录音、转写和合成实现后续接入。</span>
              </span>
            </label>
            <div className="mt-3 grid grid-cols-1 gap-3 sm:grid-cols-2">
              <label className="block">
                <span className={labelCls}>STT 后端</span>
                <input
                  className={inputCls}
                  value={form.voice_stt_backend}
                  placeholder="none / whisper / doubao"
                  onChange={(e) => set("voice_stt_backend", e.target.value)}
                />
              </label>
              <label className="block">
                <span className={labelCls}>TTS 后端</span>
                <input
                  className={inputCls}
                  value={form.voice_tts_backend}
                  placeholder="none / GPT-SoVITS / CosyVoice"
                  onChange={(e) => set("voice_tts_backend", e.target.value)}
                />
              </label>
            </div>
            <label className="mt-3 block">
              <span className={labelCls}>Voice ID / 角色音色</span>
              <input
                className={inputCls}
                value={form.voice_id}
                placeholder="例如 default、角色名或服务端 voice id"
                onChange={(e) => set("voice_id", e.target.value)}
              />
            </label>
          </div>

          <div className="rounded-2xl border border-[#eeeeee] bg-[#fafafa] p-3">
            <label className="flex items-start gap-3">
              <input
                className="mt-1 h-4 w-4 accent-[#10a37f]"
                type="checkbox"
                checked={form.computer_use_enabled}
                onChange={(e) => set("computer_use_enabled", e.target.checked)}
              />
              <span>
                <span className="block text-sm font-medium text-[#3f3f3f]">Computer Use / OCR</span>
                <span className="mt-1 block text-xs text-[#9a9a9a]">
                  启用后，Agent 可在确认后读取窗口标题、截图并用本地 OCR 识别屏幕文本。
                </span>
              </span>
            </label>
            <div className="mt-3 grid grid-cols-1 gap-3 sm:grid-cols-[1fr_auto]">
              <label className="block">
                <span className={labelCls}>OCR 模型下载源</span>
                <select
                  className={inputCls}
                  value={form.ocr_model_source}
                  onChange={(e) => set("ocr_model_source", e.target.value as OcrModelSource)}
                >
                  {ocrSources.map((s) => (
                    <option key={s.value} value={s.value}>
                      {s.label}
                    </option>
                  ))}
                </select>
              </label>
              <button
                className="self-end rounded-xl bg-[#111] px-4 py-2.5 text-sm text-white transition disabled:cursor-not-allowed disabled:bg-[#b9b9b9]"
                disabled={ocrBusy}
                onClick={downloadOcrModels}
                type="button"
              >
                {ocrBusy ? "下载中" : ocrStatus?.installed ? "重新下载" : "下载模型"}
              </button>
            </div>
            <div className="mt-2 space-y-1 text-xs text-[#7a7a7a]">
              <div>状态：{ocrStatus?.installed ? "已安装" : "未安装"} {ocrStatus ? `(${formatBytes(ocrStatus.totalBytes)})` : ""}</div>
              {ocrStatus?.modelDir ? <div className="break-all">目录：{ocrStatus.modelDir}</div> : null}
              {ocrStatus && !ocrStatus.installed ? <div>缺少：{ocrStatus.missing.join(", ")}</div> : null}
              {ocrProgress ? (
                <div>
                  {ocrProgress.file}：{formatBytes(ocrProgress.downloadedBytes)}
                  {ocrProgress.totalBytes ? ` / ${formatBytes(ocrProgress.totalBytes)}` : ""}
                  {progressPct !== null ? ` (${progressPct}%)` : ""}
                </div>
              ) : null}
              {ocrError ? <div className="text-[#b42318]">{ocrError}</div> : null}
            </div>
          </div>

          <label className="block">
            <span className={labelCls}>上下文上限（字符数，兼容兜底）</span>
            <input
              className={inputCls}
              type="number"
              min={2000}
              step={1000}
              value={form.max_context_chars}
              onChange={(e) => set("max_context_chars", Number(e.target.value) || 0)}
            />
          </label>
        </div>

        <div className="mt-6 flex justify-end gap-2">
          <button
            className="rounded-full border border-[#e5e5e5] px-4 py-2 text-sm text-[#3f3f3f] transition hover:bg-[#f7f7f7]"
            onClick={onClose}
          >
            取消
          </button>
          <button
            className="rounded-full bg-[#111] px-5 py-2 text-sm text-white transition hover:bg-[#333]"
            onClick={() => {
              const maxInput = Math.max(4000, form.max_input_tokens || 0);
              const reserved = Math.min(Math.max(512, form.reserved_output_tokens || 0), maxInput - 512);
              onSave({
                ...form,
                max_context_chars: Math.max(2000, form.max_context_chars || 0),
                max_input_tokens: maxInput,
                reserved_output_tokens: reserved,
                voice_stt_backend: form.voice_stt_backend.trim() || "none",
                voice_tts_backend: form.voice_tts_backend.trim() || "none",
                voice_id: form.voice_id.trim(),
                ocr_model_source: form.ocr_model_source || "modelscope",
              });
            }}
          >
            保存
          </button>
        </div>
      </div>
    </div>
  );
}
