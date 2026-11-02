import { useEffect, useState } from "react";
import { backend } from "../services/ipc/backend";
import type { Track } from "../lib/types";

/** The track currently playing, as seen from the settings window. */
export function useCurrentTrack(): Track | null {
  const [track, setTrack] = useState<Track | null>(null);
  useEffect(() => {
    backend.currentTrack().then(setTrack, () => setTrack(null));
    return backend.subscribe({ track: setTrack });
  }, []);
  return track;
}
