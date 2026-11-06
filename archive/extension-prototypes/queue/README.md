# Prototype: YouTube Music "Up next" reader (NOT wired in)

Research prototype from `research/prealign`; see `docs/research/prealign/REPORT.md`.
It is not referenced from `extension/manifest.json`. It needs no new permission, because
`music.youtube.com` is already a host permission.

- `queue-main.js` runs in the page's MAIN world. Every second it reads
  `document.querySelector("ytmusic-player-queue").getState().queue`, which is YTM's own
  Redux store. When the current or upcoming ids change, it posts
  `{current, position, upNext[3], ad}`.
- `queue-bridge.js` runs in the isolated world and forwards that as `UPCOMING_TRACKS` to
  `background.js`.

Why the store and not the DOM:
- The queue panel's DOM items carry no videoId attribute (`data` is a Polymer property and
  invisible from the isolated world).
- ATV thumbnails are album art, not `i.ytimg.com/vi/<id>`.
- The player API's `getPlaylist()` returns nothing on YTM.

The store holds the queue as data, with the panel closed, and also during ads (`#movie_player.ad-showing`).

Each entry carries:
- `type`: `MUSIC_VIDEO_TYPE_ATV` (the song) or `MUSIC_VIDEO_TYPE_OMV` (the music video).
- `length`.

Prefetch the id that will actually play. In video mode (`playbackContentMode: OMV_PREFERRED`)
the queue holds OMVs, whose audio often has a different intro or length than the song.

Verified live on 2026-11-02 (logged out):
- After an ad, `items` = [current] plus `automixItems` (50). After "next", `items` =
  [previous, current], `selectedItemIndex` = 1, and the next id is `automixItems[0]`.
- `queue.autoplay` gates automix.

Failure mode: if YTM renames the store or the element, `read()` throws or returns null and
the script goes silent. The app then simply doesn't prefetch, which is the current behaviour.

To try it, merge `manifest.fragment.json` into the manifest (Chrome 111+ for `"world": "MAIN"`)
and handle `UPCOMING_TRACKS` in `background.js`.
