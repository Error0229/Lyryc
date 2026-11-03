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

type LyricsSource = "lrclib" | "lrclib-estimate" | "qq" | "netease" | "kugou";
interface TimedLyrics {
  lines: LyricLine[];
  source: LyricsSource;
  pending: boolean; // a lyrics-timing-updated event may still follow
}
```

- `source: "lrclib-estimate"`: LRCLIB has only plain text; times are a
  syllable-weighted guess (~8 s median error). Show the lines, but do **not**
  drive word/line highlighting from them.
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
