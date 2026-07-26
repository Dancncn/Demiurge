import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const source = (path: string) => readFile(new URL(`../${path}`, import.meta.url), "utf8");

test("utilities are routed through an independent Tauri window", async () => {
  const [main, app, shell, api, rust, capability] = await Promise.all([
    source("src/main.tsx"),
    source("src/App.tsx"),
    source("src/components/WidgetsWindowShell.tsx"),
    source("src/lib/api.ts"),
    source("src-tauri/src/lib.rs"),
    source("src-tauri/capabilities/default.json"),
  ]);

  assert.match(main, /currentWindowLabel === "widgets"/);
  assert.match(main, /WidgetsWindowShell/);
  assert.match(app, /api\.openWidgetsWindow\(\)/);
  assert.doesNotMatch(app, /toyMenuOpen/);
  assert.match(api, /invoke<void>\("open_widgets_window"\)/);
  assert.match(rust, /const WIDGETS_WINDOW_LABEL: &str = "widgets"/);
  assert.match(rust, /fn open_widgets_window/);
  assert.match(capability, /"widgets"/);

  for (const tool of ["FortuneDialog", "CompanionCard", "PomodoroCard"]) {
    assert.ok(shell.includes(tool), `missing ${tool} from widgets window`);
  }
});

test("the utilities window is pre-created hidden so it never opens blank", async () => {
  const rust = await source("src-tauri/src/lib.rs");

  // Windows 上从 async command 线程动态建窗口只会得到空白外壳，所以窗口必须在
  // setup() 的主线程上下文里预创建，并且创建时就是隐藏的。
  const setup = rust.slice(rust.indexOf(".setup(|app|"), rust.indexOf(".invoke_handler("));
  assert.ok(setup.length > 0, "setup block not found in lib.rs");
  assert.match(setup, /create_widgets_window\(app\.handle\(\)\)/);

  const builder = rust.slice(
    rust.indexOf("fn create_widgets_window"),
    rust.indexOf("fn bind_widgets_window_lifecycle"),
  );
  assert.ok(builder.length > 0, "create_widgets_window not found in lib.rs");
  assert.match(builder, /\.visible\(false\)/);
  assert.match(builder, /bind_widgets_window_lifecycle\(&window\)/);
});

test("closing the utilities window hides it instead of destroying the webview", async () => {
  const rust = await source("src-tauri/src/lib.rs");

  const lifecycle = rust.slice(
    rust.indexOf("fn bind_widgets_window_lifecycle"),
    rust.indexOf("fn bind_main_window_lifecycle"),
  );
  assert.ok(lifecycle.length > 0, "bind_widgets_window_lifecycle not found in lib.rs");
  assert.match(lifecycle, /WindowEvent::CloseRequested/);
  assert.match(lifecycle, /api\.prevent_close\(\)/);
  assert.match(lifecycle, /window\.hide\(\)/);
});

test("closing the main window still exits the app despite the resident utilities window", async () => {
  const rust = await source("src-tauri/src/lib.rs");

  const lifecycle = rust.slice(
    rust.indexOf("fn bind_main_window_lifecycle"),
    rust.indexOf("fn show_widgets_window"),
  );
  assert.ok(lifecycle.length > 0, "bind_main_window_lifecycle not found in lib.rs");
  assert.match(lifecycle, /get_webview_window\("main"\)/);
  assert.match(lifecycle, /WindowEvent::CloseRequested/);
  assert.match(lifecycle, /app\.exit\(0\)/);

  const setup = rust.slice(rust.indexOf(".setup(|app|"), rust.indexOf(".invoke_handler("));
  assert.match(setup, /bind_main_window_lifecycle\(app\.handle\(\)\)/);
});

test("opening the utilities window reuses the resident window and falls back on the main thread", async () => {
  const rust = await source("src-tauri/src/lib.rs");

  const command = rust.slice(
    rust.indexOf("async fn open_widgets_window"),
    rust.indexOf("async fn provider_check_connection"),
  );
  assert.ok(command.length > 0, "open_widgets_window not found in lib.rs");
  // 常驻窗口路径复用共享的显示助手，不再内联重复 unminimize/show/set_focus。
  assert.match(command, /return show_widgets_window\(&window\)/);
  assert.doesNotMatch(command, /window\.unminimize\(\)/);
  assert.match(command, /run_on_main_thread/);
  assert.match(command, /create_widgets_window\(&handle\)/);

  const show = rust.slice(
    rust.indexOf("fn show_widgets_window"),
    rust.indexOf("fn sync_desktop_companion_window"),
  );
  assert.match(show, /\.unminimize\(\)/);
  assert.match(show, /\.show\(\)/);
  assert.match(show, /\.set_focus\(\)/);
});

test("the resident utilities window refreshes when it becomes visible again", async () => {
  const shell = await source("src/components/WidgetsWindowShell.tsx");

  assert.match(shell, /visibilitychange/);
  assert.match(shell, /document\.hidden/);
  assert.match(shell, /removeEventListener\("visibilitychange", onVisible\)/);
});

test("the flat fortune artwork is used by every visible fortune entry", async () => {
  const [app, dashboard, dialog, icon] = await Promise.all([
    source("src/App.tsx"),
    source("src/components/Dashboard.tsx"),
    source("src/components/FortuneDialog.tsx"),
    readFile(new URL("../public/fortune-icon.png", import.meta.url)),
  ]);

  assert.match(app, /\/fortune-icon\.png/);
  assert.match(dashboard, /\/fortune-icon\.png/);
  assert.match(dialog, /\/fortune-icon\.png/);
  assert.deepEqual([...icon.subarray(0, 8)], [137, 80, 78, 71, 13, 10, 26, 10]);
});
