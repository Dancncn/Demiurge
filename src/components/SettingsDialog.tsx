import { useEffect, useState } from "react";
import type { PackManifest, Settings } from "../lib/types";
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

export default function SettingsDialog({ open, settings, packs, onClose, onSave }: Props) {
  const [form, setForm] = useState<Settings>(settings);

  useEffect(() => {
    if (open) setForm(settings);
  }, [open, settings]);

  if (!open) return null;

  const set = <K extends keyof Settings>(k: K, v: Settings[K]) => setForm((f) => ({ ...f, [k]: v }));

  return (
    <div
      className="fixed inset-0 z-40 flex items-center justify-center bg-black/30 p-4 backdrop-blur-[2px]"
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div className="w-full max-w-md rounded-3xl border border-[#ececec] bg-white p-6 shadow-[0_24px_60px_rgba(0,0,0,0.18)]">
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
            <span className={labelCls}>LLM 接口地址 (base_url)</span>
            <input
              className={inputCls}
              value={form.base_url}
              placeholder="https://api.deepseek.com/v1"
              onChange={(e) => set("base_url", e.target.value)}
            />
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
            <span className="mt-1.5 block text-xs text-[#9a9a9a]">MVP 以明文存于本地配置文件，仅本机使用。</span>
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
