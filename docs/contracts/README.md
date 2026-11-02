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
