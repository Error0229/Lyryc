import { useEffect, useMemo, useState } from "react";
import { Search } from "lucide-react";
import { songCacheKey } from "../../lib/lyrics";
import { backend, type SavedSong } from "../../services/ipc/backend";
import { Dialog } from "../../ui/Dialog";
import { TextInput } from "../../ui/Field";
import { Segmented } from "../../ui/Segmented";
import { useCurrentTrack } from "../useTrack";
import { ago, sourceLabel } from "./labels";
import { SongEditor } from "./SongEditor";

type Filter = "all" | "missing" | "instrumental" | "edited";

const FILTERS: Record<Filter, (s: SavedSong) => boolean> = {
  all: () => true,
  missing: (s) => s.kind === "missing",
  instrumental: (s) => s.kind === "instrumental",
  edited: (s) => s.source === "edited",
};

/** Settings → Lyrics & sync → Browse: every saved song, and an editor for each. */
export function SavedLyricsBrowser({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  const [songs, setSongs] = useState<SavedSong[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<Filter>("all");
  const [song, setSong] = useState<SavedSong | null>(null);
  const track = useCurrentTrack();

  const refresh = () => backend.savedSongs().then(setSongs, (e) => setError(`Couldn't read the saved lyrics: ${String(e)}`));
  useEffect(() => {
    if (open) void refresh();
    else setSong(null);
  }, [open]);

  const playing = track && songCacheKey(track.artist, track.title);
  const shown = useMemo(() => {
    const q = query.trim().toLowerCase();
    const list = (songs ?? []).filter((s) => FILTERS[filter](s) && (!q || `${s.track} ${s.artist}`.includes(q)));
    // The song playing now first: the one most likely to need a fix.
    return list.sort((a, b) => Number(isPlaying(b)) - Number(isPlaying(a)));

    function isPlaying(s: SavedSong) {
      return songCacheKey(s.artist, s.track) === playing;
    }
  }, [songs, query, filter, playing]);

  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      size="lg"
      title="Saved lyrics"
      description={songs ? `${songs.length} songs. Open one to fix its lines, times or translation.` : "Lyrics, translations and word timings kept from songs you played."}
    >
      {song ? (
        <SongEditor song={song} onBack={() => setSong(null)} onChanged={() => void refresh()} />
      ) : (
        <div className="grid gap-3">
          <div className="sticky top-0 z-1 flex flex-wrap items-center gap-2 bg-surface pb-2">
            <div className="relative min-w-48 flex-1">
              <Search className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-fg-subtle" />
              <TextInput autoFocus aria-label="Search saved songs" placeholder="Search by title or artist" className="pl-8" value={query} onChange={(e) => setQuery(e.target.value)} />
            </div>
            <Segmented
              label="Show"
              value={filter}
              onChange={setFilter}
              options={[
                { value: "all", label: "All" },
                { value: "missing", label: "Not found" },
                { value: "instrumental", label: "Instrumental" },
                { value: "edited", label: "Edited" },
              ]}
            />
          </div>
          {error && <p className="text-xs text-danger">{error}</p>}
          {songs && shown.length === 0 && (
            <p className="py-10 text-center text-sm text-fg-muted">{songs.length === 0 ? "Nothing saved yet." : "No saved song matches."}</p>
          )}
          <ul className="grid">
            {shown.map((s) => (
              <li key={`${s.artist}\n${s.track}`}>
                <button
                  type="button"
                  onClick={() => setSong(s)}
                  className="flex w-full items-center gap-3 rounded-md px-2 py-2 text-left transition-colors duration-(--dur-fast) hover:bg-surface-muted focus:outline-none focus-visible:ring-2 focus-visible:ring-focus"
                >
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-sm font-medium text-fg capitalize">{s.track}</p>
                    <p className="truncate text-xs text-fg-muted">
                      <span className="capitalize">{s.artist}</span> · {sourceLabel(s)}
                    </p>
                  </div>
                  <div className="flex shrink-0 items-center gap-1.5">
                    {songCacheKey(s.artist, s.track) === playing && <Tag accent>Playing</Tag>}
                    {s.translations > 0 && <Tag>Translated</Tag>}
                    {s.timings > 0 && <Tag>Word timing</Tag>}
                  </div>
                  <span className="w-20 shrink-0 text-right text-xs text-fg-subtle">{ago(s.cachedAt)}</span>
                </button>
              </li>
            ))}
          </ul>
        </div>
      )}
    </Dialog>
  );
}

function Tag({ children, accent }: { children: string; accent?: boolean }) {
  return <span className={accent ? "rounded-full bg-accent-soft px-2 py-0.5 text-2xs font-medium text-accent" : "rounded-full bg-surface-sunken px-2 py-0.5 text-2xs text-fg-muted"}>{children}</span>;
}
