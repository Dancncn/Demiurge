import type { CharacterCard } from "../../lib/types";
import { useI18n } from "../../lib/i18n";
import {
  Field,
  dangerButtonCls,
  secondaryButtonCls,
  subSectionCls,
  subSectionTitleCls,
  textareaCls,
} from "../../lib/ui";
import { TagList } from "./TagList";
import { SpeechStyleForm } from "./SpeechStyleForm";
import { RelationshipForm } from "./RelationshipForm";
import { ExampleDialogueEditor } from "./ExampleDialogueEditor";

// 字符串长文本列表编辑器（每项一个 textarea，支持增删）。用于开场白、OOC 规则等多行文本。
function StringList({
  values,
  onChange,
  addLabel,
  emptyLabel,
  placeholder,
}: {
  values: string[];
  onChange: (next: string[]) => void;
  addLabel: string;
  emptyLabel: string;
  placeholder?: string;
}) {
  const { t } = useI18n();
  function update(index: number, text: string) {
    onChange(values.map((v, idx) => (idx === index ? text : v)));
  }
  return (
    <div className="space-y-1.5">
      {values.length === 0 ? (
        <div className="rounded-md border border-dashed border-[#d9d9d9] bg-white px-3 py-2 text-[12px] text-[#7a8088]">
          {emptyLabel}
        </div>
      ) : (
        values.map((text, index) => (
          <div key={index} className="flex items-start gap-2">
            <textarea
              className={textareaCls}
              value={text}
              onChange={(e) => update(index, e.target.value)}
              placeholder={placeholder}
            />
            <button
              type="button"
              className={dangerButtonCls}
              onClick={() => onChange(values.filter((_, idx) => idx !== index))}
            >
              {t("common.remove")}
            </button>
          </div>
        ))
      )}
      <button type="button" className={secondaryButtonCls} onClick={() => onChange([...values, ""])}>
        {addLabel}
      </button>
    </div>
  );
}

// 角色卡主体编辑器：identity / background / personality / habits / opening_messages / ooc_rules，
// 并嵌套 SpeechStyleForm / RelationshipForm / ExampleDialogueEditor。
export function CharacterCardForm({
  value,
  onChange,
}: {
  value: CharacterCard;
  onChange: (next: CharacterCard) => void;
}) {
  const { t } = useI18n();
  const set = (patch: Partial<CharacterCard>) => onChange({ ...value, ...patch });

  return (
    <div className="space-y-3">
      <div className={subSectionCls}>
        <div className={subSectionTitleCls}>{t("settings.card.character.identity")}</div>
        <div className="grid gap-3">
          <Field label={t("settings.card.character.identity")}>
            <textarea
              className={textareaCls}
              value={value.identity ?? ""}
              onChange={(e) => set({ identity: e.target.value })}
            />
          </Field>
          <Field label={t("settings.card.character.background")}>
            <textarea
              className={textareaCls}
              value={value.background ?? ""}
              onChange={(e) => set({ background: e.target.value })}
            />
          </Field>
          <Field label={t("settings.card.character.personality")}>
            <TagList
              values={value.personality ?? []}
              onChange={(personality) => set({ personality })}
            />
          </Field>
          <Field label={t("settings.card.character.habits")}>
            <TagList values={value.habits ?? []} onChange={(habits) => set({ habits })} />
          </Field>
        </div>
      </div>

      <SpeechStyleForm
        value={value.speech_style ?? {}}
        onChange={(speech_style) => set({ speech_style })}
      />

      <RelationshipForm
        value={value.relationship ?? {}}
        onChange={(relationship) => set({ relationship })}
      />

      <div className={subSectionCls}>
        <div className={subSectionTitleCls}>{t("settings.card.character.openingMessages")}</div>
        <StringList
          values={value.opening_messages ?? []}
          onChange={(opening_messages) => set({ opening_messages })}
          addLabel={t("settings.card.character.addOpening")}
          emptyLabel={t("settings.card.character.noOpening")}
        />
      </div>

      <ExampleDialogueEditor
        value={value.example_dialogues ?? []}
        onChange={(example_dialogues) => set({ example_dialogues })}
      />

      <div className={subSectionCls}>
        <div className={subSectionTitleCls}>{t("settings.card.character.oocRules")}</div>
        <StringList
          values={value.ooc_rules ?? []}
          onChange={(ooc_rules) => set({ ooc_rules })}
          addLabel={t("settings.card.character.addOoc")}
          emptyLabel={t("settings.card.character.noOoc")}
        />
      </div>
    </div>
  );
}
