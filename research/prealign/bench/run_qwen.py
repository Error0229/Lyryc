"""Qwen3-ForcedAligner-0.6B on the same tracks and text as run.py -> data/pred/<name>.json.

Units are this bench's own pieces (a Latin word, one CJK / kana / hangul character), passed in the
original script, so the result is scored on exactly the units the MMS run is scored on.
Qwen's aligner stops at 5 minutes: a longer track is cut once, at the line boundary nearest its
middle, by the source's own line times (the app would have them from LRCLIB), with 2 s overlap.
Runs in its own venv (transformers 4.57 + qwen-asr); needs pykakasi, uroman, torchaudio for align.py.

  python -X utf8 bench/run_qwen.py [--threads 8] [--ids 0,1] [--name qwen] [--dtype float32|bfloat16]
"""
import argparse, json, pathlib, time
import numpy as np, soundfile as sf, torch
from align import pieces, fill
from run import source_text, ORDER
from score import clean
from measure import PeakRSS

ROOT = pathlib.Path(__file__).resolve().parent.parent
LANG = {"en": "English", "ja": "Japanese", "zh": "Chinese", "ko": "Korean"}
MAX_S, OVERLAP_S = 295, 2.0


def split_point(refs, t, lines, dur):
    """Index of the first line of the second half, and its source time."""
    for src in ORDER:
        r = refs.get(src)
        if r and not r.get("error") and r.get("lines"):
            ls = clean(r["lines"], t["track"], t["artist"])
            if [l["text"] for l in ls] == lines:
                k = min(range(1, len(ls)), key=lambda i: abs(ls[i]["start"] - dur / 2))
                return k, ls[k]["start"]
    raise ValueError("no line times for the source text")


def align_once(fa, audio, units, lang):
    proc = fa.aligner_processor
    orig = proc.encode_timestamp
    proc.encode_timestamp = lambda text, language: (units, "<|audio_start|><|audio_pad|><|audio_end|>"
                                                     + "<timestamp><timestamp>".join(units) + "<timestamp><timestamp>")
    try:
        with torch.inference_mode():
            res = fa.align(audio=(audio, 16000), text="x", language=LANG[lang])[0]
    finally:
        proc.encode_timestamp = orig
    return [(it.start_time, it.end_time) for it in res]


def line_times(refs, t, lines):
    for src in ORDER:
        r = refs.get(src)
        if r and not r.get("error") and r.get("lines"):
            ls = clean(r["lines"], t["track"], t["artist"])
            if [l["text"] for l in ls] == lines:
                return [l["start"] for l in ls]
    raise ValueError("no line times for the source text")


def windows(starts, dur, win_s, margin):
    """Consecutive lines grouped into windows of <= win_s; audio = group span +- margin. Upper bound: uses line times."""
    segs, a = [], 0
    while a < len(starts):
        b = a + 1
        while b < len(starts) and starts[b] - starts[a] < win_s:
            b += 1
        end = starts[b] if b < len(starts) else dur
        segs.append((a, b, max(0.0, starts[a] - margin), min(dur, end + margin)))
        a = b
    return segs


def align_track(fa, audio, lines, lang, t, refs, win_s=0):
    plan = [pieces(text, lang) for text in lines]
    dur = len(audio) / 16000
    segs = [(0, len(lines), 0.0, dur)]
    if win_s:
        segs = windows(line_times(refs, t, lines), dur, win_s, 3.0)
    elif dur > MAX_S:
        k, at = split_point(refs, t, lines, dur)
        segs = [(0, k, 0.0, at + OVERLAP_S), (k, len(lines), max(0.0, at - OVERLAP_S), dur)]
    times = {}
    for a, b, s0, s1 in segs:
        keys = [(li, pi) for li in range(a, b) for pi in range(len(plan[li]))]
        units = [lines[li][plan[li][pi][0]:plan[li][pi][0] + plan[li][pi][1]] for li, pi in keys]
        out = align_once(fa, audio[int(s0 * 16000):int(s1 * 16000)], units, lang)
        for key, (st, en) in zip(keys, out):
            times[key] = (st + s0, en + s0)
    res = []
    for li, text in enumerate(lines):
        words = [{"start": times[(li, pi)][0], "end": times[(li, pi)][1], "text": text[pos:pos + n]}
                 for pi, (pos, n, _) in enumerate(plan[li])]
        fill(words)
        timed = [w for w in words if w["start"] is not None]
        res.append({"start": timed[0]["start"] if timed else None, "end": timed[-1]["end"] if timed else None,
                    "text": text, "conf": 1.0, "words": words})
    return res


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--threads", type=int, default=8)
    ap.add_argument("--ids", default=None)
    ap.add_argument("--name", default="qwen")
    ap.add_argument("--dtype", default="float32")
    ap.add_argument("--window", type=float, default=0, help="align in line-grouped windows of this many s (uses line times)")
    a = ap.parse_args()
    torch.set_num_threads(a.threads)
    from qwen_asr import Qwen3ForcedAligner
    tracks = json.loads((ROOT / "tracks.json").read_text(encoding="utf-8"))
    refs = json.loads((ROOT / "data/refs.json").read_text(encoding="utf-8"))
    preds, speed = {}, {}
    with PeakRSS() as mem:
        t0 = time.perf_counter()
        fa = Qwen3ForcedAligner.from_pretrained("Qwen/Qwen3-ForcedAligner-0.6B", dtype=getattr(torch, a.dtype), device_map="cpu")
        load_s = time.perf_counter() - t0
        for t in tracks:
            if a.ids and str(t["id"]) not in a.ids.split(","):
                continue
            path = ROOT / "data/audio" / f"{t['id']}.wav"
            src, lines = source_text(refs[str(t["id"])], t)
            if not path.exists() or not lines:
                continue
            audio, _ = sf.read(path, dtype="float32")
            t1 = time.perf_counter()
            out = align_track(fa, audio, lines, t["lang"], t, refs[str(t["id"])], a.window)
            el = time.perf_counter() - t1
            dur = len(audio) / 16000
            preds[t["id"]] = {"text_src": src, "lines": out}
            speed[t["id"]] = {"audio_s": dur, "emission_s": el, "viterbi_s": 0.0, "rtf": el / dur}
            print(t["id"], t["track"], src, f"{el:.1f}s rtf {el / dur:.3f}", flush=True)
    (ROOT / "data/pred").mkdir(parents=True, exist_ok=True)
    (ROOT / f"data/pred/{a.name}.json").write_text(json.dumps(preds, ensure_ascii=False), encoding="utf-8")
    (ROOT / f"results/speed_{a.name}.json").write_text(json.dumps(
        {"backend": "qwen3-fa", "threads": a.threads, "dtype": a.dtype, "load_s": load_s, "peak_rss_mb": mem.peak_mb,
         "tracks": speed}, indent=1), encoding="utf-8")
    print("load", f"{load_s:.1f}s", "peak MB", mem.peak_mb)


if __name__ == "__main__":
    main()
