import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { fileURLToPath, URL } from "node:url";

// 与 tauri.conf.json 的 devUrl 对齐
const DEV_PORT = 1420;
const host = process.env.TAURI_DEV_HOST;

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
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: DEV_PORT + 1 } : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
});
