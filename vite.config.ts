import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath, URL } from "node:url";

const DEV_PORT = Number(process.env.DEMIURGE_DEV_PORT ?? process.env.PORT ?? 38741);
const host = process.env.TAURI_DEV_HOST || process.env.DEMIURGE_DEV_HOST || "127.0.0.1";

function cleanChunkName(name: string) {
  return name
    .replace(/\.[cm]?js$/, "")
    .replace(/[^a-zA-Z0-9_-]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .toLowerCase();
}

function manualChunks(id: string) {
  const normalized = id.replace(/\\/g, "/");
  if (!normalized.includes("/node_modules/")) return undefined;

  if (normalized.includes("/node_modules/mermaid/dist/chunks/mermaid.core/")) {
    const file = normalized.split("/").pop() ?? "diagram";
    return `mermaid-${cleanChunkName(file)}`;
  }
  if (normalized.includes("/node_modules/mermaid/")) return "mermaid-core";
  if (
    normalized.includes("/node_modules/@mermaid-js/parser/") ||
    normalized.includes("/node_modules/vscode-jsonrpc/") ||
    normalized.includes("/node_modules/vscode-languageserver") ||
    normalized.includes("/node_modules/vscode-uri/")
  ) {
    return "vendor-mermaid-parser";
  }
  if (normalized.includes("/node_modules/cytoscape-fcose/")) return "vendor-cytoscape-fcose";
  if (normalized.includes("/node_modules/cytoscape-cose-bilkent/")) return "vendor-cytoscape-cose";
  if (normalized.includes("/node_modules/cose-base/")) return "vendor-cose-base";
  if (normalized.includes("/node_modules/layout-base/")) return "vendor-layout-base";
  if (normalized.includes("/node_modules/cytoscape/")) return "vendor-cytoscape-core";
  if (normalized.includes("/node_modules/d3")) return "vendor-d3";
  if (normalized.includes("/node_modules/dagre") || normalized.includes("/node_modules/graphlib")) {
    return "vendor-graph-layout";
  }
  if (
    normalized.includes("/node_modules/highlight.js/") ||
    normalized.includes("/node_modules/lowlight/") ||
    normalized.includes("/node_modules/rehype-highlight/")
  ) {
    return "vendor-highlight";
  }
  if (
    normalized.includes("/node_modules/katex/") ||
    normalized.includes("/node_modules/remark-math/") ||
    normalized.includes("/node_modules/rehype-katex/")
  ) {
    return "vendor-markdown";
  }
  if (
    normalized.includes("/node_modules/react-markdown/") ||
    normalized.includes("/node_modules/remark-gfm/") ||
    normalized.includes("/node_modules/remark-parse/") ||
    normalized.includes("/node_modules/rehype-") ||
    normalized.includes("/node_modules/unified/") ||
    normalized.includes("/node_modules/micromark") ||
    normalized.includes("/node_modules/mdast-util") ||
    normalized.includes("/node_modules/hast-util") ||
    normalized.includes("/node_modules/unist-util")
  ) {
    return "vendor-markdown";
  }
  if (normalized.includes("/node_modules/pdfjs-dist/")) return "vendor-pdf";
  if (normalized.includes("/node_modules/jszip/")) return "vendor-zip";
  if (
    normalized.includes("/node_modules/pixi.js/") ||
    normalized.includes("/node_modules/@pixi/") ||
    normalized.includes("/node_modules/untitled-pixi-live2d-engine/")
  ) {
    return "vendor-live2d";
  }
  return undefined;
}

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) },
  },
  // Tauri 自己会清屏，关掉 Vite 清屏以免吞掉 Rust 报错
  clearScreen: false,
  server: {
    port: DEV_PORT,
    strictPort: true,
    host,
    hmr: process.env.TAURI_DEV_HOST ? { protocol: "ws", host, port: DEV_PORT } : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    chunkSizeWarningLimit: 700,
    rollupOptions: {
      output: {
        manualChunks,
      },
    },
  },
});
