import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import * as api from "../lib/api";
import { useI18n } from "../lib/i18n";
import type { SkillPanelState, SkillScope, SkillSummary } from "../lib/types";
import { RotateCwIcon, FolderIcon, SparklesIcon } from "./Icons";

const SCOPE_STYLES: Record<SkillScope, string> = {
  global: "bg-[#eef2ff] text-[#4338ca] border-[#dfe3ff]",
  project: "bg-[#ecfdf5] text-[#047857] border-[#cdeee0]",
  repository: "bg-[#fef3f2] text-[#b42318] border-[#fcdcd7]",
  pack: "bg-[#fffaeb] text-[#b54708] border-[#fdedc6]",
  compat: "bg-[#f4f3ff] text-[#5b4ad1] border-[#e6e2fb]",
  legacy: "bg-[#f2f4f7] text-[#475467] border-[#e4e7ec]",
};

// Browser-preview mock so the panel renders without the Tauri backend.
function mockState(): SkillPanelState {
  return {
    skills: [
      {
        id: "demo-pdf",
        name: "PDF 处理",
        description: "读取、合并、拆分 PDF 文件，提取文本与表格。",
        scope: "global",
        path: "skills/pdf/SKILL.md",
        triggers: ["pdf", "文档", "合并"],
        declared_tool_needs: ["read_file", "shell"],
        required_permissions: ["fs.read"],
        references: ["reference.md"],
        selected: true,
        match_score: 42,
      },
      {
        id: "demo-web",
        name: "网页检索",
        description: "通过搜索引擎检索资料并汇总来源。",
        scope: "project",
        path: ".demiurge/skills/web/SKILL.md",
        triggers: ["搜索", "search", "news"],
        declared_tool_needs: ["web_search"],
        required_permissions: [],
        references: [],
        selected: false,
        match_score: 8,
      },
    ],
    diagnostics: [],
  };
}

function Badge({ children, className = "" }: { children: ReactNode; className?: string }) {
  return (
    <span
      className={`md-type-label-small inline-flex items-center rounded-full border px-2 py-0.5 ${className}`}
    >
      {children}
    </span>
  );
}

function Chips({ label, items }: { label: string; items: string[] }) {
  if (!items.length) return null;
  return (
    <div className="mt-2 flex flex-wrap items-center gap-1.5">
      <span className="md-type-label-small text-[#8a9099]">{label}</span>
      {items.map((item) => (
        <span key={item} className="md-type-label-small rounded-md bg-[#f2f4f7] px-1.5 py-0.5 text-[#475467]">
          {item}
        </span>
      ))}
    </div>
  );
}

function SkillCard({ skill }: { skill: SkillSummary }) {
  const { t } = useI18n();
  return (
    <div
      className={`skill-card cf-rise rounded-lg border bg-white p-4 shadow-[0_1px_3px_rgba(15,23,42,0.05)] ${
        skill.selected ? "is-selected border-[#10a37f]/40 ring-1 ring-[#10a37f]/20" : "border-[#e6e9ee]"
      }`}
    >
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <span className="md-type-title-small truncate font-semibold text-[#202124]">{skill.name}</span>
            <Badge className={SCOPE_STYLES[skill.scope] ?? SCOPE_STYLES.legacy}>{t(`scope.${skill.scope}`)}</Badge>
          </div>
          <div className="md-type-label-small mt-0.5 truncate text-[#9aa1ab]" title={skill.path}>
            {skill.path}
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-1.5">
          {skill.selected ? (
            <Badge className="border-[#bbf7d0] bg-[#ecfdf5] text-[#047857]">{t("skills.selected")}</Badge>
          ) : skill.match_score > 0 ? (
            <Badge className="border-[#e4e7ec] bg-[#f9fafb] text-[#667085]">
              {t("skills.score", { n: skill.match_score })}
            </Badge>
          ) : null}
        </div>
      </div>

      {skill.description && <p className="md-type-body-medium mt-2 text-[#3f4652]">{skill.description}</p>}

      <Chips label={t("skills.triggers")} items={skill.triggers} />
      <Chips label={t("skills.tools")} items={skill.declared_tool_needs} />
      <Chips label={t("skills.permissions")} items={skill.required_permissions} />
      <Chips label={t("skills.references")} items={skill.references} />
    </div>
  );
}

export function SkillsPanel() {
  const { t } = useI18n();
  const [query, setQuery] = useState("");
  const [state, setState] = useState<SkillPanelState | null>(null);
  const [loading, setLoading] = useState(false);
  const debounce = useRef<ReturnType<typeof setTimeout> | null>(null);

  async function load(q: string) {
    setLoading(true);
    try {
      setState(await api.skillPanelState(q.trim() || undefined));
    } catch {
      if (!("__TAURI_INTERNALS__" in window)) setState(mockState());
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    void load("");
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (debounce.current) clearTimeout(debounce.current);
    debounce.current = setTimeout(() => void load(query), 250);
    return () => {
      if (debounce.current) clearTimeout(debounce.current);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [query]);

  const skills = state?.skills ?? [];
  const selectedCount = useMemo(() => skills.filter((s) => s.selected).length, [skills]);

  return (
    <div className="skills-panel flex h-full min-h-0 flex-col bg-white">
      <header className="skills-header shrink-0 border-b border-[#eceff3] bg-[#fbfcfd] px-4 py-3.5 sm:px-6">
        <div className="skills-header-inner mx-auto w-full max-w-[1120px]">
          <div className="skills-header-row flex items-center justify-between gap-3">
            <div className="flex min-w-0 items-center gap-2.5">
              <span className="skills-header-icon grid size-8 shrink-0 place-items-center rounded-lg bg-[#f4f3ff] text-[#5b4ad1]">
                <SparklesIcon size={18} />
              </span>
              <div className="min-w-0">
                <h1 className="md-type-title-medium truncate font-semibold text-[#202124]">{t("skills.title")}</h1>
                <p className="md-type-body-small truncate text-[#8a9099]">{t("skills.subtitle")}</p>
              </div>
            </div>
            <div className="skills-header-actions flex items-center gap-1.5">
              <button
                onClick={() => void load(query)}
                className="md-button md-button-outlined cf-press flex items-center gap-1.5 border border-[#dfe3e8] bg-white px-2.5 py-1.5 text-[#3f4652] transition hover:bg-[#f6f7f9]"
              >
                <RotateCwIcon size={14} className={loading ? "animate-spin" : ""} />
                {t("skills.refresh")}
              </button>
              <button
                onClick={() => void api.openSkillsDir().catch(() => {})}
                className="md-button md-button-outlined cf-press flex items-center gap-1.5 border border-[#dfe3e8] bg-white px-2.5 py-1.5 text-[#3f4652] transition hover:bg-[#f6f7f9]"
              >
                <FolderIcon size={14} />
                {t("skills.openDir")}
              </button>
            </div>
          </div>
          <div className="mt-3">
            <input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder={t("skills.searchPlaceholder")}
              className="md-text-field md-type-body-medium w-full rounded-lg border border-[#dfe3e8] bg-white px-3 py-2 outline-none transition"
            />
          </div>
        </div>
      </header>

      <div className="skills-scroll min-h-0 flex-1 overflow-y-auto px-4 py-5 sm:px-6">
        <div className="skills-content mx-auto w-full max-w-[1120px]">
          <div className="md-type-label-medium mb-3 flex items-center justify-between text-[#8a9099]">
            <span>{t("skills.matchedCount", { sel: selectedCount, total: skills.length })}</span>
            {loading && <span>{t("skills.loading")}</span>}
          </div>

          {skills.length === 0 ? (
            <div className="md-type-body-medium rounded-lg border border-dashed border-[#dfe3e8] bg-[#fbfcfd] px-4 py-10 text-center text-[#9aa1ab]">
              {query.trim() ? t("skills.noMatch") : t("skills.empty")}
            </div>
          ) : (
            <div className="skills-grid grid gap-3">
              {skills.map((skill) => (
                <SkillCard key={`${skill.scope}:${skill.id}`} skill={skill} />
              ))}
            </div>
          )}

          {state?.diagnostics && state.diagnostics.length > 0 && (
            <div className="md-type-body-small mt-4 rounded-lg border border-[#fdedc6] bg-[#fffaeb] px-3 py-2.5 text-[#b54708]">
              <div className="font-semibold">{t("skills.diagnostics")}</div>
              <ul className="mt-1 list-disc pl-4">
                {state.diagnostics.map((d, i) => (
                  <li key={i}>{d}</li>
                ))}
              </ul>
            </div>
          )}

          <div className="mt-5 rounded-lg border border-[#e6e9ee] bg-[#f9fafb] px-4 py-3">
            <div className="md-type-label-large font-semibold text-[#3f4652]">{t("skills.hintTitle")}</div>
            <p className="md-type-body-small mt-1 text-[#667085]">{t("skills.hint")}</p>
          </div>
        </div>
      </div>
    </div>
  );
}

export default SkillsPanel;
