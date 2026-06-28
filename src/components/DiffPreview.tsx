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
  return (
    <pre
      className={`${maxHeightClass} overflow-auto rounded-lg border border-[#dfe3e8] bg-white py-2 font-mono text-[12px] leading-5`}
    >
      {lines.map((line, index) => (
        <div key={`${index}:${line}`} className={`grid grid-cols-[3.25rem_1fr] gap-3 px-3 ${lineClass(line)}`}>
          <span className="select-none text-right text-[#98a2b3]">{index + 1}</span>
          <code className="whitespace-pre-wrap break-words bg-transparent p-0">{line || " "}</code>
        </div>
      ))}
    </pre>
  );
}
