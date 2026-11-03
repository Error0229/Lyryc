import { useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { CircleAlert, CircleCheck, KeyRound, LoaderCircle, Pencil, Trash2 } from "lucide-react";
import { fade, spring } from "../../design/motion";
import type { LlmPreset, LlmProvider, LlmTestResult } from "../../services/ipc/llm";
import { friendlyError, llm } from "../../services/ipc/llmClient";
import { Button } from "../../ui/Button";
import { IconButton } from "../../ui/IconButton";
import { hostOf } from "./useLlm";

type TestState = { state: "idle" } | { state: "running" } | { state: "ok"; result: LlmTestResult } | { state: "failed"; message: string };

function TestResult({ test }: { test: TestState }) {
  if (test.state === "ok") {
    const r = test.result;
    return (
      <p className="flex items-center gap-1.5 text-xs text-success [&_svg]:size-3.5">
        <CircleCheck /> Connected · {r.model} · {r.latencyMs} ms
        <span className="truncate text-fg-muted">— “{r.sample}”</span>
      </p>
    );
  }
  if (test.state === "failed") {
    return (
      <p className="flex items-start gap-1.5 text-xs text-danger [&_svg]:mt-px [&_svg]:size-3.5 [&_svg]:shrink-0">
        <CircleAlert /> {test.message}
      </p>
    );
  }
  return null;
}

export interface ProviderRowProps {
  provider: LlmProvider;
  preset?: LlmPreset;
  /** Model used for the connection test. */
  testModel: string;
  onEdit: () => void;
  onRemove: () => void;
}

export function ProviderRow({ provider, preset, testModel, onEdit, onRemove }: ProviderRowProps) {
  const [test, setTest] = useState<TestState>({ state: "idle" });
  const missingKey = !!preset?.keyRequired && !provider.hasKey;

  async function run() {
    setTest({ state: "running" });
    try {
      setTest({ state: "ok", result: await llm.test(provider.id, testModel) });
    } catch (e) {
      setTest({ state: "failed", message: friendlyError(e) });
    }
  }

  return (
    <li className="grid gap-2 px-4 py-3">
      <div className="flex items-center gap-3">
        <span aria-hidden className="grid size-8 shrink-0 place-items-center rounded-md bg-surface-muted text-sm font-semibold text-fg-muted">
          {provider.label.charAt(0).toUpperCase()}
        </span>
        <div className="min-w-0 flex-1">
          <p className="truncate text-sm font-medium text-fg">{provider.label}</p>
          <p className="flex items-center gap-1.5 truncate text-xs text-fg-muted [&_svg]:size-3">
            <span>{preset?.name ?? provider.presetId}</span>
            <span aria-hidden>·</span>
            <span className="truncate">{hostOf(provider.baseUrl)}</span>
            <span aria-hidden>·</span>
            {provider.hasKey ? (
              <span className="inline-flex items-center gap-1 font-mono">
                <KeyRound /> {provider.keyHint ?? "key saved"}
              </span>
            ) : (
              <span className={missingKey ? "text-danger" : undefined}>{missingKey ? "No key" : "No key needed"}</span>
            )}
          </p>
        </div>
        <Button size="sm" disabled={test.state === "running" || !testModel} onClick={run} title={testModel ? `Test with ${testModel}` : "Pick a model first"}>
          {test.state === "running" ? <LoaderCircle className="animate-spin" /> : null}
          {test.state === "running" ? "Testing" : "Test"}
        </Button>
        <IconButton label="Edit provider" icon={<Pencil />} onClick={onEdit} />
        <IconButton label="Remove provider" icon={<Trash2 />} onClick={onRemove} />
      </div>
      <AnimatePresence initial={false} mode="wait">
        {(test.state === "ok" || test.state === "failed") && (
          <motion.div
            key={test.state}
            initial={{ opacity: 0, y: -4 }}
            animate={{ opacity: 1, y: 0, transition: spring.gentle }}
            exit={{ opacity: 0, transition: fade("fast") }}
            className="pl-11"
          >
            <TestResult test={test} />
          </motion.div>
        )}
      </AnimatePresence>
    </li>
  );
}
