import { useMemo, useState } from "react";
import type { CharacterCard, CharacterRuntime, LoreEntry, PackManifest } from "../../lib/types";
import { useI18n } from "../../lib/i18n";
import {
  Field,
  inputCls,
  monoTextareaCls,
  secondaryButtonCls,
  subSectionCls,
  subSectionTitleCls,
} from "../../lib/ui";
import { CharacterCardForm } from "./CharacterCardForm";
import { RuntimeForm } from "./RuntimeForm";
import { LorebookEditor } from "./LorebookEditor";
import { CreditsEditor } from "./CreditsEditor";

// 角色卡结构化编辑器：把 manifest JSON 解析成表单，编辑后序列化回 JSON。
// 表单为单一真源；保留可折叠"原始 JSON"作为回退与未来字段兜底。
export function PackEditor({
  manifestJson,
  onJsonChange,
}: {
  manifestJson: string;
  onJsonChange: (json: string) => void;
}) {
  const { t } = useI18n();
  const [showRaw, setShowRaw] = useState(false);

  const parsed = useMemo<{ manifest: PackManifest | null; error: string | null }>(() => {
    const text = manifestJson.trim();
    if (!text) return { manifest: null, error: null };
    try {
      const obj = JSON.parse(text) as PackManifest;
      return { manifest: obj, error: null };
    } catch (err) {
      return { manifest: null, error: (err as Error).message };
    }
  }, [manifestJson]);

  function serialize(next: PackManifest) {
    onJsonChange(`${JSON.stringify(next, null, 2)}\n`);
  }

  function setTopLevel(patch: Partial<PackManifest>) {
    if (!parsed.manifest) return;
    serialize({ ...parsed.manifest, ...patch });
  }

  if (parsed.error) {
    return (
      <div className="space-y-3">
        <div className="rounded-md border border-[#f3c3c3] bg-[#fff7f7] px-3 py-2 text-[12px] leading-5 text-[#b42318]">
          {t("settings.card.parseError")}: {parsed.error}
        </div>
        <textarea
          className={monoTextareaCls}
          spellCheck={false}
          value={manifestJson}
          onChange={(e) => onJsonChange(e.target.value)}
        />
      </div>
    );
  }

  if (!parsed.manifest) {
    return (
      <div className="rounded-md border border-dashed border-[#d9d9d9] bg-white px-3 py-2 text-[12px] text-[#7a8088]">
        {t("settings.card.empty")}
      </div>
    );
  }

  const manifest = parsed.manifest;

  return (
    <div className="space-y-3">
      <div className={subSectionCls}>
        <div className={subSectionTitleCls}>{t("settings.card.pack.section")}</div>
        <div className="grid gap-3">
          <Field label={t("settings.card.pack.id")} help={t("settings.card.pack.idHelp")}>
            <input className={inputCls} value={manifest.id ?? ""} readOnly />
          </Field>
          <Field label={t("settings.card.pack.name")}>
            <input
              className={inputCls}
              value={manifest.name ?? ""}
              onChange={(e) => setTopLevel({ name: e.target.value })}
            />
          </Field>
          <Field label={t("settings.card.pack.description")}>
            <input
              className={inputCls}
              value={manifest.description ?? ""}
              onChange={(e) => setTopLevel({ description: e.target.value })}
            />
          </Field>
          <div className="grid gap-3 sm:grid-cols-2">
            <Field label={t("settings.card.pack.persona")}>
              <input
                className={inputCls}
                value={manifest.persona ?? ""}
                onChange={(e) => setTopLevel({ persona: e.target.value })}
                placeholder="persona.md"
              />
            </Field>
            <Field label={t("settings.card.pack.avatar")}>
              <input
                className={inputCls}
                value={manifest.avatar ?? ""}
                onChange={(e) => setTopLevel({ avatar: e.target.value || undefined })}
                placeholder="assets/avatar.png"
              />
            </Field>
          </div>
        </div>
      </div>

      <div className={subSectionCls}>
        <div className={subSectionTitleCls}>{t("settings.card.character.section")}</div>
        <CharacterCardForm
          value={manifest.character ?? {}}
          onChange={(character: CharacterCard) => setTopLevel({ character })}
        />
      </div>

      <div className={subSectionCls}>
        <div className={subSectionTitleCls}>{t("settings.card.runtime.section")}</div>
        <RuntimeForm
          value={manifest.runtime ?? {}}
          onChange={(runtime: CharacterRuntime) => setTopLevel({ runtime })}
        />
      </div>

      <div className={subSectionCls}>
        <div className={subSectionTitleCls}>{t("settings.card.lorebook.section")}</div>
        <LorebookEditor
          value={manifest.lorebook ?? []}
          onChange={(lorebook: LoreEntry[]) => setTopLevel({ lorebook })}
        />
      </div>

      <CreditsEditor
        value={manifest.credits ?? []}
        onChange={(credits) => setTopLevel({ credits })}
        license={manifest.license}
        onLicenseChange={(license) => setTopLevel({ license: license || undefined })}
      />

      <div className="rounded-lg border border-[#e2e5ea] bg-white p-3">
        <button
          type="button"
          className={secondaryButtonCls}
          onClick={() => setShowRaw((v) => !v)}
        >
          {showRaw ? t("settings.card.raw.hide") : t("settings.card.raw.show")}
        </button>
        {showRaw && (
          <textarea
            className={`mt-3 ${monoTextareaCls}`}
            spellCheck={false}
            value={manifestJson}
            onChange={(e) => onJsonChange(e.target.value)}
          />
        )}
      </div>
    </div>
  );
}
