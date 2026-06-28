import { ComponentPropsWithoutRef, ReactNode, isValidElement, useState } from "react";
import ReactMarkdown from "react-markdown";
import rehypeKatex from "rehype-katex";
import rehypeHighlight from "rehype-highlight";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import { CheckIcon, CopyIcon } from "./Icons";
import { MermaidBlock } from "./MermaidBlock";

function extractText(node: ReactNode): string {
  if (typeof node === "string" || typeof node === "number") return String(node);
  if (Array.isArray(node)) return node.map(extractText).join("");
  if (node && typeof node === "object" && "props" in node) {
    const props = (node as { props?: { children?: ReactNode } }).props;
    return extractText(props?.children);
  }
  return "";
}

// 代码块外框渲染在 <pre> 这一层：被解析为「块级代码」就套同一个框（与是否有语言标记无关），
// 行内代码保持行内样式。这样流式过程中代码不会在「行内 / 块级」之间反复横跳。
function CodeBlock({ children }: ComponentPropsWithoutRef<"pre">) {
  const [copied, setCopied] = useState(false);
  const codeElement = Array.isArray(children) ? children[0] : children;
  const codeProps = isValidElement(codeElement)
    ? (codeElement.props as { className?: string; children?: ReactNode })
    : null;
  const codeClassName = codeProps?.className ?? "";
  const codeChildren = codeProps?.children ?? children;
  const match = /language-(\w+)/.exec(codeClassName);
  const label = match ? match[1] : "code";
  const code = extractText(codeChildren).replace(/\n$/, "");

  if (label.toLowerCase() === "mermaid") {
    return <MermaidBlock chart={code} />;
  }

  async function copyCode() {
    try {
      await navigator.clipboard.writeText(code);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1600);
    } catch {
      // 无剪贴板权限等情况下静默失败。
    }
  }

  return (
    <div className="code-block group/code relative my-3 overflow-hidden rounded-xl border border-[#e8e8e8] bg-[#f7f7f7] md:-mx-3">
      <div className="flex items-center justify-between border-b border-[#ececec] bg-[#f3f3f3] px-3.5 py-2 text-xs text-[#6f6f6f]">
        <span className="font-medium tracking-wide">{label}</span>
        <button
          type="button"
          onClick={copyCode}
          className="flex items-center gap-1.5 rounded-md px-2 py-1 text-[#5f5f5f] transition hover:bg-[#e6e6e6] hover:text-[#171717]"
        >
          {copied ? <CheckIcon size={14} /> : <CopyIcon size={14} />}
          {copied ? "已复制" : "复制"}
        </button>
      </div>
      <pre className="!m-0 !rounded-none !bg-transparent">
        <code className={codeClassName}>{codeChildren}</code>
      </pre>
    </div>
  );
}

// 流式时若代码围栏(```)为奇数，临时补一个闭合围栏，避免解析器在「代码块/普通文本」间横跳闪烁。
function closeUnclosedFence(content: string): string {
  const fenceCount = (content.match(/```/g) || []).length;
  return fenceCount % 2 === 1 ? content + "\n```" : content;
}

// 数学定界符归一：把 \[ \] \( \) 转成 $$ / $，并保护代码块不被误伤。
function normalizeMath(content: string): string {
  const protectedBlocks: string[] = [];
  const prefix = "__DEMIURGE_CODE_BLOCK_";
  const withoutCode = content.replace(/```[\s\S]*?```/g, (block) => {
    const index = protectedBlocks.push(block) - 1;
    return `${prefix}${index}__`;
  });
  const normalized = withoutCode
    .replace(/\\\[/g, "$$")
    .replace(/\\\]/g, "$$")
    .replace(/\\\(/g, "$")
    .replace(/\\\)/g, "$");
  return normalized.replace(new RegExp(`${prefix}(\\d+)__`, "g"), (_, i) => protectedBlocks[Number(i)] || "");
}

export function Markdown({ text, streaming = false }: { text: string; streaming?: boolean }) {
  const prepared = normalizeMath(streaming ? closeUnclosedFence(text) : text);
  return (
    <div className={`markdown-body${streaming ? " is-streaming" : ""}`}>
      <ReactMarkdown
        remarkPlugins={[remarkGfm, remarkMath]}
        // detect:false —— 只高亮带语言标记的代码块，规避流式时自动探测语言来回切换导致的变色闪烁。
        rehypePlugins={[[rehypeHighlight, { detect: false, ignoreMissing: true }], rehypeKatex]}
        components={{
          pre: CodeBlock,
          a: ({ href, children }) => (
            <a href={href} target="_blank" rel="noreferrer">
              {children}
            </a>
          ),
        }}
      >
        {prepared}
      </ReactMarkdown>
    </div>
  );
}

export default Markdown;
