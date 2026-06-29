import { memo, useEffect, useRef, useState } from "react";
import type { DisplayItem } from "../lib/types";
import { Markdown } from "./Markdown";
import ToolCard from "./ToolCard";
import { Dashboard } from "./Dashboard";
import { CheckIcon, CopyIcon, RotateCwIcon } from "./Icons";

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
      <div className="max-w-[min(680px,78%)]">
        <div className="whitespace-pre-wrap rounded-lg bg-[#eef1f5] px-4 py-2.5 text-[14px] leading-[1.6] text-[#202124]">
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
  errorTitle,
  errorHint,
  retryText,
  onRetry,
}: {
  text: string;
  streaming: boolean;
  error?: boolean;
  errorTitle?: string;
  errorHint?: string;
  retryText?: string;
  onRetry?: (text: string) => void;
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
      <img src={AVATAR} alt="AI" className="mr-3 mt-0.5 size-7 shrink-0 rounded-md border border-[#dfe3e8] bg-white object-contain" />
      <div className="min-w-0 max-w-[min(900px,82%)]">
        <div className="py-0.5 text-[14px] leading-[1.6]">
          {error ? (
            <div className="rounded-lg border border-[#fde68a] bg-[#fffbeb] px-4 py-3 text-[13px] text-[#92400e]">
              <div className="font-semibold text-[#7a3b00]">{errorTitle || "Request failed"}</div>
              <div className="mt-1 whitespace-pre-wrap">{text}</div>
              {errorHint && <div className="mt-2 text-[#8a5a00]">{errorHint}</div>}
              {retryText && onRetry && (
                <button
                  type="button"
                  onClick={() => onRetry(retryText)}
                  className="mt-3 inline-flex h-8 items-center gap-2 rounded-md border border-[#f2d7a5] bg-white px-2.5 text-xs font-medium text-[#7a3b00] transition hover:bg-[#fff8e8]"
                >
                  <RotateCwIcon size={14} />
                  Retry
                </button>
              )}
            </div>
          ) : (
            <Markdown text={text} streaming={streaming} />
          )}
          {!streaming && !error && text && (
            <div className="mt-1.5 flex items-center gap-0.5 text-[#8a8a8a] opacity-0 transition duration-200 group-hover:opacity-100">
              <button
                type="button"
                onClick={copy}
                title="Copy"
                className="grid h-8 w-8 place-items-center rounded-md transition hover:bg-[#eef1f5] hover:text-[#202124]"
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
  onRetry: (text: string) => void;
};

export function MessageList({ items, thinking, greeting, suggestions, onSuggestionClick, onRetry }: Props) {
  const bottomRef = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth", block: "end" });
  }, [items, thinking]);

  return (
    <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain bg-white [-webkit-overflow-scrolling:touch]">
      <div className="mx-auto flex w-full max-w-3xl flex-col px-4 pb-40 pt-5">
        {items.length === 0 && !thinking ? (
          <div className="cf-message-in">
            <Dashboard greeting={greeting} suggestions={suggestions} onSuggestionClick={onSuggestionClick} />
          </div>
        ) : (
          <div className="space-y-5">
            {items.map((item) =>
              item.kind === "user" ? (
                <UserMessage key={item.id} text={item.text} />
              ) : item.kind === "assistant" ? (
                <AssistantMessage
                  key={item.id}
                  text={item.text}
                  streaming={item.streaming}
                  error={item.error}
                  errorTitle={item.errorTitle}
                  errorHint={item.errorHint}
                  retryText={item.retryText}
                  onRetry={onRetry}
                />
              ) : (
                <ToolCard
                  key={item.id}
                  name={item.name}
                  args={item.args}
                  status={item.status}
                  result={item.result}
                  preview={item.preview}
                  description={item.description}
                  risk={item.risk}
                  duration_ms={item.duration_ms}
                  error_hint={item.error_hint}
                  source_quality={item.source_quality}
                />
              ),
            )}
            {thinking && (
              <article className="cf-message-in flex justify-start">
                <img src={AVATAR} alt="AI" className="mr-3 mt-0.5 size-7 shrink-0 rounded-md border border-[#dfe3e8] bg-white object-contain" />
                <div className="py-1.5">
                  <ThinkingDots label="Thinking..." />
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
