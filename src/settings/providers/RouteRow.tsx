/** Route one task (translation, cleaning) to a provider and a model. */
import { useEffect, useState } from "react";
import { Popover } from "radix-ui";
import { Check, ChevronDown, LoaderCircle, RefreshCw } from "lucide-react";
import type { LlmPreset, LlmProvider, LlmRoute, LlmTask } from "../../services/ipc/llm";
import { friendlyError, llm } from "../../services/ipc/llmClient";
import { Row, TextInput } from "../../ui/Field";
import { IconButton } from "../../ui/IconButton";
import { Select } from "../../ui/Select";
import { toast } from "../../ui/Toast";
import { presetOf } from "./useLlm";

const OFF = "__off";

export interface RouteRowProps {
  task: LlmTask;
  label: string;
  hint: string;
  route?: LlmRoute;
  /** What no route of its own means for this task: "Off", or another task's route. */
  unset: string;
  providers: LlmProvider[];
  presets: LlmPreset[];
  onChange: () => void;
}

export function RouteRow({ task, label, hint, route, unset, providers, presets, onChange }: RouteRowProps) {
  // A provider picked without a model to go with it (OpenRouter, LM Studio, Custom suggest
  // none) is held here until the user types or fetches one: a route needs both.
  const [draft, setDraft] = useState<string | null>(null);
  const provider = providers.find((p) => p.id === (draft ?? route?.providerId));
  const suggested = provider ? presetOf(presets, provider.presetId)?.suggestedModels ?? [] : [];
  const [models, setModels] = useState<string[]>([]);
  const [model, setModel] = useState(route?.model ?? "");
  const [loading, setLoading] = useState(false);
  const [open, setOpen] = useState(false);

  useEffect(() => setModel(route?.model ?? ""), [route?.model]);
  useEffect(() => setModels([]), [provider?.id]);

  async function save(next: LlmRoute | null) {
    try {
      await llm.setRoute(task, next);
      setDraft(null);
      onChange();
    } catch (e) {
      toast({ title: "Couldn't save routing", description: friendlyError(e), tone: "danger" });
    }
  }

  function pickProvider(id: string) {
    if (id === OFF) return void save(null);
    const p = providers.find((x) => x.id === id)!;
    const first = presetOf(presets, p.presetId)?.suggestedModels[0];
    if (first) return void save({ providerId: id, model: first });
    setDraft(id);
    setModel("");
  }

  function commitModel() {
    const m = model.trim();
    if (provider && m && (provider.id !== route?.providerId || m !== route.model)) {
      void save({ providerId: provider.id, model: m });
    }
  }

  async function fetchModels(quiet = false) {
    if (!provider) return;
    setLoading(true);
    try {
      const list = await llm.listModels(provider.id);
      setModels(list);
      if (!quiet) toast({ title: `${list.length} models available`, description: provider.label, tone: "success", duration: 2500 });
    } catch (e) {
      toast({ title: "Couldn't list models", description: friendlyError(e), tone: "danger" });
    } finally {
      setLoading(false);
    }
  }

  // Every model we know of. Typing narrows the list; the saved model alone doesn't
  // (the browser's own datalist did, so it only ever offered the current model).
  const choices = [...new Set([...models, ...suggested])];
  const typed = model.trim() === (route?.model ?? "") ? "" : model.trim().toLowerCase();
  const shown = typed ? choices.filter((m) => m.toLowerCase().includes(typed)) : choices;

  function openList(next: boolean) {
    setOpen(next);
    if (next && models.length === 0) void fetchModels(true);
  }

  function choose(m: string) {
    setModel(m);
    setOpen(false);
    if (provider && (provider.id !== route?.providerId || m !== route.model)) {
      void save({ providerId: provider.id, model: m });
    }
  }
  const options = [{ value: OFF, label: unset }, ...providers.map((p) => ({ value: p.id, label: p.label }))];

  return (
    <Row label={label} hint={hint}>
      <div className="grid gap-2">
        <Select label={`${label} provider`} value={provider?.id ?? OFF} options={options} onChange={pickProvider} />
        {provider && (
          <Popover.Root open={open} onOpenChange={openList}>
            <Popover.Anchor asChild>
              <div className="flex w-56 items-center gap-1">
                <TextInput
                  aria-label={`${label} model`}
                  value={model}
                  placeholder="Model id"
                  onChange={(e) => {
                    setModel(e.target.value);
                    if (!open) setOpen(true);
                  }}
                  onBlur={commitModel}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") commitModel();
                    if (e.key === "Escape") setOpen(false);
                  }}
                  className="font-mono text-xs"
                />
                <Popover.Trigger
                  aria-label="Choose a model"
                  className="inline-flex size-7 shrink-0 items-center justify-center rounded-md text-fg-muted transition-colors duration-(--dur-fast) hover:bg-surface-muted hover:text-fg [&_svg]:size-4"
                >
                  <ChevronDown />
                </Popover.Trigger>
              </div>
            </Popover.Anchor>
            <Popover.Portal>
              <Popover.Content
                align="end"
                sideOffset={4}
                onOpenAutoFocus={(e) => e.preventDefault()}
                className="z-(--z-popover) w-72 animate-pop rounded-md bg-surface p-1 shadow-3"
              >
                <div className="flex items-center justify-between px-2 pb-1 text-xs text-fg-subtle">
                  <span>{loading ? "Loading models…" : `${shown.length} model${shown.length === 1 ? "" : "s"}`}</span>
                  <IconButton
                    label="Refresh the list"
                    icon={loading ? <LoaderCircle className="animate-spin" /> : <RefreshCw />}
                    disabled={loading}
                    onClick={() => void fetchModels()}
                  />
                </div>
                <div className="max-h-64 overflow-y-auto">
                  {shown.map((m) => (
                    <button
                      key={m}
                      type="button"
                      onMouseDown={(e) => e.preventDefault()}
                      onClick={() => choose(m)}
                      className="relative flex h-8 w-full items-center rounded-sm pr-8 pl-2 text-left font-mono text-xs text-fg outline-none hover:bg-surface-muted focus-visible:bg-surface-muted"
                    >
                      <span className="truncate">{m}</span>
                      {m === route?.model && <Check className="absolute right-2 size-3.5 text-accent" />}
                    </button>
                  ))}
                  {!loading && shown.length === 0 && <p className="px-2 py-2 text-xs text-fg-subtle">No model matches “{model.trim()}”. Enter saves it as typed.</p>}
                </div>
              </Popover.Content>
            </Popover.Portal>
          </Popover.Root>
        )}
      </div>
    </Row>
  );
}
