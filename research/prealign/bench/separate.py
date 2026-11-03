"""Vocal separation with htdemucs (84 MB) -> data/stems/<id>.wav (16 kHz mono vocals).

Unlike research/timing, the input is the original YTM stream (opus, 48 kHz stereo)
decoded straight to 44.1 kHz stereo by ffmpeg, so Demucs sees the full band.
Weights: the official htdemucs checkpoint via a Hugging Face mirror.
Usage: uv run python -X utf8 bench/separate.py --ids 2,3,16,19 [--threads 8]
"""
import argparse, json, pathlib, subprocess, time
import numpy as np
import soundfile as sf
import torch, torchaudio
from demucs.apply import apply_model
from demucs.states import load_model
from huggingface_hub import hf_hub_download
from measure import PeakRSS

ROOT = pathlib.Path(__file__).resolve().parent.parent


def decode(src, sr):
    raw = subprocess.run(["ffmpeg", "-v", "error", "-i", str(src), "-f", "f32le", "-ac", "2", "-ar", str(sr), "-"],
                         capture_output=True, check=True).stdout
    return torch.from_numpy(np.frombuffer(raw, dtype=np.float32).reshape(-1, 2).T.copy())


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ids", required=True)
    ap.add_argument("--threads", type=int, default=8)
    a = ap.parse_args()
    torch.set_num_threads(a.threads)
    out = ROOT / "data/stems"
    out.mkdir(parents=True, exist_ok=True)
    timing = {}
    with PeakRSS() as m:
        model = load_model(hf_hub_download("anchor-flux/demucs-htdemucs", "htdemucs.th")).eval()
        vocals = model.sources.index("vocals")
        for tid in a.ids.split(","):
            src = next((ROOT / "data/audio").glob(f"{tid}.webm"), None) or next((ROOT / "data/audio").glob(f"{tid}.m4a"))
            x = decode(src, model.samplerate).unsqueeze(0)
            t0 = time.perf_counter()
            with torch.inference_mode():
                y = apply_model(model, x, shifts=0, split=True, overlap=0.1, progress=False)[0, vocals]
            secs = time.perf_counter() - t0
            dur = x.shape[-1] / model.samplerate
            timing[tid] = {"secs": secs, "audio_s": dur, "rtf": secs / dur}
            y = torchaudio.functional.resample(y.mean(0), model.samplerate, 16000)
            sf.write(out / f"{tid}.wav", y.numpy(), 16000)
            print(tid, f"{secs:.1f}s rtf {secs / dur:.2f}", flush=True)
    (ROOT / "results/speed_demucs.json").write_text(json.dumps({"tracks": timing, "peak_rss_mb": m.peak_mb}, indent=1))
    print("peak MB", m.peak_mb)


if __name__ == "__main__":
    main()
