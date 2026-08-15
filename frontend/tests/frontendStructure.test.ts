import assert from "node:assert/strict";
import { access, readFile, readdir } from "node:fs/promises";
import test from "node:test";

const source = async (path: string) =>
  (await readFile(new URL(`../${path}`, import.meta.url), "utf8")).replace(/\r\n/g, "\n");

test("completed and historical message rendering keeps expensive work stable", async () => {
  const [markdown, renderer, toolCard, messageList] = await Promise.all([
    source("src/shared/components/Markdown.tsx"),
    source("src/shared/components/MarkdownRenderer.tsx"),
    source("src/features/agent/ToolCard.tsx"),
    source("src/features/chat/MessageList.tsx"),
  ]);

  assert.match(markdown, /memo\(MarkdownView\)/);
  assert.match(renderer, /renderMermaid=\{!streaming\}/);
  assert.match(renderer, /memo\(MarkdownRenderer\)/);
  assert.match(toolCard, /memo\(ToolCard\)/);
  assert.match(messageList, /isScrollNearBottom\(viewport\)/);
  assert.match(messageList, /followTailRef\.current/);
  assert.doesNotMatch(messageList, /scrollIntoView/);
});

test("workspace requests and accessibility contracts remain wired", async () => {
  const [workspace, sidebar, dialog] = await Promise.all([
    source("src/features/workspace/WorkspaceExplorer.tsx"),
    source("src/app/Sidebar.tsx"),
    source("src/features/agent/ConfirmDialog.tsx"),
  ]);

  for (const scope of ["directory:", '"changes"', '"preview"']) {
    assert.ok(workspace.includes(scope), `missing workspace generation scope ${scope}`);
  }
  assert.match(workspace, /normalizeExplorerTab\(current, false\)/);

  const sessionList = sidebar.slice(sidebar.indexOf("sessions.map"), sidebar.indexOf("app-navigation-footer"));
  assert.doesNotMatch(sessionList, /title=\{t\("nav\.live2d"\)\}/);
  assert.match(sessionList, /aria-current=\{activeView === "chat" && s\.id === activeId/);
  assert.match(sidebar, /event\.key === "F2"/);
  assert.match(dialog, /role="dialog"/);
  assert.match(dialog, /aria-modal="true"/);
  assert.match(dialog, /event\.key === "Escape"/);
});

test("resource center keeps theme, locale, chart, and dialog contracts", async () => {
  const [resource, css, i18n, settings, sidebar] = await Promise.all([
    source("src/features/agent/IntegrationCenter.tsx"),
    source("src/style.css"),
    source("src/lib/i18n.tsx"),
    source("src/features/settings/SettingsDialog.tsx"),
    source("src/app/Sidebar.tsx"),
  ]);

  assert.match(resource, /useI18n\(\)/);
  assert.match(resource, /resource-panel/);
  assert.match(resource, /role="img" aria-label=\{t\("resource\.chartDailyLabel"/);
  assert.match(resource, /role="dialog"/);
  assert.match(resource, /previewCloseRef\.current\?\.focus\(\)/);
  assert.match(resource, /event\.key === "Escape"/);
  assert.match(css, /\.resource-center\s*\{/);
  assert.match(css, /--resource-surface:/);
  assert.match(css, /\.resource-usage-layout/);
  assert.match(i18n, /"resource\.title": "资源中心"/);
  assert.match(i18n, /"resource\.title": "Resource center"/);
  assert.match(settings, /document\.hidden/);
  assert.match(settings, /event\.currentTarget\.contains\(next\)/);
  assert.match(sidebar, /sidebar\.archivePending/);
  assert.match(sidebar, /left\.id\.localeCompare\(right\.id\)/);
});

test("App commits assistant_done as the canonical turn body", async () => {
  const app = await source("src/app/App.tsx");

  assert.match(app, /new AgentEventReducer\(\)/);
  assert.match(app, /kind: "done"/);
  assert.match(app, /text: canonicalText/);
  assert.doesNotMatch(app, /text: it\.text\.trim\(\) \? it\.text : text/);
});

test("layout contains explicit safeguards for 980, 1280, and 1811 pixel widths", async () => {
  const css = await source("src/style.css");

  assert.match(css, /@media \(max-width: 980px\)/);
  assert.match(css, /@media \(min-width: 1280px\) and \(max-width: 1810px\)/);
  assert.match(css, /@media \(min-width: 1811px\)/);
});

test("appearance presets share Material layout while keeping separate palettes", async () => {
  const [css, i18n] = await Promise.all([
    source("src/style.css"),
    source("src/lib/i18n.tsx"),
  ]);

  assert.equal((css.match(/html\[data-appearance="material_bloom"\]/g) ?? []).length, 2);
  assert.equal((css.match(/html\[data-appearance="classic"\]/g) ?? []).length, 2);
  assert.match(css, /html\[data-appearance\] \.md-button/);
  assert.match(css, /html\[data-appearance\] \.app-navigation-rail/);
  assert.match(css, /html\[data-appearance\] \.settings-nav-item/);
  assert.match(css, /html\[data-appearance\] \.dashboard-panel/);
  assert.match(css, /html\[data-appearance="classic"\]\s*\{[^}]*--md-primary: #4f5d6b/s);
  assert.match(i18n, /"settings\.general\.appearance\.materialBloom": "水晶花"/);
  assert.match(i18n, /"settings\.general\.appearance\.materialBloom": "Crystal Bloom"/);
  assert.doesNotMatch(i18n, /"settings\.general\.appearance\.materialBloom": "Material Bloom"/);
});

test("frontend code is owned by app, feature, shared, or library modules", async () => {
  const featureDir = new URL("../src/features/", import.meta.url);
  const features = await readdir(featureDir);
  for (const expected of [
    "agent",
    "chat",
    "companion",
    "live2d",
    "media",
    "pack",
    "pet",
    "settings",
    "voice",
    "workflow",
    "workspace",
  ]) {
    assert.ok(features.includes(expected), `missing frontend feature ${expected}`);
  }

  await assert.rejects(
    access(new URL("../src/components/", import.meta.url)),
    "the legacy components bucket must not be recreated",
  );

  const main = await source("src/main.tsx");
  assert.match(main, /@\/app\/App/);
  assert.match(main, /@\/features\/companion\/DesktopCompanionShell/);
  assert.match(main, /@\/features\/live2d\/Live2DWindowShell/);
});
