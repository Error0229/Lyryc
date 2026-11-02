"""Shared dataset loading, line cleaning and scoring for the timing benchmark."""
import json, pathlib, re, statistics

ROOT = pathlib.Path(__file__).resolve().parent.parent
LRC = re.compile(r"^\[(\d+):(\d+(?:\.\d+)?)\](.*)$")
# Credit / header lines that LRCLIB entries scraped from NetEase/QQ carry at 0:00.
CREDIT = re.compile(r"^(作词|作詞|作曲|编曲|編曲|词|詞|曲|制作人|製作人|监制|合声|吉他|贝斯|鼓|录音|混音|母带|和声|"
                    r"(Lyrics|Composed|Arranged|Produced|Written|Music)( by)?|Composer|Producer)\s*[:：]", re.I)


def clean_text(text):
    # "원문^translation" bilingual lines: keep the sung part only.
    return text.split("^")[0].strip()


def is_credit(text, track, artist):
    if CREDIT.search(text):
        return True
    t = text.replace(" ", "").lower()
    return " - " in text and (track.replace(" ", "").lower() in t or artist.replace(" ", "").lower() in t)


def parse_lrc(lrc, track="", artist=""):
    out = []
    for raw in lrc.splitlines():
        m = LRC.match(raw.strip())
        if not m:
            continue
        text = clean_text(m.group(3))
        if not text or is_credit(text, track, artist):
            continue
        out.append((int(m.group(1)) * 60 + float(m.group(2)), text))
    out.sort()
    return out


def load():
    data = json.loads((ROOT / "data/dataset.json").read_text(encoding="utf-8"))
    for t in data:
        t["truth"] = parse_lrc(t["synced"], t["track"], t["artist"])
        # Methods get the line texts only. We use the synced text (not plainLyrics)
        # so predictions and truth line up 1:1; on LRCLIB they are the same text
        # in 26/27 tracks anyway.
        t["lines"] = [x for _, x in t["truth"]]
    return data


def score(truth, pred):
    """truth: [(t, text)], pred: [float|None] same length. None = line not timed."""
    errs = [abs(p - t) for (t, _), p in zip(truth, pred) if p is not None]
    n = len(truth)
    if not errs:
        return None
    return {
        "n": n, "timed": len(errs),
        "mean": statistics.fmean(errs), "median": statistics.median(errs),
        "le03": sum(e <= 0.3 for e in errs) / n, "le05": sum(e <= 0.5 for e in errs) / n,
        "le10": sum(e <= 1.0 for e in errs) / n,
    }
