"""Vocal separation with htdemucs (84 MB) -> data/stems/<id>.wav (16 kHz mono vocals).

Weights: the official htdemucs checkpoint via a Hugging Face mirror (dl.fbaipublicfiles.com
is unreachable here). Input is our 16 kHz mono WAV upsampled to 44.1 kHz stereo, so the
model sees no content above 8 kHz; this understates what separation on the real
stream could do. Usage: uv run python bench/separate.py [--ids 0,1]
"""
import argparse, json, time
import soundfile as sf
import torch, torchaudio
from demucs.apply import apply_model
from demucs.states import load_model
from huggingface_hub import hf_hub_download
from common import ROOT, load
from measure import PeakRSS


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ids", default=None)
    a = ap.parse_args()
    torch.set_num_threads(8)
    out_dir = ROOT / "data/stems"
    out_dir.mkdir(exist_ok=True)
    timing = {}
    with PeakRSS() as m:
        model = load_model(hf_hub_download("anchor-flux/demucs-htdemucs", "htdemucs.th")).eval()
        vocals = model.sources.index("vocals")
        for t in load():
            if a.ids and str(t["id"]) not in a.ids.split(","):
                continue
            dst = out_dir / f"{t['id']}.wav"
            if dst.exists():
                continue
            audio, sr = sf.read(ROOT / f"data/audio/{t['id']}.wav", dtype="float32")
            x = torchaudio.functional.resample(torch.from_numpy(audio), sr, model.samplerate)
            x = x.expand(2, -1).unsqueeze(0)
            t0 = time.perf_counter()
            with torch.inference_mode():
                y = apply_model(model, x, shifts=0, split=True, overlap=0.1, progress=False)[0, vocals]
            timing[t["id"]] = time.perf_counter() - t0
            y = torchaudio.functional.resample(y.mean(0), model.samplerate, 16000)
            sf.write(dst, y.numpy(), 16000)
            print(t["id"], t["track"], f"{timing[t['id']]:.1f}s", flush=True)
    (out_dir / "timing.json").write_text(json.dumps({"secs": timing, "peak_mb": m.peak_mb}))
    print("peak MB", m.peak_mb)


if __name__ == "__main__":
    main()
