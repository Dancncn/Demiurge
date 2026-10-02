import assert from "node:assert/strict";
import { mkdir } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { createServer } from "vite";

const options = new Map();
for (let i = 2; i < process.argv.length; i += 2) options.set(process.argv[i], process.argv[i + 1]);
const modulePath = options.get("--playwright-module") ?? process.env.PLAYWRIGHT_MODULE;
const browserPath = options.get("--browser") ?? process.env.BROWSER_EXECUTABLE;
if (!modulePath || !browserPath) throw new Error("Pass --playwright-module <index.mjs> and --browser <executable>.");
const artifacts = resolve(options.get("--artifacts") ?? join(tmpdir(), "demiurge-browser-smoke"));
await mkdir(artifacts, { recursive: true });
const { chromium } = await import(pathToFileURL(resolve(modulePath)).href);
const frontend = fileURLToPath(new URL("../../", import.meta.url));
const server = await createServer({
  root: frontend, configFile: join(frontend, "vite.config.ts"), logLevel: "error",
  server: { host: "127.0.0.1", port: 0, strictPort: false },
});
await server.listen();
const origin = server.resolvedUrls.local[0];
let browser;
let passed = 0;

async function check(name, run) {
  const context = await browser.newContext({ viewport: { width: 1440, height: 1000 } });
  const page = await context.newPage();
  page.setDefaultTimeout(12_000);
  const pageErrors = [];
  page.on("pageerror", (error) => pageErrors.push(String(error)));
  try {
    await page.goto(`${origin}tests/browser/smoke.html`);
    await page.getByRole("button", { name: "Smoke A", exact: true }).waitFor();
    await run(page);
    assert.deepEqual(pageErrors, [], "Unexpected browser errors");
    assert.deepEqual(await page.evaluate(() => window.__DEMIURGE_SMOKE__.unknownCommands()), [], "Unexpected native commands");
    console.log(`PASS ${name}`);
    passed += 1;
  } catch (error) {
    const screenshot = join(artifacts, `${name}.png`);
    await page.screenshot({ path: screenshot, fullPage: true });
    console.error(`FAIL ${name}; screenshot: ${screenshot}`);
    console.error("Browser errors:", pageErrors);
    console.error("Native commands:", await page.evaluate(() => window.__DEMIURGE_SMOKE__?.calls()));
    throw error;
  } finally {
    await context.close();
  }
}

async function send(page, text) {
  await page.getByRole("textbox").fill(text);
  await page.getByRole("button", { name: "Send", exact: true }).click();
  await page.waitForFunction(() => window.__DEMIURGE_SMOKE__.pending());
}

async function visible(page, text) {
  await page.getByText(text, { exact: true }).waitFor({ state: "visible" });
}

try {
  browser = await chromium.launch({ executablePath: browserPath, headless: true, args: ["--autoplay-policy=no-user-gesture-required"] });
  await check("goal-continuations", async (page) => {
    await send(page, "Run the Goal smoke");
    await page.evaluate(async () => {
      const mock = window.__DEMIURGE_SMOKE__;
      await mock.agentEvent("assistant_start", null, "answer-1");
      await mock.agentEvent("assistant_delta", "partial", "answer-1");
      await mock.agentEvent("assistant_done", "Goal first canonical answer", "answer-1");
    });
    await visible(page, "Goal first canonical answer");
    assert.equal(await page.getByRole("button", { name: "Smoke B", exact: true }).isDisabled(), true, "Answer done must not unlock a running engine");
    await page.evaluate(async () => {
      const mock = window.__DEMIURGE_SMOKE__;
      await mock.agentEvent("assistant_start", null, "answer-2");
      await mock.agentEvent("assistant_delta", "continuing", "answer-2");
      await mock.agentEvent("assistant_done", "Goal continuation canonical answer", "answer-2");
      await mock.agentEvent("assistant_done", "STALE ANSWER MUST NOT APPEAR", "answer-1");
      await mock.settle();
    });
    await visible(page, "Goal continuation canonical answer");
    assert.equal(await page.getByText("STALE ANSWER MUST NOT APPEAR").count(), 0);
    assert.equal(await page.locator(".assistant-message-content").count(), 2);
  });

  for (const eventFirst of [true, false]) {
    await check(eventFirst ? "error-before-rejection" : "rejection-before-error", async (page) => {
      await send(page, "Exercise error ordering");
      await page.evaluate(() => window.__DEMIURGE_SMOKE__.agentEvent("assistant_start", null, "error-answer"));
      const emitError = () => page.evaluate(() => window.__DEMIURGE_SMOKE__.agentEvent("assistant_error", {
        kind: "llm", message: "Smoke model failure", hint: "Smoke event-specific hint", retryable: false,
      }, "error-answer"));
      const reject = () => page.evaluate(() => window.__DEMIURGE_SMOKE__.settle("Smoke model failure"));
      if (eventFirst) { await emitError(); await reject(); }
      else { await reject(); await visible(page, "Smoke model failure"); await emitError(); }
      await visible(page, "Smoke event-specific hint");
      assert.equal(await page.locator(".assistant-message-content").filter({ hasText: "Smoke model failure" }).count(), 1);
    });
  }

  await check("missing-final-event-recovers-history", async (page) => {
    await send(page, "Recover the canonical history");
    await page.evaluate(async () => {
      const mock = window.__DEMIURGE_SMOKE__;
      await mock.agentEvent("assistant_start", null, "recover-answer");
      await mock.agentEvent("assistant_delta", "Smoke tool preamble", "recover-answer");
      await mock.agentEvent("tool_start", { tool_call_id: "tool-1", name: "read_file", args: { path: "a.txt" } }, "recover-answer");
      await mock.agentEvent("tool_end", { tool_call_id: "tool-1", name: "read_file", ok: true, result: "read complete" }, "recover-answer");
    });
    await visible(page, "Smoke tool preamble");
    await page.evaluate(async () => {
      const mock = window.__DEMIURGE_SMOKE__;
      mock.saveAnswer("Recovered final answer from history");
      await mock.settle();
    });
    await visible(page, "Recovered final answer from history");
  });

  await check("stop-rejects-late-audio-and-ui", async (page) => {
    await page.getByRole("button", { name: "Speak replies", exact: true }).click();
    await send(page, "Stop a pending voice reply");
    await page.evaluate(async () => {
      const mock = window.__DEMIURGE_SMOKE__;
      await mock.agentEvent("assistant_start", null, "stop-answer");
      await mock.agentEvent("assistant_delta", "Buffered sentence!", "stop-answer");
    });
    await page.waitForFunction(() => window.__DEMIURGE_SMOKE__.synthesisCount() === 1);
    await page.getByRole("button", { name: "Stop", exact: true }).click();
    await page.evaluate(async () => {
      const mock = window.__DEMIURGE_SMOKE__;
      mock.releaseSynthesis();
      await mock.agentEvent("assistant_delta", "LATE UI RESIDUE", "stop-answer");
      await mock.confirm(mock.turn(), "late-stopped-confirmation");
    });
    await visible(page, "Buffered sentence!");
    await page.waitForTimeout(150);
    assert.equal(await page.getByText("LATE UI RESIDUE").count(), 0);
    assert.equal(await page.getByRole("dialog").count(), 0);
    assert.equal((await page.evaluate(() => window.__DEMIURGE_SMOKE__.audio())).played, 0);
  });

  await check("stop-pauses-real-browser-audio", async (page) => {
    await page.getByRole("button", { name: "Speak replies", exact: true }).click();
    await send(page, "Stop real browser playback");
    await page.evaluate(async () => {
      const mock = window.__DEMIURGE_SMOKE__;
      await mock.agentEvent("assistant_start", null, "stop-answer");
      await mock.agentEvent("assistant_delta", "Play this sentence!", "stop-answer");
    });
    await page.waitForFunction(() => window.__DEMIURGE_SMOKE__.synthesisCount() === 1);
    await page.evaluate(() => window.__DEMIURGE_SMOKE__.releaseSynthesis());
    await page.waitForFunction(() => window.__DEMIURGE_SMOKE__.audio().played === 1 && !window.__DEMIURGE_SMOKE__.audio().allPaused);
    await page.getByRole("button", { name: "Stop", exact: true }).click();
    await page.waitForFunction(() => window.__DEMIURGE_SMOKE__.audio().allPaused);
  });

  await check("session-and-confirmation-isolation", async (page) => {
    await send(page, "First session message");
    const oldTurn = await page.evaluate(() => window.__DEMIURGE_SMOKE__.turn());
    await page.evaluate(async () => {
      const mock = window.__DEMIURGE_SMOKE__;
      await mock.agentEvent("assistant_done", "Session A answer", "session-a-answer");
      mock.saveAnswer("Session A answer");
      await mock.settle();
    });
    await page.getByRole("button", { name: "Smoke B", exact: true }).click();
    await send(page, "Second session message");
    await page.evaluate(async (old) => {
      const mock = window.__DEMIURGE_SMOKE__;
      await mock.agentEvent("assistant_delta", "WRONG SESSION", "session-a-answer", old);
      await mock.confirm(old, "old-session-confirm");
      await mock.confirm(mock.turn(), "current-confirm");
    }, oldTurn);
    await page.getByRole("dialog").waitFor();
    assert.equal(await page.getByText("Session A answer", { exact: true }).count(), 0);
    assert.equal(await page.getByText("WRONG SESSION").count(), 0);
    await page.evaluate(async () => {
      const mock = window.__DEMIURGE_SMOKE__;
      await mock.agentEvent("assistant_done", "Session B answer", "session-b-answer");
      await mock.settle();
    });
    await visible(page, "Session B answer");
    assert.equal(await page.getByRole("dialog").count(), 0);
  });

  await check("same-session-stale-confirmation", async (page) => {
    await send(page, "Stop the earlier engine turn");
    const oldTurn = await page.evaluate(() => window.__DEMIURGE_SMOKE__.turn());
    await page.evaluate(() => window.__DEMIURGE_SMOKE__.agentEvent("assistant_start", null, "stop-answer"));
    await page.getByRole("button", { name: "Stop", exact: true }).click();
    await page.waitForFunction(() => !window.__DEMIURGE_SMOKE__.pending());
    await send(page, "Start the next turn in the same session");
    await page.evaluate((old) => window.__DEMIURGE_SMOKE__.confirm(old, "stale-same-session"), oldTurn);
    assert.equal(await page.getByRole("dialog").count(), 0);
    await page.evaluate(() => window.__DEMIURGE_SMOKE__.confirm(window.__DEMIURGE_SMOKE__.turn(), "fresh-same-session"));
    await page.getByRole("dialog").waitFor();
    await page.evaluate(async () => {
      const mock = window.__DEMIURGE_SMOKE__;
      await mock.agentEvent("assistant_done", "New engine answer", "new-engine-answer");
      await mock.settle();
    });
    await visible(page, "New engine answer");
    assert.equal(await page.getByRole("dialog").count(), 0);
  });

  await check("permission-draft-and-refresh-failure", async (page) => {
    await page.getByRole("button", { name: "Settings", exact: true }).click();
    await page.getByRole("button", { name: /^Tools / }).click();
    const panel = page.locator("section.md-settings-section").filter({ has: page.getByRole("heading", { name: "Permission Rules", exact: true }) });
    await panel.getByRole("combobox", { name: /^Tool/ }).selectOption("shell");
    await panel.getByRole("combobox", { name: /^Scope/ }).selectOption("project");
    await panel.getByLabel("Reason", { exact: true }).fill("Smoke permission draft");
    await page.getByRole("button", { name: /^General / }).click();
    await page.getByRole("button", { name: /^Tools / }).click();
    assert.equal(await panel.getByRole("combobox", { name: /^Tool/ }).inputValue(), "shell");
    assert.equal(await panel.getByRole("combobox", { name: /^Scope/ }).inputValue(), "project");
    assert.equal(await panel.getByLabel("Reason", { exact: true }).inputValue(), "Smoke permission draft");
    await page.evaluate(() => window.__DEMIURGE_SMOKE__.failPermissionRefresh());
    await panel.getByRole("button", { name: "Refresh", exact: true }).click();
    await panel.getByText("Error: Smoke permission refresh failed", { exact: true }).waitFor();
    await panel.getByText("Smoke saved rule", { exact: true }).waitFor();
    assert.equal(await panel.getByLabel("Reason", { exact: true }).inputValue(), "Smoke permission draft");
  });

  console.log(`Browser smoke: ${passed} scenarios passed; 0 page exceptions; real App, Composer, projection and browser media.`);
} finally {
  await browser?.close();
  await server.close();
}
