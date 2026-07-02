import { useState, type KeyboardEvent } from "react";
import { CloseIcon } from "../Icons";
import { useI18n } from "../../lib/i18n";
import { chipCls } from "../../lib/ui";

// 标签列表编辑器：渲染字符串数组为可删除 chip，并提供回车/逗号追加的输入框。
// 用于 personality / habits / tone / tags 等字符串数组字段。
export function TagList({
  values,
  onChange,
  placeholder,
  addLabel,
}: {
  values: string[];
  onChange: (next: string[]) => void;
  placeholder?: string;
  addLabel?: string;
}) {
  const { t } = useI18n();
  const [draft, setDraft] = useState("");

  function commit() {
    const parts = draft
      .split(/[,\n]/)
      .map((s) => s.trim())
      .filter((s) => s.length > 0);
    if (parts.length === 0) {
      setDraft("");
      return;
    }
    const seen = new Set(values.map((v) => v.toLowerCase()));
    const merged = [...values];
    for (const p of parts) {
      if (!seen.has(p.toLowerCase())) {
        merged.push(p);
        seen.add(p.toLowerCase());
      }
    }
    setDraft("");
    onChange(merged);
  }

  function removeAt(index: number) {
    const next = values.filter((_, idx) => idx !== index);
    onChange(next);
  }

  function onKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    if (event.key === "Enter" || event.key === ",") {
      event.preventDefault();
      commit();
    } else if (event.key === "Backspace" && draft === "" && values.length > 0) {
      event.preventDefault();
      removeAt(values.length - 1);
    }
  }

  return (
    <div className="flex flex-wrap items-center gap-1.5 rounded-md border border-[#d9d9d9] bg-white px-2 py-1.5">
      {values.map((value, index) => (
        <span key={`${value}-${index}`} className={chipCls}>
          <span>{value}</span>
          <button
            type="button"
            className="ml-0.5 inline-flex size-3.5 items-center justify-center rounded-sm text-[#6366f1] hover:bg-[#e0e7ff]"
            aria-label={addLabel ?? t("common.remove")}
            onClick={() => removeAt(index)}
          >
            <CloseIcon size={10} />
          </button>
        </span>
      ))}
      <input
        className="min-w-[120px] flex-1 border-0 bg-transparent px-1 py-0.5 text-[13px] text-[#202124] outline-none"
        value={draft}
        onChange={(event) => setDraft(event.target.value)}
        onKeyDown={onKeyDown}
        onBlur={commit}
        placeholder={placeholder}
      />
    </div>
  );
}
