import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { LanguageProvider } from "./lib/i18n";
import "@fontsource-variable/inter";
import "@fontsource-variable/jetbrains-mono";
// MiSans 子集化字体：family "MiSans"、标准字重 400/500/600/700，子集后 ~1.9MB（原 ~8MB）。
// 离散字重消除合成加粗（fake bold）导致的中文笔画糊化；罕用字回退系统中文字体。
import "./assets/fonts/misans-subset.css";
import "./style.css";
import "katex/dist/katex.min.css";
import "highlight.js/styles/github.css";

ReactDOM.createRoot(document.getElementById("app") as HTMLElement).render(
  <React.StrictMode>
    <LanguageProvider>
      <App />
    </LanguageProvider>
  </React.StrictMode>,
);
