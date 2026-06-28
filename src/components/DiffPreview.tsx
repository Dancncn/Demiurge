interface DiffPreviewProps {
  text: string;
  maxHeightClass?: string;
}

function lineClass(line: string) {
  if (line.startsWith("---") || line.startsWith("+++")) {
    return "bg-[#f4f6f8] text-[#475467]";
  }
  if (line.startsWith("@@")) {
    return "bg-[#eef4ff] text-[#2559a8]";
  }
  if (line.startsWith("+")) {
    return "bg-[#ecfdf3] text-[#087443]";
  }
  if (line.startsWith("-")) {
    return "bg-[#fff1f3] text-[#b42318]";
  }
  if (line.toLowerCase().startsWith("error:") || line.toLowerCase().includes("failed")) {
    return "bg-[#fff7ed] text-[#b54708]";
  }
  return "text-[#344054]";
}

export default function DiffPreview({ text, maxHeightClass = "max-h-72" }: DiffPreviewProps) {
  const lines = text.split(/\r?\n/);
  const added = lines.filter((line) => line.startsWith("+") && !line.startsWith("+++")).length;
  const removed = lines.filter((line) => line.startsWith("-") && !line.startsWith("---")).length;
  const hunks = lines.filter((line) => line.startsWith("@@")).length;

  return (
    <div className="overflow-hidden rounded-lg border border-[#dfe3e8] bg-white">
      <div className="flex items-center justify-between gap-3 border-b border-[#eceff3] bg-[#fbfcfd] px-3 py-2 text-[11px] text-[#667085]">
        <span className="font-semibold uppercase tracking-wide text-[#475467]">Diff</span>
        <div className="flex shrink-0 items-center gap-2 font-mono tabular-nums">
          <span className="rounded-md bg-[#ecfdf3] px-1.5 py-0.5 text-[#087443]">+{added}</span>
          <span className="rounded-md bg-[#fff1f3] px-1.5 py-0.5 text-[#b42318]">-{removed}</span>
          {hunks > 0 && (
            <span className="rounded-md bg-[#eef4ff] px-1.5 py-0.5 text-[#2559a8]">
              {hunks} {hunks === 1 ? "hunk" : "hunks"}
            </span>
          )}
        </div>
      </div>
      <pre className={`${maxHeightClass} overflow-auto py-2 font-mono text-[12px] leading-5`}>
        {lines.map((line, index) => (
          <div key={`${index}:${line}`} className={`grid grid-cols-[3.25rem_1fr] gap-3 px-3 ${lineClass(line)}`}>
            <span className="select-none text-right text-[#98a2b3]">{index + 1}</span>
            <code className="whitespace-pre-wrap break-words bg-transparent p-0">{line || " "}</code>
          </div>
        ))}
      </pre>
    </div>
  );
}
