import type { SpeechStyle } from "../../lib/types";
import { useI18n } from "../../lib/i18n";
import { Field, inputCls, subSectionCls, subSectionTitleCls } from "../../lib/ui";
import { TagList } from "./TagList";

// 说话风格编辑器：tone / first_person / address_user_as / catchphrases / taboo_phrases / sentence_patterns。
export function SpeechStyleForm({
  value,
  onChange,
}: {
  value: SpeechStyle;
  onChange: (next: SpeechStyle) => void;
}) {
  const { t } = useI18n();
  const set = (patch: Partial<SpeechStyle>) => onChange({ ...value, ...patch });
  return (
    <div className={subSectionCls}>
      <div className={subSectionTitleCls}>{t("settings.card.speechStyle.section")}</div>
      <div className="grid gap-3">
        <Field label={t("settings.card.speechStyle.tone")}>
          <TagList
            values={value.tone ?? []}
            onChange={(tone) => set({ tone })}
            placeholder={t("settings.card.speechStyle.tonePlaceholder")}
          />
        </Field>
        <div className="grid gap-3 sm:grid-cols-2">
          <Field label={t("settings.card.speechStyle.firstPerson")}>
            <input
              className={inputCls}
              value={value.first_person ?? ""}
              onChange={(e) => set({ first_person: e.target.value })}
            />
          </Field>
          <Field label={t("settings.card.speechStyle.addressUserAs")}>
            <input
              className={inputCls}
              value={value.address_user_as ?? ""}
              onChange={(e) => set({ address_user_as: e.target.value })}
            />
          </Field>
        </div>
        <Field label={t("settings.card.speechStyle.catchphrases")}>
          <TagList
            values={value.catchphrases ?? []}
            onChange={(catchphrases) => set({ catchphrases })}
          />
        </Field>
        <Field label={t("settings.card.speechStyle.tabooPhrases")}>
          <TagList
            values={value.taboo_phrases ?? []}
            onChange={(taboo_phrases) => set({ taboo_phrases })}
          />
        </Field>
        <Field label={t("settings.card.speechStyle.sentencePatterns")}>
          <TagList
            values={value.sentence_patterns ?? []}
            onChange={(sentence_patterns) => set({ sentence_patterns })}
          />
        </Field>
      </div>
    </div>
  );
}
