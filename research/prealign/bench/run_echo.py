"""echora-kara lead aligner, clean-room: int8 ONNX encoder -> echo_head -> echo_decode -> data/pred/<name>.json.

Lattice: one continuous lattice in 15 s windows with 3 s of context each side (align.Model.emissions),
clock (199.5 + 320 t) / 16000 s (spec §10). Labels: one token per align.pieces piece with letters.

  uv run python -X utf8 bench/run_echo.py [--name echo_clean] [--head torch|onnx|onnx-int8] [--threads 8] [--ids 0,1]
"""
import argparse, json, pathlib, time
import numpy as np
import soundfile as sf
import onnxruntime as ort
from align import pieces, fill, SR
from run import source_text
from echo_head import Head, SNAP, pack, text_share
from echo_decode import activity, decode
from measure import PeakRSS

ROOT = pathlib.Path(__file__).resolve().parent.parent
ENC = ROOT / "models/echora_enc.int8.onnx"
VOCAB = json.loads((SNAP / "vocab.json").read_text(encoding="utf-8"))


def lattice(sess, x, chunk_s=15, ctx_s=3):
    """(log_probs [T,30], hidden [T,1024]) with T = (N - 400) // 320 + 1."""
    lp, hid, n, hop, ctx = [], [], len(x), chunk_s * SR, ctx_s * SR
    for s in range(0, n, hop):
        a, e = max(0, s - ctx), min(n, s + hop)
        w = x[a:min(n, e + ctx)]
        if len(w) < 400:
            break
        w = (w - w.mean()) / np.sqrt(w.var() + 1e-7)
        logits, h = sess.run(["logits", "hidden"], {"input_values": w[None].astype(np.float32)})
        lo = (s - a) // 320
        hi = None if e == n else lo + (e - s) // 320
        lp.append(logits[0, lo:hi]); hid.append(h[0, lo:hi])
    logits = np.concatenate(lp)
    m = logits.max(1, keepdims=True)
    return logits - m - np.log(np.exp(logits - m).sum(1, keepdims=True)), np.concatenate(hid)


def labels(lines, lang):
    """plan[line] = [(pos, len, token index or None)], encoded per token, line number per token."""
    plan, encoded, line_of = [], [], []
    for text in lines:
        row, ln = [], len(set(line_of))
        for pos, n, rom in pieces(text, lang):
            ids = [VOCAB[c] for c in rom if c in VOCAB]
            row.append((pos, n, len(encoded) if ids else None))
            if ids:
                encoded.append(ids); line_of.append(ln)
        plan.append(row)
    return plan, encoded, line_of


def ranges(line_of):
    starts = [k for k in range(len(line_of)) if k == 0 or line_of[k] != line_of[k - 1]]
    return [[a, b] for a, b in zip(starts, starts[1:] + [len(line_of)])]


def output(lines, plan, out, E, encoded):
    ids = [c for tok in encoded for c in tok]
    first = np.cumsum([0] + [len(t) for t in encoded])
    prob = [float(np.exp(E[s:e, c]).mean()) for (s, e), c in zip(out["final_letter_spans"], ids)]
    res = []
    for text, row in zip(lines, plan):
        words = [{"start": out["times_s"][k][0] if k is not None else None, "end": out["times_s"][k][1] if k is not None else None,
                  "text": text[pos:pos + n]} for pos, n, k in row]
        fill(words)
        ks = [k for _, _, k in row if k is not None]
        p = [prob[j] for k in ks for j in range(first[k], first[k + 1])]
        timed = [w for w in words if w["start"] is not None]
        res.append({"start": timed[0]["start"] if timed else None, "end": timed[-1]["end"] if timed else None,
                    "text": text, "conf": float(np.mean(p)) if p else 0.0, "words": words})
    return res


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--head", default="onnx-int8", choices=["torch", "onnx", "onnx-int8"])
    ap.add_argument("--threads", type=int, default=8)
    ap.add_argument("--ids", default=None)
    ap.add_argument("--name", default="echo_clean")
    a = ap.parse_args()
    tracks = json.loads((ROOT / "tracks.json").read_text(encoding="utf-8"))
    refs = json.loads((ROOT / "data/refs.json").read_text(encoding="utf-8"))
    preds, speed = {}, {}
    with PeakRSS() as mem:
        t0 = time.perf_counter()
        so = ort.SessionOptions()
        so.intra_op_num_threads = a.threads
        so.graph_optimization_level = ort.GraphOptimizationLevel.ORT_ENABLE_ALL
        enc = ort.InferenceSession(str(ENC), so, providers=["CPUExecutionProvider"])
        head = Head(a.head, a.threads)
        load_s = time.perf_counter() - t0
        for t in tracks:
            if a.ids and str(t["id"]) not in a.ids.split(","):
                continue
            path = ROOT / "data/audio" / f"{t['id']}.wav"
            src, lines = source_text(refs[str(t["id"])], t)
            if not path.exists() or not lines:
                continue
            x, _ = sf.read(path, dtype="float32")
            t0 = time.perf_counter()
            E, hid = lattice(enc, x)
            t1 = time.perf_counter()
            plan, encoded, line_of = labels(lines, t["lang"])
            S = text_share(head, hid, E, *pack(encoded, line_of))
            t2 = time.perf_counter()
            T = len(E)
            out = decode(E, activity(x, T), S, encoded, ranges(line_of), (199.5 + 320 * np.arange(T)) / SR, len(x) / SR / T)
            t3 = time.perf_counter()
            preds[t["id"]] = {"text_src": src, "lines": output(lines, plan, out, E, encoded)}
            dur = len(x) / SR
            speed[t["id"]] = {"audio_s": dur, "emission_s": t2 - t0, "viterbi_s": t3 - t2, "rtf": (t3 - t0) / dur,
                              "encoder_s": t1 - t0, "head_s": t2 - t1}
            print(t["id"], t["track"], src, f"K {len(encoded)} T {T} enc {t1 - t0:.1f}s head {t2 - t1:.1f}s "
                  f"dec {t3 - t2:.1f}s rtf {(t3 - t0) / dur:.3f}{' FALLBACK' if out['fallback'] else ''}", flush=True)
    (ROOT / f"data/pred/{a.name}.json").write_text(json.dumps(preds, ensure_ascii=False), encoding="utf-8")
    (ROOT / f"results/speed_{a.name}.json").write_text(json.dumps(
        {"backend": f"echo clean onnx {ENC.name} head {a.head}", "threads": a.threads, "chunk": 15, "ctx": 3, "load_s": load_s,
         "peak_rss_mb": mem.peak_mb, "tracks": speed}, indent=1), encoding="utf-8")
    print("load", f"{load_s:.1f}s", "peak MB", mem.peak_mb)


if __name__ == "__main__":
    main()
