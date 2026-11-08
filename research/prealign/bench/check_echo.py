"""Check echo_head / echo_decode against the spec test vectors (data/spec_vectors, spec §9).

  uv run python -X utf8 bench/check_echo.py [--ids 5,13,18] [--heads torch,onnx,onnx-int8]

Inputs are the vectors' own log_probs, hidden, activity and frame_times. Reports, per track:
head S max |Δ log p| (and argmax agreement) per backend, and decoder token-ms mismatches
given the vectors' text_share and given each head's S.
"""
import argparse, json, pathlib, time
import numpy as np
from echo_head import Head, pack, text_share
from echo_decode import decode

ROOT = pathlib.Path(__file__).resolve().parent.parent
VEC = ROOT / "data/spec_vectors"


def mismatches(out, ref):
    return int((np.asarray(out["times_ms"]) != np.asarray(ref["token_times_ms"])).any(1).sum())


def run_decoder(z, j, S):
    t0 = time.perf_counter()
    out = decode(z["log_probs"], z["activity"], S, j["encoded"], j["line_ranges"], z["frame_times"], j["frame_shift"])
    return out, time.perf_counter() - t0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ids", default="5,13,18")
    ap.add_argument("--heads", default="torch,onnx,onnx-int8")
    a = ap.parse_args()
    heads = {h: Head(h) for h in a.heads.split(",")}
    for tid in a.ids.split(","):
        z = np.load(VEC / f"track_{tid}.npz")
        j = json.loads((VEC / f"track_{tid}.json").read_text(encoding="utf-8"))
        letters, line, pos = pack(j["encoded"], j["line_of_token"])
        assert (letters == z["letters"]).all() and (line == z["line"]).all() and (pos == z["position"]).all(), "packing"
        K = len(j["encoded"])
        out, dt = run_decoder(z, j, z["text_share"])
        steps = {"plain": (out["plain_letter_spans"] == np.array(j["plain_pass_letter_spans"])).all(),
                 "floors": out["floors"] == j["min_frames_per_letter"],
                 "final": (out["final_letter_spans"] == np.array(j["final_letter_spans"])).all()}
        print(f"track {tid}: T={len(z['log_probs'])} K={K} | decoder on vector S: {mismatches(out, j)}/{K} token-ms mismatches"
              f" ({dt:.1f}s, steps {steps})")
        for h, head in heads.items():
            t0 = time.perf_counter()
            S = text_share(head, z["hidden"], z["log_probs"], letters, line, pos)
            hs = time.perf_counter() - t0
            d = np.abs(S - z["text_share"]).max()
            agree = (S.argmax(1) == z["text_share"].argmax(1)).mean()
            out, _ = run_decoder(z, j, S)
            print(f"  head {h:9s}: max|dS| {d:.2e}  argmax agree {agree:.4f}  ({hs:.1f}s) | decoder on own S: "
                  f"{mismatches(out, j)}/{K} token-ms mismatches")


if __name__ == "__main__":
    main()
