import { useEffect, useState, type ReactNode } from "react";
import {
  lorebookIndexStatus,
  lorebookRecallDetail,
  lorebookRebuildIndex,
} from "../../lib/api";
import type { LoreHitDetail, LoreIndexStatus, LoreRecallDetail } from "../../lib/types";
import { useI18n } from "../../lib/i18n";
import { inputCls, secondaryButtonCls } from "../../lib/ui";

// 把匹配关键词高亮成 <mark>。split 用捕获组，奇数索引为匹配段。
function highlight(text: string, terms: string[]): ReactNode {
  const escaped = terms
    .map((t) => t.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"))
    .filter((t) => t.length > 0);
  if (escaped.length === 0) return text;
  const re = new RegExp(`(${escaped.join("|")})`, "gi");
  const parts = text.split(re);
  return parts.map((part, i) => {
    if (i % 2 === 1) {
      return (
        <mark key={i} className="rounded bg-[#fde68a] px-0.5 text-[#7c5108]">
          {part}
        </mark>
      );
    }
    return <span key={i}>{part}</span>;
  });
}

function formatTime(ms: number): string {
  if (!ms) return "-";
  const d = new Date(ms);
  return d.toLocaleString();
}

// Lorebook 召回可视化：索引状态、重建按钮、chunk 列表（score + 命中关键词高亮）。
export function LorebookRecallPanel({ packId }: { packId: string }) {
  const { t } = useI18n();
  const [status, setStatus] = useState<LoreIndexStatus | null>(null);
  const [query, setQuery] = useState("");
  const [detail, setDetail] = useState<LoreRecallDetail | null>(null);
  const [busy, setBusy] = useState(false);
  const [rebuildBusy, setRebuildBusy] = useState(false);
  const [error, setError] = useState("");
  const [expanded, setExpanded] = useState<Set<number>>(new Set());

  useEffect(() => {
    if (!packId) {
      setStatus(null);
      return;
    }
    setError("");
    lorebookIndexStatus(packId)
      .then(setStatus)
      .catch((e) => setError(String(e)));
  }, [packId]);

  async function rebuild() {
    if (!packId) return;
    setRebuildBusy(true);
    setError("");
    try {
      const s = await lorebookRebuildIndex(packId);
      setStatus(s);
    } catch (e) {
      setError(String(e));
    } finally {
      setRebuildBusy(false);
    }
  }

  async function runPreview() {
    if (!packId || !query.trim()) return;
    setBusy(true);
    setError("");
    try {
      const d = await lorebookRecallDetail(packId, query.trim(), 20);
      setDetail(d);
      setExpanded(new Set());
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const maxScore = detail?.hits.reduce((m, h) => Math.max(m, h.score), 0) || 0;

  function toggle(idx: number) {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(idx)) next.delete(idx);
      else next.add(idx);
      return next;
    });
  }

  return (
    <div className="rounded-md border border-[#e2e5ea] bg-[#fbfcfd] p-3">
      <div className="mb-2 flex flex-wrap items-center justify-between gap-3">
        <div>
          <div className="text-[12px] font-medium text-[#202124]">Lorebook RAG</div>
          <div className="mt-0.5 text-[12px] leading-5 text-[#7a8088]">
            {t("settings.persona.lorebookDesc")}
          </div>
        </div>
      </div>

      {status && (
        <div className="mb-2 flex flex-wrap items-center gap-2 rounded-md border border-[#eceff3] bg-white px-3 py-2 text-[12px] text-[#5f6368]">
          <span>
            {t("settings.persona.indexFiles")}: <b>{status.file_count}</b>
          </span>
          <span>
            {t("settings.persona.indexChunks")}: <b>{status.chunk_count}</b>
          </span>
          <span>
            {t("settings.persona.indexState")}:{" "}
            {status.files_stale ? (
              <span className="text-[#9a6b00]">{t("settings.persona.indexStale")}</span>
            ) : status.cache_exists ? (
              <span className="text-[#3f6212]">{t("settings.persona.indexFresh")}</span>
            ) : (
              <span className="text-[#9a6b00]">{t("settings.persona.indexMissing")}</span>
            )}
          </span>
          {status.last_built_ms > 0 && (
            <span className="text-[#9aa1ab]">
              {t("settings.persona.indexBuilt", { time: formatTime(status.last_built_ms) })}
            </span>
          )}
          <button
            type="button"
            className={`ml-auto ${secondaryButtonCls}`}
            disabled={rebuildBusy}
            onClick={() => void rebuild()}
          >
            {rebuildBusy ? t("settings.persona.rebuilding") : t("settings.persona.rebuild")}
          </button>
        </div>
      )}

      <div className="flex flex-col gap-2 sm:flex-row">
        <input
          className={inputCls}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") void runPreview();
          }}
          placeholder={t("settings.persona.previewPlaceholder")}
        />
        <button
          type="button"
          className={secondaryButtonCls}
          disabled={busy || !packId || !query.trim()}
          onClick={() => void runPreview()}
        >
          {busy ? t("settings.persona.previewing") : t("settings.persona.preview")}
        </button>
      </div>

      {error && (
        <div className="mt-2 rounded-md border border-[#f3c3c3] bg-[#fff7f7] px-3 py-2 text-[12px] text-[#b42318]">
          {error}
        </div>
      )}

      {detail && (
        <div className="mt-2 space-y-1.5">
          <div className="text-[12px] text-[#7a8088]">
            {t("settings.persona.recallSummary", {
              total: detail.total_chunks,
              hits: detail.hits.length,
            })}
          </div>
          {detail.hits.length === 0 ? (
            <div className="rounded-md border border-dashed border-[#d9d9d9] bg-white px-3 py-2 text-[12px] text-[#7a8088]">
              {t("settings.persona.noHits")}
            </div>
          ) : (
            detail.hits.map((hit, idx) => (
              <RecallHit
                key={idx}
                hit={hit}
                maxScore={maxScore}
                expanded={expanded.has(idx)}
                onToggle={() => toggle(idx)}
              />
            ))
          )}
        </div>
      )}
    </div>
  );
}

function RecallHit({
  hit,
  maxScore,
  expanded,
  onToggle,
}: {
  hit: LoreHitDetail;
  maxScore: number;
  expanded: boolean;
  onToggle: () => void;
}) {
  const pct = maxScore > 0 ? Math.max(4, Math.round((hit.score / maxScore) * 100)) : 4;
  const heading = hit.heading?.trim() || hit.title;
  return (
    <div className="rounded-md border border-[#e5e7eb] bg-white px-3 py-2 text-[12px] leading-5">
      <div className="flex items-center gap-2">
        <div className="flex flex-1 items-center gap-2">
          <span className="font-mono text-[11px] text-[#202124]">
            {hit.source}#{hit.chunk_index}
          </span>
          <div className="h-1.5 w-24 overflow-hidden rounded-full bg-[#eceff3]">
            <div className="h-full rounded-full bg-[#6366f1]" style={{ width: `${pct}%` }} />
          </div>
          <span className="text-[11px] text-[#6b7280]">
            score={hit.score.toFixed(3)}
            {hit.dense_score != null && (
              <span className="ml-2 text-[#3f6212]">dense={hit.dense_score.toFixed(3)}</span>
            )}
          </span>
        </div>
        <button
          type="button"
          className="text-[11px] text-[#6b7280] hover:text-[#202124]"
          onClick={onToggle}
        >
          {expanded ? "−" : "+"}
        </button>
      </div>
      <div className="mt-0.5 text-[11px] text-[#7a8088]">{heading}</div>
      {hit.matched_terms.length > 0 && (
        <div className="mt-0.5 text-[11px] text-[#7c5108]">
          {hit.matched_terms.join(" · ")}
        </div>
      )}
      {expanded && (
        <pre className="mt-1 whitespace-pre-wrap break-words text-[11px] leading-5 text-[#3f4650]">
          {highlight(hit.text, hit.matched_terms)}
        </pre>
      )}
    </div>
  );
}
