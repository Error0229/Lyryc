import { useEffect, useState } from "react";
import { backend, type CacheSize } from "../../services/ipc/backend";
import { Button } from "../../ui/Button";
import { ConfirmDialog } from "../../ui/Dialog";
import { Row } from "../../ui/Field";

const HINT =
  "Kept so a song played again needs no lookups or AI calls. Lyrics are looked up again after 30 days; translations and word timings stay until you clear them.";

const mb = (bytes: number) => (bytes < 1048576 ? "under 1 MB" : `${Math.round(bytes / 1048576)} MB`);

/** Settings → Lyrics & sync: the cache of lyrics, translations and word timings. */
export function SavedLyrics() {
  const [size, setSize] = useState<CacheSize | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirm, setConfirm] = useState(false);

  useEffect(() => {
    let live = true;
    backend.cacheSize().then((s) => live && setSize(s), () => {});
    return () => {
      live = false;
    };
  }, []);

  const clear = () => {
    setBusy(true);
    setError(null);
    backend
      .cacheClear()
      .then(setSize, (e) => setError(`Couldn't clear it: ${String(e)}`))
      .finally(() => setBusy(false));
  };

  const empty = !size || size.entries === 0;
  return (
    <>
      <Row
        label="Saved lyrics and translations"
        hint={
          <>
            {HINT}
            {error && <span className="mt-1 block text-danger">{error}</span>}
          </>
        }
      >
        <Button size="sm" variant="ghost" disabled={empty || busy} onClick={() => setConfirm(true)}>
          {busy ? "Clearing…" : empty ? "Nothing saved" : `Clear (${mb(size.bytes)})`}
        </Button>
      </Row>
      <ConfirmDialog
        open={confirm}
        onOpenChange={setConfirm}
        title="Clear saved lyrics?"
        description="Forgets every saved lyric, translation and word timing. Songs are looked up and translated again as they play, which uses your AI provider again."
        confirmLabel="Clear"
        onConfirm={clear}
      />
    </>
  );
}
