"""echora-kara-300m text head ("text aligner"), written from docs/research/prealign/echora-spec.md §2-3.

Gives S[t, k] = log P(token k | frame t is singing) for the lead voice.

Two graph halves, so ONNX can run the text side once per song and the frame side per window:
  text(letters [K,16], line [K], pos [K]) -> text [K,512], kappa [K,512] (= key.0(text)), m [K,30]
  frames(hidden [W,1024], logp [W,30], text, kappa, m) -> S [W,K]

  uv run python -X utf8 bench/echo_head.py      # -> models/echo_{text,frame}{,.int8}.onnx
"""
import math, pathlib, time
import numpy as np
import torch
import torch.nn as nn
import torch.nn.functional as F

ROOT = pathlib.Path(__file__).resolve().parent.parent
SNAP = pathlib.Path.home() / ".cache/huggingface/hub/models--hcxlabs--echora-kara-300m/snapshots/64087e1078b1aaf39cc1e6a9fc93874ca2f60e22"
D, H, PAD, MAXL = 512, 8, 32, 16
WIN, HOP, KEEP = 1250, 1000, 125
ONNX = {"onnx": ("echo_text.onnx", "echo_frame.onnx"), "onnx-int8": ("echo_text.int8.onnx", "echo_frame.int8.onnx")}


def sinusoid(n, omega):
    p = torch.arange(n, dtype=torch.float32)[:, None] * omega
    return torch.cat([p.sin(), p.cos()], -1)


class MHA(nn.Module):
    """Packed q/k/v attention on [1, N, 512]; optional per-head LayerNorm(64) on q and k."""

    def __init__(self):
        super().__init__()
        self.in_proj_weight = nn.Parameter(torch.empty(3 * D, D))
        self.in_proj_bias = nn.Parameter(torch.empty(3 * D))
        self.out_proj = nn.Linear(D, D)

    def forward(self, x, mem, qn=None, kn=None):
        w, b = self.in_proj_weight.chunk(3), self.in_proj_bias.chunk(3)
        q, k, v = (F.linear(y, wi, bi).unflatten(-1, (H, D // H)).transpose(1, 2)
                   for y, wi, bi in ((x, w[0], b[0]), (mem, w[1], b[1]), (mem, w[2], b[2])))
        if qn is not None:
            q, k = qn(q), kn(k)
        a = torch.softmax(q @ k.transpose(-1, -2) / 8.0, -1) @ v
        return self.out_proj(a.transpose(1, 2).flatten(2))


class TextLayer(nn.Module):
    def __init__(self):
        super().__init__()
        self.norm1, self.norm2 = nn.LayerNorm(D), nn.LayerNorm(D)
        self.self_attn = MHA()
        self.linear1, self.linear2 = nn.Linear(D, 4 * D), nn.Linear(4 * D, D)

    def forward(self, x):
        y = self.norm1(x)
        x = x + self.self_attn(y, y)
        return x + self.linear2(F.gelu(self.linear1(self.norm2(x))))


class CrossLayer(nn.Module):
    def __init__(self):
        super().__init__()
        for n in ("self_norm", "cross_norm", "ff_norm"):
            setattr(self, n, nn.LayerNorm(D))
        for n in ("self_q_norm", "self_k_norm", "cross_q_norm", "cross_k_norm"):
            setattr(self, n, nn.LayerNorm(D // H))
        self.self_attention, self.cross_attention = MHA(), MHA()
        self.ff = nn.Sequential(nn.Linear(D, 4 * D), nn.GELU(), nn.Dropout(0.0), nn.Linear(4 * D, D))

    def forward(self, f, text):
        y = self.self_norm(f)
        f = f + self.self_attention(y, y, self.self_q_norm, self.self_k_norm)
        f = f + self.cross_attention(self.cross_norm(f), text, self.cross_q_norm, self.cross_k_norm)
        return f + self.ff(self.ff_norm(f))


class Tokens(nn.Module):
    def __init__(self):
        super().__init__()
        self.letters = nn.Embedding(PAD + 1, D)
        self.conv = nn.Conv1d(D, D, 3, padding=1)


class TextHead(nn.Module):
    """Parameter names are the checkpoint's `text_aligner.*` names, minus the prefix."""

    def __init__(self):
        super().__init__()
        self.tokens = Tokens()
        self.voice, self.line, self.line_position = nn.Embedding(2, D), nn.Embedding(256, D), nn.Embedding(64, D)
        self.text_encoder = nn.Module()
        self.text_encoder.layers = nn.ModuleList(TextLayer() for _ in range(6))
        self.text_norm = nn.LayerNorm(D)
        self.frame_in = nn.Sequential(nn.LayerNorm(1024 + 30), nn.Linear(1024 + 30, D))
        self.cross = nn.ModuleList(CrossLayer() for _ in range(4))
        self.frame_norm = nn.LayerNorm(D)
        self.query, self.key = nn.ModuleList(nn.Linear(D, D) for _ in range(2)), nn.ModuleList(nn.Linear(D, D) for _ in range(2))
        self.none = nn.ModuleList(nn.Linear(D, 1) for _ in range(2))
        self.letter_scale = nn.Parameter(torch.empty(2))
        omega = np.exp(-math.log(10000.0) * np.arange(D // 2) / (D // 2))
        self.register_buffer("omega", torch.tensor(omega, dtype=torch.float32), persistent=False)

    def text(self, letters, line, pos):
        real = (letters != PAD).float()                                      # [K,16]
        e = F.gelu(self.tokens.conv(self.tokens.letters(letters).transpose(1, 2))).transpose(1, 2)
        e = (e * real[..., None]).sum(1) / real.sum(1, keepdim=True).clamp(min=1.0)
        x = (e + self.voice.weight[0] + sinusoid(letters.shape[0], self.omega)
             + self.line(line.clamp(max=255)) + self.line_position(pos.clamp(max=63)))[None]
        for layer in self.text_encoder.layers:
            x = layer(x)
        text = self.text_norm(x)[0]
        hist = F.one_hot(letters.clamp(max=30), 31)[..., :30].float().sum(1)   # ids >= 30 (pad) dropped
        return text, self.key[0](text), hist / hist.sum(1, keepdim=True).clamp(min=1.0)

    def frames(self, hidden, logp, text, kappa, m):
        f = self.frame_in(torch.cat([hidden, logp], -1)) + sinusoid(hidden.shape[0], self.omega)
        f, t = f[None], text[None]
        for layer in self.cross:
            f = layer(f, t)
        q = self.query[0](self.frame_norm(f))[0]
        ev = torch.log((logp.exp() @ m.T).clamp(min=1e-6))
        return torch.log_softmax(q @ kappa.T / math.sqrt(D) + self.letter_scale[0] * ev, -1)


def load_torch():
    from safetensors.torch import load_file
    sd = load_file(str(SNAP / "model.safetensors"))
    net = TextHead()
    net.load_state_dict({k[len("text_aligner."):]: v for k, v in sd.items() if k.startswith("text_aligner.")}, strict=True)
    return net.eval()


def pack(encoded, line_of_token):
    """§3.1: letters [K,16] (pad 32, truncated), line [K], position-in-line [K] (int64)."""
    K = len(encoded)
    letters = np.full((K, MAXL), PAD, np.int64)
    pos = np.zeros(K, np.int64)
    for k, ids in enumerate(encoded):
        letters[k, :min(MAXL, len(ids))] = ids[:MAXL]
        pos[k] = pos[k - 1] + 1 if k and line_of_token[k] == line_of_token[k - 1] else 0
    return letters, np.asarray(line_of_token, np.int64), pos


class Head:
    """backend: torch | onnx | onnx-int8. text()/frames() take and return numpy."""

    def __init__(self, backend="torch", threads=8):
        self.backend = backend
        if backend == "torch":
            torch.set_num_threads(threads)
            self.net = load_torch()
            return
        import onnxruntime as ort
        so = ort.SessionOptions()
        so.intra_op_num_threads = threads
        so.graph_optimization_level = ort.GraphOptimizationLevel.ORT_ENABLE_ALL
        self.sess = [ort.InferenceSession(str(ROOT / "models" / n), so, providers=["CPUExecutionProvider"]) for n in ONNX[backend]]

    def _run(self, i, *xs):
        if self.backend == "torch":
            with torch.inference_mode():
                out = (self.net.text, self.net.frames)[i](*map(torch.from_numpy, xs))
            return [o.numpy() for o in out] if isinstance(out, tuple) else out.numpy()
        s = self.sess[i]
        out = s.run(None, {a.name: x for a, x in zip(s.get_inputs(), xs)})
        return out if i == 0 else out[0]

    def text(self, letters, line, pos):
        return self._run(0, letters, line, pos)

    def frames(self, hidden, logp, text, kappa, m):
        return self._run(1, hidden, logp, text, kappa, m)


def windows(T):
    """§3.2: [(start, end, keep_lo, keep_hi)] in global frames."""
    starts = list(range(0, max(1, T - (WIN - HOP)), HOP))
    out = []
    for n, s in enumerate(starts):
        e = min(T, s + WIN)
        out.append((s, e, s + (KEEP if n else 0), e - (KEEP if n < len(starts) - 1 else 0)))
    return out


def text_share(head, hidden, logp, letters, line, pos):
    """S [T, K] float32, the head run over 1250-frame windows and stitched (§3.2)."""
    text, kappa, m = head.text(letters, line, pos)
    T = len(logp)
    S = np.empty((T, len(letters)), np.float32)
    for s, e, lo, hi in windows(T):
        S[lo:hi] = head.frames(np.ascontiguousarray(hidden[s:e]), np.ascontiguousarray(logp[s:e]), text, kappa, m)[lo - s:hi - s]
    return S


class _Text(nn.Module):
    def __init__(self, net):
        super().__init__()
        self.net = net

    def forward(self, letters, line, pos):
        return self.net.text(letters, line, pos)


class _Frame(_Text):
    def forward(self, hidden, logp, text, kappa, m):
        return self.net.frames(hidden, logp, text, kappa, m)


def export():
    out = ROOT / "models"
    net = load_torch()
    K, W = 37, 300
    letters = torch.randint(1, 27, (K, MAXL))
    letters[:, 5:] = PAD
    args_t = (letters, torch.zeros(K, dtype=torch.int64), torch.arange(K))
    with torch.no_grad():
        text, kappa, m = net.text(*args_t)
    args_f = (torch.randn(W, 1024), torch.log_softmax(torch.randn(W, 30), -1), text, kappa, m)
    specs = [(_Text(net), args_t, ["letters", "line", "pos"], ["text", "kappa", "m"],
              {"letters": {0: "K"}, "line": {0: "K"}, "pos": {0: "K"}, "text": {0: "K"}, "kappa": {0: "K"}, "m": {0: "K"}}),
             (_Frame(net), args_f, ["hidden", "logp", "text", "kappa", "m"], ["share"],
              {"hidden": {0: "W"}, "logp": {0: "W"}, "text": {0: "K"}, "kappa": {0: "K"}, "m": {0: "K"}, "share": {0: "W", 1: "K"}})]
    from onnxruntime.quantization import QuantType, quantize_dynamic
    for (fp32, int8), (mod, args, ins, outs, dyn) in zip(zip(ONNX["onnx"], ONNX["onnx-int8"]), specs):
        t0 = time.perf_counter()
        torch.onnx.export(mod, args, str(out / fp32), input_names=ins, output_names=outs, dynamic_axes=dyn,
                          opset_version=17, dynamo=False)
        quantize_dynamic(str(out / fp32), str(out / int8), weight_type=QuantType.QInt8, per_channel=True, op_types_to_quantize=["MatMul"])
        print(fp32, f"{(out / fp32).stat().st_size / 2**20:.1f} MiB", int8, f"{(out / int8).stat().st_size / 2**20:.1f} MiB",
              f"{time.perf_counter() - t0:.0f}s")


if __name__ == "__main__":
    export()
