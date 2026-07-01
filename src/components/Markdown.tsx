import { Suspense, lazy } from "react";

const MarkdownRenderer = lazy(() => import("./MarkdownRenderer"));

type Props = {
  text: string;
  streaming?: boolean;
};

function MarkdownFallback({ text }: { text: string }) {
  return <div className="whitespace-pre-wrap text-[14px] leading-[1.6] text-[#202124]">{text}</div>;
}

export function Markdown({ text, streaming = false }: Props) {
  return (
    <Suspense fallback={<MarkdownFallback text={text} />}>
      <MarkdownRenderer text={text} streaming={streaming} />
    </Suspense>
  );
}

export default Markdown;
