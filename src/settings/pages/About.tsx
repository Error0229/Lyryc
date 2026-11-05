import { useEffect, useState } from "react";
import { ExternalLink } from "lucide-react";
import type { UpdateStatus } from "../../lib/types";
import { backend } from "../../services/ipc/backend";
import { settings } from "../../state/settings";
import { installUpdate, useUpdateStatus } from "../../state/update";
import { Button } from "../../ui/Button";
import { ConfirmDialog } from "../../ui/Dialog";
import { Row } from "../../ui/Field";
import { Section } from "../../ui/Section";
import { toast } from "../../ui/Toast";
import { PageHeader } from "../Page";

const REPO = "https://github.com/Error0229/Lyryc";

const shortcuts = [
  { keys: ["Ctrl", "Shift", "D"], label: "Move or lock the lyrics", hint: "Unlocked, the strip can be dragged and shows its controls; locked, clicks pass through it. Esc locks it too." },
  { keys: ["Ctrl", "Shift", "M"], label: "Show or hide the overlay" },
];

const mb = (bytes: number) => `${(bytes / 1048576).toFixed(1)} MB`;

function updateHint(s: UpdateStatus): string {
  switch (s.state) {
    case "idle":
      return "Lyryc checks by itself a little after it starts, then every few hours.";
    case "checking":
      return "Checking…";
    case "upToDate":
      return "You have the latest version.";
    case "available":
      return `Lyryc ${s.version} is out. Installing restarts the app.`;
    case "downloading":
      return `Downloading ${s.version}: ${mb(s.done)}${s.total ? ` of ${mb(s.total)}` : ""}`;
    case "error":
      return s.message;
  }
}

function UpdateRow() {
  const update = useUpdateStatus();
  const busy = update.state === "checking" || update.state === "downloading";
  return (
    <>
      <Row label="Updates" hint={updateHint(update)}>
        {update.state === "available" ? (
          <Button size="sm" variant="primary" onClick={installUpdate}>
            Install and restart
          </Button>
        ) : (
          <Button size="sm" disabled={busy} onClick={() => void backend.updateCheck()}>
            {update.state === "checking" ? "Checking…" : "Check for updates"}
          </Button>
        )}
      </Row>
      {update.state === "available" && update.notes && (
        <p className="px-4 pb-3 text-xs whitespace-pre-line text-fg-muted">{update.notes}</p>
      )}
    </>
  );
}

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
        <UpdateRow />
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
