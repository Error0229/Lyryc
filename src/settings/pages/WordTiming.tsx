import { useEffect, useState } from "react";
import type { PrealignStatus } from "../../lib/types";
import { backend } from "../../services/ipc/backend";
import { settings, useSettings } from "../../state/settings";
import { Button } from "../../ui/Button";
import { Callout } from "../../ui/Callout";
import { ConfirmDialog } from "../../ui/Dialog";
import { Row } from "../../ui/Field";
import { Switch } from "../../ui/Switch";

const LABEL = "Word-level timing (experimental, YouTube Music)";
const FILES: Record<string, string> = { model: "alignment model", runtime: "ONNX Runtime", "yt-dlp": "yt-dlp" };

const mb = (bytes: number) => `${Math.round(bytes / 1048576)} MB`;

/** The backend's state for the feature, live. */
function usePrealignStatus(): PrealignStatus | null {
  const [status, setStatus] = useState<PrealignStatus | null>(null);
  useEffect(() => {
    let live = true;
    backend.prealignStatus().then((s) => live && setStatus(s), () => {});
    const off = backend.subscribe({ prealign: setStatus });
    return () => {
      live = false;
      off();
    };
  }, []);
  return status;
}

function Progress({ done, total }: { done: number; total: number | null }) {
  const share = total ? Math.min(1, done / total) : 0;
  return (
    <div className="h-1.5 w-40 overflow-hidden rounded-sm bg-surface-sunken" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(share * 100)}>
      <div className="h-full origin-left bg-accent transition-transform duration-(--dur-base) ease-standard" style={{ transform: `scaleX(${share})` }} />
    </div>
  );
}

function statusLine(s: PrealignStatus): string {
  switch (s.state) {
    case "off":
      return s.diskBytes ? `Off. ${mb(s.diskBytes)} of downloads still on disk.` : "Off.";
    case "downloading":
      if (!s.download) return "Preparing the download…";
      return `Downloading the ${FILES[s.download.file] ?? s.download.file}: ${mb(s.download.done)}${s.download.total ? ` of ${mb(s.download.total)}` : ""}`;
    case "error":
      return s.error ?? "Something went wrong.";
    case "ready":
      if (s.workingOn) return `Aligning ${s.workingOn}…`;
      return s.aligned ? `Ready. ${s.aligned} song${s.aligned === 1 ? "" : "s"} aligned this session.` : "Ready. Play something on YouTube Music.";
  }
}

/** Settings → Lyrics & sync: the opt-in pre-aligner. */
export function WordTiming() {
  const enabled = useSettings((s) => s.prealign);
  const extraSources = useSettings((s) => s.extraSources);
  const status = usePrealignStatus();
  const [confirm, setConfirm] = useState(false);

  const toggle = (on: boolean) => {
    settings.setPrealign(on);
    backend.prealignConfigure(on, extraSources).catch(() => {});
  };
  const remove = () => {
    settings.setPrealign(false);
    backend.prealignRemoveFiles().catch(() => {});
  };

  return (
    <div>
      <Row
        label={LABEL}
        hint={
          <>
            Times each word by listening to the song. For every YouTube Music track that plays or is up next, it downloads the
            audio with yt-dlp, which YouTube's terms of service don't permit. The audio is deleted right after alignment and
            only the timings are kept; nothing leaves your computer. Needs a one-time download of about 415 MB (a 380 MB
            model, ONNX Runtime and yt-dlp) and uses several CPU cores for about a minute per song. The model is
            echora-kara-300m by hcxlabs, licensed CC BY-NC 4.0: non-commercial use only.
          </>
        }
      >
        <Switch label={LABEL} checked={enabled} onChange={toggle} />
      </Row>

      {status && (enabled || status.diskBytes > 0) && (
        <div className="grid gap-3 px-4 pb-4">
          {status.state === "error" ? (
            <Callout tone="danger" title="Word-level timing is unavailable">
              {statusLine(status)} Lyrics keep their usual timing.
            </Callout>
          ) : (
            <div className="flex items-center gap-3 text-xs text-fg-muted" aria-live="polite">
              <span className="min-w-0 flex-1 truncate">{statusLine(status)}</span>
              {status.state === "downloading" && status.download && <Progress done={status.download.done} total={status.download.total} />}
            </div>
          )}
          <div>
            <Button size="sm" variant="ghost" disabled={status.diskBytes === 0 && status.state !== "downloading"} onClick={() => setConfirm(true)}>
              Remove downloaded files{status.diskBytes ? ` (${mb(status.diskBytes)})` : ""}
            </Button>
          </div>
        </div>
      )}

      <ConfirmDialog
        open={confirm}
        onOpenChange={setConfirm}
        title="Remove downloaded files?"
        description="Deletes the alignment model, ONNX Runtime and yt-dlp, and turns word-level timing off. Timings already made are kept. Turning it back on downloads everything again."
        confirmLabel="Remove files"
        onConfirm={remove}
      />
    </div>
  );
}
