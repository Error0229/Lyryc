// YouTube Music "Up next" reader (MAIN world). Feeds the app's opt-in
// pre-aligner, which fetches and aligns the next songs before they play.
//
// Source: the page's own Redux store, reached through the queue element
// (`ytmusic-player-queue`.getState().queue). It holds the queue as data, works
// with the panel closed and during ads, and is only visible from the page's JS
// world, hence `"world": "MAIN"` and a postMessage hop to youtube-detector.js.
// Verified on music.youtube.com 2026-11-02 (logged out); see
// docs/research/prealign/REPORT.md, "Queue reader".
//
//   queue.items              history + current, in play order
//   queue.selectedItemIndex  position of the current item in `items`
//   queue.automixItems       autoplay continuation (used when `items` runs out)
//   item.playlistPanelVideoRenderer | item.playlistPanelVideoWrapperRenderer
//     .primaryRenderer / .counterpart[].counterpartRenderer   (song ↔ video pair)
//   renderer.navigationEndpoint...musicVideoType   MUSIC_VIDEO_TYPE_ATV (song) | _OMV (video)
//
// Song/video pairs: the id that will actually play depends on the player's
// mode (OMV_PREFERRED = video), so pick the matching side of the pair. If the
// store changes shape the script goes silent and the app simply doesn't
// prefetch, which is the behaviour without it.

(() => {
  if (window.__lyrycQueue) return; // injected twice (install + manifest)
  window.__lyrycQueue = true;

  const TAG = "lyryc-queue";
  const LOOKAHEAD = 3;
  const POLL_MS = 1000;
  // Repeat an unchanged snapshot this often, for a relay or app that restarted.
  const RESEND_MS = 30_000;

  const typeOf = (r) =>
    r?.navigationEndpoint?.watchEndpoint?.watchEndpointMusicSupportedConfigs?.watchEndpointMusicConfig?.musicVideoType ?? null;

  /** The renderer that will play: the side of a song/video pair that matches the mode. */
  function renderer(item, videoMode) {
    const wrap = item?.playlistPanelVideoWrapperRenderer;
    if (!wrap) return item?.playlistPanelVideoRenderer || null;
    const sides = [wrap.primaryRenderer, ...(wrap.counterpart || []).map((c) => c.counterpartRenderer)]
      .map((x) => x?.playlistPanelVideoRenderer)
      .filter(Boolean);
    const want = videoMode ? "MUSIC_VIDEO_TYPE_OMV" : "MUSIC_VIDEO_TYPE_ATV";
    return sides.find((r) => typeOf(r) === want) || sides[0] || null;
  }

  function entry(item, videoMode) {
    const r = renderer(item, videoMode);
    if (!r?.videoId) return null;
    return {
      videoId: r.videoId,
      title: r.title?.runs?.map((x) => x.text).join("") ?? null,
      artist: r.shortBylineText?.runs?.[0]?.text ?? null,
      length: r.lengthText?.runs?.[0]?.text ?? null,
      type: typeOf(r),
    };
  }

  function read() {
    const q = document.querySelector("ytmusic-player-queue")?.getState?.()?.queue;
    if (!q?.items) return null;
    const videoMode = q.playbackContentMode === "OMV_PREFERRED";
    const pos = q.selectedItemIndex ?? q.items.findIndex((it) => renderer(it, videoMode)?.selected);
    const items = q.items.map((it) => entry(it, videoMode));
    const tail = q.autoplay ? (q.automixItems || []).map((it) => entry(it, videoMode)) : [];
    const upcoming = [...items.slice(pos + 1), ...tail].filter(Boolean);
    return {
      current: items[pos] || null,
      position: pos,
      upNext: upcoming.slice(0, LOOKAHEAD),
      ad: document.getElementById("movie_player")?.classList.contains("ad-showing") ?? false,
    };
  }

  let last = "";
  let sentAt = 0;
  setInterval(() => {
    let state;
    try {
      state = read();
    } catch {
      return; // store shape changed: stay silent
    }
    if (!state) return;
    const key = JSON.stringify([state.current?.videoId, state.upNext.map((e) => e.videoId)]);
    if (key === last && Date.now() - sentAt < RESEND_MS) return;
    last = key;
    sentAt = Date.now();
    window.postMessage({ source: TAG, state }, location.origin);
  }, POLL_MS);
})();
