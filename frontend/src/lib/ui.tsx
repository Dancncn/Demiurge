// 共享表单控件 CSS 类常量。与 SettingsDialog 用法保持一致，供 pack-editor 等独立组件复用。
import type { ReactNode } from "react";

export const inputCls =
  "h-9 w-full rounded-md border border-[#d9d9d9] bg-white px-3 text-[13px] text-[#202124] outline-none transition focus:border-[#7a7f87] focus:ring-1 focus:ring-[#202124]/10";

export const labelCls = "mb-1.5 block text-[12px] font-medium text-[#5f6368]";

export const secondaryButtonCls =
  "cf-press inline-flex h-8 items-center justify-center rounded-md border border-[#d9d9d9] bg-white px-3 text-[12px] font-medium text-[#333] hover:bg-[#f5f5f5] disabled:cursor-not-allowed disabled:opacity-50";

export const dangerButtonCls =
  "cf-press inline-flex h-8 items-center justify-center rounded-md border border-[#f3c3c3] bg-white px-3 text-[12px] font-medium text-[#b42318] hover:bg-[#fff7f7] disabled:cursor-not-allowed disabled:opacity-50";

export const textareaCls =
  "min-h-[96px] w-full resize-y rounded-md border border-[#d9d9d9] bg-white px-3 py-2 text-[13px] leading-5 text-[#202124] outline-none transition focus:border-[#7a7f87] focus:ring-1 focus:ring-[#202124]/10";

export const monoTextareaCls =
  "min-h-[260px] w-full resize-y rounded-md border border-[#d9d9d9] bg-[#fbfcfd] px-3 py-2 font-mono text-[12px] leading-5 text-[#202124] outline-none transition focus:border-[#7a7f87] focus:ring-1 focus:ring-[#202124]/10";

export const subSectionCls =
  "rounded-lg border border-[#e2e5ea] bg-[#fbfcfd] p-3";

export const subSectionTitleCls =
  "mb-2 text-[12px] font-semibold text-[#202124]";

export const chipCls =
  "inline-flex items-center gap-1 rounded bg-[#eef2ff] px-1.5 py-0.5 text-[11px] text-[#3730a3]";

// 通用 Section / Field 轻量封装，供 pack-editor 等独立组件在不依赖 SettingsDialog 内部组件的情况下复用同一视觉风格。
export function Field({
  label,
  help,
  children,
}: {
  label: string;
  help?: string;
  children: ReactNode;
}) {
  return (
    <label className="block">
      <span className={labelCls}>{label}</span>
      {children}
      {help && <span className="mt-1.5 block text-[12px] leading-5 text-[#8a9099]">{help}</span>}
    </label>
  );
}
