import { useEffect, useState } from "react";
import { ExternalLink } from "lucide-react";
import { backend } from "../../services/ipc/backend";
import { settings } from "../../state/settings";
import { Button } from "../../ui/Button";
import { ConfirmDialog } from "../../ui/Dialog";
import { Row } from "../../ui/Field";
import { Section } from "../../ui/Section";
import { toast } from "../../ui/Toast";
import { PageHeader } from "../Page";

const REPO = "https://github.com/Error0229/Lyryc";

const shortcuts = [
  { keys: ["Ctrl", "Shift", "D"], label: "Toggle click-through", hint: "Let clicks pass through the overlay, or grab it again." },
  { keys: ["Ctrl", "Shift", "M"], label: "Show or hide the overlay" },
];

function Keys({ keys }: { keys: string[] }) {
  return (
    <span className="flex gap-1">
      {keys.map((k) => (
        <kbd key={k} className="min-w-6 rounded-sm bg-surface-muted px-1.5 py-0.5 text-center font-mono text-2xs text-fg-muted shadow-1">
          {k}
        </kbd>
      ))}
    </span>
  );
}

export function AboutPage() {
  const [version, setVersion] = useState("");
  const [confirm, setConfirm] = useState(false);
  useEffect(() => {
    backend.appVersion().then(setVersion, () => {});
  }, []);

  return (
    <div className="grid gap-8">
      <PageHeader title="About" description="Synced lyrics for whatever is playing in your browser." />

      <Section title="Lyryc">
        <Row label="Version">
          <span className="font-mono text-xs text-fg-muted">{version || "—"}</span>
        </Row>
        <Row label="Source code" hint="Issues and ideas welcome.">
          <Button size="sm" onClick={() => void backend.openUrl(REPO)}>
            GitHub <ExternalLink />
          </Button>
        </Row>
      </Section>

      <Section title="Shortcuts">
        {shortcuts.map((s) => (
          <Row key={s.label} label={s.label} hint={s.hint}>
            <Keys keys={s.keys} />
          </Row>
        ))}
      </Section>

      <Section title="Reset">
        <Row label="Reset all settings" hint="Appearance, translation, furigana and every offset. AI provider keys are kept.">
          <Button size="sm" variant="danger" onClick={() => setConfirm(true)}>
            Reset…
          </Button>
        </Row>
      </Section>

      <ConfirmDialog
        open={confirm}
        onOpenChange={setConfirm}
        title="Reset all settings?"
        description="Everything on these pages goes back to its default. Provider keys stay."
        confirmLabel="Reset settings"
        onConfirm={() => {
          settings.resetAll();
          toast({ title: "Settings reset", tone: "success" });
        }}
      />
    </div>
  );
}
