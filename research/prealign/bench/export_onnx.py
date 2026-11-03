"""Export the MMS forced-alignment wav2vec2 to ONNX (fp32) and quantize it (int8 weights).

  uv run python -X utf8 bench/export_onnx.py      # -> models/mms_fa.onnx, models/mms_fa.int8.onnx

The graph takes input_values [1, samples] (16 kHz, normalized) and returns logits
[1, frames, 31]; the time axis is dynamic. int8 is ONNX Runtime dynamic quantization
(MatMul weights per-channel int8, activations quantized on the fly), the format a
Rust `ort` build would load.
"""
import pathlib, time
import torch
from transformers import Wav2Vec2ForCTC

ROOT = pathlib.Path(__file__).resolve().parent.parent
REPO = "MahmoudAshraf/mms-300m-1130-forced-aligner"


def main():
    out = ROOT / "models"
    out.mkdir(exist_ok=True)
    fp32, int8 = out / "mms_fa.onnx", out / "mms_fa.int8.onnx"
    if not fp32.exists():
        model = Wav2Vec2ForCTC.from_pretrained(REPO).eval()
        x = torch.randn(1, 16000 * 10)
        t0 = time.perf_counter()
        torch.onnx.export(model, (x,), str(fp32), input_names=["input_values"], output_names=["logits"],
                          dynamic_axes={"input_values": {1: "samples"}, "logits": {1: "frames"}},
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
    main()
