import { useEffect, useMemo, useRef, useState } from "react";
import * as api from "@/lib/api";
import { useI18n } from "@/lib/i18n";
import type {
  ExternalSessionMessage,
  IntegrationSnapshot,
  MarketSearchResult,
  SkillCandidate,
  UsageSummary,
} from "@/lib/types";
import { pickFolder } from "@/lib/folderPicker";
import { FolderIcon, RotateCwIcon, SparklesIcon } from "@/shared/components/Icons";

type Tab = "skills" | "sessions" | "usage";

const EMPTY_SNAPSHOT: IntegrationSnapshot = { skills: [], sessions: [], configs: [], diagnostics: [] };

function Chip({ children }: { children: React.ReactNode }) {
  return <span className="resource-chip rounded-full px-2 py-0.5 text-[11px]">{children}</span>;
}

function SkillRow({ skill, onImport, onToggle, onRemove }: {
  skill: SkillCandidate;
  onImport: (skill: SkillCandidate) => void;
  onToggle: (skill: SkillCandidate) => void;
  onRemove: (skill: SkillCandidate) => void;
}) {
  const { t } = useI18n();
  return (
    <div className="resource-panel rounded-lg p-4">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <span className="resource-strong font-semibold">{skill.name}</span>
            <Chip>{skill.source}</Chip>
            {skill.managed && <Chip>{skill.enabled ? t("resource.enabled") : t("resource.disabled")}</Chip>}
          </div>
          <p className="resource-muted mt-1 truncate text-[12px]" title={skill.path}>{skill.path}</p>
          {skill.description && <p className="resource-muted mt-2 text-[13px] leading-5">{skill.description}</p>}
        </div>
        <div className="flex shrink-0 flex-wrap justify-end gap-1.5">
          {skill.managed ? (
            <>
              <button className="resource-control rounded-md px-2 py-1 text-[12px]" onClick={() => onToggle(skill)}>
                {skill.enabled ? t("resource.disable") : t("resource.enable")}
              </button>
              <button className="resource-danger rounded-md px-2 py-1 text-[12px]" onClick={() => onRemove(skill)}>
                {t("resource.skillRemove")}
              </button>
            </>
          ) : (
            <button className="resource-primary rounded-md px-2.5 py-1 text-[12px]" onClick={() => onImport(skill)}>
              {t("resource.importManage")}
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

function formatUsageTokens(value: number, locale: string) {
  return value.toLocaleString(locale);
}

function formatUsageCost(value: number | null | undefined, locale: string, unpriced: string) {
  return value == null ? unpriced : new Intl.NumberFormat(locale, { style: "currency", currency: "USD", minimumFractionDigits: 4, maximumFractionDigits: 4 }).format(value);
}

function formatUsageTime(timestamp: number, locale: string) {
  return new Date(timestamp).toLocaleString(locale, {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function formatUsageDay(key: string, locale: string) {
  const day = Number(key.replace(/^day-/, ""));
  if (!Number.isFinite(day)) return key;
  return new Date(day * 86_400_000).toLocaleDateString(locale, { month: "2-digit", day: "2-digit" });
}

function UsageMetric({ label, value, hint }: { label: string; value: string; hint?: string }) {
  return (
    <div className="resource-panel rounded-lg p-3.5">
      <div className="resource-muted text-[11px]">{label}</div>
      <div className="resource-strong mt-1 text-[20px] font-semibold">{value}</div>
      {hint && <div className="resource-faint mt-1 truncate text-[10px]">{hint}</div>}
    </div>
  );
}

function UsageDashboard({ usage }: { usage: UsageSummary | null }) {
  const { lang, t } = useI18n();
  const locale = lang === "zh" ? "zh-CN" : "en-US";
  if (!usage) {
    return <div className="resource-empty rounded-lg border border-dashed px-4 py-10 text-center text-[12px]">{t("resource.loadingUsage")}</div>;
  }

  const successRate = usage.total_requests > 0 ? usage.successful_requests / usage.total_requests : 0;
  const daily = usage.daily.slice(-14);
  const maxDailyTokens = Math.max(...daily.map((bucket) => bucket.total_tokens), 1);
  const tokenParts = [
    { label: t("resource.usageInputPart"), value: usage.input_tokens, color: "resource-chart-input" },
    { label: t("resource.usageOutputPart"), value: usage.output_tokens, color: "resource-chart-output" },
    { label: t("resource.usageCacheRead"), value: usage.cache_read_tokens, color: "resource-chart-cache-read" },
    { label: t("resource.usageCacheWrite"), value: usage.cache_creation_tokens, color: "resource-chart-cache-write" },
  ];
  const maxTokenPart = Math.max(...tokenParts.map((part) => part.value), 1);
  const logDiagnostics = {
    read: usage.log_read_errors ?? 0,
    malformed: usage.malformed_lines ?? 0,
    oversized: usage.oversized_lines ?? 0,
    write: usage.write_failures ?? 0,
  };
  const hasLogDiagnostics = Object.values(logDiagnostics).some((value) => value > 0);

  return (
    <div className="grid gap-4">
      {hasLogDiagnostics && (
        <div className="resource-error rounded-lg px-3 py-2 text-[12px]" role="status">
          {t("resource.usageDiagnostics", logDiagnostics)}
        </div>
      )}
      <section className="resource-usage-metrics grid gap-3">
        <UsageMetric label={t("resource.usageRequests")} value={usage.total_requests.toLocaleString(locale)} />
        <UsageMetric label={t("resource.usageSuccessRate")} value={`${(successRate * 100).toFixed(1)}%`} hint={t("resource.usageFailures", { n: usage.failed_requests.toLocaleString(locale), m: usage.interrupted_requests.toLocaleString(locale) })} />
        <UsageMetric label={t("resource.usageInput")} value={formatUsageTokens(usage.input_tokens, locale)} />
        <UsageMetric label={t("resource.usageOutput")} value={formatUsageTokens(usage.output_tokens, locale)} />
        <UsageMetric label={t("resource.usageTotal")} value={formatUsageTokens(usage.total_tokens, locale)} />
        <UsageMetric label={t("resource.usageCacheRate")} value={`${(usage.cache_hit_rate * 100).toFixed(1)}%`} hint={t("resource.usageCacheRateHint")} />
        <UsageMetric label={t("resource.usageLatency")} value={`${usage.average_latency_ms.toLocaleString(locale)} ms`} />
        <UsageMetric label={t("resource.usageCost")} value={formatUsageCost(usage.total_cost_usd, locale, t("resource.unpriced"))} hint={t("resource.usageUnpriced", { n: usage.unpriced_requests.toLocaleString(locale) })} />
      </section>

      <section className="resource-usage-layout grid gap-4">
        <div className="resource-panel rounded-lg p-4">
          <div className="flex flex-wrap items-baseline justify-between gap-2">
            <div>
              <h2 className="resource-strong text-[14px] font-semibold">{t("resource.usageDailyTitle")}</h2>
              <p className="resource-muted mt-1 text-[11px]">{t("resource.usageDailyDesc", { n: daily.length })}</p>
            </div>
            <span className="resource-muted text-[11px]">{t("resource.usageGrandTotal", { n: formatUsageTokens(usage.total_tokens, locale) })}</span>
          </div>
          {daily.length === 0 ? (
            <div className="resource-empty py-14 text-center text-[12px]">{t("resource.usageNoDaily")}</div>
          ) : (
            <div className="resource-chart-axis mt-5 flex h-44 items-end gap-2 overflow-x-auto border-b pb-2">
              {daily.map((bucket) => (
                <div key={bucket.key} className="flex h-full min-w-[44px] flex-1 flex-col justify-end gap-1">
                  <div className="flex min-h-0 flex-1 items-end justify-center" role="img" aria-label={t("resource.chartDailyLabel", { day: formatUsageDay(bucket.key, locale), requests: bucket.requests.toLocaleString(locale), tokens: formatUsageTokens(bucket.total_tokens, locale) })}>
                    <div className="resource-chart-input w-full max-w-[28px] rounded-t transition-all" style={{ height: `${Math.max(8, (bucket.total_tokens / maxDailyTokens) * 100)}%` }} />
                  </div>
                  <span className="resource-faint truncate text-center text-[10px]">{formatUsageDay(bucket.key, locale)}</span>
                  <span className="resource-muted text-center text-[10px]">{t("resource.usageRequestsCount", { n: bucket.requests.toLocaleString(locale) })}</span>
                </div>
              ))}
            </div>
          )}
        </div>

        <div className="resource-panel rounded-lg p-4">
          <h2 className="resource-strong text-[14px] font-semibold">{t("resource.usageTokensCacheTitle")}</h2>
          <p className="resource-muted mt-1 text-[11px]">{t("resource.usageTokensCacheDesc")}</p>
          <div className="mt-5 grid gap-3">
            {tokenParts.map((part) => (
              <div key={part.label}>
                <div className="mb-1 flex items-center justify-between gap-2 text-[12px]">
                  <span className="resource-muted">{part.label}</span>
                  <span className="resource-strong font-medium">{formatUsageTokens(part.value, locale)}</span>
                </div>
                <div className="resource-track h-2 overflow-hidden rounded-full">
                  <div className={`h-full rounded-full ${part.color}`} style={{ width: `${Math.max(part.value > 0 ? 3 : 0, (part.value / maxTokenPart) * 100)}%` }} />
                </div>
              </div>
            ))}
          </div>
          <div className="resource-subgrid mt-5 grid grid-cols-2 gap-2 text-[11px]">
            <div className="resource-subpanel rounded-md px-2.5 py-2">{t("resource.usageCacheRead")}<br /><span className="resource-strong font-semibold">{formatUsageTokens(usage.cache_read_tokens, locale)}</span></div>
            <div className="resource-subpanel rounded-md px-2.5 py-2">{t("resource.usageCacheWrite")}<br /><span className="resource-strong font-semibold">{formatUsageTokens(usage.cache_creation_tokens, locale)}</span></div>
          </div>
        </div>
      </section>

      <section className="resource-distribution-grid grid gap-4">
        <div className="resource-panel rounded-lg p-4">
          <h2 className="resource-strong text-[14px] font-semibold">{t("resource.usageProviderTitle")}</h2>
          <div className="mt-3 grid gap-2">
            {usage.providers.length === 0 && <div className="resource-empty text-[12px]">{t("resource.usageNoData")}</div>}
            {usage.providers.map((bucket) => (
              <div key={bucket.key} className="resource-subpanel flex items-center justify-between gap-3 rounded-md px-3 py-2 text-[12px]">
                <span className="truncate">{bucket.key}</span>
                <span className="resource-muted shrink-0">{t("resource.usageRequestsCount", { n: bucket.requests.toLocaleString(locale) })} · {formatUsageTokens(bucket.total_tokens, locale)} · {formatUsageCost(bucket.cost_usd, locale, t("resource.unpriced"))}</span>
              </div>
            ))}
          </div>
        </div>
        <div className="resource-panel rounded-lg p-4">
          <h2 className="resource-strong text-[14px] font-semibold">{t("resource.usageModelTitle")}</h2>
          <div className="mt-3 grid gap-2">
            {usage.models.length === 0 && <div className="resource-empty text-[12px]">{t("resource.usageNoData")}</div>}
            {usage.models.map((bucket) => (
              <div key={bucket.key} className="resource-subpanel flex items-center justify-between gap-3 rounded-md px-3 py-2 text-[12px]">
                <span className="min-w-0 truncate" title={bucket.key}>{bucket.key}</span>
                <span className="resource-muted shrink-0">{formatUsageTokens(bucket.total_tokens, locale)} tokens · {formatUsageCost(bucket.cost_usd, locale, t("resource.unpriced"))}</span>
              </div>
            ))}
          </div>
        </div>
      </section>

      <section className="resource-panel overflow-hidden rounded-lg">
        <div className="resource-divider flex flex-wrap items-baseline justify-between gap-2 border-b px-4 py-3">
          <div>
            <h2 className="resource-strong text-[14px] font-semibold">{t("resource.usageRequestLogTitle")}</h2>
            <p className="resource-muted mt-1 text-[11px]">{t("resource.usageRequestLogDesc", { n: usage.recent_records.length })}</p>
          </div>
          <span className="resource-muted text-[11px]">{t("resource.usagePriced", { n: usage.priced_requests, m: usage.total_requests })}</span>
        </div>
        {usage.recent_records.length === 0 ? (
          <div className="resource-empty px-4 py-10 text-center text-[12px]">{t("resource.usageNoLogs")}</div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full min-w-[860px] text-left text-[11px]">
              <thead className="resource-table-head">
                <tr>
                  <th className="px-4 py-2 font-medium">{t("resource.usageTime")}</th>
                  <th className="px-3 py-2 font-medium">{t("resource.usageProviderModel")}</th>
                  <th className="px-3 py-2 font-medium">{t("resource.usagePurpose")}</th>
                  <th className="px-3 py-2 text-right font-medium">{t("resource.usageInputOutput")}</th>
                  <th className="px-3 py-2 text-right font-medium">{t("resource.usageCacheReadWrite")}</th>
                  <th className="px-3 py-2 text-right font-medium">{t("resource.usageDelay")}</th>
                  <th className="px-4 py-2 text-right font-medium">{t("resource.usageCostStatus")}</th>
                </tr>
              </thead>
              <tbody className="resource-table-body divide-y">
                {usage.recent_records.map((record) => (
                  <tr key={record.id} className="resource-table-row">
                    <td className="resource-muted whitespace-nowrap px-4 py-2.5">{formatUsageTime(record.created_at, locale)}</td>
                    <td className="max-w-[250px] px-3 py-2.5">
                      <div className="resource-strong truncate font-medium" title={record.model}>{record.provider}</div>
                      <div className="resource-muted truncate" title={record.model}>{record.model}</div>
                    </td>
                    <td className="resource-muted max-w-[130px] truncate px-3 py-2.5" title={record.purpose}>{record.purpose}</td>
                    <td className="resource-muted whitespace-nowrap px-3 py-2.5 text-right">{formatUsageTokens(record.input_tokens, locale)} / {formatUsageTokens(record.output_tokens, locale)}</td>
                    <td className="resource-muted whitespace-nowrap px-3 py-2.5 text-right">{formatUsageTokens(record.cache_read_tokens, locale)} / {formatUsageTokens(record.cache_creation_tokens, locale)}</td>
                    <td className="resource-muted whitespace-nowrap px-3 py-2.5 text-right">{record.latency_ms.toLocaleString(locale)} ms</td>
                    <td className="whitespace-nowrap px-4 py-2.5 text-right">
                      <div className="resource-strong font-medium">{formatUsageCost(record.cost_usd, locale, t("resource.unpriced"))}</div>
                      <div className={record.status === "success" ? "resource-success" : record.status === "interrupted" ? "resource-warning" : "resource-danger-text"}>{record.status === "success" ? t("resource.statusSuccess") : record.status === "interrupted" ? t("resource.statusInterrupted") : t("resource.statusFailed")}</div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>

      <section className="resource-panel rounded-lg p-4">
        <h2 className="resource-strong text-[14px] font-semibold">{t("resource.usageMethodTitle")}</h2>
        <p className="resource-muted mt-1 text-[12px] leading-5">{t("resource.usageMethodDesc")}</p>
      </section>
    </div>
  );
}

export default function IntegrationCenter({ onSessionImported }: { onSessionImported?: () => void | Promise<void> }) {
  const { lang, t } = useI18n();
  const [tab, setTab] = useState<Tab>("skills");
  const [snapshot, setSnapshot] = useState<IntegrationSnapshot>(EMPTY_SNAPSHOT);
  const [usage, setUsage] = useState<UsageSummary | null>(null);
  const [market, setMarket] = useState<MarketSearchResult | null>(null);
  const [marketQuery, setMarketQuery] = useState("");
  const [preview, setPreview] = useState<{ path: string; messages: ExternalSessionMessage[] } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const previewCloseRef = useRef<HTMLButtonElement | null>(null);
  const previewPreviousFocusRef = useRef<HTMLElement | null>(null);

  async function refresh() {
    setBusy(true);
    setError("");
    try {
      setSnapshot(await api.integrationScan());
      if (tab === "usage") setUsage(await api.usageSummary());
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  useEffect(() => { void refresh(); }, []);

  async function importSkill(skill: SkillCandidate) {
    setBusy(true); setError("");
    try { await api.integrationImportSkill(skill.path, skill.source); await refresh(); }
    catch (e) { setError(String(e)); setBusy(false); }
  }

  async function pickAndImportSkill() {
    const outcome = await pickFolder(t("resource.chooseFolderPrompt"));
    if (outcome.status !== "selected") {
      if (outcome.status === "failed") setError(outcome.error);
      return;
    }
    setBusy(true); setError("");
    try { await api.integrationImportSkill(outcome.path, "manual"); await refresh(); }
    catch (e) { setError(String(e)); setBusy(false); }
  }

  async function searchMarket() {
    if (!marketQuery.trim()) return;
    setBusy(true); setError("");
    try { setMarket(await api.integrationMarketSearch(marketQuery.trim())); }
    catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  }

  async function importSession(provider: string, path: string) {
    setBusy(true); setError("");
    try { await api.integrationImportSession(provider, path); await onSessionImported?.(); await refresh(); }
    catch (e) { setError(String(e)); setBusy(false); }
  }

  useEffect(() => {
    if (!preview) return;
    previewPreviousFocusRef.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    requestAnimationFrame(() => previewCloseRef.current?.focus());
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setPreview(null);
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("keydown", handleKeyDown);
      previewPreviousFocusRef.current?.focus();
    };
  }, [preview]);

  const managedCount = useMemo(() => snapshot.skills.filter((skill) => skill.managed).length, [snapshot.skills]);

  return (
    <div className="resource-center flex h-full min-h-0 flex-col">
      <header className="resource-header shrink-0 border-b px-4 py-3.5 sm:px-6">
        <div className="resource-center-content mx-auto w-full">
          <div className="flex items-center justify-between gap-3">
            <div className="flex min-w-0 items-center gap-2.5">
              <span className="resource-brand grid size-8 shrink-0 place-items-center rounded-lg"><SparklesIcon size={18} /></span>
              <div className="min-w-0">
                <h1 className="resource-strong truncate text-[16px] font-semibold">{t("resource.title")}</h1>
                <p className="resource-muted truncate text-[12px]">{t("resource.subtitle")}</p>
              </div>
            </div>
            <button type="button" onClick={() => void refresh()} className="resource-control flex items-center gap-1.5 rounded-md px-2.5 py-1.5 text-[12px]" aria-busy={busy}>
              <RotateCwIcon size={14} className={busy ? "animate-spin" : ""} /> {t("resource.refresh")}
            </button>
          </div>
          <div className="resource-tabs mt-3 flex gap-1 rounded-lg p-1" role="tablist" aria-label={t("resource.title")}>
            {([["skills", t("resource.tabSkills", { n: managedCount })], ["sessions", t("resource.tabSessions", { n: snapshot.sessions.length })], ["usage", t("resource.tabUsage")]] as const).map(([value, label]) => (
              <button key={value} type="button" role="tab" aria-selected={tab === value} onClick={() => { setTab(value); if (value === "usage") void api.usageSummary().then(setUsage).catch((e) => setError(String(e))); }} className={`resource-tab flex-1 rounded-md px-3 py-1.5 text-[12px] ${tab === value ? "is-active" : ""}`}>
                {label}
              </button>
            ))}
          </div>
        </div>
      </header>

      <main className="min-h-0 flex-1 overflow-y-auto px-4 py-5 sm:px-6">
        <div className="resource-center-content mx-auto w-full">
          {error && <div className="resource-error mb-4 rounded-lg px-3 py-2 text-[12px]" role="alert">{error}</div>}

          {tab === "skills" && (
            <div className="grid gap-4">
              <section className="resource-subpanel rounded-lg p-4">
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div><h2 className="resource-strong text-[14px] font-semibold">{t("resource.discoveryTitle")}</h2><p className="resource-muted mt-1 text-[12px]">{t("resource.discoveryDesc")}</p></div>
                  <div className="flex gap-2"><button type="button" onClick={() => void pickAndImportSkill()} className="resource-primary rounded-md px-3 py-1.5 text-[12px]">{t("resource.chooseImport")}</button><button type="button" onClick={() => void api.openSkillsDir()} className="resource-control flex items-center gap-1.5 rounded-md px-3 py-1.5 text-[12px]"><FolderIcon size={14} />{t("resource.openManagedDir")}</button></div>
                </div>
              </section>
              <div className="grid gap-3">{snapshot.skills.map((skill) => <SkillRow key={`${skill.source}:${skill.id}`} skill={skill} onImport={(item) => void importSkill(item)} onToggle={(item) => void api.integrationSetSkillEnabled(item.id, !item.enabled).then(refresh).catch((e) => setError(String(e)))} onRemove={(item) => void api.integrationRemoveSkill(item.id).then(refresh).catch((e) => setError(String(e)))} />)}</div>

              <section className="resource-panel rounded-lg p-4">
                <div className="flex flex-wrap items-center justify-between gap-2"><div><h2 className="resource-strong text-[14px] font-semibold">{t("resource.marketTitle")}</h2><p className="resource-muted mt-1 text-[12px]">{t("resource.marketDesc")}</p></div><div className="flex gap-2"><input aria-label={t("resource.marketTitle")} value={marketQuery} onChange={(e) => setMarketQuery(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter") void searchMarket(); }} placeholder={t("resource.marketPlaceholder")} className="resource-input w-44 rounded-md px-2.5 py-1.5 text-[12px] outline-none" /><button type="button" onClick={() => void searchMarket()} className="resource-control rounded-md px-3 py-1.5 text-[12px]">{t("resource.search")}</button></div></div>
                {market && <div className="mt-3 grid gap-2">{market.skills.map((skill) => <div key={skill.key} className="resource-subpanel flex flex-wrap items-center justify-between gap-2 rounded-md px-3 py-2"><div><div className="resource-strong text-[13px] font-medium">{skill.name}</div><div className="resource-muted text-[11px]">{skill.repo_owner}/{skill.repo_name} · {t("resource.installs", { n: skill.installs.toLocaleString(lang === "zh" ? "zh-CN" : "en-US") })}</div></div><button type="button" onClick={() => void api.integrationMarketInstall(skill).then(refresh).catch((e) => setError(String(e)))} className="resource-primary rounded-md px-2.5 py-1 text-[12px]">{t("resource.installManage")}</button></div>)}</div>}
              </section>
            </div>
          )}

          {tab === "sessions" && (
            <div className="grid gap-4">
              <section className="resource-subpanel rounded-lg p-4"><h2 className="resource-strong text-[14px] font-semibold">{t("resource.sessionsTitle")}</h2><p className="resource-muted mt-1 text-[12px] leading-5">{t("resource.sessionsDesc")}</p></section>
              <div className="grid gap-3">{snapshot.sessions.map((session) => <div key={`${session.provider}:${session.source_path}`} className="resource-panel rounded-lg p-4"><div className="flex flex-wrap items-start justify-between gap-3"><div className="min-w-0"><div className="flex flex-wrap items-center gap-2"><span className="resource-strong font-semibold">{session.title}</span><Chip>{session.provider}</Chip><Chip>{t("resource.messageCount", { n: session.message_count })}</Chip></div><p className="resource-muted mt-1 truncate text-[12px]" title={session.source_path}>{session.source_path}</p>{session.project_dir && <p className="resource-muted mt-1 truncate text-[12px]">{t("resource.project", { path: session.project_dir })}</p>}</div><div className="flex gap-1.5"><button type="button" onClick={() => void api.integrationSessionMessages(session.provider, session.source_path).then((messages) => setPreview({ path: session.source_path, messages })).catch((e) => setError(String(e)))} className="resource-control rounded-md px-2.5 py-1 text-[12px]">{t("resource.preview")}</button><button type="button" onClick={() => void importSession(session.provider, session.source_path)} className="resource-primary rounded-md px-2.5 py-1 text-[12px]">{t("resource.importSession")}</button></div></div></div>)}</div>
              {snapshot.configs.length > 0 && <section className="resource-panel rounded-lg p-4"><h2 className="resource-strong text-[14px] font-semibold">{t("resource.configsTitle")}</h2><p className="resource-muted mt-1 text-[12px]">{t("resource.configsDesc")}</p><div className="mt-3 grid gap-2">{snapshot.configs.filter((item) => item.available).map((item) => <div key={item.kind} className="resource-subpanel flex flex-wrap items-center justify-between gap-2 rounded-md px-3 py-2"><div><div className="resource-strong text-[13px] font-medium">{item.provider} · {item.kind}</div><div className="resource-muted text-[11px]">{t("resource.safeKeys", { keys: item.safe_keys.join(", ") || t("resource.none") })}</div></div><button type="button" disabled={!item.safe_keys.length} onClick={() => void api.integrationImportConfig(item.provider, item.kind).then(() => setError(t("resource.safeConfigImported")), (e) => setError(String(e)))} className="resource-control rounded-md px-2.5 py-1 text-[12px] disabled:opacity-40">{t("resource.importSafeConfig")}</button></div>)}</div></section>}
            </div>
          )}

          {tab === "usage" && <UsageDashboard usage={usage} />}

          {preview && <div className="resource-dialog-backdrop fixed inset-0 z-50 grid place-items-center p-4" role="presentation" onMouseDown={() => setPreview(null)}><div className="resource-dialog max-h-[70vh] w-full max-w-[720px] overflow-y-auto rounded-xl p-5 shadow-xl" role="dialog" aria-modal="true" aria-labelledby="session-preview-title" aria-describedby="session-preview-path" tabIndex={-1} onMouseDown={(event) => event.stopPropagation()}><div className="flex items-center justify-between"><h2 id="session-preview-title" className="resource-strong text-[15px] font-semibold">{t("resource.previewTitle")}</h2><button ref={previewCloseRef} type="button" onClick={() => setPreview(null)} className="resource-control rounded-md px-2 py-1 text-[12px]">{t("resource.closePreview")}</button></div><p id="session-preview-path" className="resource-muted mt-1 truncate text-[11px]">{preview.path}</p><div className="mt-4 grid gap-2">{preview.messages.slice(0, 80).map((message, index) => <div key={`${index}:${message.timestamp}`} className="resource-subpanel rounded-md px-3 py-2 text-[12px]"><span className="resource-muted mr-2 font-semibold">{message.role}</span>{message.content}</div>)}</div></div></div>}
        </div>
      </main>
    </div>
  );
}
