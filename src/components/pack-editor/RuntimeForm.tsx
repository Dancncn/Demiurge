import type { AutoSkillBinding, CharacterRuntime } from "../../lib/types";
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
import { KVRows } from "./KVRows";

const PERMISSION_OPTIONS = ["allow", "deny", "ask_once", "ask_every_time", "default"];

// 角色卡 Runtime 策略编辑器：skills / memory / voice / permissions。
export function RuntimeForm({
  value,
  onChange,
}: {
  value: CharacterRuntime;
  onChange: (next: CharacterRuntime) => void;
}) {
  const { t } = useI18n();
  const set = (patch: Partial<CharacterRuntime>) => onChange({ ...value, ...patch });

  const skills = value.skills ?? { recommended: [], disabled: [], auto_activate: [] };
  function setSkills(patch: Partial<typeof skills>) {
    set({ skills: { ...skills, ...patch } });
  }
  const memory = value.memory ?? {
    preferred_facts: [],
    must_remember: [],
    avoid_remembering: [],
  };
  function setMemory(patch: Partial<typeof memory>) {
    set({ memory: { ...memory, ...patch } });
  }
  const voice = value.voice ?? {};
  function setVoice(patch: Partial<typeof voice>) {
    set({ voice: { ...voice, ...patch } });
  }

  function updateAutoActivate(index: number, patch: Partial<AutoSkillBinding>) {
    const next = (skills.auto_activate ?? []).map((b, idx) => (idx === index ? { ...b, ...patch } : b));
    setSkills({ auto_activate: next });
  }
  function addAutoActivate() {
    setSkills({ auto_activate: [...(skills.auto_activate ?? []), { skill: "", when: [] }] });
  }
  function removeAutoActivate(index: number) {
    setSkills({ auto_activate: (skills.auto_activate ?? []).filter((_, idx) => idx !== index) });
  }

  return (
    <div className="space-y-3">
      <div className={subSectionCls}>
        <div className={subSectionTitleCls}>{t("settings.card.runtime.skills.section")}</div>
        <div className="grid gap-3">
          <Field label={t("settings.card.runtime.skills.recommended")}>
            <TagList
              values={skills.recommended ?? []}
              onChange={(recommended) => setSkills({ recommended })}
            />
          </Field>
          <Field label={t("settings.card.runtime.skills.disabled")}>
            <TagList
              values={skills.disabled ?? []}
              onChange={(disabled) => setSkills({ disabled })}
            />
          </Field>
          <Field
            label={t("settings.card.runtime.skills.autoActivate")}
            help={t("settings.card.runtime.skills.autoActivateHelp")}
          >
            <div className="space-y-1.5">
              {(skills.auto_activate ?? []).map((binding, index) => (
                <div key={index} className="flex items-start gap-2">
                  <div className="min-w-[160px] flex-1">
                    <input
                      className={inputCls}
                      value={binding.skill}
                      onChange={(e) => updateAutoActivate(index, { skill: e.target.value })}
                      placeholder={t("settings.card.runtime.skills.skillPlaceholder")}
                    />
                  </div>
                  <div className="min-w-[220px] flex-[2]">
                    <TagList
                      values={binding.when ?? []}
                      onChange={(when) => updateAutoActivate(index, { when })}
                      placeholder={t("settings.card.runtime.skills.whenPlaceholder")}
                    />
                  </div>
                  <button
                    type="button"
                    className={dangerButtonCls}
                    onClick={() => removeAutoActivate(index)}
                  >
                    {t("common.remove")}
                  </button>
                </div>
              ))}
              <button type="button" className={secondaryButtonCls} onClick={addAutoActivate}>
                {t("common.add")}
              </button>
            </div>
          </Field>
        </div>
      </div>

      <div className={subSectionCls}>
        <div className={subSectionTitleCls}>{t("settings.card.runtime.memory.section")}</div>
        <div className="grid gap-3">
          <div className="grid gap-3 sm:grid-cols-2">
            <Field
              label={t("settings.card.runtime.memory.namespace")}
              help={t("settings.card.runtime.memory.namespaceHelp")}
            >
              <input
                className={inputCls}
                value={memory.namespace ?? ""}
                onChange={(e) => setMemory({ namespace: e.target.value })}
                placeholder="default"
              />
            </Field>
            <Field label={t("settings.card.runtime.memory.writePolicy")}>
              <input
                className={inputCls}
                value={memory.write_policy ?? ""}
                onChange={(e) => setMemory({ write_policy: e.target.value })}
              />
            </Field>
          </div>
          <Field label={t("settings.card.runtime.memory.preferredFacts")}>
            <TagList
              values={memory.preferred_facts ?? []}
              onChange={(preferred_facts) => setMemory({ preferred_facts })}
            />
          </Field>
          <Field label={t("settings.card.runtime.memory.mustRemember")}>
            <TagList
              values={memory.must_remember ?? []}
              onChange={(must_remember) => setMemory({ must_remember })}
            />
          </Field>
          <Field label={t("settings.card.runtime.memory.avoidRemembering")}>
            <TagList
              values={memory.avoid_remembering ?? []}
              onChange={(avoid_remembering) => setMemory({ avoid_remembering })}
            />
          </Field>
        </div>
      </div>

      <div className={subSectionCls}>
        <div className={subSectionTitleCls}>{t("settings.card.runtime.voice.section")}</div>
        <div className="grid gap-3 sm:grid-cols-2">
          <Field label={t("settings.card.runtime.voice.ttsProfile")}>
            <input
              className={inputCls}
              value={voice.tts_profile ?? ""}
              onChange={(e) => setVoice({ tts_profile: e.target.value })}
            />
          </Field>
          <Field label={t("settings.card.runtime.voice.speed")}>
            <input
              className={inputCls}
              type="number"
              step="0.1"
              min="0.5"
              max="2"
              value={voice.speed ?? ""}
              onChange={(e) => {
                const raw = e.target.value;
                setVoice({ speed: raw === "" ? undefined : Number(raw) });
              }}
            />
          </Field>
        </div>
      </div>

      <div className={subSectionCls}>
        <div className={subSectionTitleCls}>{t("settings.card.runtime.permissions.section")}</div>
        <p className="mb-2 text-[12px] leading-5 text-[#7a8088]">
          {t("settings.card.runtime.permissions.help")}
        </p>
        <KVRows
          entries={value.permissions ?? {}}
          onChange={(permissions) => set({ permissions })}
          valueOptions={PERMISSION_OPTIONS}
          keyPlaceholder={t("settings.card.runtime.permissions.keyPlaceholder")}
        />
      </div>
    </div>
  );
}
