import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "@/app/App";
import DesktopCompanionShell from "@/features/companion/DesktopCompanionShell";
import Live2DWindowShell from "@/features/live2d/Live2DWindowShell";
import WidgetsWindowShell from "@/features/companion/WidgetsWindowShell";
import { LanguageProvider } from "@/lib/i18n";
import "@fontsource-variable/inter";
import "@fontsource-variable/jetbrains-mono";
// MiSans 子集化字体：family "MiSans"、标准字重 400/500/600/700，子集后 ~1.9MB（原 ~8MB）。
// 离散字重消除合成加粗（fake bold）导致的中文笔画糊化；罕用字回退系统中文字体。
import "@/assets/fonts/misans-subset.css";
import "@/style.css";

const requestedWindowLabel = new URLSearchParams(window.location.search).get("window");
const currentWindowLabel =
  requestedWindowLabel || ("__TAURI_INTERNALS__" in window ? getCurrentWindow().label : "main");
document.documentElement.dataset.window = currentWindowLabel;
const Root =
  currentWindowLabel === "desktop_companion"
    ? DesktopCompanionShell
    : currentWindowLabel === "live2d"
      ? Live2DWindowShell
    : currentWindowLabel === "widgets"
      ? WidgetsWindowShell
      : App;

ReactDOM.createRoot(document.getElementById("app") as HTMLElement).render(
  <React.StrictMode>
    <LanguageProvider>
      <Root />
    </LanguageProvider>
  </React.StrictMode>,
);
