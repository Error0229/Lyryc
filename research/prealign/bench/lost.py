"""Lost alignments, for the track-level reject rule: one track's lyrics on another's audio,
and full lyrics on cut audio (a short version). Prints each run's line-confidence shares.

  uv run python -X utf8 bench/lost.py
"""
import json
import numpy as np, soundfile as sf, onnxruntime as ort
from run_echo import ROOT, ENC, lattice, labels, ranges, output
from run import source_text
from echo_head import Head, pack, text_share
from echo_decode import activity, decode
tracks = {t["id"]: t for t in json.loads((ROOT / "tracks.json").read_text(encoding="utf-8"))}
refs = json.loads((ROOT / "data/refs.json").read_text(encoding="utf-8"))
so = ort.SessionOptions(); so.intra_op_num_threads = 8
enc = ort.InferenceSession(str(ENC), so, providers=["CPUExecutionProvider"]); head = Head("onnx-int8", 8)
def run(audio_id, text_id, cut=1.0):
    x, _ = sf.read(ROOT / "data/audio" / f"{audio_id}.wav", dtype="float32"); x = x[: int(len(x) * cut)]
    t = tracks[text_id]; _, lines = source_text(refs[str(text_id)], t)
    E, hid = lattice(enc, x); plan, encoded, line_of = labels(lines, t["lang"])
    S = text_share(head, hid, E, *pack(encoded, line_of)); T = len(E)
    out = decode(E, activity(x, T), S, encoded, ranges(line_of), (199.5 + 320 * np.arange(T)) / 16000, len(x) / 16000 / T)
    c = np.array([l["conf"] for l in output(lines, plan, out, E, encoded, S) if l["start"] is not None])
    print(f"audio {audio_id} text {text_id} cut {cut}: median conf {np.median(c):.3f}, <0.10 {np.mean(c < 0.10):.0%}, <0.15 {np.mean(c < 0.15):.0%}, <0.20 {np.mean(c < 0.20):.0%}", flush=True)
for a, b in [(0, 1), (5, 7), (10, 11), (16, 17)]:
    run(a, b)
for a, cut in [(0, 0.45), (5, 0.45), (11, 0.45), (15, 0.6)]:
    run(a, a, cut)
