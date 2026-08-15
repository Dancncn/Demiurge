import { useEffect, useMemo, useState } from "react";
import * as api from "@/lib/api";
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
  return <span className="rounded-full border border-[#e4e7ec] bg-[#f8fafc] px-2 py-0.5 text-[11px] text-[#667085]">{children}</span>;
}

function SkillRow({ skill, onImport, onToggle, onRemove }: {
  skill: SkillCandidate;
  onImport: (skill: SkillCandidate) => void;
  onToggle: (skill: SkillCandidate) => void;
  onRemove: (skill: SkillCandidate) => void;
}) {
  return (
    <div className="rounded-lg border border-[#e6e9ee] bg-white p-4 shadow-[0_1px_3px_rgba(15,23,42,0.04)]">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <span className="font-semibold text-[#202124]">{skill.name}</span>
            <Chip>{skill.source}</Chip>
            {skill.managed && <Chip>{skill.enabled ? "已启用" : "已停用"}</Chip>}
          </div>
          <p className="mt-1 truncate text-[12px] text-[#8a9099]" title={skill.path}>{skill.path}</p>
          {skill.description && <p className="mt-2 text-[13px] leading-5 text-[#475467]">{skill.description}</p>}
        </div>
        <div className="flex shrink-0 flex-wrap justify-end gap-1.5">
          {skill.managed ? (
            <>
              <button className="rounded-md border border-[#dfe3e8] px-2 py-1 text-[12px] hover:bg-[#f6f7f9]" onClick={() => onToggle(skill)}>
                {skill.enabled ? "停用" : "启用"}
              </button>
              <button className="rounded-md border border-[#fecdca] px-2 py-1 text-[12px] text-[#b42318] hover:bg-[#fff5f4]" onClick={() => onRemove(skill)}>
                移除
              </button>
            </>
          ) : (
            <button className="rounded-md bg-[#10a37f] px-2.5 py-1 text-[12px] text-white hover:bg-[#0d8f70]" onClick={() => onImport(skill)}>
              导入管理
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

function formatUsageTokens(value: number) {
  return value.toLocaleString();
}

function formatUsageCost(value: number | null | undefined) {
  return value == null ? "未定价" : `$${value.toFixed(4)}`;
}

function formatUsageTime(timestamp: number) {
  return new Date(timestamp).toLocaleString("zh-CN", {
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function formatUsageDay(key: string) {
  const day = Number(key.replace(/^day-/, ""));
  if (!Number.isFinite(day)) return key;
  return new Date(day * 86_400_000).toLocaleDateString("zh-CN", { month: "2-digit", day: "2-digit" });
}

function UsageMetric({ label, value, hint }: { label: string; value: string; hint?: string }) {
  return (
    <div className="rounded-lg border border-[#e6e9ee] bg-white p-3.5">
      <div className="text-[11px] text-[#8a9099]">{label}</div>
      <div className="mt-1 text-[20px] font-semibold text-[#202124]">{value}</div>
      {hint && <div className="mt-1 truncate text-[10px] text-[#98a2b3]">{hint}</div>}
    </div>
  );
}

function UsageDashboard({ usage }: { usage: UsageSummary | null }) {
  if (!usage) {
    return <div className="rounded-lg border border-dashed border-[#dfe3e8] px-4 py-10 text-center text-[12px] text-[#8a9099]">正在读取使用量…</div>;
  }

  const successRate = usage.total_requests > 0 ? usage.successful_requests / usage.total_requests : 0;
  const daily = usage.daily.slice(-14);
  const maxDailyTokens = Math.max(...daily.map((bucket) => bucket.total_tokens), 1);
  const tokenParts = [
    { label: "输入", value: usage.input_tokens, color: "bg-[#6d5dfc]" },
    { label: "输出", value: usage.output_tokens, color: "bg-[#10a37f]" },
    { label: "缓存读取", value: usage.cache_read_tokens, color: "bg-[#f79009]" },
    { label: "缓存写入", value: usage.cache_creation_tokens, color: "bg-[#98a2b3]" },
  ];
  const maxTokenPart = Math.max(...tokenParts.map((part) => part.value), 1);

  return (
    <div className="grid gap-4">
      <section className="resource-usage-metrics grid gap-3">
        <UsageMetric label="请求" value={usage.total_requests.toLocaleString()} />
        <UsageMetric label="成功率" value={`${(successRate * 100).toFixed(1)}%`} hint={`${usage.failed_requests.toLocaleString()} 次失败 · ${usage.interrupted_requests.toLocaleString()} 次中断`} />
        <UsageMetric label="输入 tokens" value={formatUsageTokens(usage.input_tokens)} />
        <UsageMetric label="输出 tokens" value={formatUsageTokens(usage.output_tokens)} />
        <UsageMetric label="总 tokens" value={formatUsageTokens(usage.total_tokens)} />
        <UsageMetric label="缓存命中率" value={`${(usage.cache_hit_rate * 100).toFixed(1)}%`} hint="按 provider 报告口径" />
        <UsageMetric label="平均延迟" value={`${usage.average_latency_ms.toLocaleString()} ms`} />
        <UsageMetric label="估算成本" value={formatUsageCost(usage.total_cost_usd)} hint={`${usage.unpriced_requests.toLocaleString()} 次未定价`} />
      </section>

      <section className="grid gap-4 lg:grid-cols-[minmax(0,1.35fr)_minmax(260px,0.65fr)]">
        <div className="rounded-lg border border-[#e6e9ee] bg-white p-4">
          <div className="flex flex-wrap items-baseline justify-between gap-2">
            <div>
              <h2 className="text-[14px] font-semibold text-[#202124]">每日用量趋势</h2>
              <p className="mt-1 text-[11px] text-[#8a9099]">最近 {daily.length} 天 · 柱高按总 tokens 归一化</p>
            </div>
            <span className="text-[11px] text-[#667085]">总计 {formatUsageTokens(usage.total_tokens)} tokens</span>
          </div>
          {daily.length === 0 ? (
            <div className="py-14 text-center text-[12px] text-[#98a2b3]">暂无按天数据</div>
          ) : (
            <div className="mt-5 flex h-44 items-end gap-2 overflow-x-auto border-b border-[#eef0f3] pb-2">
              {daily.map((bucket) => (
                <div key={bucket.key} className="flex h-full min-w-[44px] flex-1 flex-col justify-end gap-1">
                  <div className="flex min-h-0 flex-1 items-end justify-center" title={`${bucket.requests} 次 · ${formatUsageTokens(bucket.total_tokens)} tokens`}>
                    <div className="w-full max-w-[28px] rounded-t bg-[#6d5dfc] transition-all" style={{ height: `${Math.max(8, (bucket.total_tokens / maxDailyTokens) * 100)}%` }} />
                  </div>
                  <span className="truncate text-center text-[10px] text-[#98a2b3]">{formatUsageDay(bucket.key)}</span>
                  <span className="text-center text-[10px] text-[#667085]">{bucket.requests} 次</span>
                </div>
              ))}
            </div>
          )}
        </div>

        <div className="rounded-lg border border-[#e6e9ee] bg-white p-4">
          <h2 className="text-[14px] font-semibold text-[#202124]">Token 与缓存构成</h2>
          <p className="mt-1 text-[11px] text-[#8a9099]">缓存字段可能包含在输入 tokens 内，不能直接相加。</p>
          <div className="mt-5 grid gap-3">
            {tokenParts.map((part) => (
              <div key={part.label}>
                <div className="mb-1 flex items-center justify-between gap-2 text-[12px]">
                  <span className="text-[#475467]">{part.label}</span>
                  <span className="font-medium text-[#202124]">{formatUsageTokens(part.value)}</span>
                </div>
                <div className="h-2 overflow-hidden rounded-full bg-[#eef1f5]">
                  <div className={`h-full rounded-full ${part.color}`} style={{ width: `${Math.max(part.value > 0 ? 3 : 0, (part.value / maxTokenPart) * 100)}%` }} />
                </div>
              </div>
            ))}
          </div>
          <div className="mt-5 grid grid-cols-2 gap-2 text-[11px] text-[#667085]">
            <div className="rounded-md bg-[#f9fafb] px-2.5 py-2">缓存读取<br /><span className="font-semibold text-[#202124]">{formatUsageTokens(usage.cache_read_tokens)}</span></div>
            <div className="rounded-md bg-[#f9fafb] px-2.5 py-2">缓存写入<br /><span className="font-semibold text-[#202124]">{formatUsageTokens(usage.cache_creation_tokens)}</span></div>
          </div>
        </div>
      </section>

      <section className="grid gap-4 sm:grid-cols-2">
        <div className="rounded-lg border border-[#e6e9ee] bg-white p-4">
          <h2 className="text-[14px] font-semibold text-[#202124]">Provider 分布</h2>
          <div className="mt-3 grid gap-2">
            {usage.providers.length === 0 && <div className="text-[12px] text-[#98a2b3]">暂无数据</div>}
            {usage.providers.map((bucket) => (
              <div key={bucket.key} className="flex items-center justify-between gap-3 rounded-md bg-[#f9fafb] px-3 py-2 text-[12px]">
                <span className="truncate">{bucket.key}</span>
                <span className="shrink-0 text-[#667085]">{bucket.requests} 次 · {formatUsageTokens(bucket.total_tokens)} · {formatUsageCost(bucket.cost_usd)}</span>
              </div>
            ))}
          </div>
        </div>
        <div className="rounded-lg border border-[#e6e9ee] bg-white p-4">
          <h2 className="text-[14px] font-semibold text-[#202124]">模型分布</h2>
          <div className="mt-3 grid gap-2">
            {usage.models.length === 0 && <div className="text-[12px] text-[#98a2b3]">暂无数据</div>}
            {usage.models.map((bucket) => (
              <div key={bucket.key} className="flex items-center justify-between gap-3 rounded-md bg-[#f9fafb] px-3 py-2 text-[12px]">
                <span className="min-w-0 truncate" title={bucket.key}>{bucket.key}</span>
                <span className="shrink-0 text-[#667085]">{formatUsageTokens(bucket.total_tokens)} tokens · {formatUsageCost(bucket.cost_usd)}</span>
              </div>
            ))}
          </div>
        </div>
      </section>

      <section className="overflow-hidden rounded-lg border border-[#e6e9ee] bg-white">
        <div className="flex flex-wrap items-baseline justify-between gap-2 border-b border-[#eceff3] px-4 py-3">
          <div>
            <h2 className="text-[14px] font-semibold text-[#202124]">请求日志</h2>
            <p className="mt-1 text-[11px] text-[#8a9099]">最近 {usage.recent_records.length} 条 · 可由 usage.jsonl 重建</p>
          </div>
          <span className="text-[11px] text-[#667085]">已定价 {usage.priced_requests} / {usage.total_requests}</span>
        </div>
        {usage.recent_records.length === 0 ? (
          <div className="px-4 py-10 text-center text-[12px] text-[#98a2b3]">暂无请求日志</div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full min-w-[860px] text-left text-[11px]">
              <thead className="bg-[#f8f9fb] text-[#667085]">
                <tr>
                  <th className="px-4 py-2 font-medium">时间</th>
                  <th className="px-3 py-2 font-medium">Provider / 模型</th>
                  <th className="px-3 py-2 font-medium">用途</th>
                  <th className="px-3 py-2 text-right font-medium">输入 / 输出</th>
                  <th className="px-3 py-2 text-right font-medium">缓存读 / 写</th>
                  <th className="px-3 py-2 text-right font-medium">延迟</th>
                  <th className="px-4 py-2 text-right font-medium">成本 / 状态</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[#f0f2f5]">
                {usage.recent_records.map((record) => (
                  <tr key={record.id} className="hover:bg-[#fbfcfd]">
                    <td className="whitespace-nowrap px-4 py-2.5 text-[#667085]">{formatUsageTime(record.created_at)}</td>
                    <td className="max-w-[250px] px-3 py-2.5">
                      <div className="truncate font-medium text-[#202124]" title={record.model}>{record.provider}</div>
                      <div className="truncate text-[#8a9099]" title={record.model}>{record.model}</div>
                    </td>
                    <td className="max-w-[130px] truncate px-3 py-2.5 text-[#667085]" title={record.purpose}>{record.purpose}</td>
                    <td className="whitespace-nowrap px-3 py-2.5 text-right text-[#475467]">{formatUsageTokens(record.input_tokens)} / {formatUsageTokens(record.output_tokens)}</td>
                    <td className="whitespace-nowrap px-3 py-2.5 text-right text-[#475467]">{formatUsageTokens(record.cache_read_tokens)} / {formatUsageTokens(record.cache_creation_tokens)}</td>
                    <td className="whitespace-nowrap px-3 py-2.5 text-right text-[#667085]">{record.latency_ms.toLocaleString()} ms</td>
                    <td className="whitespace-nowrap px-4 py-2.5 text-right">
                      <div className="font-medium text-[#202124]">{formatUsageCost(record.cost_usd)}</div>
                      <div className={record.status === "success" ? "text-[#039855]" : "text-[#b42318]"}>{record.status}</div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </section>

      <section className="rounded-lg border border-[#e6e9ee] bg-white p-4">
        <h2 className="text-[14px] font-semibold text-[#202124]">统计口径</h2>
        <p className="mt-1 text-[12px] leading-5 text-[#667085]">统计来自追加式 usage.jsonl。OpenRouter 刷新的目录会提供上下文与 prompt/completion/cache 价格；只有匹配到目录的请求才显示成本，其余请求明确标为未定价。价格是本地估算，不替代 provider 账单。</p>
      </section>
    </div>
  );
}

export default function IntegrationCenter({ onSessionImported }: { onSessionImported?: () => void | Promise<void> }) {
  const [tab, setTab] = useState<Tab>("skills");
  const [snapshot, setSnapshot] = useState<IntegrationSnapshot>(EMPTY_SNAPSHOT);
  const [usage, setUsage] = useState<UsageSummary | null>(null);
  const [market, setMarket] = useState<MarketSearchResult | null>(null);
  const [marketQuery, setMarketQuery] = useState("");
  const [preview, setPreview] = useState<{ path: string; messages: ExternalSessionMessage[] } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

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
    const outcome = await pickFolder("选择包含 SKILL.md 的 Skill 目录");
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
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setPreview(null);
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [preview]);

  const managedCount = useMemo(() => snapshot.skills.filter((skill) => skill.managed).length, [snapshot.skills]);

  return (
    <div className="resource-center flex h-full min-h-0 flex-col bg-white">
      <header className="shrink-0 border-b border-[#eceff3] bg-[#fbfcfd] px-4 py-3.5 sm:px-6">
        <div className="resource-center-content mx-auto w-full max-w-[1120px]">
          <div className="flex items-center justify-between gap-3">
            <div className="flex min-w-0 items-center gap-2.5">
              <span className="grid size-8 shrink-0 place-items-center rounded-lg bg-[#f4f3ff] text-[#5b4ad1]"><SparklesIcon size={18} /></span>
              <div className="min-w-0">
                <h1 className="truncate text-[16px] font-semibold text-[#202124]">资源中心</h1>
                <p className="truncate text-[12px] text-[#8a9099]">导入和管理 Skill、Codex / Claude Code 会话，以及模型使用量</p>
              </div>
            </div>
            <button onClick={() => void refresh()} className="flex items-center gap-1.5 rounded-md border border-[#dfe3e8] bg-white px-2.5 py-1.5 text-[12px] text-[#3f4652] hover:bg-[#f6f7f9]">
              <RotateCwIcon size={14} className={busy ? "animate-spin" : ""} /> 刷新发现
            </button>
          </div>
          <div className="mt-3 flex gap-1 rounded-lg bg-[#eef1f5] p-1">
            {([["skills", `Skill 管理 (${managedCount})`], ["sessions", `外部会话 (${snapshot.sessions.length})`], ["usage", "使用统计"]] as const).map(([value, label]) => (
              <button key={value} onClick={() => { setTab(value); if (value === "usage") void api.usageSummary().then(setUsage).catch((e) => setError(String(e))); }} className={`flex-1 rounded-md px-3 py-1.5 text-[12px] ${tab === value ? "bg-white font-semibold text-[#202124] shadow-sm" : "text-[#667085]"}`}>
                {label}
              </button>
            ))}
          </div>
        </div>
      </header>

      <main className="min-h-0 flex-1 overflow-y-auto px-4 py-5 sm:px-6">
        <div className="mx-auto w-full max-w-[1120px]">
          {error && <div className="mb-4 rounded-lg border border-[#fecdca] bg-[#fff5f4] px-3 py-2 text-[12px] text-[#b42318]">{error}</div>}

          {tab === "skills" && (
            <div className="grid gap-4">
              <section className="rounded-lg border border-[#e6e9ee] bg-[#f9fafb] p-4">
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <div><h2 className="text-[14px] font-semibold text-[#202124]">发现来源</h2><p className="mt-1 text-[12px] text-[#667085]">自动扫描 Codex、Claude Code、统一 .agents 目录和当前项目；导入前不会执行 Skill。</p></div>
                  <div className="flex gap-2"><button onClick={() => void pickAndImportSkill()} className="rounded-md bg-[#10a37f] px-3 py-1.5 text-[12px] text-white hover:bg-[#0d8f70]">选择目录导入</button><button onClick={() => void api.openSkillsDir()} className="flex items-center gap-1.5 rounded-md border border-[#dfe3e8] bg-white px-3 py-1.5 text-[12px] hover:bg-[#f6f7f9]"><FolderIcon size={14} />打开管理目录</button></div>
                </div>
              </section>
              <div className="grid gap-3">{snapshot.skills.map((skill) => <SkillRow key={`${skill.source}:${skill.id}`} skill={skill} onImport={(item) => void importSkill(item)} onToggle={(item) => void api.integrationSetSkillEnabled(item.id, !item.enabled).then(refresh).catch((e) => setError(String(e)))} onRemove={(item) => void api.integrationRemoveSkill(item.id).then(refresh).catch((e) => setError(String(e)))} />)}</div>

              <section className="rounded-lg border border-[#e6e9ee] bg-white p-4">
                <div className="flex flex-wrap items-center justify-between gap-2"><div><h2 className="text-[14px] font-semibold text-[#202124]">Skill 市场</h2><p className="mt-1 text-[12px] text-[#667085]">从 skills.sh 搜索公开 Skill；下载后仍要经过本地目录校验和显式导入。</p></div><div className="flex gap-2"><input value={marketQuery} onChange={(e) => setMarketQuery(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter") void searchMarket(); }} placeholder="例如：pdf、research" className="w-44 rounded-md border border-[#dfe3e8] px-2.5 py-1.5 text-[12px] outline-none" /><button onClick={() => void searchMarket()} className="rounded-md border border-[#dfe3e8] px-3 py-1.5 text-[12px] hover:bg-[#f6f7f9]">搜索</button></div></div>
                {market && <div className="mt-3 grid gap-2">{market.skills.map((skill) => <div key={skill.key} className="flex flex-wrap items-center justify-between gap-2 rounded-md border border-[#eef0f3] px-3 py-2"><div><div className="text-[13px] font-medium text-[#202124]">{skill.name}</div><div className="text-[11px] text-[#8a9099]">{skill.repo_owner}/{skill.repo_name} · {skill.installs} installs</div></div><button onClick={() => void api.integrationMarketInstall(skill).then(refresh).catch((e) => setError(String(e)))} className="rounded-md bg-[#eef2ff] px-2.5 py-1 text-[12px] text-[#4338ca] hover:bg-[#e0e7ff]">安装并管理</button></div>)}</div>}
              </section>
            </div>
          )}

          {tab === "sessions" && <div className="grid gap-4"><section className="rounded-lg border border-[#e6e9ee] bg-[#f9fafb] p-4"><h2 className="text-[14px] font-semibold text-[#202124]">Codex / Claude Code 会话</h2><p className="mt-1 text-[12px] leading-5 text-[#667085]">只读发现本地 JSONL，会话导入会创建新的 Demiurge 会话并追加到现有会话存储，不会删除或修改原文件。</p></section><div className="grid gap-3">{snapshot.sessions.map((session) => <div key={`${session.provider}:${session.source_path}`} className="rounded-lg border border-[#e6e9ee] bg-white p-4"><div className="flex flex-wrap items-start justify-between gap-3"><div className="min-w-0"><div className="flex flex-wrap items-center gap-2"><span className="font-semibold text-[#202124]">{session.title}</span><Chip>{session.provider}</Chip><Chip>{session.message_count} 条消息</Chip></div><p className="mt-1 truncate text-[12px] text-[#8a9099]" title={session.source_path}>{session.source_path}</p>{session.project_dir && <p className="mt-1 truncate text-[12px] text-[#667085]">项目：{session.project_dir}</p>}</div><div className="flex gap-1.5"><button onClick={() => void api.integrationSessionMessages(session.provider, session.source_path).then((messages) => setPreview({ path: session.source_path, messages })).catch((e) => setError(String(e)))} className="rounded-md border border-[#dfe3e8] px-2.5 py-1 text-[12px] hover:bg-[#f6f7f9]">预览</button><button onClick={() => void importSession(session.provider, session.source_path)} className="rounded-md bg-[#10a37f] px-2.5 py-1 text-[12px] text-white hover:bg-[#0d8f70]">导入会话</button></div></div></div>)}</div>{snapshot.configs.length > 0 && <section className="rounded-lg border border-[#e6e9ee] bg-white p-4"><h2 className="text-[14px] font-semibold text-[#202124]">发现的会话配置</h2><p className="mt-1 text-[12px] text-[#667085]">仅导入安全偏好字段；API key、hooks 和权限设置永不复制。</p><div className="mt-3 grid gap-2">{snapshot.configs.filter((item) => item.available).map((item) => <div key={item.kind} className="flex flex-wrap items-center justify-between gap-2 rounded-md border border-[#eef0f3] px-3 py-2"><div><div className="text-[13px] font-medium text-[#202124]">{item.provider} · {item.kind}</div><div className="text-[11px] text-[#8a9099]">安全字段：{item.safe_keys.join(", ") || "无"}</div></div><button disabled={!item.safe_keys.length} onClick={() => void api.integrationImportConfig(item.provider, item.kind).then(() => setError("已导入安全配置快照；未覆盖当前凭据和权限设置。"), (e) => setError(String(e)))} className="rounded-md border border-[#dfe3e8] px-2.5 py-1 text-[12px] disabled:opacity-40">导入安全配置</button></div>)}</div></section>}</div>}

          {tab === "usage" && <UsageDashboard usage={usage} />}

          {preview && <div className="fixed inset-0 z-50 grid place-items-center bg-black/20 p-4" role="presentation" onClick={() => setPreview(null)}><div className="max-h-[70vh] w-full max-w-[720px] overflow-y-auto rounded-xl bg-white p-5 shadow-xl" role="dialog" aria-modal="true" aria-labelledby="session-preview-title" onClick={(event) => event.stopPropagation()}><div className="flex items-center justify-between"><h2 id="session-preview-title" className="text-[15px] font-semibold">会话预览</h2><button onClick={() => setPreview(null)} className="text-[12px] text-[#667085]">关闭</button></div><p className="mt-1 truncate text-[11px] text-[#8a9099]">{preview.path}</p><div className="mt-4 grid gap-2">{preview.messages.slice(0, 80).map((message, index) => <div key={`${index}:${message.timestamp}`} className="rounded-md bg-[#f9fafb] px-3 py-2 text-[12px]"><span className="mr-2 font-semibold text-[#667085]">{message.role}</span>{message.content}</div>)}</div></div></div>}
        </div>
      </main>
    </div>
  );
}
