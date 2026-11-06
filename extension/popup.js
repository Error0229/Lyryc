// The toolbar popup: is the desktop app connected, and what is playing.
const SOURCES = {
  "youtube-music": "YouTube Music",
  spotify: "Spotify",
  "apple-music": "Apple Music",
  soundcloud: "SoundCloud",
};

const $ = (id) => document.getElementById(id);

function show({ connected, track, error }) {
  const conn = $("conn");
  conn.dataset.state = error ? "error" : connected ? "on" : "off";
  conn.textContent = error ? "Extension error" : connected ? "Connected" : "App not running";
  $("hint").hidden = connected || !!error;

  $("track").hidden = !track;
  $("empty").hidden = !!track;
  if (!track) return;
  $("title").textContent = track.title;
  $("artist").textContent = track.artist;
  const source = SOURCES[track.source] ?? track.source ?? "";
  const playing = track.isPlaying !== false;
  $("where").textContent = [source, playing ? "Playing" : "Paused"].filter(Boolean).join(" · ");
  $("eyebrow").toggleAttribute("data-paused", !playing);
}

async function refresh() {
  try {
    const r = await chrome.runtime.sendMessage({ type: "GET_CURRENT_TRACK" });
    show({ connected: !!r?.connected, track: r?.track?.title ? r.track : null });
  } catch (e) {
    console.error("Lyryc popup: couldn't reach the extension's background worker", e);
    show({ connected: false, track: null, error: true });
  }
}

refresh();
setInterval(refresh, 2000); // the popup stays current while it is open
