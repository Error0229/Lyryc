"""Audio-free baselines. Usage: python methods/heuristics.py"""
import re, sys, time, pathlib
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent / "bench"))
from common import load
from results import save


def fixed_5s(lines, duration):
    """Current src-tauri/src/lyrics.rs::convert_plain_lyrics_to_lines."""
    return [5.0 * i for i in range(len(lines))]


def _ts_weight(text):
    """Port of src/services/textAlign.ts::lineWeight."""
    w = 1 + min(len(text) / 12, 6)
    w += len(re.findall(r"[,.!?;:]", text)) * 0.5
    w += len(re.findall(r"\b\w{7,}\b", text)) * 0.3
    if re.search(r"\b(chorus|hook|副歌)\b", text, re.I):
        w += 1
    w += min(len(re.findall(r"[^\w\s]", text)) * 0.1, 1)
    return max(0.5, w)


def textalign_ts(lines, duration):
    """Port of src/services/textAlign.ts::alignPlainLyrics (length-weighted spread)."""
    ws = [_ts_weight(l) for l in lines]
    tot = sum(ws)
    d = [min(8.0, max(1.0, w / tot * duration)) for w in ws]
    scale = duration / sum(d)
    out, t = [], 0.0
    for x in d:
        out.append(t)
        t += x * scale
    return out


def syllables(text):
    """Rough sung-syllable count: CJK/kana/hangul char = 1, Latin vowel group = 1."""
    cjk = len(re.findall(r"[぀-ヿ㐀-鿿가-힯]", text))
    latin = len(re.findall(r"[aeiouy]+", re.sub(r"[^a-z]", " ", text.lower())))
    return max(1, cjk + latin)


def syllable_spread(lines, duration, intro=0.08, outro=0.08):
    """Untuned improved heuristic: skip a fixed intro/outro share, weight by syllables."""
    start, end = duration * intro, duration * (1 - outro)
    ws = [syllables(l) + 2 for l in lines]  # +2: per-line breath/gap
    tot, out, t = sum(ws), [], start
    for w in ws:
        out.append(t)
        t += w / tot * (end - start)
    return out


METHODS = {"h0_fixed5s": fixed_5s, "h1_textalign_ts": textalign_ts, "h2_syllable_spread": syllable_spread}

if __name__ == "__main__":
    data = load()
    for name, fn in METHODS.items():
        res = {}
        for t in data:
            t0 = time.perf_counter()
            res[t["id"]] = {"times": fn(t["lines"], t["duration"]), "secs": time.perf_counter() - t0}
        save(name, res, {"download_mb": 0, "ram_mb": 0, "needs_audio": False})
        print("wrote", name)
