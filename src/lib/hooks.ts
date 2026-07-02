import { useCallback, useEffect, useRef, useState } from "react";
import type { RefObject } from "react";

/** 剪贴板「已复制」状态自动复位的延迟（毫秒）。 */
export const COPY_RESET_MS = 1600;

/**
 * 点击外部关闭菜单 / 弹层的通用 hook。
 *
 * 在 ref 元素外部按下鼠标时调用 onClose；可选监听 Escape 键关闭。
 * 抽取自 App.tsx 中 packMenu / titleMenu / agentMenu / toyMenu 四段逐字重复的 effect。
 *
 * @param ref       需要判定「内部」的容器元素 ref。
 * @param onClose   外点 / Escape 时调用（通常是把 open 状态置回 false / null）。
 * @param opts.escape 是否监听 Escape 键关闭（默认 false，与原先仅 mousedown 的菜单一致）。
 * @param opts.enabled 是否启用监听（默认 true）。对应原先 `if (!open) return` 的早退守卫，
 *                     关闭时不挂监听以避免无谓的全局事件开销。
 */
export function useClickOutside(
  ref: RefObject<HTMLElement | null>,
  onClose: () => void,
  opts?: { escape?: boolean; enabled?: boolean },
) {
  const enabled = opts?.enabled ?? true;
  useEffect(() => {
    if (!enabled) return;
    const onDown = (e: MouseEvent) => {
      if (!ref.current?.contains(e.target as Node)) onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("mousedown", onDown);
    if (opts?.escape) document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [ref, onClose, opts?.escape, enabled]);
}

/**
 * 剪贴板复制 hook：统一 writeText → setCopied(true) → 延时复位 → 静默 catch 的逻辑。
 * 抽取自 MessageList / MarkdownRenderer / MermaidBlock 三处逐字重复的复制实现。
 *
 * @returns copied  是否处于「已复制」高亮态（COPY_RESET_MS 后自动复位）。
 *          copy    复制文本的异步函数，失败时静默忽略。
 */
export function useCopyToClipboard() {
  const [copied, setCopied] = useState(false);
  const timerRef = useRef<number | undefined>(undefined);

  const copy = useCallback(async (text: string) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      if (timerRef.current) window.clearTimeout(timerRef.current);
      timerRef.current = window.setTimeout(() => setCopied(false), COPY_RESET_MS);
    } catch {
      /* 无剪贴板权限等情况下静默失败 */
    }
  }, []);

  // 卸载时清理定时器，避免在已卸载组件上 setState。
  useEffect(() => {
    return () => {
      if (timerRef.current) window.clearTimeout(timerRef.current);
    };
  }, []);

  return { copied, copy } as const;
}
