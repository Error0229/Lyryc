# Prototype: tab audio capture for lyric alignment (NOT wired in)

Research prototype from `rebuild/timing` (see `docs/research/timing/REPORT.md`).
It is deliberately **not** referenced from `extension/manifest.json`, because merging
it adds two permissions (`tabCapture`, `offscreen`) and Chrome will re-prompt users.

Flow (Chrome 116+, MV3):
1. The user clicks the extension action (Chrome requires a user gesture for `tabCapture`).
2. `capture-sw.js` calls `chrome.tabCapture.getMediaStreamId({targetTabId})` and passes the
   id to an offscreen document (`offscreen.html`).
3. `offscreen.js` opens the stream with `getUserMedia`. It re-plays the audio to the
   speakers, because capturing a tab mutes it. It then downmixes to 16 kHz mono PCM16 in an
   AudioWorklet and sends 1 s binary frames to `ws://localhost:8765` as
   `{"message_type":"audio_chunk"}` JSON headers followed by binary PCM.

To try it, merge `manifest.fragment.json` into the manifest and import `capture-sw.js`
from `background.js`. The desktop side (an `audio_chunk` handler) is not implemented.
