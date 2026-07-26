import { open } from "@tauri-apps/plugin-dialog";

/**
 * 原生文件夹选择的统一入口。
 *
 * 这里刻意用静态 import 而不是调用时 `await import(...)`：动态 import 会把插件
 * 拆成独立 chunk，chunk 请求失败时 open() 根本没被调用，用户看到的就是"点了没
 * 反应"。插件模块本身在非桌面环境下也能安全求值（只在真正调用时才走 IPC），
 * 所以静态引入不会拖累浏览器预览。
 *
 * 返回值把"取消"和"失败"分成两种结果，调用方因此不会把用户主动取消当成错误，
 * 也不会把真实错误静默吞掉。
 */
export type FolderPickOutcome =
  | { status: "selected"; path: string }
  | { status: "cancelled" }
  | { status: "unavailable" }
  | { status: "failed"; error: string };

/** 桌面运行时探测：非 Tauri 环境下原生对话框不存在。 */
export function isDesktopRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export async function pickFolder(title?: string): Promise<FolderPickOutcome> {
  if (!isDesktopRuntime()) return { status: "unavailable" };

  let selected: string | string[] | null;
  try {
    selected = await open({ directory: true, multiple: false, ...(title ? { title } : {}) });
  } catch (e) {
    // 插件权限缺失、对话框拉起失败都会 reject。必须把原因交回调用方浮出来。
    return { status: "failed", error: String(e) };
  }

  const path = Array.isArray(selected) ? selected[0] : selected;
  if (typeof path !== "string" || !path) return { status: "cancelled" };
  return { status: "selected", path };
}
