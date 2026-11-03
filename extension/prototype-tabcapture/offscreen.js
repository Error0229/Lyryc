// Offscreen document: owns the MediaStream and streams PCM to the desktop app.
let ctx = null;

chrome.runtime.onMessage.addListener(async (msg) => {
  if (msg.target !== 'offscreen' || msg.type !== 'start-capture') return;
  const stream = await navigator.mediaDevices.getUserMedia({
    audio: { mandatory: { chromeMediaSource: 'tab', chromeMediaSourceId: msg.streamId } },
  });
  ctx = new AudioContext();
  await ctx.audioWorklet.addModule('pcm-worklet.js');
  const src = ctx.createMediaStreamSource(stream);
  src.connect(ctx.destination); // capturing mutes the tab; keep it audible
  const node = new AudioWorkletNode(ctx, 'pcm-16k');
  src.connect(node);

  const ws = new WebSocket('ws://localhost:8765');
  ws.binaryType = 'arraybuffer';
  let seq = 0;
  node.port.onmessage = ({ data }) => {
    if (ws.readyState !== WebSocket.OPEN) return;
    // Header carries wall-clock capture time; the app maps it to track position
    // using the extension's currentTime updates.
    ws.send(JSON.stringify({ message_type: 'audio_chunk', data: { seq: seq++, tabId: msg.tabId, sampleRate: 16000, capturedAt: Date.now() } }));
    ws.send(data);
  };
});
