/** Add (preset → details) or edit a provider. Keys are write-only. */
import { useEffect, useState } from "react";
import { ArrowLeft, ExternalLink, Eye, EyeOff } from "lucide-react";
import type { LlmPreset, LlmProvider, LlmProviderInput } from "../../services/ipc/llm";
import { friendlyError, llm } from "../../services/ipc/llmClient";
import { backend } from "../../services/ipc/backend";
import { cx } from "../../lib/cx";
import { Button } from "../../ui/Button";
import { Dialog } from "../../ui/Dialog";
import { Field, TextInput } from "../../ui/Field";
import { IconButton } from "../../ui/IconButton";

export interface ProviderDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  presets: LlmPreset[];
  /** Present when editing. */
  provider?: LlmProvider;
  onSaved: (provider: LlmProvider, created: boolean) => void;
}

function PresetGrid({ presets, onPick }: { presets: LlmPreset[]; onPick: (p: LlmPreset) => void }) {
  return (
    <div className="grid grid-cols-2 gap-2">
      {presets.map((p) => (
        <button
          key={p.id}
          type="button"
          onClick={() => onPick(p)}
          className="grid gap-0.5 rounded-md bg-surface-muted p-3 text-left transition-[background-color,transform] duration-(--dur-fast) ease-standard hover:bg-surface-sunken active:scale-[0.98]"
        >
          <span className="text-sm font-medium text-fg">{p.name}</span>
          <span className="text-xs text-fg-muted">{!p.keyRequired ? "Local · no key needed" : p.baseUrl ? "API key" : "Any OpenAI-compatible URL"}</span>
        </button>
      ))}
    </div>
  );
}

function KeyInput({ value, onChange, placeholder }: { value: string; onChange: (v: string) => void; placeholder: string }) {
  const [shown, setShown] = useState(false);
  return (
    <div className="relative">
      <TextInput
        id="provider-key"
        type={shown ? "text" : "password"}
        autoComplete="off"
        spellCheck={false}
        value={value}
        placeholder={placeholder}
        onChange={(e) => onChange(e.target.value)}
        className="pr-9 font-mono"
      />
      <IconButton
        label={shown ? "Hide key" : "Show key"}
        icon={shown ? <EyeOff /> : <Eye />}
        onClick={() => setShown(!shown)}
        className="absolute top-0.5 right-0.5"
      />
    </div>
  );
}

export function ProviderDialog({ open, onOpenChange, presets, provider, onSaved }: ProviderDialogProps) {
  const [preset, setPreset] = useState<LlmPreset | undefined>();
  const [label, setLabel] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [key, setKey] = useState("");
  const [clearKey, setClearKey] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    setPreset(provider ? presets.find((p) => p.id === provider.presetId) : undefined);
    setLabel(provider?.label ?? "");
    setBaseUrl(provider?.baseUrl ?? "");
    setKey("");
    setClearKey(false);
    setError(null);
  }, [open, provider, presets]);

  const pick = (p: LlmPreset) => {
    setPreset(p);
    setBaseUrl(p.baseUrl);
  };

  const editableUrl = !!preset && (!preset.baseUrl || !preset.keyRequired);
  const needsUrl = !!preset && !preset.baseUrl && !baseUrl.trim();
  const needsKey = !!preset?.keyRequired && !provider?.hasKey && !key.trim();

  async function save() {
    if (!preset) return;
    const input: LlmProviderInput = {
      id: provider?.id,
      presetId: preset.id,
      label: label.trim() || undefined,
      baseUrl: editableUrl ? baseUrl.trim() : undefined,
      apiKey: key.trim() ? key.trim() : clearKey ? "" : undefined,
    };
    setBusy(true);
    setError(null);
    try {
      const saved = await llm.upsertProvider(input);
      onSaved(saved, !provider);
      onOpenChange(false);
    } catch (e) {
      setError(friendlyError(e));
    } finally {
      setBusy(false);
    }
  }

  const title = provider ? `Edit ${provider.label}` : preset ? `Add ${preset.name}` : "Add a provider";

  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title={title}
      description={preset ? undefined : "Pick where your model runs. You can add several and route tasks to each."}
      footer={
        preset && (
          <>
            {!provider && (
              <Button variant="ghost" className="mr-auto" onClick={() => setPreset(undefined)}>
                <ArrowLeft /> Back
              </Button>
            )}
            <Button variant="ghost" onClick={() => onOpenChange(false)}>
              Cancel
            </Button>
            <Button variant="primary" disabled={busy || needsUrl || needsKey} onClick={save}>
              {busy ? "Saving…" : "Save"}
            </Button>
          </>
        )
      }
    >
      {!preset ? (
        <PresetGrid presets={presets} onPick={pick} />
      ) : (
        <form
          className="grid gap-4"
          onSubmit={(e) => {
            e.preventDefault();
            if (!needsUrl && !needsKey) void save();
          }}
        >
          <Field label="Name" htmlFor="provider-label" hint="Shown in task routing.">
            <TextInput id="provider-label" value={label} placeholder={preset.name} onChange={(e) => setLabel(e.target.value)} />
          </Field>
          {editableUrl && (
            <Field label="Base URL" htmlFor="provider-url" hint={preset.baseUrl ? "Where your local server listens." : "Any OpenAI-compatible endpoint, usually ending in /v1."}>
              <TextInput id="provider-url" value={baseUrl} placeholder={preset.baseUrl || "https://example.com/v1"} onChange={(e) => setBaseUrl(e.target.value)} />
            </Field>
          )}
          <Field
            label={preset.keyRequired ? "API key" : "API key (optional)"}
            htmlFor="provider-key"
            hint={
              provider?.hasKey && !clearKey ? (
                <span className="flex items-center gap-2">
                  Saved key <span className="font-mono">{provider.keyHint}</span> · leave blank to keep it ·
                  <button type="button" className="text-danger underline-offset-2 hover:underline" onClick={() => setClearKey(true)}>
                    Remove key
                  </button>
                </span>
              ) : clearKey ? (
                "The saved key will be removed when you save."
              ) : (
                "Stored by Lyryc on this computer. It is never shown again."
              )
            }
          >
            <KeyInput value={key} onChange={setKey} placeholder={provider?.hasKey ? provider.keyHint ?? "••••" : "Paste your key"} />
          </Field>
          {preset.keyUrl && (
            <button
              type="button"
              onClick={() => void backend.openUrl(preset.keyUrl!)}
              className={cx("inline-flex items-center gap-1 justify-self-start text-xs font-medium text-accent hover:underline", "[&_svg]:size-3")}
            >
              Get a {preset.name} key <ExternalLink />
            </button>
          )}
          {error && <p className="rounded-md bg-danger-soft px-3 py-2 text-xs text-danger">{error}</p>}
          <button type="submit" hidden />
        </form>
      )}
    </Dialog>
  );
}
