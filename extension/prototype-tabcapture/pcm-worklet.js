// Downmix to mono and decimate to 16 kHz, emitting Int16 frames of 1 s.
class Pcm16k extends AudioWorkletProcessor {
  constructor() {
    super();
    this.ratio = sampleRate / 16000;
    this.acc = 0;
    this.buf = new Int16Array(16000);
    this.n = 0;
  }
  process([input]) {
    const ch = input.length;
    if (!ch) return true;
    for (let i = 0; i < input[0].length; i++) {
      this.acc += 1;
      if (this.acc < this.ratio) continue;
      this.acc -= this.ratio;
      let s = 0;
      for (let c = 0; c < ch; c++) s += input[c][i];
      this.buf[this.n++] = Math.max(-1, Math.min(1, s / ch)) * 0x7fff;
      if (this.n === this.buf.length) {
        this.port.postMessage(this.buf.slice().buffer, []);
        this.n = 0;
      }
    }
    return true;
  }
}
registerProcessor('pcm-16k', Pcm16k);
