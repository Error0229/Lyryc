"""Export a wav2vec2 CTC aligner to ONNX (fp32) and quantize it (int8 weights).

  uv run python -X utf8 bench/export_onnx.py           # MMS    -> models/mms_fa.onnx, models/mms_fa.int8.onnx
  uv run python -X utf8 bench/export_onnx.py echora    # echora -> models/echora_enc.onnx, models/echora_enc.int8.onnx

The graph takes input_values [1, samples] (16 kHz, normalized); the time axis is dynamic.
MMS returns logits [1, frames, 31]. echora returns logits [1, frames, 30] and the encoder's
last hidden state [1, frames, 1024] (the text head reads it; bench/echo_head.py exports that
head). int8 is ONNX Runtime dynamic quantization (MatMul weights per-channel int8, activations
quantized on the fly), the format a Rust `ort` build would load.
"""
import pathlib, sys, time
import torch
from transformers import Wav2Vec2ForCTC

ROOT = pathlib.Path(__file__).resolve().parent.parent
MODELS = {
    "mms": ("MahmoudAshraf/mms-300m-1130-forced-aligner", "mms_fa", ["logits"]),
    "echora": ("hcxlabs/echora-kara-300m", "echora_enc", ["logits", "hidden"]),
}


class WithHidden(torch.nn.Module):
    """CTC logits and the hidden state that feeds the CTC head."""

    def __init__(self, m):
        super().__init__()
        self.wav2vec2, self.lm_head = m.wav2vec2, m.lm_head

    def forward(self, x):
        h = self.wav2vec2(x).last_hidden_state
        return self.lm_head(h), h


def main(which="mms"):
    repo, stem, outs = MODELS[which]
    out = ROOT / "models"
    out.mkdir(exist_ok=True)
    fp32, int8 = out / f"{stem}.onnx", out / f"{stem}.int8.onnx"
    if not fp32.exists():
        model = Wav2Vec2ForCTC.from_pretrained(repo).eval()
        model = WithHidden(model) if "hidden" in outs else model
        x = torch.randn(1, 16000 * 10)
        t0 = time.perf_counter()
        torch.onnx.export(model, (x,), str(fp32), input_names=["input_values"], output_names=outs,
                          dynamic_axes={"input_values": {1: "samples"}, **{o: {1: "frames"} for o in outs}},
                          opset_version=17, dynamo=False)
        print(f"export {time.perf_counter() - t0:.0f}s")
        del model
    if not int8.exists():
        from onnxruntime.quantization import QuantType, quantize_dynamic
        t0 = time.perf_counter()
        quantize_dynamic(str(fp32), str(int8), weight_type=QuantType.QInt8, per_channel=True, op_types_to_quantize=["MatMul"])
        print(f"quantize {time.perf_counter() - t0:.0f}s")
    for p in sorted(out.glob("*.onnx")):
        size = p.stat().st_size + sum(q.stat().st_size for q in out.glob(p.name + ".data"))
        print(p.name, f"{size / 2**20:.0f} MiB")


if __name__ == "__main__":
    main(*sys.argv[1:2])
