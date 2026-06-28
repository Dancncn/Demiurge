import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath, URL } from "node:url";

const DEV_PORT = Number(process.env.DEMIURGE_DEV_PORT ?? process.env.PORT ?? 38741);
const host = process.env.TAURI_DEV_HOST || process.env.DEMIURGE_DEV_HOST || "127.0.0.1";

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
});
