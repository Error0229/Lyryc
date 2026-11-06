// YouTube Music "Up next" reader (prototype, MAIN world).
//
// Source: the page's own Redux store, reached through the queue element
// (`ytmusic-player-queue`.getState().queue). It holds the queue as data, so it
// survives CSS/layout redesigns and works with the queue panel closed. It is only
// visible from the page's JS world: an isolated-world content script sees the
// element but not its Polymer properties, hence `"world": "MAIN"` and a
// postMessage hop to queue-bridge.js.
//
// Verified against music.youtube.com on 2026-11-02, logged out:
//   queue.items              history + current, in play order
//   queue.selectedItemIndex  position of the current item in `items`
//   queue.automixItems       autoplay continuation (used when `items` runs out)
//   item.playlistPanelVideoRenderer | item.playlistPanelVideoWrapperRenderer.primaryRenderer...
//   renderer.navigationEndpoint...musicVideoType  MUSIC_VIDEO_TYPE_ATV (song) | _OMV (music video)
// player.getPlaylist() is empty on YTM, so the player API is not an alternative.

(() => {
  const TAG = "lyryc-queue";
  const LOOKAHEAD = 3;
  const POLL_MS = 1000;

  function renderer(item) {
    const wrap = item.playlistPanelVideoWrapperRenderer;
    return (wrap ? wrap.primaryRenderer : item)?.playlistPanelVideoRenderer || null;
  }

  function entry(item) {
    const r = renderer(item);
    if (!r?.videoId) return null;
    const cfg = r.navigationEndpoint?.watchEndpoint?.watchEndpointMusicSupportedConfigs?.watchEndpointMusicConfig;
    return {
      videoId: r.videoId,
      title: r.title?.runs?.map((x) => x.text).join("") ?? null,
      artist: r.shortBylineText?.runs?.[0]?.text ?? null,
      length: r.lengthText?.runs?.[0]?.text ?? null,
      type: cfg?.musicVideoType ?? null,
    };
  }

  function read() {
    const q = document.querySelector("ytmusic-player-queue")?.getState?.()?.queue;
    if (!q?.items) return null;
    const items = q.items.map(entry);
    const pos = q.selectedItemIndex ?? items.findIndex((_, i) => renderer(q.items[i])?.selected);
    const tail = q.autoplay ? (q.automixItems || []).map(entry) : [];
    const upcoming = [...items.slice(pos + 1), ...tail].filter(Boolean);
    return {
      current: items[pos] || null,
      position: pos,
      upNext: upcoming.slice(0, LOOKAHEAD),
      ad: document.getElementById("movie_player")?.classList.contains("ad-showing") ?? false,
    };
  }

  let last = "";
  setInterval(() => {
    let state;
    try {
      state = read();
    } catch {
      return; // store shape changed: stay silent, the app falls back to no prefetch
    }
    if (!state) return;
    const key = JSON.stringify([state.current?.videoId, state.upNext.map((e) => e.videoId)]);
    if (key === last) return;
    last = key;
    window.postMessage({ source: TAG, state }, location.origin);
  }, POLL_MS);
})();
