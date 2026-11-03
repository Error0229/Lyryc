// Isolated-world relay for queue-main.js (prototype). Forwards the queue to
// background.js as UPCOMING_TRACKS; background would pass it on to the app's
// websocket like the other TRACK_* messages.

window.addEventListener("message", (e) => {
  if (e.source !== window || e.data?.source !== "lyryc-queue") return;
  try {
    chrome.runtime.sendMessage({ type: "UPCOMING_TRACKS", data: e.data.state });
  } catch {
    // Extension reloaded; this content script is orphaned.
  }
});
