/**
 * The LlmBridge the UI talks to. In Tauri it is the real bridge, falling back
 * to the in-memory fake per call when the backend lacks the llm_* commands
 * (the BYOK backend lands in a parallel stream). In a browser it is the fake.
 */
import { isTauri } from "@tauri-apps/api/core";
import { LLM_NOT_CONFIGURED, LlmBridge } from "./llm";
import { createFakeLlm } from "./llmFake";

type Bridge = typeof LlmBridge;

const fake = createFakeLlm();

/** Tauri rejects unknown commands with "command <name> not found". */
export function isMissingCommand(err: unknown): boolean {
  return /command \S+ not found/i.test(errorText(err));
}

function withFallback(real: Bridge, backup: Bridge): Bridge {
  let missing = false;
  const wrap = <K extends keyof Bridge>(k: K): Bridge[K] =>
    (async (...args: unknown[]) => {
      const call = (b: Bridge) => (b[k] as (...a: unknown[]) => Promise<unknown>)(...args);
      if (missing) return call(backup);
      try {
        return await call(real);
      } catch (e) {
        if (!isMissingCommand(e)) throw e;
        missing = true;
        console.warn("[llm] backend has no llm_* commands; using the in-memory fake");
        return call(backup);
      }
    }) as Bridge[K];
  return Object.fromEntries((Object.keys(real) as Array<keyof Bridge>).map((k) => [k, wrap(k)])) as Bridge;
}

export const llm: Bridge = isTauri() ? withFallback(LlmBridge, fake) : fake;

export function errorText(err: unknown): string {
  if (typeof err === "string") return err;
  if (err instanceof Error) return err.message;
  return String(err);
}

/** True when an LLM-backed command failed because no provider is routed. */
export function isNotConfigured(err: unknown): boolean {
  return errorText(err).startsWith(LLM_NOT_CONFIGURED);
}

/** Error text without the machine prefix, for display. */
export function friendlyError(err: unknown): string {
  const text = errorText(err);
  return text.startsWith(LLM_NOT_CONFIGURED) ? text.slice(LLM_NOT_CONFIGURED.length).trim() : text;
}
