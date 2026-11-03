"""Compare two word-timed lyric lists (a reference and a prediction, or two references).

Both sides are flattened into a stream of normalized characters, each with a time
(linear inside its word). The streams are matched with difflib, so the texts may
differ a little (punctuation, a dropped ad-lib, a different line split). A reference
word's onset error is prediction-time minus reference-time at the word's first
character; a line's error is the same at the line's first character.
"""
import difflib, re, statistics, unicodedata

CREDIT = re.compile(r"^(作词|作詞|作曲|编曲|編曲|词|詞|曲|制作人|製作人|监制|監製|和声|和聲|合声|吉他|贝斯|鼓|录音|混音|母带|"
                    r"弦乐|出品|发行|OP|SP|演唱|原唱|"
                    r"(Lyrics|Composed|Arranged|Produced|Written|Music)( by)?|Composer|Producer|Lyricist)\s*[:：]", re.I)
KEEP = re.compile(r"[^\W_]")


def norm_chars(s):
    return [c for c in unicodedata.normalize("NFKC", s).lower() if KEEP.match(c)]


def clean(lines, track="", artist=""):
    """Drop credit lines, the "title - artist" header and untimed/empty lines."""
    out = []
    for i, l in enumerate(lines):
        t = l["text"].strip()
        if not t or l["start"] is None or CREDIT.search(t) or re.search(r"[:：]\s*\S", t) and i < 12:
            continue
        if i < 3 and " - " in t and (norm_chars(track)[:2] == norm_chars(t)[:2] or "".join(norm_chars(artist)) in "".join(norm_chars(t))):
            continue
        out.append(l)
    return out


def stream(lines):
    """[(char, time, line_idx, is_word_start, is_line_start)]"""
    out = []
    for li, l in enumerate(lines):
        words = l.get("words") or [{"start": l["start"], "end": l.get("end") or l["start"], "text": l["text"]}]
        first = True
        for w in words:
            cs = norm_chars(w["text"])
            if not cs or w["start"] is None:
                continue
            end = w["end"] if w.get("end") is not None else w["start"]
            for k, c in enumerate(cs):
                out.append((c, w["start"] + (end - w["start"]) * k / len(cs), li, k == 0, first and k == 0))
            first = False
    return out


def compare(ref, pred):
    """Signed errors. Returns {"word": [(err, ref_line)], "line": [(err, ref_line)], "matched": share of ref chars}."""
    a, b = stream(ref), stream(pred)
    sm = difflib.SequenceMatcher(None, [x[0] for x in a], [x[0] for x in b], autojunk=False)
    m = {}
    for i, j, n in sm.get_matching_blocks():
        for k in range(n):
            m[i + k] = j + k
    word, line = [], []
    for i, (c, t, li, ws, ls) in enumerate(a):
        if i not in m:
            continue
        e = b[m[i]][1] - t
        if ws:
            word.append((e, li))
        if ls:
            line.append((e, li))
    return {"word": word, "line": line, "matched": len(m) / max(1, len(a))}


def summary(errs):
    """errs: signed seconds. Absolute-error distribution."""
    if not errs:
        return None
    ab = [abs(e) for e in errs]
    return {"n": len(ab), "median": statistics.median(ab), "mean": statistics.fmean(ab),
            "le01": sum(e <= 0.1 for e in ab) / len(ab), "le02": sum(e <= 0.2 for e in ab) / len(ab),
            "le03": sum(e <= 0.3 for e in ab) / len(ab), "le05": sum(e <= 0.5 for e in ab) / len(ab),
            "le10": sum(e <= 1.0 for e in ab) / len(ab), "signed_median": statistics.median(errs)}
