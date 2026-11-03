# Cross-stream contracts

Three workstreams run in parallel on separate branches off `rebuild/integration`:

| Stream | Branch prefix | Owns |
|---|---|---|
| Frontend rebuild | `rebuild/frontend` | `src/**` (except the contract files below), `index.html`, `settings.html`, `tailwind.config.js`, `postcss.config.js`, `vite.config.ts`, `package.json` frontend deps |
| LLM BYOK | `rebuild/llm` | `src-tauri/src/llm/**`, `src-tauri/src/translation/**`, `src-tauri/src/cleaning/**`, `src-tauri/src/commands.rs`, LLM command registration in `lib.rs` |
| Lyric timing | `rebuild/timing` | `docs/research/timing/**`, `src-tauri/src/timing/**` (or a sidecar dir), the plain-lyrics path in `src-tauri/src/lyrics.rs` |

Shared files — change only by agreement, never silently:

- `src/services/ipc/llm.ts` — LLM provider API (frontend consumes, backend implements).
- `src-tauri/Cargo.toml`, `src-tauri/src/lib.rs` — additive edits only; expect a trivial merge.

## Lyrics payload (unchanged, extended)

`fetch_lyrics` keeps returning `LyricLine[]` (`{ time, text, duration? }`, seconds).
Lines may be replaced later for the same track when better timing becomes available,
so the UI must treat the lyric list as swappable mid-song without a jarring re-render.

## Timed lyrics and extra sources (additive)

`fetch_lyrics` is unchanged: same arguments, `LyricLine[]`, never asks third-party
sources. It is a thin wrapper over the command below with `extraSources: false`.

### Command `fetch_lyrics_timed`

```ts
invoke<TimedLyrics>("fetch_lyrics_timed", {
  trackName: string,
  artistName: string,
  duration: number | null, // seconds; 0/absent = unknown
  extraSources: boolean,   // the user's "Extra lyric sources" setting
});

type LyricsSource = "lrclib" | "lrclib-estimate" | "qq" | "netease" | "kugou" | "aligned";
interface TimedLyrics {
  lines: LyricLine[];
  source: LyricsSource;
  pending: boolean; // a lyrics-timing-updated event may still follow
}
```

- `source: "lrclib-estimate"`: LRCLIB has only plain text; times are a
  syllable-weighted guess (~8 s median error). Show the lines, but do **not**
  drive word/line highlighting from them.
- `source: "aligned"`: word-level timing from the opt-in pre-aligner (below).
  Lines carry `words: { time, text, duration? }[]`; a word without `duration`
  runs until the next word. Lines that failed the reject rule have no `words`
  and keep their line time.
- The backend keeps no copy of the setting. The frontend passes it on every call.

Routing:

| LRCLIB has | `extraSources: false` | `extraSources: true` |
|---|---|---|
| synced lyrics | `lrclib` | `lrclib` (sources never run) |
| plain only | `lrclib-estimate`, `pending: false` | `lrclib-estimate`, `pending: true` at once; sources run in the background |
| nothing | error | sources run before answering; `qq` / `netease` / `kugou`, or the error |

Sources are tried QQ → NetEase → Kugou, one request at a time, 3 s timeout, no
retries. A provider is skipped for the rest of the session after 3 consecutive
failures. A result is used only if it has ≥ 3 timed lines, the same length
(±3 s), and either ≥ 50% of LRCLIB's lines text-matched or, with no LRCLIB text,
a matching title. With LRCLIB text, the lines stay LRCLIB's; only times come
from the source. Real timing from any source is cached. Estimates are not.

### Event `lyrics-timing-updated`

```ts
{ title: string; artist: string; lines: LyricLine[]; source: LyricsSource }
```

Emitted once, when a background upgrade of an estimate succeeds. `title` and
`artist` are exactly the `trackName`/`artistName` of the request, so a listener
ignores updates whose track is no longer playing. The lines are LRCLIB's lines
with new times, so they can be swapped in place.

Also emitted (once) with `source: "aligned"` when the pre-aligner finishes the
track that is playing. The lines are the request's lines, re-timed, with `words`.
A listener should not replace `aligned` lines with any other source.

## Word-level timing (pre-align, opt-in, YouTube Music)

`fetch_lyrics` never carries `words` (the field is omitted when absent, so its
payload is unchanged). `fetch_lyrics_timed` answers `source: "aligned"` when the
playing YouTube Music video has a cached alignment for these lyrics, or
`pending: true` while one is being made. Details: `docs/prealign.md`.

The backend keeps no copy of the setting: the overlay mirrors it.

```ts
invoke<PrealignStatus>("prealign_configure", { enabled: boolean, extraSources: boolean });
invoke<PrealignStatus>("prealign_status");
invoke<PrealignStatus>("prealign_remove_files"); // deletes the downloads, turns it off

// Event "prealign-status": PrealignStatus, on state changes and download progress.
interface PrealignStatus {
  enabled: boolean;
  state: "off" | "downloading" | "ready" | "error";
  download: { file: "model" | "runtime" | "yt-dlp"; done: number; total: number | null } | null;
  error: string | null;
  diskBytes: number;
  workingOn: string | null; // "Title — Artist"
  aligned: number;          // this session
}
```

Extension → app WebSocket message (YouTube Music only):

```ts
{ message_type: "UPCOMING_TRACKS", data: {
    current: QueueEntry | null; position: number; upNext: QueueEntry[]; // ≤ 3
    ad: boolean } }
interface QueueEntry { videoId: string; title: string | null; artist: string | null;
                       length: string | null /* "3:45" */; type: string | null /* MUSIC_VIDEO_TYPE_ATV|OMV */ }
```
