"""Align every track's lyric text to its YouTube Music audio -> data/pred/<name>.json.

The input text is the QQ lyric (credits stripped), else Kugou, NetEase, LRCLIB: the
aligner is given the reference's words and has to find their times, which is what
the app would do with any plain text. Per-track timings go to results/speed_<name>.json
(no lyric text, so that file is committed).

Usage: uv run python -X utf8 bench/run.py [--backend torch|torch-int8|onnx] [--onnx models/x.onnx]
           [--threads 8] [--chunk 30] [--stems] [--ids 0,1] [--name NAME]
"""
import argparse, json, pathlib, time
import soundfile as sf
import torch
from align import Model, run
from score import clean
from measure import PeakRSS

ROOT = pathlib.Path(__file__).resolve().parent.parent
ORDER = ["qq", "kugou", "netease", "lrclib"]


def source_text(refs, t):
    for src in ORDER:
        r = refs.get(src)
        if r and not r.get("error") and r.get("lines"):
            lines = clean(r["lines"], t["track"], t["artist"])
            if lines:
                return src, [l["text"] for l in lines]
    return None, None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--backend", default="torch")
    ap.add_argument("--onnx", default=None)
    ap.add_argument("--threads", type=int, default=8)
    ap.add_argument("--chunk", type=int, default=30)
    ap.add_argument("--ctx", type=int, default=0)
    ap.add_argument("--stems", action="store_true")
    ap.add_argument("--ids", default=None)
    ap.add_argument("--name", default=None)
    a = ap.parse_args()
    name = a.name or f"{a.backend}_t{a.threads}_c{a.chunk}" + (f"x{a.ctx}" if a.ctx else "") + ("_vocals" if a.stems else "")
    tracks = json.loads((ROOT / "tracks.json").read_text(encoding="utf-8"))
    refs = json.loads((ROOT / "data/refs.json").read_text(encoding="utf-8"))
    preds, speed = {}, {}
    with PeakRSS() as mem:
        t0 = time.perf_counter()
        model = Model(a.backend, a.threads, a.onnx)
        load_s = time.perf_counter() - t0
        for t in tracks:
            if a.ids and str(t["id"]) not in a.ids.split(","):
                continue
            path = ROOT / ("data/stems" if a.stems else "data/audio") / f"{t['id']}.wav"
            src, lines = source_text(refs[str(t["id"])], t)
            if not path.exists() or not lines:
                continue
            audio, _ = sf.read(path, dtype="float32")
            wav = torch.from_numpy(audio).unsqueeze(0)
            out, em_s, vit_s = run(model, wav, lines, t["lang"], a.chunk, a.ctx)
            preds[t["id"]] = {"text_src": src, "lines": out}
            dur = len(audio) / 16000
            speed[t["id"]] = {"audio_s": dur, "emission_s": em_s, "viterbi_s": vit_s, "rtf": (em_s + vit_s) / dur}
            print(t["id"], t["track"], src, f"em {em_s:.1f}s vit {vit_s:.1f}s rtf {(em_s + vit_s) / dur:.3f}", flush=True)
    (ROOT / "data/pred").mkdir(parents=True, exist_ok=True)
    (ROOT / f"data/pred/{name}.json").write_text(json.dumps(preds, ensure_ascii=False), encoding="utf-8")
    (ROOT / "results").mkdir(exist_ok=True)
    (ROOT / f"results/speed_{name}.json").write_text(json.dumps(
        {"backend": a.backend, "threads": a.threads, "chunk": a.chunk, "ctx": a.ctx, "load_s": load_s, "peak_rss_mb": mem.peak_mb,
         "tracks": speed}, indent=1), encoding="utf-8")
    print("load", f"{load_s:.1f}s", "peak MB", mem.peak_mb)


if __name__ == "__main__":
    main()
