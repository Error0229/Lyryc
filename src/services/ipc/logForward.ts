/**
 * Copies the webview's errors and warnings into the app's log file
 * (`log_frontend`), next to the backend's own lines. Without this they only
 * reach devtools, which a release build doesn't open.
 */
import { invoke } from "@tauri-apps/api/core";

type Level = "warn" | "error";

function text(args: unknown[]): string {
  return args
    .map((a) => (a instanceof Error ? (a.stack ?? a.message) : typeof a === "string" ? a : safeJson(a)))
    .join(" ");
}

function safeJson(v: unknown): string {
  try {
    return JSON.stringify(v) ?? String(v);
  } catch {
    return String(v);
  }
}

export function forwardLogs(window: string): void {
  const send = (level: Level, message: string) => {
    // A failed send must not recurse through console.error.
    invoke("log_frontend", { level, message: `[${window}] ${message}` }).catch(() => {});
  };
  for (const level of ["warn", "error"] as const) {
    const original = console[level].bind(console);
    console[level] = (...args: unknown[]) => {
      original(...args);
      send(level, text(args));
    };
  }
  addEventListener("error", (e) => send("error", text([e.error ?? e.message])));
  addEventListener("unhandledrejection", (e) => send("error", `Unhandled rejection: ${text([e.reason])}`));
}
