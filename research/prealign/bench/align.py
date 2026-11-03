"""MMS CTC forced alignment of known lyrics, down to words / characters.

Same model and <star> wildcard as research/timing/methods/mms_fa.py, plus:
  - each line is cut into *pieces*: a Latin word, or one CJK / kana / hangul character.
    Every piece owns a run of romanized target tokens, so its span comes straight
    out of the CTC path. Japanese is romanized per pykakasi segment and the
    segment's romaji is shared out over its characters by their own reading length.
  - backends: torch fp32, torch dynamic int8, ONNX Runtime (fp32 / int8 file).
  - a per-line confidence: mean frame posterior of the line's tokens.

Output lines use the proposed LyricLine shape:
  {"start", "end", "text", "conf", "words": [{"start", "end", "text"}]}
"""
import json, re, time
import numpy as np
import torch
import torchaudio.functional as F
import pykakasi
import uroman

REPO = "MahmoudAshraf/mms-300m-1130-forced-aligner"
SR = 16000
_kks = pykakasi.kakasi()
_ur = uroman.Uroman()
CJK = re.compile(r"[぀-ヿ㐀-鿿豈-﫿가-힯]")
LATIN = re.compile(r"[A-Za-z0-9À-ɏ'’]+")
KANA = re.compile(r"[぀-ヿ]")


def roman(s):
    return re.sub(r"[^a-z']+", "", _ur.romanize_string(s).lower().replace("’", "'"))


def ja_pieces(text):
    """[(pos, len, romaji)] for a Japanese line, one piece per kana/kanji, one per Latin word."""
    out, pos = [], 0
    for seg in _kks.convert(text):
        orig = seg["orig"]
        at = text.find(orig, pos)
        pos = at if at >= 0 else pos
        if not CJK.search(orig):
            out += [(pos + m.start(), len(m.group()), roman(m.group())) for m in LATIN.finditer(orig)]
        else:
            rom = re.sub(r"[^a-z]", "", seg["hepburn"].lower())
            if seg["hira"].endswith("っ"):
                rom = rom[:-3] if rom.endswith("tsu") else rom   # geminate: silence, not "tsu"
            chars = [(i, c) for i, c in enumerate(orig) if CJK.match(c)]
            w = [max(1, len(re.sub(r"[^a-z]", "", _kks.convert(c)[0]["hepburn"]))) if c != "っ" else 0 for _, c in chars]
            cut, acc = [0], 0
            for x in w:
                acc += x
                cut.append(round(acc / max(1, sum(w)) * len(rom)))
            out += [(pos + i, 1, rom[cut[k]:cut[k + 1]]) for k, (i, _) in enumerate(chars)]
        pos += len(orig)
    return out


def pieces(text, lang):
    if lang == "ja" or KANA.search(text):
        return ja_pieces(text)
    out = []
    for m in re.finditer(LATIN.pattern + "|" + CJK.pattern, text):
        out.append((m.start(), len(m.group()), roman(m.group())))
    return out


class Model:
    def __init__(self, backend="torch", threads=8, onnx_path=None):
        from huggingface_hub import hf_hub_download
        torch.set_num_threads(threads)
        self.dictionary = json.load(open(hf_hub_download(REPO, "vocab.json"), encoding="utf-8"))
        self.backend = backend
        if backend == "onnx":
            import onnxruntime as ort
            so = ort.SessionOptions()
            so.intra_op_num_threads = threads
            so.graph_optimization_level = ort.GraphOptimizationLevel.ORT_ENABLE_ALL
            self.sess = ort.InferenceSession(str(onnx_path), so, providers=["CPUExecutionProvider"])
        else:
            from transformers import Wav2Vec2ForCTC
            self.net = Wav2Vec2ForCTC.from_pretrained(REPO).eval()
            if backend == "torch-int8":
                self.net = torch.ao.quantization.quantize_dynamic(self.net, {torch.nn.Linear}, dtype=torch.qint8)

    def logits(self, x):
        if self.backend == "onnx":
            return torch.from_numpy(self.sess.run(None, {"input_values": x.numpy()})[0])
        with torch.inference_mode():
            return self.net(x).logits

    def emissions(self, wav, chunk_s=30):
        """Log-probs [1, T, V+1]; the extra column is the <star> wildcard (log p = 0)."""
        out = []
        for s in range(0, wav.shape[1], chunk_s * SR):
            x = wav[:, s:s + chunk_s * SR]
            if x.shape[1] < 400:
                break
            x = (x - x.mean()) / (x.std() + 1e-7)
            out.append(torch.log_softmax(self.logits(x), dim=-1))
        em = torch.cat(out, dim=1)
        return torch.cat([em, torch.zeros(1, em.shape[1], 1)], dim=-1)


def align(em, dictionary, lines, lang, sec_per_frame):
    star = len(dictionary)
    targets, owner = [], []           # owner[k] = (line, piece) of target k, or None for <star>
    plan = []
    for li, text in enumerate(lines):
        ps = pieces(text, lang)
        plan.append(ps)
        targets.append(star)
        owner.append(None)
        for pi, (_, _, rom) in enumerate(ps):
            for c in rom:
                if c in dictionary:
                    targets.append(dictionary[c])
                    owner.append((li, pi))
    targets.append(star)
    owner.append(None)
    ali, scores = F.forced_align(em, torch.tensor([targets], dtype=torch.int32), blank=0)
    spans = F.merge_tokens(ali[0], scores[0].exp())
    span = {}                          # (line, piece) -> [start_frame, end_frame, [probs]]
    for k, sp in enumerate(spans):
        if owner[k] is None:
            continue
        s = span.setdefault(owner[k], [sp.start, sp.end, []])
        s[1] = sp.end
        s[2].append(sp.score)
    out = []
    for li, (text, ps) in enumerate(zip(lines, plan)):
        words = []
        for pi, (pos, n, _) in enumerate(ps):
            s = span.get((li, pi))
            words.append({"start": s[0] * sec_per_frame if s else None, "end": s[1] * sec_per_frame if s else None,
                          "text": text[pos:pos + n]})
        fill(words)
        probs = [p for pi in range(len(ps)) for p in span.get((li, pi), [0, 0, []])[2]]
        timed = [w for w in words if w["start"] is not None]
        out.append({"start": timed[0]["start"] if timed else None, "end": timed[-1]["end"] if timed else None,
                    "text": text, "conf": float(np.mean(probs)) if probs else 0.0, "words": words})
    return out


def fill(words):
    """Pieces with no tokens (digits, symbols) take the next timed piece's start."""
    nxt = None
    for w in reversed(words):
        if w["start"] is None and nxt is not None:
            w["start"] = w["end"] = nxt
        nxt = w["start"] if w["start"] is not None else nxt


def run(model, wav, lines, lang, chunk_s=30):
    """wav: float32 tensor [1, N] at 16 kHz. Returns (lines, emission_s, viterbi_s)."""
    t0 = time.perf_counter()
    em = model.emissions(wav, chunk_s)
    t1 = time.perf_counter()
    out = align(em, model.dictionary, lines, lang, wav.shape[1] / em.shape[1] / SR)
    return out, t1 - t0, time.perf_counter() - t1
