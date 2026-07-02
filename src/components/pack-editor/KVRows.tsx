import { dangerButtonCls, inputCls } from "../../lib/ui";
import { useI18n } from "../../lib/i18n";
import { Select } from "../Select";

// 键值对编辑器：渲染 Record<string,string> 为 key+value 行，支持增删。
// 用于角色卡 runtime.permissions（tool -> 偏好字符串）。valueOptions 提供时用下拉，否则自由文本。
export function KVRows({
  entries,
  onChange,
  keyPlaceholder,
  valuePlaceholder,
  valueOptions,
  addLabel,
}: {
  entries: Record<string, string>;
  onChange: (next: Record<string, string>) => void;
  keyPlaceholder?: string;
  valuePlaceholder?: string;
  valueOptions?: string[];
  addLabel?: string;
}) {
  const { t } = useI18n();
  const pairs = Object.entries(entries);

  function updateKey(oldKey: string, newKey: string) {
    const next: Record<string, string> = {};
    for (const [k, v] of pairs) {
      if (k === oldKey) {
        next[newKey.trim()] = v;
      } else {
        next[k] = v;
      }
    }
    onChange(next);
  }

  function updateValue(key: string, value: string) {
    onChange({ ...entries, [key]: value });
  }

  function removeKey(key: string) {
    const next = { ...entries };
    delete next[key];
    onChange(next);
  }

  function addRow() {
    let base = "tool";
    let idx = 1;
    while (entries[base]) {
      base = `tool${idx}`;
      idx += 1;
    }
    onChange({ ...entries, [base]: valueOptions?.[0] ?? "" });
  }

  return (
    <div className="space-y-1.5">
      {pairs.map(([key, value]) => (
        <div key={key} className="flex items-center gap-2">
          <input
            className={inputCls}
            value={key}
            onChange={(event) => updateKey(key, event.target.value)}
            placeholder={keyPlaceholder}
          />
          {valueOptions ? (
            <div className="w-[170px] shrink-0">
              <Select
                value={value}
                onChange={(v) => updateValue(key, v)}
                options={valueOptions.map((opt) => ({ value: opt, label: opt }))}
              />
            </div>
          ) : (
            <input
              className={inputCls}
              value={value}
              onChange={(event) => updateValue(key, event.target.value)}
              placeholder={valuePlaceholder}
            />
          )}
          <button
            type="button"
            className={dangerButtonCls}
            aria-label={t("common.remove")}
            onClick={() => removeKey(key)}
          >
            {t("common.remove")}
          </button>
        </div>
      ))}
      <button type="button" className={dangerButtonCls} onClick={addRow}>
        {addLabel ?? t("common.add")}
      </button>
    </div>
  );
}
