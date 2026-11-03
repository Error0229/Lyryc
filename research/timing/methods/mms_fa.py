"""CTC forced alignment of known lyrics with the MMS forced-alignment model.

The weights are the torchaudio MMS_FA checkpoint (wav2vec2 300M, 1130 languages,
uroman character vocab) via its Hugging Face mirror, because dl.fbaipublicfiles.com
is unreachable from the benchmark machine. Alignment uses torchaudio forced_align.

Usage: uv run python methods/mms_fa.py [--stems] [--int8] [--ids 0,1,2]
  --stems : align data/stems/<id>.wav (separated vocals) instead of the full mix.
  --int8  : dynamic int8 quantization of Linear layers (what a ~320 MB ONNX ship would be).
Each line is preceded by a <star> token so intros, breaks and ad-libs that are not
in the lyric text can be absorbed instead of dragging the alignment.
"""
import argparse, sys, time, pathlib
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent / "bench"))
import soundfile as sf
import torch
import torchaudio.functional as F
from transformers import Wav2Vec2ForCTC
from common import ROOT, load
from results import save
from romanize import romanize
from measure import PeakRSS

CHUNK_S = 30
REPO = "MahmoudAshraf/mms-300m-1130-forced-aligner"


def emissions(model, wav):
    """Log-probs [1, T, V+1]; the extra column is the <star> wildcard (log p = 0, as torchaudio's with_star)."""
    sr = 16000
    out = []
    with torch.inference_mode():
        for s in range(0, wav.shape[1], CHUNK_S * sr):
            x = wav[:, s:s + CHUNK_S * sr]
            x = (x - x.mean()) / (x.std() + 1e-7)
            out.append(torch.log_softmax(model(x).logits, dim=-1))
    em = torch.cat(out, dim=1)
    return torch.cat([em, torch.zeros(1, em.shape[1], 1)], dim=-1)


def align(model, dictionary, wav, lines, lang):
    em = emissions(model, wav)
    star = len(dictionary)
    targets, starts = [], []
    for line in lines:
        chars = [dictionary[c] for w in romanize(line, lang) for c in w if c in dictionary]
        targets.append(star)
        starts.append(len(targets) if chars else None)
        targets.extend(chars)
    targets.append(star)
    tgt = torch.tensor([targets], dtype=torch.int32)
    ali, scores = F.forced_align(em, tgt, blank=0)
    spans = F.merge_tokens(ali[0], scores[0].exp())
    sec_per_frame = wav.shape[1] / em.shape[1] / 16000
    return [spans[i].start * sec_per_frame if i is not None else None for i in starts]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--stems", action="store_true")
    ap.add_argument("--int8", action="store_true")
    ap.add_argument("--ids", default=None)
    a = ap.parse_args()
    torch.set_num_threads(8)
    with PeakRSS() as m:
        model = Wav2Vec2ForCTC.from_pretrained(REPO).eval()
        if a.int8:
            model = torch.ao.quantization.quantize_dynamic(model, {torch.nn.Linear}, dtype=torch.qint8)
        from huggingface_hub import hf_hub_download
        import json
        dictionary = json.load(open(hf_hub_download(REPO, "vocab.json"), encoding="utf-8"))
        res = {}
        data = load()
        if a.ids:
            data = [t for t in data if str(t["id"]) in a.ids.split(",")]
        for t in data:
            path = ROOT / ("data/stems" if a.stems else "data/audio") / f"{t['id']}.wav"
            if not path.exists():
                continue
            audio, sr = sf.read(path, dtype="float32")
            wav = torch.from_numpy(audio).unsqueeze(0)
            t0 = time.perf_counter()
            times = align(model, dictionary, wav, t["lines"], t["lang"])
            res[t["id"]] = {"times": times, "secs": time.perf_counter() - t0}
            print(t["id"], t["track"], f"{res[t['id']]['secs']:.1f}s", flush=True)
    name = "a_mms_fa" + ("_int8" if a.int8 else "") + ("_vocals" if a.stems else "")
    save(name, res, {"download_mb": 320 if a.int8 else 1260, "ram_mb": m.peak_mb, "needs_audio": True})
    print("peak MB", m.peak_mb)


if __name__ == "__main__":
    main()
