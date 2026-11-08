"""Echora Kara's own aligner (CLI, recommended decoder settings) on the bench tracks -> data/pred/<name>.json.

Two steps, two venvs:
  uv run python -X utf8 bench/run_echora.py dump                   # data/echora/<id>/i.txt from run.py's source text
  <echora venv>/python -X utf8 bench/run_echora.py run --name echora_full [--fusion on|gate0] [--text-bonus 0.4] [--ids 0,1]
  <echora venv>/python -X utf8 bench/run_echora.py export          # models/echora_enc.onnx + .int8.onnx
  <echora venv>/python -X utf8 bench/run_echora.py run --name echora_int8 --fusion gate0 --onnx models/echora_enc.int8.onnx
Each echora token (a word, or a kana / character unit) becomes one predicted word with its start/end.

--onnx runs the CLI in this process (model loaded once) with the wav2vec2 encoder + CTC head replaced by
an ONNX Runtime session; the small heads (edge, background, text) stay in torch and read the ONNX hidden
state, so the decode sees exactly what a shipped int8 encoder would produce. --inproc alone does the same
with the torch encoder (the in-process baseline).
"""
import argparse, json, os, pathlib, subprocess, sys, time
from huggingface_hub import snapshot_download

ROOT = pathlib.Path(__file__).resolve().parent.parent
WORK = ROOT / "data/echora"
REPO = "hcxlabs/echora-kara-300m"


def encoder():
    """wav2vec2 + CTC head: input_values [1, samples] -> logits [1, frames, 30], hidden [1, frames, 1024]."""
    import torch
    from transformers import Wav2Vec2ForCTC

    class Net(torch.nn.Module):
        def __init__(self, m):
            super().__init__()
            self.wav2vec2, self.lm_head = m.wav2vec2, m.lm_head

        def forward(self, x):
            h = self.wav2vec2(x).last_hidden_state
            return self.lm_head(h), h

    return Net(Wav2Vec2ForCTC.from_pretrained(snapshot_download(REPO))).eval()


def export():
    import torch
    out = ROOT / "models"
    out.mkdir(exist_ok=True)
    fp32, int8 = out / "echora_enc.onnx", out / "echora_enc.int8.onnx"
    if not fp32.exists():
        x = torch.randn(1, 16000 * 10)
        torch.onnx.export(encoder(), (x,), str(fp32), input_names=["input_values"], output_names=["logits", "hidden"],
                          dynamic_axes={"input_values": {1: "samples"}, "logits": {1: "frames"}, "hidden": {1: "frames"}},
                          opset_version=17, dynamo=False)
    if not int8.exists():
        from onnxruntime.quantization import QuantType, quantize_dynamic
        quantize_dynamic(str(fp32), str(int8), weight_type=QuantType.QInt8, per_channel=True, op_types_to_quantize=["MatMul"])
    for p in sorted(out.glob("echora_enc*.onnx")):
        size = p.stat().st_size + sum(q.stat().st_size for q in out.glob(p.name + ".data"))
        print(p.name, f"{size / 2**20:.0f} MiB")


def our_labels(lang):
    """Label records from align.py's pieces instead of echora's normalizer: one record per piece
    ({orig, type, pron}; type 1 Latin word, 3 CJK / kana, 5 hangul), the text between pieces as
    type-0 display records, a newline record after each line."""
    from align import pieces, CJK, LATIN

    def kind(orig):
        return 1 if LATIN.fullmatch(orig) else 5 if "가" <= orig <= "힯" else 3 if CJK.match(orig) else 0

    def normalize(lines, language, *_, **__):
        records, texts = [], []
        for line in lines:
            text = line.rstrip("\r\n")
            texts.append(text)
            at = 0
            for pos, n, rom in pieces(text, lang):
                if pos > at:
                    records.append({"orig": text[at:pos], "type": 0})
                orig = text[pos:pos + n]
                records.append({"orig": orig, "type": kind(orig), "pron": rom} if rom else {"orig": orig, "type": 0})
                at = pos + n
            if at < len(text):
                records.append({"orig": text[at:], "type": 0})
            records.append({"orig": "\n", "type": 0, "pron": ""})
        return records, texts
    return normalize


def patch_encoder(onnx_path, threads, sub=None):
    """Swap the loaded Wav2Vec2ForCTC for an ONNX session. echora reads the hidden state with a
    pre-hook on lm_head, so the stub still calls an lm_head module, which hands back the ONNX logits.
    sub=(chunk_s, ctx_s) runs each of echora's 45 s chunks as chunk_s windows with ctx_s of audio on
    both sides (the shipped MMS scheme), keeping each window's own frames (320 samples each)."""
    import numpy as np, onnxruntime as ort, torch
    from types import SimpleNamespace
    from echora_kara.engine import aligner

    so = ort.SessionOptions()
    so.intra_op_num_threads = threads
    so.graph_optimization_level = ort.GraphOptimizationLevel.ORT_ENABLE_ALL
    sess = ort.InferenceSession(str(onnx_path), so, providers=["CPUExecutionProvider"])

    class Head(torch.nn.Module):
        def __init__(self, lm_head):
            super().__init__()
            self.in_features, self.out_features, self.logits = lm_head.in_features, lm_head.out_features, None

        def forward(self, hidden):
            return self.logits

    class Stub(torch.nn.Module):
        def __init__(self, model):
            super().__init__()
            self.config, self.lm_head = model.config, Head(model.lm_head)

        def forward(self, input_values, **_):
            x = input_values.numpy().astype(np.float32)
            logits, hidden = run_windows(x) if sub else sess.run(None, {"input_values": x})
            self.lm_head.logits = torch.from_numpy(logits)
            return SimpleNamespace(logits=self.lm_head(torch.from_numpy(hidden)))

    def run_windows(x):
        n, hop, ctx = x.shape[1], int(sub[0] * 16000), int(sub[1] * 16000)
        frames = (n - 400) // 320 + 1             # what one pass over x yields
        outs = []
        for s in range(0, n, hop):
            a, e = max(0, s - ctx), min(n, s + hop)
            lo = (s - a) // 320
            o = sess.run(None, {"input_values": x[:, a:min(n, e + ctx)]})
            outs.append([t[:, lo:] if e == n else t[:, lo:lo + (e - s) // 320] for t in o])
        cat = [np.concatenate(ts, 1) for ts in zip(*outs)]
        return [np.concatenate([t, np.repeat(t[:, -1:], frames - t.shape[1], 1)], 1)[:, :frames] for t in cat]

    init = aligner.Wav2Vec2ForcedAligner.__init__

    def patched(self, *a, **k):
        init(self, *a, **k)
        self.model = Stub(self.model)

    aligner.Wav2Vec2ForcedAligner.__init__ = patched


def dump():
    from run import source_text
    tracks = json.loads((ROOT / "tracks.json").read_text(encoding="utf-8"))
    refs = json.loads((ROOT / "data/refs.json").read_text(encoding="utf-8"))
    for t in tracks:
        src, lines = source_text(refs[str(t["id"])], t)
        if not lines:
            continue
        d = WORK / str(t["id"])
        d.mkdir(parents=True, exist_ok=True)
        (d / "i.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")
        (d / "src.json").write_text(json.dumps({"text_src": src, "lines": lines}, ensure_ascii=False), encoding="utf-8")


def run(a):
    tracks = json.loads((ROOT / "tracks.json").read_text(encoding="utf-8"))
    preds, speed = {}, {}
    env = {**os.environ, "ECHORA_KARA_TEXT_BONUS": str(a.text_bonus), "ECHORA_KARA_IN_LINE_GAP_COST": "0.2",
           "PYTHONUTF8": "1", **dict(kv.split("=", 1) for kv in a.env)}
    exe = pathlib.Path(sys.executable).with_name("echora-kara-align.exe")
    inproc = a.inproc or a.onnx
    if inproc:
        os.environ.update(env)
        import torch
        torch.set_num_threads(a.threads)
        if a.onnx:
            patch_encoder(ROOT / a.onnx, a.threads, tuple(map(float, a.sub.split(","))) if a.sub else None)
        from echora_kara.inference import cli
    elif a.labels == "ours":
        raise SystemExit("--labels ours needs --inproc or --onnx")
    for t in tracks:
        if a.ids and str(t["id"]) not in a.ids.split(","):
            continue
        d = WORK / str(t["id"])
        if not (d / "i.txt").exists():
            continue
        out = d / a.name
        out.mkdir(exist_ok=True)
        (out / "i.txt").write_text((d / "i.txt").read_text(encoding="utf-8"), encoding="utf-8")
        opts = {"--path_io": str(out), "--input_audio": str(ROOT / "data/audio" / f"{t['id']}.wav"),
                "--input_text": "i.txt", "--model": "yohane", "--hf_model_path": snapshot_download(REPO),
                "--fusion": a.fusion, "--label_rules": "v3-sung", "--lang": "auto", "--no-gpu": None,
                "--head_correct": "0", "--tail_correct": "0", "--timing_offset_ms": "0",
                "--min_token_ms": "100", "--min_letter_ms": "40", "--duration_pace_ratio": "0.9",
                "--vocal_activity_weight": "2.0", "--exact_frame_clock": None, "--blank_free_tokens": None,
                "--background_decoding": None}
        for kv in a.set:                       # --set --flag=value overrides, --set --flag drops it
            k, _, v = kv.partition("=")
            opts.pop(k, None) if not v else opts.__setitem__(k, v)
        cmd = [x for k, v in opts.items() for x in ((k,) if v is None else (k, v))]
        doc_path = out / "o.alignment.json"
        doc_path.unlink(missing_ok=True)
        t0 = time.perf_counter()
        if a.labels == "ours":
            cli.normalize_source_lines = our_labels(t["lang"])
        if inproc:
            try:
                cli.main(cmd)
                err = ""
            except BaseException as e:   # the CLI may sys.exit
                err = repr(e)
        else:
            r = subprocess.run([str(exe)] + cmd, env=env, capture_output=True, text=True, encoding="utf-8", errors="replace")
            err = r.stderr[-1500:] if r.returncode else ""
        el = time.perf_counter() - t0
        if err or not doc_path.exists():
            print(t["id"], "FAILED", err, flush=True)
            continue
        doc = json.loads(doc_path.read_text(encoding="utf-8"))
        src = json.loads((d / "src.json").read_text(encoding="utf-8"))
        lines = []
        for l in doc["alignment"]["lines"]:
            words = [{"start": k["start_ms"] / 1000, "end": k["end_ms"] / 1000, "text": k["text"]} for k in l["tokens"]]
            lines.append({"start": words[0]["start"] if words else None, "end": words[-1]["end"] if words else None,
                          "text": l["text"], "conf": 1.0, "words": words})
        preds[t["id"]] = {"text_src": src["text_src"], "lines": lines}
        dur = t["duration"]
        speed[t["id"]] = {"audio_s": dur, "emission_s": el, "viterbi_s": 0.0, "rtf": el / dur}
        print(t["id"], t["track"], f"{el:.1f}s rtf {el / dur:.3f}", flush=True)
    (ROOT / "data/pred").mkdir(parents=True, exist_ok=True)
    (ROOT / f"data/pred/{a.name}.json").write_text(json.dumps(preds, ensure_ascii=False), encoding="utf-8")
    (ROOT / f"results/speed_{a.name}.json").write_text(json.dumps(
        {"backend": "echora-kara cli" + (f" onnx {a.onnx}" if a.onnx else " inproc" if inproc else ""), "fusion": a.fusion, "text_bonus": a.text_bonus, "tracks": speed}, indent=1),
        encoding="utf-8")


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", choices=["dump", "run", "export"])
    ap.add_argument("--name", default="echora_full")
    ap.add_argument("--fusion", default="on")
    ap.add_argument("--text-bonus", type=float, default=0.4)
    ap.add_argument("--ids", default=None)
    ap.add_argument("--onnx", default=None, help="encoder ONNX file (relative to research/prealign)")
    ap.add_argument("--inproc", action="store_true", help="run the CLI in this process with the torch encoder")
    ap.add_argument("--threads", type=int, default=8)
    ap.add_argument("--set", action="append", default=[], help="--flag=value to override a CLI option, --flag to drop it")
    ap.add_argument("--env", action="append", default=[], help="NAME=value for the echora process")
    ap.add_argument("--labels", default="echora", choices=["echora", "ours"], help="ours: align.py's romanization")
    ap.add_argument("--sub", default=None, help="chunk_s,ctx_s: encoder windows inside echora's 45 s chunks")
    a = ap.parse_args()
    {"dump": dump, "export": export}.get(a.cmd, lambda: run(a))()
