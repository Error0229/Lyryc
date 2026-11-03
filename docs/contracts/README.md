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
  videoId?: string | null, // YouTube Music's id for the track (Track.videoId)
  webSearch?: boolean,     // "Identify songs with web search": when no source finds the
                           // song, re-identify it with the provider's web search (paid)
  refetch?: boolean,       // "Search again": forget the cached answer, the session's
                           // identification and the aligner's "done" mark, then ask the
                           // LLM with web search, then without, then the player's names
});

type LyricsSource =
  | "lrclib" | "lrclib-estimate"
  | "qq" | "netease" | "kugou"
  | "ytmusic" | "ytmusic-estimate"
  | "aligned"
  | "cover-estimate" // a cover: the original's words spread over this recording
  | "instrumental";  // the song has no words; `lines` is empty
interface TimedLyrics {
  lines: LyricLine[];
  source: LyricsSource;
  pending: boolean; // a lyrics-timing-updated event may still follow
}
```

- `source: "lrclib-estimate"`: LRCLIB has only plain text; times are a
  syllable-weighted guess (~8 s median error). Show the lines, but do **not**
  drive word/line highlighting from them. `ytmusic-estimate` is the same for
  YouTube Music's plain text. Estimates are cached like real timing; a cached
  one answers at once and, with the extra sources on, still comes back
  `pending: true` while they look for real timing.
- `source: "cover-estimate"`: the identification says the recording covers
  another artist's song, and only the original has lyrics anywhere. Its words
  (LRCLIB, else the extra sources at any length) are spread over this
  recording: treat as an estimate. Word-level timing retimes them to the
  cover's audio. Trusted only when the player's title says "cover" (or
  歌ってみた, 翻唱, カバー) or web search found it.
- `source: "instrumental"`: the LLM's identification (or LRCLIB's exact match)
  says the recording has no words. Cached, so a replay does no lookups at all.
  "Search again" (`refetch`) doubts it and runs every pass; it only answers
  `instrumental` again when nothing is found. The user can also mark a track
  by hand: `invoke("mark_instrumental", { trackName, artistName })` writes the
  same cache entry.
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

### YouTube Music lyrics (additive)

`track-updated` (and `get_current_track`) carry `videoId` when the extension's
`url` is a `music.youtube.com/watch?v=` page; the frontend passes it back as
`videoId`. With `extraSources: true` and a `videoId`, YouTube Music's own lyrics
(its "Lyrics" tab, through the internal InnerTube API) are asked for after the
cache and before LRCLIB: it is the exact track, so no title matching.

| YouTube Music has | Answer |
|---|---|
| timed lyrics | `ytmusic`, cached |
| plain lyrics, LRCLIB synced for the song with ≥ 50% of lines matching | YouTube Music's lines with LRCLIB's times: `lrclib`, cached |
| plain lyrics, no such timing | `ytmusic-estimate` (cached); with a length, `pending: true` and the extra sources time YouTube Music's lines in the background |
| nothing, or no answer | the routing above, unchanged |

Two requests (`next`, then `browse`), 4 s timeout, no retries; skipped for the
session after 3 consecutive failures. A changed response shape is a failure.

### Event `lyrics-timing-updated`

```ts
{ title: string; artist: string; lines: LyricLine[]; source: LyricsSource }
```

Emitted once, when a background upgrade of an estimate succeeds. `title` and
`artist` are exactly the `trackName`/`artistName` of the request, so a listener
ignores updates whose track is no longer playing. The lines are the estimate's
lines (LRCLIB's or YouTube Music's) with new times, so they can be swapped in place.

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
// "Prepare lyrics for upcoming songs": UPCOMING_TRACKS songs are looked up in
// the background (cache, YouTube Music, LRCLIB, LLM without search, sources),
// one at a time, each once a session. Mirrored the same way.
invoke<void>("lookahead_configure", { enabled: boolean, extraSources: boolean });
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
