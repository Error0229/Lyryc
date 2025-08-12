export interface PlainLine {
  text: string;
}

export interface AlignedLine {
  time: number; // seconds
  text: string;
  duration: number; // seconds
}

export interface TextAlignOptions {
  totalDurationSec: number; // seconds
  minLineDurationSec?: number;
  maxLineDurationSec?: number;
}

// Split plain lyrics into logical lines, ignoring empty lines around blocks
export function splitPlainLyrics(plain: string): PlainLine[] {
  return plain
    .split(/\r?\n/)
    .map((l) => l.trim())
    .filter((l, idx, arr) => {
      if (!l) return false;
      // drop duplicated consecutive blank lines (already filtered) and headers like [Verse]
      if (/^\[.*?\]$/.test(l)) return false;
      return true;
    })
    .map((text) => ({ text }));
}

// Compute a per-line weight based on length, punctuation, and simple heuristics
function lineWeight(text: string): number {
  let w = 1;
  w += Math.min(text.length / 12, 6); // length factor
  const punct = (text.match(/[,.!?;:]/g) || []).length;
  w += punct * 0.5;
  const longWords = (text.match(/\b\w{7,}\b/g) || []).length;
  w += longWords * 0.3;
  // chorus or repeated markers tend to be slower
  if (/\b(chorus|hook|副歌)\b/i.test(text)) w += 1;
  // emojis/special chars hint at emphasis
  const special = (text.match(/[^\w\s]/g) || []).length;
  w += Math.min(special * 0.1, 1);
  return Math.max(0.5, w);
}

export function alignPlainLyrics(
  plain: string,
  options: TextAlignOptions
): AlignedLine[] {
  const { totalDurationSec, minLineDurationSec = 1.0, maxLineDurationSec = 8.0 } = options;
  const lines = splitPlainLyrics(plain);
  if (lines.length === 0 || totalDurationSec <= 0) return [];

  // Assign weights and normalize
  const weights = lines.map((l) => lineWeight(l.text));
  const totalWeight = weights.reduce((a, b) => a + b, 0);
  const allocations = weights.map((w) => (w / totalWeight) * totalDurationSec);

  // Clamp per-line durations and renormalize leftover/overflow
  let durations = allocations.map((d) => Math.min(maxLineDurationSec, Math.max(minLineDurationSec, d)));
  const sumDur = durations.reduce((a, b) => a + b, 0);
  // Scale durations to exactly fit total duration
  const scale = totalDurationSec / sumDur;
  durations = durations.map((d) => d * scale);

  const result: AlignedLine[] = [];
  let t = 0;
  for (let i = 0; i < lines.length; i++) {
    result.push({ time: t, text: lines[i].text, duration: durations[i] });
    t += durations[i];
  }
  // If rounding left a gap due to floats, ensure last line ends at totalDurationSec
  if (result.length > 0 && t !== totalDurationSec) {
    const last = result[result.length - 1];
    last.duration += totalDurationSec - t;
  }
  return result;
}

// Parse LRC to AlignedLine for evaluation
export function parseLRCToAligned(lrc: string): AlignedLine[] {
  const out: AlignedLine[] = [];
  const lines = lrc.split(/\r?\n/);
  for (const line of lines) {
    const m = line.match(/^\[(\d{1,2}):(\d{2})(?:\.(\d{1,3}))?\](.*)$/);
    if (!m) continue;
    const min = parseInt(m[1], 10);
    const sec = parseInt(m[2], 10);
    const frac = m[3] ? parseInt(m[3].padEnd(3, '0'), 10) : 0; // ms
    const time = min * 60 + sec + frac / 1000;
    const text = m[4].trim();
    if (text) out.push({ time, text, duration: 0 });
  }
  // derive durations
  out.sort((a, b) => a.time - b.time);
  for (let i = 0; i < out.length; i++) {
    const next = out[i + 1];
    out[i].duration = next ? Math.max(0, next.time - out[i].time) : 3;
  }
  return out;
}

export interface AlignmentMetrics {
  mae: number;
  rmse: number;
  meanOffset: number;
  matched: number;
}

export function compareAlignments(a: AlignedLine[], b: AlignedLine[]): AlignmentMetrics {
  // Match by index after filtering to similar sized sets
  const n = Math.min(a.length, b.length);
  if (n === 0) return { mae: 0, rmse: 0, meanOffset: 0, matched: 0 };
  let sumAbs = 0;
  let sumSq = 0;
  let sum = 0;
  for (let i = 0; i < n; i++) {
    const diff = a[i].time - b[i].time;
    sum += diff;
    sumAbs += Math.abs(diff);
    sumSq += diff * diff;
  }
  return {
    mae: sumAbs / n,
    rmse: Math.sqrt(sumSq / n),
    meanOffset: sum / n,
    matched: n,
  };
}

