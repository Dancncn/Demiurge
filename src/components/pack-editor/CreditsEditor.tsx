import type { AssetCredit } from "../../lib/types";
import { useI18n } from "../../lib/i18n";
import {
  Field,
  dangerButtonCls,
  inputCls,
  secondaryButtonCls,
  subSectionCls,
  subSectionTitleCls,
} from "../../lib/ui";

// 素材授权清单编辑器：credits[] (asset/author/source/license) + 整包 license。
export function CreditsEditor({
  value,
  onChange,
  license,
  onLicenseChange,
}: {
  value: AssetCredit[];
  onChange: (next: AssetCredit[]) => void;
  license?: string;
  onLicenseChange: (next: string) => void;
}) {
  const { t } = useI18n();
  function update(index: number, patch: Partial<AssetCredit>) {
    onChange(value.map((c, idx) => (idx === index ? { ...c, ...patch } : c)));
  }
  function add() {
    onChange([...value, { asset: "" }]);
  }
  function remove(index: number) {
    onChange(value.filter((_, idx) => idx !== index));
  }
  return (
    <div className={subSectionCls}>
      <div className={subSectionTitleCls}>{t("settings.card.credits.section")}</div>
      <Field label={t("settings.card.credits.license")} help={t("settings.card.credits.licenseHelp")}>
        <input
          className={inputCls}
          value={license ?? ""}
          onChange={(e) => onLicenseChange(e.target.value)}
          placeholder="MIT / CC-BY-4.0 ..."
        />
      </Field>
      <div className="mt-2 space-y-2">
        {value.map((credit, index) => (
          <div key={index} className="rounded-md border border-[#e5e7eb] bg-white p-2">
            <div className="mb-2 flex justify-end">
              <button type="button" className={dangerButtonCls} onClick={() => remove(index)}>
                {t("common.remove")}
              </button>
            </div>
            <div className="grid gap-2 sm:grid-cols-2">
              <Field label={t("settings.card.credits.asset")}>
                <input
                  className={inputCls}
                  value={credit.asset}
                  onChange={(e) => update(index, { asset: e.target.value })}
                  placeholder="assets/avatar.png"
                />
              </Field>
              <Field label={t("settings.card.credits.author")}>
                <input
                  className={inputCls}
                  value={credit.author ?? ""}
                  onChange={(e) => update(index, { author: e.target.value })}
                />
              </Field>
              <Field label={t("settings.card.credits.source")}>
                <input
                  className={inputCls}
                  value={credit.source ?? ""}
                  onChange={(e) => update(index, { source: e.target.value })}
                />
              </Field>
              <Field label={t("settings.card.credits.licenseField")}>
                <input
                  className={inputCls}
                  value={credit.license ?? ""}
                  onChange={(e) => update(index, { license: e.target.value })}
                />
              </Field>
            </div>
          </div>
        ))}
        <button type="button" className={secondaryButtonCls} onClick={add}>
          {t("settings.card.credits.add")}
        </button>
      </div>
    </div>
  );
}
