import { memo, useEffect, useRef, useState } from "react";
import type { DisplayItem } from "../lib/types";
import { Markdown } from "./Markdown";
import ToolCard from "./ToolCard";
import { CheckIcon, CopyIcon } from "./Icons";

const AVATAR = "/demiurge.png";

function ThinkingDots({ label }: { label: string }) {
  return (
    <span className="inline-flex items-center gap-2 text-[#9a9a9a]">
      <span className="cf-dots text-[#b4b4b4]">
        <span />
        <span />
        <span />
      </span>
      <span className="text-[#8a8a8a]">{label}</span>
    </span>
  );
}

const UserMessage = memo(function UserMessage({ text }: { text: string }) {
  return (
    <article className="cf-message-in flex justify-end">
      <div className="max-w-[78%]">
        <div className="whitespace-pre-wrap rounded-[22px] bg-[#f4f4f4] px-5 py-2.5 text-[15px] leading-[1.6]">
          {text}
        </div>
      </div>
    </article>
  );
});

const AssistantMessage = memo(function AssistantMessage({
  text,
  streaming,
  error,
}: {
  text: string;
  streaming: boolean;
  error?: boolean;
}) {
  const [copied, setCopied] = useState(false);
  async function copy() {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1600);
    } catch {
      /* 忽略剪贴板错误 */
    }
  }

  return (
    <article className="cf-message-in group flex justify-start">
      <img src={AVATAR} alt="AI" className="mr-3 mt-0.5 size-8 shrink-0 rounded-full border border-[#ececec] object-contain bg-[#faf8fd]" />
      <div className="min-w-0 max-w-[82%]">
        <div className="py-0.5 text-[15px] leading-[1.55]">
          {error ? (
            <div className="rounded-2xl border border-[#fde68a] bg-[#fffbeb] px-4 py-3 text-sm text-[#92400e]">{text}</div>
          ) : (
            <Markdown text={text} streaming={streaming} />
          )}
          {!streaming && !error && text && (
            <div className="mt-1.5 flex items-center gap-0.5 text-[#8a8a8a] opacity-0 transition duration-200 group-hover:opacity-100">
              <button
                type="button"
                onClick={copy}
                title="复制"
                className="grid h-8 w-8 place-items-center rounded-lg transition hover:bg-[#f0f0f0] hover:text-[#3f3f3f]"
              >
                {copied ? <CheckIcon size={16} /> : <CopyIcon size={16} />}
              </button>
            </div>
          )}
        </div>
      </div>
    </article>
  );
});

type Props = {
  items: DisplayItem[];
  thinking: boolean;
  greeting: string;
  suggestions: string[];
  onSuggestionClick: (text: string) => void;
};

export function MessageList({ items, thinking, greeting, suggestions, onSuggestionClick }: Props) {
  const bottomRef = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [items, thinking]);

  return (
    <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain [-webkit-overflow-scrolling:touch]">
      <div className="mx-auto flex w-full max-w-3xl flex-col px-4 pb-44 pt-6">
        {items.length === 0 && !thinking ? (
          <div className="cf-message-in flex flex-1 flex-col items-center justify-center pb-24 text-center">
            <img
              src={AVATAR}
              alt="AI"
              className="mb-6 size-14 rounded-full border border-[#ececec] object-contain bg-[#faf8fd] shadow-[0_8px_24px_rgba(0,0,0,0.08)]"
            />
            <h1 className="mb-8 text-[28px] font-semibold tracking-tight text-[#2b2b2b]">{greeting}</h1>
            <div className="grid w-full max-w-2xl grid-cols-1 gap-2.5 sm:grid-cols-2">
              {suggestions.map((item) => (
                <button
                  key={item}
                  onClick={() => onSuggestionClick(item)}
                  className="rounded-2xl border border-[#e8e8e8] px-4 py-3.5 text-left text-sm text-[#3f3f3f] transition hover:border-[#dcdcdc] hover:bg-[#f7f7f7] hover:shadow-[0_4px_14px_rgba(0,0,0,0.04)]"
                >
                  {item}
                </button>
              ))}
            </div>
          </div>
        ) : (
          <div className="space-y-5">
            {items.map((item) =>
              item.kind === "user" ? (
                <UserMessage key={item.id} text={item.text} />
              ) : item.kind === "assistant" ? (
                <AssistantMessage key={item.id} text={item.text} streaming={item.streaming} error={item.error} />
              ) : (
                <ToolCard
                  key={item.id}
                  name={item.name}
                  args={item.args}
                  status={item.status}
                  result={item.result}
                  description={item.description}
                  risk={item.risk}
                />
              ),
            )}
            {thinking && (
              <article className="cf-message-in flex justify-start">
                <img src={AVATAR} alt="AI" className="mr-3 mt-0.5 size-8 shrink-0 rounded-full border border-[#ececec] object-contain bg-[#faf8fd]" />
                <div className="py-1.5">
                  <ThinkingDots label="正在思考…" />
                </div>
              </article>
            )}
          </div>
        )}
        <div ref={bottomRef} />
      </div>
    </div>
  );
}
