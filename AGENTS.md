# Repository Guidelines

## Project Structure & Module Organization
- `src/`: React + TypeScript app. Key folders: `components/`, `hooks/`, `services/`, `stores/`, `assets/`.
- `src-tauri/`: Tauri (Rust) backend. Entry in `src/main.rs`, shared lib in `src/lib.rs`, config in `Cargo.toml` and `tauri.conf.json`.
- `extension/`: Browser extension (MV3) with site detectors (`*-detector.js`), `background.js`, `manifest.json`.
- `public/`, `index.html`, configs: `vite.config.ts`, `tailwind.config.js`, `tsconfig.json`.
- `websocket_test.html|.js`: Manual test utilities for the extension socket.

## Build, Test, and Development Commands
- `npm run dev`: Start Vite dev server (web UI only).
- `npm run tauri dev`: Launch desktop app with Tauri.
- `npm run build`: Type-check (`tsc`) and bundle with Vite.
- `npm run preview`: Serve the production build locally.
- `cd src-tauri && cargo build`: Build Rust backend.
- `cd src-tauri && cargo test`: Run Rust tests (add as needed).
- Load the extension: Chrome/Edge → `chrome://extensions` → Developer Mode → Load unpacked → `extension/`.

## Coding Style & Naming Conventions
- TypeScript + React hooks; functional components only.
- Indent 2 spaces; use semicolons and double quotes.
- Components: `PascalCase` in `src/components`; functions/vars: `camelCase`.
- Place pure logic in `src/services`; global state in `src/stores` (Zustand).
- Tailwind-first styling; prefer class utilities over inline styles.

## Testing Guidelines
- JS/TS: no framework configured yet. Prefer Vitest; name tests `*.test.ts(x)` colocated with source.
- Rust: add `#[cfg(test)]` modules where relevant; run `cargo test`.
- Manual: use `websocket_test.html` to simulate track/time events.

## Commit & Pull Request Guidelines
- Current history is mixed; write clear, imperative messages.
- Prefer Conventional Commits (`feat:`, `fix:`, `refactor:`, `docs:`).
- PRs: concise description, linked issues, UI screenshots/video when applicable, reproduction and test steps. Keep changes scoped.

## Security & Configuration Tips
- Extension ↔ app uses WebSocket `ws://localhost:8765`; allow local connections.
- Do not commit secrets (none required). 
- Network HMR: set `TAURI_DEV_HOST=0.0.0.0` for external devices; Vite uses ports `5174` (dev) and `5175` (HMR) per `vite.config.ts`.
