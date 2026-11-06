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

/** At most this many messages per window of WINDOW_MS reach the file. */
const BUDGET = 30;
const WINDOW_MS = 10_000;
/** The same message again within this long is a repeat, not news. */
const REPEAT_MS = 2_000;

/**
 * Whether a message should be written: a warning in a render loop must not
 * fill the log (and rotate away the history worth keeping). Repeats of the
 * last message within REPEAT_MS are dropped, and past BUDGET in a window the
 * rest are too.
 */
export function limiter(budget = BUDGET, windowMs = WINDOW_MS, repeatMs = REPEAT_MS) {
  let start = -Infinity;
  let sent = 0;
  let last = { message: "", at: -Infinity };
  return (message: string, now: number): boolean => {
    if (message === last.message && now - last.at < repeatMs) return false;
    if (now - start >= windowMs) {
      start = now;
      sent = 0;
    }
    if (sent >= budget) return false;
    sent += 1;
    last = { message, at: now };
    return true;
  };
}

/** Set once per page: a hot reload re-runs this module but must not wrap console twice. */
const INSTALLED = "__lyrycLogsForwarded";

export function forwardLogs(window: string): void {
  const page = globalThis as Record<string, unknown>;
  if (page[INSTALLED]) return;
  page[INSTALLED] = true;
  const allow = limiter();
  const send = (level: Level, message: string) => {
    if (!allow(message, Date.now())) return;
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
