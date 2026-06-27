import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "@fontsource-variable/inter";
import "misans/lib/Normal/MiSans-Normal.min.css";
import "misans/lib/Normal/MiSans-Semibold.min.css";
import "./style.css";
import "katex/dist/katex.min.css";
import "highlight.js/styles/github.css";

ReactDOM.createRoot(document.getElementById("app") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
