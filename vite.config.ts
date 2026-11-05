import { defineConfig } from "vite";
import type { UserConfig as VitestUserConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { resolve } from "path";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;
// @ts-expect-error process is a nodejs global
const port = parseInt(process.env.VITE_PORT || "5274", 10);

export default defineConfig(async () => ({
  plugins: [react(), tailwindcss()],

  build: {
    // Tauri webviews are evergreen (WebView2, WebKitGTK, WKWebView).
    target: "es2022",
    rollupOptions: {
      input: {
        main: resolve(__dirname, "index.html"),
        settings: resolve(__dirname, "settings.html"),
        tray: resolve(__dirname, "tray.html"),
      },
    },
  },

  // Tauri: keep Rust errors visible, fixed port, don't watch src-tauri.
  clearScreen: false,
  server: {
    port,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: port + 1 } : undefined,
    // Only the app's own sources reload the overlay: research runs, docs and
    // the extension write files the webview never imports, and each write
    // used to reload it (and re-run the song's lyric lookup).
    watch: { ignored: ["**/src-tauri/**", "**/research/**", "**/docs/**", "**/extension/**", "**/model_tester/**", "**/tests/**"] },
  },

  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
  } as VitestUserConfig["test"],
}));
