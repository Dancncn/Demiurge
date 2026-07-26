import { Suspense, lazy, memo } from "react";

const MarkdownRenderer = lazy(() => import("@/shared/components/MarkdownRenderer"));

type Props = {
  text: string;
  streaming?: boolean;
};

function MarkdownFallback({ text }: { text: string }) {
  return <div className="whitespace-pre-wrap text-[14px] leading-[1.6] text-[#202124]">{text}</div>;
}

function MarkdownView({ text, streaming = false }: Props) {
  return (
    <Suspense fallback={<MarkdownFallback text={text} />}>
      <MarkdownRenderer text={text} streaming={streaming} />
    </Suspense>
  );
}

export const Markdown = memo(MarkdownView);

export default Markdown;
