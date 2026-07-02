import type { RelationshipStyle } from "../../lib/types";
import { useI18n } from "../../lib/i18n";
import { Field, subSectionCls, subSectionTitleCls, textareaCls } from "../../lib/ui";

// 关系风格编辑器：default / progression 两个长文本。
export function RelationshipForm({
  value,
  onChange,
}: {
  value: RelationshipStyle;
  onChange: (next: RelationshipStyle) => void;
}) {
  const { t } = useI18n();
  const set = (patch: Partial<RelationshipStyle>) => onChange({ ...value, ...patch });
  return (
    <div className={subSectionCls}>
      <div className={subSectionTitleCls}>{t("settings.card.relationship.section")}</div>
      <div className="grid gap-3">
        <Field label={t("settings.card.relationship.default")}>
          <textarea
            className={textareaCls}
            value={value.default ?? ""}
            onChange={(e) => set({ default: e.target.value })}
          />
        </Field>
        <Field label={t("settings.card.relationship.progression")}>
          <textarea
            className={textareaCls}
            value={value.progression ?? ""}
            onChange={(e) => set({ progression: e.target.value })}
          />
        </Field>
      </div>
    </div>
  );
}
