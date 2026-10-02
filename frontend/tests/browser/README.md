# Real App browser smoke

This harness runs the production `src/main.tsx`, App, Composer, settings, event adapter,
message projection, and voice queue in a real Chromium browser. Only Tauri IPC,
events, and window adapters are replaced using `@tauri-apps/api/mocks`. Backend
responses and event order are controlled fixtures; this is **not a packaged Tauri
or live Rust/LLM end-to-end test**.

Install the frontend dependencies normally. Install Playwright into a separate
tools directory so this check does not change application dependencies. For
example, from the repository root in PowerShell:

```powershell
npm install --prefix "$env:TEMP/demiurge-browser-smoke-tools" playwright --no-save --package-lock=false
node frontend/tests/browser/smoke.mjs `
  --playwright-module "$env:TEMP/demiurge-browser-smoke-tools/node_modules/playwright/index.mjs" `
  --browser "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe"
```

Pass the path to an installed Chrome/Edge executable; no browser download is
required. `PLAYWRIGHT_MODULE` and `BROWSER_EXECUTABLE` environment variables can
replace the corresponding arguments. Optional `--artifacts <directory>` controls
failure screenshots (default: the OS temporary directory's
`demiurge-browser-smoke` folder). The script starts an isolated local Vite server
on an available port and closes the server, browser, and contexts on completion.
Each scenario gets a fresh page and native fixture state. Unexpected IPC calls
and page exceptions fail the check.

The ten scenarios cover:

- A real Composer image attachment is encoded and sent through multimodal IPC
  with its image bytes, OCR context, and agent selection; the conversation can be
  reloaded from history.

- Two Goal answers under one engine turn, busy state between answers, and a late
  event from a completed answer.
- Model error event and rejected send promise arriving in either order, producing
  one visible error with event metadata.
- A tool preamble followed by a missing final event, recovered from saved history.
- Stop while speech synthesis is pending: late audio, text, and confirmation do
  not appear.
- Stop during real browser audio playback: the media element is paused. The
  fixture is a silent WAV; the original browser `play()` implementation runs.
- Session navigation isolates old events and confirmations, while a current
  confirmation opens and is cleared by answer completion.
- A new turn in the same session rejects the previous turn's late confirmation.
- Permission draft fields survive tab changes, and a failed refresh displays an
  error while retaining both the previous rules and draft.

No production startup entry or default package script is changed. `smoke.html`
installs native mocks before importing the normal application entry point.
