// YouTube Music detector.
//
// The title comes from the player bar (the full bilingual name), falling back to
// navigator.mediaSession, which survives layout redesigns. The artist comes from
// mediaSession first. Playback state and time come from the <video> element.
//
// Output (unchanged): TRACK_DETECTED / TRACK_PAUSED / TRACK_STOPPED /
// TRACK_PROGRESS messages to background.js.

const SOURCE = "youtube-music";
const TICK_MS = 250;

const $ = (sel) => document.querySelector(sel);
const video = () => $("video");
const isAd = () => $("#movie_player.ad-showing") !== null;

function text(sel) {
  return $(sel)?.textContent?.trim() || null;
}

function readTrack() {
  const md = navigator.mediaSession?.metadata;
  // Title: the player bar first. mediaSession drops the native-script part of
  // bilingual titles ("雪国 - Snow Country" -> "Snow Country"), and LRCLIB files
  // such songs under the full name. mediaSession is the fallback for redesigns.
  const title = text("ytmusic-player-bar .content-info-wrapper .title") || md?.title;
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
    url: location.href,
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

  const playing = !v.paused && !v.ended;
  const key = `${track.title}\u0000${track.artist}`;
  if (!last || last.key !== key || last.playing !== playing) {
    last = { key, playing, track };
    send(playing ? "TRACK_DETECTED" : "TRACK_PAUSED", track);
  }

  if (playing) {
    send("TRACK_PROGRESS", {
      ...last.track,
      currentTime: v.currentTime,
      duration: Number.isFinite(v.duration) ? v.duration : null,
      isPlaying: true,
    });
  }
}

const timer = setInterval(tick, TICK_MS);

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
