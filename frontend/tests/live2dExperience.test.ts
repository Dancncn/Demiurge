import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const source = async (path: string) =>
  (await readFile(new URL(`../${path}`, import.meta.url), "utf8")).replace(/\r\n/g, "\n");
const backendSource = async (path: string) =>
  (await readFile(new URL(`../../backend/Demiurge-desktop/${path}`, import.meta.url), "utf8")).replace(
    /\r\n/g,
    "\n",
  );

test("Live2D uses the direct asset URL and exposes staged loading progress", async () => {
  const [panel, engine] = await Promise.all([
    source("src/features/live2d/Live2DPanel.tsx"),
    source("src/lib/live2d.ts"),
  ]);

  assert.match(panel, /resolvePackLive2dPath/);
  assert.match(panel, /createLive2DAssetModelUrl/);
  assert.doesNotMatch(panel, /packLive2dBundle/);
  assert.match(engine, /convertFileSrc/);
  assert.match(engine, /rewriteModelReferences/);
  assert.match(panel, /role="progressbar"/);
  assert.match(panel, /aria-valuenow=\{progress\}/);
  assert.match(engine, /preloadLive2DEngine/);
  assert.match(engine, /onProgress/);
});

test("the main Live2D panel stays mounted after its first visit", async () => {
  const app = await source("src/app/App.tsx");

  assert.match(app, /live2dMounted/);
  assert.match(app, /active=\{activeView === "live2d"\}/);
  assert.match(app, /live2d-keep-alive/);
});

test("Live2D can be pulled into a resident transparent window", async () => {
  const [main, shell, panel, api, rust, capability] = await Promise.all([
    source("src/main.tsx"),
    source("src/features/live2d/Live2DWindowShell.tsx"),
    source("src/features/live2d/Live2DPanel.tsx"),
    source("src/lib/api.ts"),
    backendSource("src/biz/window.rs"),
    backendSource("capabilities/default.json"),
  ]);

  assert.match(main, /currentWindowLabel === "live2d"/);
  assert.match(main, /Live2DWindowShell/);
  assert.match(shell, /onFocusChanged/);
  assert.match(shell, /windowMode/);
  assert.match(panel, /openLive2dWindow/);
  assert.match(api, /invoke<void>\("open_live2d_window"\)/);
  assert.match(rust, /const LIVE2D_WINDOW_LABEL: &str = "live2d"/);
  assert.match(rust, /fn create_live2d_window/);
  assert.match(rust, /fn open_live2d_window/);
  assert.match(rust, /\.transparent\(true\)/);
  assert.match(rust, /bind_live2d_window_lifecycle/);
  assert.match(capability, /"live2d"/);
});

test("the detached Live2D window pauses rendering while hidden", async () => {
  const [shell, panel, rust] = await Promise.all([
    source("src/features/live2d/Live2DWindowShell.tsx"),
    source("src/features/live2d/Live2DPanel.tsx"),
    backendSource("src/biz/window.rs"),
  ]);

  assert.match(shell, /renderingActive/);
  assert.match(shell, /live2d-visibility-changed/);
  assert.match(shell, /setRenderingActive\(visible\)/);
  assert.match(shell, /active=\{renderingActive\}/);
  assert.match(panel, /appRef\.current\?\.ticker\?\.stop\(\)/);
  assert.match(panel, /appRef\.current\?\.ticker\?\.start\(\)/);
  assert.match(rust, /LIVE2D_VISIBILITY_EVENT/);
  assert.match(rust, /emit\(LIVE2D_VISIBILITY_EVENT, false\)/);
  assert.match(rust, /emit\(LIVE2D_VISIBILITY_EVENT, true\)/);
});

test("mouse following is an explicit persisted Live2D preference", async () => {
  const [panel, engine] = await Promise.all([
    source("src/features/live2d/Live2DPanel.tsx"),
    source("src/lib/live2d.ts"),
  ]);

  assert.match(panel, /demiurge-live2d-mouse-follow/);
  assert.match(panel, /setMouseFollow/);
  assert.match(panel, /live2d\.mouseFollow/);
  assert.match(engine, /autoFocus: mouseFollow/);
  assert.match(engine, /setMouseFollowEnabled/);
});
