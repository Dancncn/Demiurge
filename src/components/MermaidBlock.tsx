import { useEffect, useRef, useState } from "react";
import { CheckIcon, CopyIcon } from "./Icons";

type MermaidModule = typeof import("mermaid").default;

let mermaidPromise: Promise<MermaidModule> | null = null;

function loadMermaid() {
  if (!mermaidPromise) {
    mermaidPromise = import("mermaid").then((module) => {
      const mermaid = module.default;
      mermaid.initialize({
        startOnLoad: false,
        securityLevel: "strict",
        theme: "default",
      });
      return mermaid;
    });
  }
  return mermaidPromise;
}

export function MermaidBlock({ chart }: { chart: string }) {
  const idRef = useRef(`mermaid-${Math.random().toString(36).slice(2)}`);
  const [svg, setSvg] = useState("");
  const [error, setError] = useState("");
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    let disposed = false;
    setError("");
    setSvg("");

    void loadMermaid()
      .then(async (mermaid) => {
        await mermaid.parse(chart);
        const rendered = await mermaid.render(idRef.current, chart);
        if (!disposed) setSvg(rendered.svg.replace(/translate\(undefined,\s*NaN\)/g, "translate(0, 0)"));
      })
      .catch((err) => {
        if (!disposed) setError(err instanceof Error ? err.message : String(err));
      });

    return () => {
      disposed = true;
    };
  }, [chart]);

  async function copySource() {
    try {
      await navigator.clipboard.writeText(chart);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1600);
    } catch {
      // Ignore clipboard errors.
    }
  }

  return (
    <div className="code-block group/code relative my-3 overflow-hidden rounded-xl border border-[#dfe3e8] bg-[#fbfcfd] md:-mx-3">
      <div className="flex items-center justify-between border-b border-[#eceff3] bg-[#f3f5f7] px-3.5 py-2 text-xs text-[#6f7782]">
        <span className="font-medium tracking-wide">mermaid</span>
        <button
          type="button"
          onClick={copySource}
          className="flex items-center gap-1.5 rounded-md px-2 py-1 text-[#5f6672] transition hover:bg-[#e6eaf0] hover:text-[#202124]"
        >
          {copied ? <CheckIcon size={14} /> : <CopyIcon size={14} />}
          {copied ? "Copied" : "Copy"}
        </button>
      </div>
      <div className="overflow-auto p-4">
        {error ? (
          <pre className="!m-0 whitespace-pre-wrap rounded-md border border-[#fde68a] bg-[#fffbeb] p-3 text-[12px] text-[#92400e]">
            {error}
          </pre>
        ) : svg ? (
          <div className="mermaid-render mx-auto min-w-max" dangerouslySetInnerHTML={{ __html: svg }} />
        ) : (
          <div className="text-[12px] text-[#7a8088]">Rendering diagram...</div>
        )}
      </div>
    </div>
  );
}
