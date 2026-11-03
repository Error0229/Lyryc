/** Route one task (translation, cleaning) to a provider and a model. */
import { useEffect, useState } from "react";
import { LoaderCircle, RefreshCw } from "lucide-react";
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

  async function fetchModels() {
    if (!provider) return;
    setLoading(true);
    try {
      const list = await llm.listModels(provider.id);
      setModels(list);
      toast({ title: `${list.length} models available`, description: provider.label, tone: "success", duration: 2500 });
    } catch (e) {
      toast({ title: "Couldn't list models", description: friendlyError(e), tone: "danger" });
    } finally {
      setLoading(false);
    }
  }

  const listId = `models-${task}`;
  const options = [{ value: OFF, label: unset }, ...providers.map((p) => ({ value: p.id, label: p.label }))];

  return (
    <Row label={label} hint={hint}>
      <div className="grid gap-2">
        <Select label={`${label} provider`} value={provider?.id ?? OFF} options={options} onChange={pickProvider} />
        {provider && (
          <div className="flex w-56 items-center gap-1">
            <TextInput
              aria-label={`${label} model`}
              list={listId}
              value={model}
              placeholder="Model id"
              onChange={(e) => setModel(e.target.value)}
              onBlur={commitModel}
              onKeyDown={(e) => e.key === "Enter" && commitModel()}
              className="font-mono text-xs"
            />
            <datalist id={listId}>
              {[...new Set([...models, ...suggested])].map((m) => (
                <option key={m} value={m} />
              ))}
            </datalist>
            <IconButton label="Fetch models" icon={loading ? <LoaderCircle className="animate-spin" /> : <RefreshCw />} disabled={loading} onClick={fetchModels} />
          </div>
        )}
      </div>
    </Row>
  );
}
