# Archive

Retired material, kept because it still explains a decision or could be picked up
again. Nothing here is built, shipped or kept up to date, and paths inside these
files point to where things were, not where they are now.

| What | Why it's kept |
| --- | --- |
| `SPEC.md` | The original design spec (in Chinese). Shows where the project started; the app has since moved past most of it. |
| `rebuild-contracts.md` | The interface agreements between the parallel rebuild branches (frontend, LLM, timing). Still a useful record of the lyrics and timing payloads as first designed. |
| `extension-prototypes/queue/` | The YouTube Music "Up next" reader prototype from the pre-align study. It shipped, rewritten, as `extension/youtube-queue.js`. Linked from `docs/research/prealign/REPORT.md`. |
| `extension-prototypes/tabcapture/` | Tab-audio capture for alignment (offscreen document, 16 kHz PCM over the app socket). Never shipped: it needs the `tabCapture` and `offscreen` permissions, and the yt-dlp route replaced it. Linked from `docs/research/timing/REPORT.md`. |
| `model-tester/` | A standalone tool that benchmarked GitHub Models for track-title cleaning, with its prompt (`TrackNameCleaner.prompt.yml`), test titles and results. The model list, the price list and `Cargo.lock` were dropped (git history has them), so it no longer builds as is. GitHub Models is retired as a provider. The app's own cleaning test data is in `src-tauri/tests/fixtures/`. |

Deleted outright, since git history has them: the pre-redesign screenshots
(`docs/screenshots/`) and Vite's template icons (`public/`).
