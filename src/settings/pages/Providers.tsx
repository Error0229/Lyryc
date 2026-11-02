import { useState } from "react";
import { Plus } from "lucide-react";
import type { LlmProvider, LlmTask } from "../../services/ipc/llm";
import { friendlyError, llm } from "../../services/ipc/llmClient";
import { Button } from "../../ui/Button";
import { Callout } from "../../ui/Callout";
import { ConfirmDialog } from "../../ui/Dialog";
import { Section } from "../../ui/Section";
import { toast } from "../../ui/Toast";
import { PageHeader } from "../Page";
import { ProviderDialog } from "../providers/ProviderDialog";
import { ProviderRow } from "../providers/ProviderRow";
import { RouteRow } from "../providers/RouteRow";
import { presetOf, useLlm } from "../providers/useLlm";

const tasks: Array<{ task: LlmTask; label: string; hint: string }> = [
  { task: "translation", label: "Translation", hint: "Translates lyrics line by line." },
  { task: "cleaning", label: "Track name cleanup", hint: "Strips “(Official Video)” and the like before searching lyrics." },
];

function EmptyState({ onAdd }: { onAdd: () => void }) {
  return (
    <div className="grid justify-items-center gap-3 px-6 py-10 text-center">
      <p className="text-sm font-medium text-fg">No providers yet</p>
      <p className="max-w-sm text-xs text-fg-muted">
        Lyryc uses a model you choose for translation and for cleaning up track names. Bring a key from any provider, or point it at a local server.
      </p>
      <Button variant="primary" onClick={onAdd}>
        <Plus /> Add a provider
      </Button>
    </div>
  );
}

export function ProvidersPage() {
  const { presets, settings, error, refresh } = useLlm();
  const [dialog, setDialog] = useState<{ open: boolean; provider?: LlmProvider }>({ open: false });
  const [removing, setRemoving] = useState<LlmProvider | null>(null);
  const providers = settings?.providers ?? [];
  const routes = settings?.routes ?? {};

  /** A first provider is routed to every task that has none, so it just works. */
  async function onSaved(p: LlmProvider, created: boolean) {
    const model = presetOf(presets, p.presetId)?.suggestedModels[0];
    const unrouted = created && model ? tasks.filter((t) => !routes[t.task]) : [];
    try {
      await Promise.all(unrouted.map((t) => llm.setRoute(t.task, { providerId: p.id, model: model! })));
    } catch (e) {
      toast({ title: "Saved, but routing failed", description: friendlyError(e), tone: "danger" });
    }
    await refresh();
    const routed = unrouted.length ? ` · now used for ${unrouted.map((t) => t.label.toLowerCase()).join(" and ")}` : "";
    toast({ title: `${p.label} saved${routed}`, tone: "success" });
  }

  async function remove(p: LlmProvider) {
    try {
      await llm.removeProvider(p.id);
      await refresh();
      toast({ title: `${p.label} removed` });
    } catch (e) {
      toast({ title: "Couldn't remove provider", description: friendlyError(e), tone: "danger" });
    }
  }

  const testModel = (p: LlmProvider) =>
    Object.values(routes).find((r) => r?.providerId === p.id)?.model || presetOf(presets, p.presetId)?.suggestedModels[0] || "";

  return (
    <div className="grid gap-8">
      <PageHeader title="AI providers" description="Bring your own API key. Keys are stored by Lyryc on this computer and never shown again." />

      {error && (
        <Callout tone="danger" title="Couldn't load providers">
          {friendlyError(error)}
        </Callout>
      )}

      <Section
        title="Providers"
        action={
          providers.length > 0 && (
            <Button size="sm" onClick={() => setDialog({ open: true })}>
              <Plus /> Add provider
            </Button>
          )
        }
      >
        {providers.length === 0 ? (
          <EmptyState onAdd={() => setDialog({ open: true })} />
        ) : (
          <ul className="divide-y divide-line">
            {providers.map((p) => (
              <ProviderRow
                key={p.id}
                provider={p}
                preset={presetOf(presets, p.presetId)}
                testModel={testModel(p)}
                onEdit={() => setDialog({ open: true, provider: p })}
                onRemove={() => setRemoving(p)}
              />
            ))}
          </ul>
        )}
      </Section>

      <Section title="Tasks" description="Pick which provider and model handles each job.">
        {tasks.map((t) => (
          <RouteRow key={t.task} {...t} route={routes[t.task]} providers={providers} presets={presets} onChange={refresh} />
        ))}
      </Section>

      <ProviderDialog
        open={dialog.open}
        provider={dialog.provider}
        presets={presets}
        onOpenChange={(open) => setDialog((d) => ({ ...d, open }))}
        onSaved={onSaved}
      />
      <ConfirmDialog
        open={!!removing}
        onOpenChange={(open) => !open && setRemoving(null)}
        title={`Remove ${removing?.label ?? "provider"}?`}
        description="Its key is deleted and any task routed to it is turned off."
        confirmLabel="Remove"
        onConfirm={() => removing && void remove(removing)}
      />
    </div>
  );
}
