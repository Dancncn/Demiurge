import type { LoreEntry } from "../../lib/types";
import { useI18n } from "../../lib/i18n";
import {
  Field,
  dangerButtonCls,
  inputCls,
  secondaryButtonCls,
  subSectionCls,
  subSectionTitleCls,
} from "../../lib/ui";
import { TagList } from "./TagList";

// Lorebook 条目编辑器：path / title / tags / priority / recursive / extensions，支持增删。
export function LorebookEditor({
  value,
  onChange,
}: {
  value: LoreEntry[];
  onChange: (next: LoreEntry[]) => void;
}) {
  const { t } = useI18n();

  function update(index: number, patch: Partial<LoreEntry>) {
    onChange(value.map((e, idx) => (idx === index ? { ...e, ...patch } : e)));
  }
  function remove(index: number) {
    onChange(value.filter((_, idx) => idx !== index));
  }
  function add() {
    onChange([...value, { path: "lore/" }]);
  }

  return (
    <div className={subSectionCls}>
      <div className="mb-2 flex items-center justify-between gap-2">
        <div className={subSectionTitleCls}>{t("settings.card.lorebook.section")}</div>
        <button type="button" className={secondaryButtonCls} onClick={add}>
          {t("settings.card.lorebook.add")}
        </button>
      </div>
      {value.length === 0 ? (
        <div className="rounded-md border border-dashed border-[#d9d9d9] bg-white px-3 py-2 text-[12px] text-[#7a8088]">
          {t("settings.card.lorebook.empty")}
        </div>
      ) : (
        <div className="space-y-2">
          {value.map((entry, index) => (
            <div key={index} className="rounded-md border border-[#e5e7eb] bg-white p-2">
              <div className="mb-2 flex items-center justify-between gap-2">
                <span className="font-mono text-[11px] text-[#202124]">{entry.path}</span>
                <button type="button" className={dangerButtonCls} onClick={() => remove(index)}>
                  {t("common.remove")}
                </button>
              </div>
              <div className="grid gap-2">
                <Field label={t("settings.card.lorebook.path")}>
                  <input
                    className={inputCls}
                    value={entry.path}
                    onChange={(e) => update(index, { path: e.target.value })}
                  />
                </Field>
                <div className="grid gap-2 sm:grid-cols-2">
                  <Field label={t("settings.card.lorebook.entryTitle")}>
                    <input
                      className={inputCls}
                      value={entry.title ?? ""}
                      onChange={(e) => update(index, { title: e.target.value })}
                    />
                  </Field>
                  <Field label={t("settings.card.lorebook.priority")}>
                    <input
                      className={inputCls}
                      type="number"
                      step="0.1"
                      value={entry.priority ?? ""}
                      onChange={(e) => {
                        const raw = e.target.value;
                        update(index, { priority: raw === "" ? undefined : Number(raw) });
                      }}
                    />
                  </Field>
                </div>
                <Field label={t("settings.card.lorebook.tags")}>
                  <TagList
                    values={entry.tags ?? []}
                    onChange={(tags) => update(index, { tags })}
                  />
                </Field>
                <Field label={t("settings.card.lorebook.extensions")}>
                  <TagList
                    values={entry.extensions ?? []}
                    onChange={(extensions) => update(index, { extensions })}
                    placeholder="md, markdown, txt"
                  />
                </Field>
                <label className="flex items-center gap-2 text-[12px] text-[#5f6368]">
                  <input
                    className="h-4 w-4 accent-[#111827]"
                    type="checkbox"
                    checked={entry.recursive ?? false}
                    onChange={(e) => update(index, { recursive: e.target.checked })}
                  />
                  {t("settings.card.lorebook.recursive")}
                </label>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
