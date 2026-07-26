import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const source = (path: string) => readFile(new URL(`../${path}`, import.meta.url), "utf8");

const rustSlice = (rust: string, from: string, to: string) => {
  const start = rust.indexOf(from);
  const end = rust.indexOf(to);
  assert.ok(start >= 0, `${from} not found in lib.rs`);
  assert.ok(end > start, `${to} not found after ${from} in lib.rs`);
  return rust.slice(start, end);
};

test("native window controls resolve the main window in the backend", async () => {
  const rust = await source("src-tauri/src/lib.rs");

  // 前端 getCurrentWindow() 在多窗口下不一定指向 main，最小化又依赖 webview IPC，
  // 所以三个控制点统一按 label 在后端解析窗口。
  const resolver = rustSlice(rust, "fn main_window(", "#[tauri::command]\nfn main_window_minimize");
  assert.match(resolver, /get_webview_window\("main"\)/);
  assert.match(resolver, /ok_or_else/);

  const minimize = rustSlice(rust, "fn main_window_minimize", "fn main_window_toggle_maximize");
  assert.match(minimize, /main_window\(&app\)\?/);
  assert.match(minimize, /native_main_window::minimize/);

  const toggle = rustSlice(rust, "fn main_window_toggle_maximize", "fn main_window_close");
  assert.match(toggle, /native_main_window::toggle_maximize/);

  const close = rustSlice(rust, "fn main_window_close", "/// 呼出桌宠陪伴壳");
  assert.match(close, /main_window\(&app\)\?/);
  assert.match(close, /native_main_window::close/);

  const windowsNative = rustSlice(
    rust,
    '#[cfg(target_os = "windows")]\nmod native_main_window',
    '#[cfg(not(target_os = "windows"))]',
  );
  assert.match(windowsNative, /ShowWindow\(handle, SW_MINIMIZE\)/);
  assert.match(windowsNative, /IsZoomed\(handle\)/);
  assert.match(windowsNative, /ShowWindow\(handle, command\)/);
  assert.match(windowsNative, /PostMessageW\(handle, WM_CLOSE, 0, 0\)/);

  for (const command of ["main_window_minimize", "main_window_toggle_maximize", "main_window_close"]) {
    assert.match(rust, new RegExp(`^\\s+${command},$`, "m"), `${command} is not registered`);
  }
});

test("window control buttons invoke the backend commands and stay inert outside Tauri", async () => {
  const [app, api] = await Promise.all([source("src/App.tsx"), source("src/lib/api.ts")]);

  assert.match(api, /invoke<void>\("main_window_minimize"\)/);
  assert.match(api, /invoke<void>\("main_window_toggle_maximize"\)/);
  assert.match(api, /invoke<void>\("main_window_close"\)/);

  const handlers = app.slice(
    app.indexOf("async function handleWindowMinimize"),
    app.indexOf("return (", app.indexOf("async function handleWindowClose")),
  );
  assert.ok(handlers.length > 0, "window handlers not found in App.tsx");
  assert.match(handlers, /api\.mainWindowMinimize\(\)/);
  assert.match(handlers, /api\.mainWindowToggleMaximize\(\)/);
  assert.match(handlers, /api\.mainWindowClose\(\)/);
  // 非 Tauri 环境（浏览器预览/测试）保持 no-op，不能抛错。
  assert.equal(handlers.match(/"__TAURI_INTERNALS__" in window/g)?.length, 3);
  assert.doesNotMatch(handlers, /getCurrentWindow\(\)/);

  // 三个标题栏按钮仍然各自绑定原来的动作。
  assert.match(app, /onClick=\{\(\) => void handleWindowMinimize\(\)\}/);
  assert.match(app, /onClick=\{\(\) => void handleWindowToggleMaximize\(\)\}/);
  assert.match(app, /onClick=\{\(\) => void handleWindowClose\(\)\}/);
});

test("the companion toolbar button summons the shell instead of toggling it off", async () => {
  const [app, api] = await Promise.all([source("src/App.tsx"), source("src/lib/api.ts")]);

  assert.match(api, /invoke<void>\("desktop_companion_restore"\)/);
  assert.doesNotMatch(app, /toggleDesktopCompanion/);

  const summon = app.slice(
    app.indexOf("async function summonDesktopCompanion"),
    app.indexOf("function startVoiceCall"),
  );
  assert.ok(summon.length > 0, "summonDesktopCompanion not found in App.tsx");
  // 已启用时只呼出窗口，绝不回写 settings（否则会反向停用）。
  assert.match(summon, /if \(settings\.desktop_companion_enabled\) \{/);
  assert.match(summon, /api\.desktopCompanionRestore\(\)/);
  const enabledBranch = summon.slice(
    summon.indexOf("if (settings.desktop_companion_enabled)"),
    summon.indexOf("const next ="),
  );
  assert.doesNotMatch(enabledBranch, /saveSettings/);
  // 未启用时保留原有的"启用"写入。
  assert.match(summon, /desktop_companion_enabled: true/);
  assert.match(summon, /api\.saveSettings\(next\)/);
  assert.doesNotMatch(summon, /desktop_companion_enabled: false/);

  const summonClick = app.indexOf("void summonDesktopCompanion()");
  assert.ok(summonClick > 0, "summon button not found in App.tsx");
  const button = app.slice(summonClick, app.indexOf("</button>", summonClick));
  assert.match(button, /aria-label=\{t\("desktopCompanion\.summon"\)\}/);
  assert.match(button, /title=\{t\("desktopCompanion\.summon"\)\}/);
  // 呼出不是开关，不应再声明按下状态。
  assert.doesNotMatch(button, /aria-pressed/);
});

test("restoring the companion window unminimizes, shows and focuses it", async () => {
  const rust = await source("src-tauri/src/lib.rs");

  const restore = rustSlice(rust, "fn desktop_companion_restore", "fn desktop_companion_show_main");
  // 设置仍是唯一权威：未启用时呼出不能凭空造窗口。
  assert.match(restore, /if !settings\.desktop_companion_enabled \{/);
  // 窗口不存在时按现有设置重建（同步置顶/尺寸/穿透），再显示。
  assert.match(restore, /sync_desktop_companion_window\(&app, &settings\)\?/);
  assert.match(restore, /ensure_desktop_companion_window\(&app, &settings\)\?/);
  assert.match(restore, /\.unminimize\(\)/);
  assert.match(restore, /\.show\(\)/);
  assert.match(restore, /\.set_focus\(\)/);
  // 呼出只动窗口，绝不写回设置。
  assert.doesNotMatch(restore, /save_settings|emit_settings_updated/);
  assert.match(rust, /^\s+desktop_companion_restore,$/m);
});

test("deactivating the companion shell stays available outside the toolbar", async () => {
  const [shell, settingsDialog] = await Promise.all([
    source("src/components/DesktopCompanionShell.tsx"),
    source("src/components/SettingsDialog.tsx"),
  ]);

  const closeButton = shell.slice(
    shell.indexOf('title={t("desktopCompanion.close")}'),
    shell.indexOf("</footer>"),
  );
  assert.ok(closeButton.length > 0, "companion close button not found");
  assert.match(closeButton, /desktop_companion_enabled: false/);
  assert.match(closeButton, /desktop_companion_click_through: false/);

  assert.match(settingsDialog, /checked=\{form\.desktop_companion_enabled\}/);
  assert.match(settingsDialog, /set\("desktop_companion_enabled", checked\)/);
});

test("both locales expose the summon label", async () => {
  const i18n = await source("src/lib/i18n.tsx");

  const summon = [...i18n.matchAll(/"desktopCompanion\.summon": "([^"]+)"/g)].map((m) => m[1]);
  assert.equal(summon.length, 2, "desktopCompanion.summon must exist in both zh and en");
  assert.notEqual(summon[0], summon[1]);
  assert.match(summon[0], /[一-龥]/);
  assert.match(summon[1], /^[\x20-\x7e]+$/);
});
