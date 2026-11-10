# Repository Guidelines

## Project Structure
- `src/`: React + TypeScript frontend for three windows, each with its own entry:
  `main.tsx` (the lyrics strip, `src/overlay/`), `settings.tsx` (`src/settings/`) and
  `tray.tsx` (the tray menu, `src/tray/`). Shared code: `src/ui/` (components),
  `src/state/` (Zustand stores and the settings schema), `src/services/ipc/` (the typed
  backend API; `mock.ts` serves it in a plain browser), `src/lib/`, `src/design/`
  (tokens and motion).
- `src-tauri/`: Rust backend. `src/lib.rs` wires commands; lyrics lookup in `lyrics.rs`,
  `sources/` and `ytmusic/`; translation in `translation/` and `llm/`; word timing in
  `prealign/`; the strip's window in `placement.rs`; the tray in `tray.rs`; the
  extension's socket in `websocket*.rs`.
- `extension/`: the MV3 browser extension: site detectors (`*-detector.js`), the YouTube
  Music queue reader, `background.js` and the popup.
- `docs/`: design notes (`prealign.md`) and research reports (`docs/research/`); the
  research code behind them is in `research/`.
- `archive/`: retired material kept for reference (see its README). Nothing builds from it.

## Build, Test and Development Commands
- `npm run tauri dev`: the desktop app, with the frontend hot-reloading.
- `npm run dev`: the frontend alone in a browser, served by the mock backend.
- `npm run build`: type-check (`tsc`) and bundle with Vite.
- `npm test`: frontend tests (Vitest).
- `cd src-tauri && cargo test --lib`: backend tests.
- Load the extension: `chrome://extensions` → Developer mode → Load unpacked → `extension/`.
  Reload the music tab afterwards.

## Coding Style
- TypeScript + React function components and hooks.
- 2-space indent, semicolons, double quotes.
- Tailwind for styling, with the tokens in `src/design/tokens.css`.
- Keep pure logic out of components (`src/lib/`, `src/overlay/model.ts`), and test it.

## One User, No Compatibility Layers
The app has one user: its author. Nothing else reads its data, settings or IPC.
- Don't write for old data or old versions: no migrations, no "rows written before this
  existed" branches, no keeping an old command or setting alive. Change the schema or the
  API and move on; data that doesn't fit is played or fetched again.
- Don't add fallbacks. When something is needed, store it and read it from that one place.
  A second or third way to get the same answer ("else guess it, else search for it") is
  code that hides when the first way is broken.
- When the one source has nothing, say so to the user plainly and stop.

## Testing
- Frontend: Vitest, `*.test.ts(x)` next to the source.
- Rust: `#[cfg(test)]` modules in the file they test.
- The app log (`%LOCALAPPDATA%\com.lyryc.app\logs\lyryc.log`) has backend logs plus the
  overlay's warnings and errors.

## Commits and Pull Requests
- Conventional Commits (`feat:`, `fix:`, `refactor:`, `docs:`, `chore:`).
- PRs: a concise description, linked issues, screenshots or video for UI changes,
  reproduction and test steps. Keep changes scoped.

## Security and Configuration
- The extension talks to the app on `ws://localhost:8765`; the app refuses web pages.
- Never commit secrets. AI keys live in the app's settings, not the repo; `.env` (see
  `.env.example`) is for development only.
- Vite serves on port 5274 (HMR on 5275 when `TAURI_DEV_HOST` is set), matching
  `devUrl` in `src-tauri/tauri.conf.json`.
