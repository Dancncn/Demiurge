import type { ExampleDialogue } from "../../lib/types";
import { useI18n } from "../../lib/i18n";
import {
  Field,
  dangerButtonCls,
  secondaryButtonCls,
  subSectionCls,
  subSectionTitleCls,
  textareaCls,
} from "../../lib/ui";

// 示例对话编辑器：user/assistant 文本对列表，支持增删与上下移动。
export function ExampleDialogueEditor({
  value,
  onChange,
}: {
  value: ExampleDialogue[];
  onChange: (next: ExampleDialogue[]) => void;
}) {
  const { t } = useI18n();

  function update(index: number, patch: Partial<ExampleDialogue>) {
    const next = value.map((d, idx) => (idx === index ? { ...d, ...patch } : d));
    onChange(next);
  }
  function remove(index: number) {
    onChange(value.filter((_, idx) => idx !== index));
  }
  function move(index: number, dir: -1 | 1) {
    const target = index + dir;
    if (target < 0 || target >= value.length) return;
    const next = [...value];
    [next[index], next[target]] = [next[target], next[index]];
    onChange(next);
  }
  function add() {
    onChange([...value, { user: "", assistant: "" }]);
  }

  return (
    <div className={subSectionCls}>
      <div className="mb-2 flex items-center justify-between gap-2">
        <div className={subSectionTitleCls}>{t("settings.card.example.section")}</div>
        <button type="button" className={secondaryButtonCls} onClick={add}>
          {t("settings.card.example.add")}
        </button>
      </div>
      {value.length === 0 ? (
        <div className="rounded-md border border-dashed border-[#d9d9d9] bg-white px-3 py-2 text-[12px] text-[#7a8088]">
          {t("settings.card.example.empty")}
        </div>
      ) : (
        <div className="space-y-2">
          {value.map((dialogue, index) => (
            <div key={index} className="rounded-md border border-[#e5e7eb] bg-white p-2">
              <div className="mb-2 flex items-center justify-between gap-2">
                <span className="text-[11px] font-medium text-[#5f6368]">#{index + 1}</span>
                <div className="flex gap-1.5">
                  <button
                    type="button"
                    className={secondaryButtonCls}
                    disabled={index === 0}
                    onClick={() => move(index, -1)}
                  >
                    {t("common.moveUp")}
                  </button>
                  <button
                    type="button"
                    className={secondaryButtonCls}
                    disabled={index === value.length - 1}
                    onClick={() => move(index, 1)}
                  >
                    {t("common.moveDown")}
                  </button>
                  <button type="button" className={dangerButtonCls} onClick={() => remove(index)}>
                    {t("common.remove")}
                  </button>
                </div>
              </div>
              <div className="grid gap-2">
                <Field label={t("settings.card.example.user")}>
                  <textarea
                    className={textareaCls}
                    value={dialogue.user}
                    onChange={(e) => update(index, { user: e.target.value })}
                  />
                </Field>
                <Field label={t("settings.card.example.assistant")}>
                  <textarea
                    className={textareaCls}
                    value={dialogue.assistant}
                    onChange={(e) => update(index, { assistant: e.target.value })}
                  />
                </Field>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
