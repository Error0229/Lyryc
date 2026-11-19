/** Word-level timing's live state, for the settings pages that show it. */
import { useEffect, useState } from "react";
import { songCacheKey } from "../lib/lyrics";
import type { PrealignStatus } from "../lib/types";
import { backend } from "../services/ipc/backend";
import { toast } from "../ui/Toast";

/** The backend's state for the feature, live; null until it answers. */
export function usePrealignStatus(): PrealignStatus | null {
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

/** Is this saved song being timed (or waiting its turn)? */
export function isTiming(status: PrealignStatus | null, artist: string, track: string): boolean {
  const key = songCacheKey(artist, track);
  return !!status?.timing.some((t) => songCacheKey(t.artist, t.track) === key);
}

const sentence = (s: string) => `${s.charAt(0).toUpperCase()}${s.slice(1)}${/[.!?]$/.test(s) ? "" : "."}`;

/** Says, on whatever page is open, how each song the editor asked to time ended: the job runs on its own. */
export function useTimedToasts() {
  useEffect(
    () =>
      backend.subscribe({
        timed: ({ track, pasted, error }) =>
          error
            ? toast({ title: `Couldn't time “${track}”`, description: sentence(error), tone: "danger", duration: 10_000 })
            : toast({ title: pasted ? `Timed “${track}”` : `Word timing ready for “${track}”`, description: pasted ? "Its lines are saved with the times heard." : undefined, tone: "success" }),
      }),
    [],
  );
}
