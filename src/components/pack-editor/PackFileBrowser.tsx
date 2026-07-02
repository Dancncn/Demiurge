import { useEffect, useState } from "react";
import { listPackFiles, readPackFile } from "../../lib/api";
import type { PackFileContent, PackFileEntry } from "../../lib/types";
import { useI18n } from "../../lib/i18n";
import { secondaryButtonCls } from "../../lib/ui";
import { FolderIcon, FileIcon } from "../Icons";

// 包内文件浏览器：列出当前目录、面包屑导航、文件预览（文本/图片）。
export function PackFileBrowser({ packId }: { packId: string }) {
  const { t } = useI18n();
  const [subDir, setSubDir] = useState<string>("");
  const [entries, setEntries] = useState<PackFileEntry[]>([]);
  const [selected, setSelected] = useState<PackFileContent | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    if (!packId) {
      setEntries([]);
      return;
    }
    setLoading(true);
    setError("");
    setSelected(null);
    listPackFiles(packId, subDir || undefined)
      .then(setEntries)
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }, [packId, subDir]);

  async function openEntry(entry: PackFileEntry) {
    if (entry.is_dir) {
      setSubDir(entry.path);
      return;
    }
    setError("");
    try {
      const content = await readPackFile(packId, entry.path);
      setSelected(content);
    } catch (e) {
      setError(String(e));
    }
  }

  const segments = subDir ? subDir.split("/") : [];

  function gotoSegment(index: number) {
    setSubDir(index < 0 ? "" : segments.slice(0, index + 1).join("/"));
  }

  if (!packId) return null;

  return (
    <div className="rounded-lg border border-[#e2e5ea] bg-white p-3">
      <div className="mb-2 flex flex-wrap items-center gap-1 text-[12px] text-[#5f6368]">
        <button
          type="button"
          className="font-medium hover:text-[#202124]"
          onClick={() => setSubDir("")}
        >
          {t("settings.card.browser.root")}
        </button>
        {segments.map((seg, idx) => (
          <span key={idx} className="flex items-center gap-1">
            <span className="text-[#bcc2cb]">/</span>
            <button
              type="button"
              className={`hover:text-[#202124] ${idx === segments.length - 1 ? "font-medium text-[#202124]" : ""}`}
              onClick={() => gotoSegment(idx)}
            >
              {seg}
            </button>
          </span>
        ))}
        <button
          type="button"
          className={`ml-auto ${secondaryButtonCls}`}
          disabled={loading}
          onClick={() => setSubDir((prev) => prev)}
        >
          {loading ? t("settings.card.browser.loading") : t("settings.card.browser.refresh")}
        </button>
      </div>

      {error && (
        <div className="mb-2 rounded-md border border-[#f3c3c3] bg-[#fff7f7] px-3 py-2 text-[12px] text-[#b42318]">
          {error}
        </div>
      )}

      <div className="grid gap-3 sm:grid-cols-2">
        <div className="max-h-[280px] overflow-auto rounded-md border border-[#eceff3] bg-white">
          {entries.length === 0 ? (
            <div className="px-3 py-2 text-[12px] text-[#7a8088]">
              {t("settings.card.browser.empty")}
            </div>
          ) : (
            entries.map((entry) => (
              <button
                key={entry.path}
                type="button"
                className="flex w-full items-center gap-2 border-b border-[#f3f4f7] px-3 py-1.5 text-left text-[12px] text-[#3f4650] last:border-b-0 hover:bg-[#f8f9fb]"
                onClick={() => openEntry(entry)}
              >
                {entry.is_dir ? (
                  <FolderIcon size={14} className="shrink-0 text-[#6b7280]" />
                ) : (
                  <FileIcon size={14} className="shrink-0 text-[#9aa1ab]" />
                )}
                <span className="min-w-0 flex-1 truncate">
                  {entry.path.split("/").pop()}
                </span>
                {!entry.is_dir && (
                  <span className="shrink-0 text-[11px] text-[#9aa1ab]">
                    {entry.size.toLocaleString()} B
                  </span>
                )}
              </button>
            ))
          )}
        </div>

        <div className="max-h-[280px] overflow-auto rounded-md border border-[#eceff3] bg-[#fbfcfd] p-2">
          {selected ? (
            selected.data_url ? (
              <img
                src={selected.data_url}
                alt={selected.path}
                className="max-h-[260px] max-w-full rounded"
              />
            ) : selected.text ? (
              <pre className="whitespace-pre-wrap break-words text-[11px] leading-5 text-[#3f4650]">
                {selected.text}
                {selected.truncated ? `\n… (${t("settings.card.browser.truncated")})` : ""}
              </pre>
            ) : (
              <div className="px-2 py-2 text-[12px] text-[#7a8088]">
                {selected.path} — {t("settings.card.browser.binary")}
              </div>
            )
          ) : (
            <div className="px-2 py-2 text-[12px] text-[#7a8088]">
              {t("settings.card.browser.selectHint")}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
