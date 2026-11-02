"""Whisper word timestamps (faster-whisper / CTranslate2, CPU int8) matched to the known lyrics.

Whisper transcribes the song; we then align transcript and lyric text as romanized
character streams (difflib), so kanji/kana or trad/simp differences still match.
Each lyric line takes the time of its first matched character, backed off by the
characters before it at a nominal singing rate; unmatched lines are interpolated.

Usage: uv run python methods/whisper_match.py --model small [--stems] [--ids ...]
"""
import argparse, difflib, sys, time, pathlib
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent / "bench"))
from faster_whisper import WhisperModel
from common import ROOT, load
from results import save
from romanize import romanize
from measure import PeakRSS
from align_text import interpolate

CHAR_S = 0.06  # nominal seconds per romanized char, only used to back off to line start
# whisper.cpp ggml q5_1 file sizes (what Lyryc would ship), from the whisper.cpp HF repo
SHIP_MB = {"tiny": 32, "base": 60, "small": 190, "medium": 539, "large-v3-turbo": 574}


def transcript_chars(segments, lang):
    chars, times = [], []
    for seg in segments:
        for w in seg.words or []:
            r = "".join(romanize(w.word, lang))
            for k, c in enumerate(r):
                chars.append(c)
                times.append(w.start + (w.end - w.start) * k / max(1, len(r)))
    return "".join(chars), times


def match(lines, lang, tchars, ttimes, duration):
    lchars, owner = [], []
    for i, line in enumerate(lines):
        r = "".join(romanize(line, lang))
        lchars.extend(r)
        owner.extend([i] * len(r))
    sm = difflib.SequenceMatcher(None, "".join(lchars), tchars, autojunk=False)
    first = {}
    for a, b, size in sm.get_matching_blocks():
        if size < 3:  # ignore chance matches of 1-2 chars
            continue
        for k in range(size):
            i = owner[a + k]
            if i not in first:
                line_off = (a + k) - owner.index(i)
                first[i] = ttimes[b + k] - line_off * CHAR_S
    times = [first.get(i) for i in range(len(lines))]
    # enforce monotonicity: drop anchors that go backwards
    last = -1.0
    for i, t in enumerate(times):
        if t is not None and t < last:
            times[i] = None
        elif t is not None:
            last = t
    return interpolate(times, duration), sum(t is not None for t in times)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--model", default="small")
    ap.add_argument("--stems", action="store_true")
    ap.add_argument("--ids", default=None)
    a = ap.parse_args()
    data = load()
    if a.ids:
        data = [t for t in data if str(t["id"]) in a.ids.split(",")]
    res = {}
    with PeakRSS() as m:
        model = WhisperModel(a.model, device="cpu", compute_type="int8", cpu_threads=8)
        for t in data:
            path = ROOT / ("data/stems" if a.stems else "data/audio") / f"{t['id']}.wav"
            if not path.exists():
                continue
            t0 = time.perf_counter()
            segs, _ = model.transcribe(str(path), language=t["lang"], word_timestamps=True,
                                       condition_on_previous_text=False, vad_filter=False,
                                       initial_prompt=" ".join(t["lines"][:4]))
            tchars, ttimes = transcript_chars(list(segs), t["lang"])
            times, anchors = match(t["lines"], t["lang"], tchars, ttimes, t["duration"])
            secs = time.perf_counter() - t0
            res[t["id"]] = {"times": times if anchors else None, "secs": secs, "meta": {"anchors": anchors}}
            print(t["id"], t["track"], f"{secs:.1f}s anchors {anchors}/{len(t['lines'])}", flush=True)
    name = f"a_whisper_{a.model}" + ("_vocals" if a.stems else "")
    save(name, res, {"download_mb": SHIP_MB.get(a.model, "?"), "ram_mb": m.peak_mb, "needs_audio": True})
    print("peak MB", m.peak_mb)


if __name__ == "__main__":
    main()
