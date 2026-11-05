// YouTube Music detector.
//
// The title comes from the player bar (the full bilingual name), falling back to
// navigator.mediaSession, which survives layout redesigns. The artist comes from
// mediaSession first. Playback state and time come from the <video> element.
//
// Output: TRACK_DETECTED / TRACK_PAUSED / TRACK_STOPPED / TRACK_PROGRESS
// messages to background.js, plus UPCOMING_TRACKS relayed from the queue
// reader (youtube-queue.js, MAIN world): {current, position, upNext[≤3], ad},
// each entry {videoId, title, artist, length, type}.

const SOURCE = "youtube-music";
const TICK_MS = 250;
// The app runs its own clock between reports; more often only queues them up.
// With YouTube Music in a background tab, Chrome restarts the extension's
// service worker every minute or two, and reports at 4 a second piled up
// behind it, reaching the app 20-60 s late.
const PROGRESS_MS = 1000;
let progressAt = 0;

const $ = (sel) => document.querySelector(sel);
const video = () => $("video");
const isAd = () => $("#movie_player.ad-showing") !== null;

// The queue's playing item (from youtube-queue.js). Its videoId names the track
// even while the page shows a playlist, the library or an artist, whose URL
// has no `v=`.
let queueCurrent = null;

/**
 * The release's own title when the player bar shows a translation of it: in an
 * English UI it shows "From Neverland" where the queue keeps "ネバーランドより -
 * From Neverland". Only when one half of the queue's title is exactly the
 * player's (and the videos agree), so a stale queue at a song change never
 * renames the new song.
 */
function fullTitle(title) {
  const c = queueCurrent;
  if (!c?.title || c.title === title) return title;
  const v = new URL(location.href).searchParams.get("v");
  if (v && c.videoId !== v) return title;
  return c.title.split(" - ").some((half) => half.trim() === title) ? c.title : title;
}

/** A watch URL for the track: the page's own, else the queue's playing item if it is this title. */
function watchUrl(title) {
  if (new URL(location.href).searchParams.has("v")) return location.href;
  const c = queueCurrent;
  return c?.videoId && c.title === title ? `https://music.youtube.com/watch?v=${c.videoId}` : location.href;
}

function text(sel) {
  return $(sel)?.textContent?.trim() || null;
}

function readTrack() {
  const md = navigator.mediaSession?.metadata;
  // Title: the player bar first. mediaSession drops the native-script part of
  // bilingual titles ("雪国 - Snow Country" -> "Snow Country"), and LRCLIB files
  // such songs under the full name. mediaSession is the fallback for redesigns.
  const shown = text("ytmusic-player-bar .content-info-wrapper .title") || md?.title;
  const title = shown && fullTitle(shown);
  // The player-bar byline is "Artist • Album • Year"; mediaSession is just the artist.
  const artist = (md?.artist || text("ytmusic-player-bar .content-info-wrapper .byline"))
    ?.split("•")[0]
    .trim();
  if (!title || !artist) return null;

  const art = md?.artwork;
  const duration = video()?.duration;
  return {
    title,
    artist,
    duration: Number.isFinite(duration) ? duration : null, // lets the app do an exact lookup
    thumbnail: art?.length ? art[art.length - 1].src : $("ytmusic-player-bar img")?.src || null,
    source: SOURCE,
    url: watchUrl(title),
    timestamp: Date.now(),
  };
}

function send(type, data) {
  try {
    chrome.runtime.sendMessage(data ? { type, data } : { type });
  } catch {
    // The extension was reloaded; this content script is orphaned.
    clearInterval(timer);
  }
}

// last = { key, playing, track } for the track we last reported, or null.
let last = null;

function tick() {
  const v = video();
  const track = v && !isAd() ? readTrack() : null;

  if (!track) {
    if (last) send("TRACK_STOPPED");
    last = null;
    return;
  }

  // Playing means the media can advance. A new video still loading (no length
  // yet) or a stall isn't: reported as playing, the app's clock ran 5–8 s
  // ahead at each song start and snapped back when the first real time came.
  const playing = !v.paused && !v.ended && v.readyState >= HTMLMediaElement.HAVE_FUTURE_DATA;
  const key = `${track.title}\u0000${track.artist}`;
  if (!last || last.key !== key || last.playing !== playing) {
    last = { key, playing, track };
    // With the media's own time: without it the app took a resume as 0:00.
    send(playing ? "TRACK_DETECTED" : "TRACK_PAUSED", { ...track, currentTime: v.currentTime });
  }

  if (playing && Date.now() - progressAt >= PROGRESS_MS) {
    progressAt = Date.now();
    send("TRACK_PROGRESS", {
      ...track, // re-read: the length and the queue's videoId can arrive after the title
      currentTime: v.currentTime,
      duration: Number.isFinite(v.duration) ? v.duration : null,
      isPlaying: true,
    });
  }
}

const timer = setInterval(tick, TICK_MS);

// --- Up next (for the app's opt-in pre-aligner) -----------------------------

window.addEventListener("message", (e) => {
  if (e.source !== window || e.origin !== location.origin || e.data?.source !== "lyryc-queue") return;
  queueCurrent = e.data.state?.current ?? null;
  send("UPCOMING_TRACKS", e.data.state);
});

// --- Playback commands from the desktop app --------------------------------

// The player bar renders some buttons twice (desktop and mobile layouts) and
// hides whichever the current width doesn't use. Prefer the visible one, but a
// hidden one still works. Class names are language-independent, aria-labels are not.
function click(sel) {
  const hosts = [...document.querySelectorAll(`ytmusic-player-bar ${sel}`)];
  const host = hosts.find((el) => el.offsetParent !== null) ?? hosts[0];
  if (!host) return false;
  (host.querySelector("button") || host).click();
  return true;
}

const COMMANDS = {
  async play() {
    const v = video();
    if (!v) return false;
    await v.play();
    return true;
  },
  pause() {
    const v = video();
    if (!v) return false;
    v.pause();
    return true;
  },
  next: () => click(".next-button"),
  previous: () => click(".previous-button"),
  seek({ seekTime }) {
    const v = video();
    if (!v || !Number.isFinite(seekTime)) return false;
    v.currentTime = seekTime;
    return true;
  },
};

chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (message.type !== "PLAYBACK_COMMAND") return false;

  const run = COMMANDS[message.command];
  Promise.resolve()
    .then(() => (run ? run(message) : false))
    .then((ok) =>
      sendResponse(
        ok ? { success: true } : { success: false, error: `Command '${message.command}' failed` },
      ),
    )
    .catch((error) => sendResponse({ success: false, error: String(error) }));
  return true; // respond asynchronously
});
