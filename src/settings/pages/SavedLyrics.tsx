import { useEffect, useState } from "react";
import { backend } from "../../services/ipc/backend";
import { Button } from "../../ui/Button";
import { ConfirmDialog } from "../../ui/Dialog";
import { Row } from "../../ui/Field";

const mb = (bytes: number) => (bytes < 1048576 ? "under 1 MB" : `${Math.round(bytes / 1048576)} MB`);

/** Settings → Lyrics & sync: the cache of lyrics, translations and word timings. */
export function SavedLyrics() {
  const [size, setSize] = useState<number | null>(null);
  const [confirm, setConfirm] = useState(false);

  useEffect(() => {
    let live = true;
    backend.cacheSize().then((s) => live && setSize(s), () => {});
    return () => {
      live = false;
    };
  }, []);

  const clear = () => {
    backend.cacheClear().then(setSize, () => {});
  };

  return (
    <>
      <Row
        label="Saved lyrics and translations"
        hint="Kept so a song played again needs no lookups or AI calls. Lyrics are looked up again after 30 days; translations and word timings stay until you clear them."
      >
        <Button size="sm" variant="ghost" disabled={!size} onClick={() => setConfirm(true)}>
          Clear{size ? ` (${mb(size)})` : ""}
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
