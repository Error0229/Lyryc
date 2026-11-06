# Lyryc

Synced lyrics for the music playing in your browser, shown in a strip that floats
over your other windows. It can translate them, add furigana to Japanese, and
highlight each word as it's sung.

A browser extension reports what's playing; the desktop app (Tauri, Rust and
React) finds the lyrics and keeps them in time with the song.

## Features

- **An always-on-top lyrics strip.** It's click-through while locked, so it never
  gets in the way. Hover it to show the Unlock button; unlocked, drag it anywhere
  and resize it. It snaps to a screen edge and remembers its place on each display.
- **Lyrics from several sources.** YouTube Music's own lyrics, [LRCLIB](https://lrclib.net/)
  and, with "Extra lyric sources", QQ Music, NetEase and Kugou. Covers and remixes
  are matched to the original song's lyrics. Instrumentals are recognised and remembered.
- **Translation with your own AI provider.** OpenAI, Anthropic, Google Gemini,
  OpenRouter, Groq, DeepSeek, Mistral, xAI, or a local Ollama or LM Studio. Show the
  translation under each line or on its own; choose a style, or write your own prompt.
  If the provider declines a song (some models won't translate copyrighted lyrics)
  or is down, the translation backup you pick in Settings → AI providers takes it.
- **Furigana for Japanese,** over or under the kanji, or with the kanji replaced by kana.
- **Word-by-word highlighting** when the lyrics have word timing. With word-level
  timing on (YouTube Music, experimental), Lyryc downloads a speech model and
  aligns the words itself, on your machine.
- **Lyrics ready before a song starts.** On YouTube Music, Lyryc looks up, translates
  and times the next songs in your queue in advance.
- **Timing you can nudge:** a global offset, and one per track.
- **Styling:** font, size, weight, spacing, colours for sung, active and upcoming
  words, glow, shadow and background, with a live preview.
- **Updates itself** from GitHub Releases.

### Supported players

| Player | Works with |
| --- | --- |
| YouTube Music | Everything above, including its own lyrics, upcoming-song preparation and word-level timing |
| Spotify Web Player, Apple Music (web), SoundCloud | Track detection and lyrics lookup |

Release builds are Windows only. The code has macOS and Linux paths, but they
aren't built or tested.

## Install (Windows)

1. From the [latest release](https://github.com/Error0229/Lyryc/releases/latest),
   download `Lyryc_<version>_x64-setup.exe` and `lyryc-extension.zip`. Run the
   installer; it doesn't need admin rights.
2. Unzip the extension. Open `chrome://extensions` (or `edge://extensions`), turn on
   **Developer mode**, click **Load unpacked** and pick the unzipped folder.
3. Reload any music tab that was already open, so the extension can see it.

Lyryc checks for updates shortly after it starts and every 6 hours after that. When
one is out, an arrow appears in the overlay's toolbar and under Settings → About.
Clicking it installs the update and restarts the app. The extension doesn't update
itself: after an update, unzip the new `lyryc-extension.zip` over the old folder and
click reload on `chrome://extensions`.

## Using it

| | |
| --- | --- |
| **Ctrl+Shift+D** | Lock the strip, or unlock it to move it |
| **Ctrl+Shift+M** | Show or hide the lyrics |
| **Esc** (while unlocked) | Lock again |

Right-click the tray icon to show or hide the lyrics, lock or move the strip, reset
its position, open Settings or quit. When you start Lyryc for the first time, the
strip opens unlocked so you can put it where you want it. Lock it once it's in place.

Translation needs an AI provider. Add one under **Settings → AI providers** (paste
an API key, or point it at a local Ollama or LM Studio), then turn it on under
**Translation**.

## Privacy and keys

- **API keys** are stored in `llm-settings.json` in the app's data folder (on Windows,
  `%APPDATA%\com.lyryc.app`). The file is plain JSON, written atomically and readable
  only by your user. Keys are not in the OS keychain, so anything running as you
  can read them. Settings shows only a masked hint and never sends a key back to
  the window.
- **What goes to your AI provider**: song titles and artists (to clean titles and identify
  songs), the lyric lines you have translated, and, with "Identify songs with web
  search", a web search the provider runs. Nothing goes to a provider you haven't
  added, and keys go only to that provider's base URL. A plain `http://` base URL
  (a local Ollama or LM Studio) sends the key unencrypted.
- **Lyrics lookups** send the title, artist and length to LRCLIB and, with "Extra lyric
  sources", to QQ Music, NetEase and Kugou.
- **Word-level timing** downloads the playing song's audio from YouTube with yt-dlp,
  aligns it on your machine and deletes the audio straight away.
- **The extension** talks only to the app on `localhost:8765`. The app refuses web pages
  that try to connect.
- **The log** (see below) records song titles and artists, never keys.

## Troubleshooting

- **Nothing shows when music plays:** reload the music tab. The
  extension only sees tabs opened or reloaded after it was installed or updated.
  The extension's toolbar popup says whether it's connected to the app.
- **Translation doesn't appear:** check that a provider is added and set for
  translation, and use **Test** next to it under AI providers. Free tiers run out
  quickly; Lyryc retries, but a provider that's out of quota stays out until it resets.
- **The log:** `%LOCALAPPDATA%\com.lyryc.app\logs\lyryc.log`. It holds what the app
  and the overlay did, including errors. Attach it when you report a problem.

## Development

You need [Node.js](https://nodejs.org/) 22 and [Rust](https://rustup.rs/) (stable),
plus the [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your OS.

```bash
git clone https://github.com/Error0229/Lyryc.git
cd Lyryc
npm install
npm run tauri dev          # the app, with the frontend hot-reloading
```

Load `extension/` unpacked in Chrome or Edge (see Install), and reload the music tab.

| Command | |
| --- | --- |
| `npm test` | Frontend tests (Vitest) |
| `cargo test --lib` (in `src-tauri/`) | Backend tests |
| `npm run build` | Type-check and build the frontend |
| `npm run tauri build` | Build the installer locally |
| `npm run dev:debug` | Run with `RUST_LOG=debug` |

### Layout

```
src/                  React frontend for the app's three windows
  overlay/            the lyrics strip
  settings/           the settings window
  tray/               the tray menu
  state/  services/   settings store, typed IPC to the backend
src-tauri/src/        Rust backend
  sources/ ytmusic/   lyrics sources
  lyrics.rs           the lookup: cache, sources, covers, instrumentals
  translation/ llm/   translation and the AI providers
  prealign/           word-level timing (docs/prealign.md)
  placement.rs        where the strip sits, lock, drag, hover
  websocket*.rs       the extension's connection
extension/            the browser extension (Manifest V3)
docs/  research/      design notes and research reports, and the code behind them
archive/              retired material kept for reference
```

### Releasing

```bash
npm version minor        # or patch / major: bumps package.json (the only version source), commits, tags vX.Y.Z
git push --follow-tags
```

The tag runs `.github/workflows/release.yml`, which builds the NSIS installer, signs it for the updater, and opens a **draft** release with `latest.json` and `lyryc-extension.zip`. Installed apps only see published releases, so **publishing the draft is what ships the update**. Edit the notes before you publish: they appear in the app's update prompt.

Updates are signed with the key in the `TAURI_SIGNING_PRIVATE_KEY` repo secret, and its public half is in `src-tauri/tauri.conf.json`. Keep a backup of the private key. If it's lost, existing installs can't verify new updates and every user has to reinstall by hand.

## Acknowledgments

- [LRCLIB](https://lrclib.net/) for free synced lyrics
- [Tauri](https://tauri.app/), [React](https://react.dev/) and [Motion](https://motion.dev/)
- [yt-dlp](https://github.com/yt-dlp/yt-dlp), [ONNX Runtime](https://onnxruntime.ai/) and the [MMS](https://huggingface.co/docs/transformers/model_doc/mms) alignment model for word-level timing
