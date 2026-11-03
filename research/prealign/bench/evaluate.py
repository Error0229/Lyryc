"""Score predictions (or one reference) against the word-timed references.

  uv run python -X utf8 bench/evaluate.py torch_t8_c30            # prediction vs every reference
  uv run python -X utf8 bench/evaluate.py --refs                  # reference vs reference (noise floor)
  uv run python -X utf8 bench/evaluate.py torch_t8_c30 --tracks   # per-track line / word medians
  uv run python -X utf8 bench/evaluate.py torch_t8_c30 --offset   # minus each (track, ref) median line offset

Writes results/score_<name>.json (numbers only, no lyric text).
Reference roles: qq / kugou / netease-yrc are word-timed (QRC, KRC, YRC); lrclib is line-only.
A reference is skipped for a track if its duration differs from the YTM audio by > 2 s.
"""
import argparse, json, pathlib, statistics
from score import clean, compare, summary

ROOT = pathlib.Path(__file__).resolve().parent.parent
LANGS = ["en", "ja", "zh", "ko"]
REFS = ["qq", "kugou", "netease", "lrclib"]


def load():
    tracks = json.loads((ROOT / "tracks.json").read_text(encoding="utf-8"))
    refs = json.loads((ROOT / "data/refs.json").read_text(encoding="utf-8"))
    fetch = json.loads((ROOT / "data/audio/fetch.json").read_text(encoding="utf-8"))
    return tracks, refs, fetch


def usable(r, t, fetch, word_level):
    if not r or r.get("error") or not r.get("lines"):
        return None
    if word_level and (r.get("line_only") or not any(l.get("words") for l in r["lines"])):
        return None
    dur = (fetch.get(str(t["id"])) or {}).get("duration") or t["duration"]
    if r["meta"][3] and abs(r["meta"][3] - dur) > 2:
        return None
    return clean(r["lines"], t["track"], t["artist"])


def pool(rows):
    """rows: [(lang, kind, errs)] -> {kind: {lang|all: summary}}"""
    out = {}
    for kind in ("word", "line"):
        out[kind] = {}
        for lang in LANGS + ["all"]:
            errs = [e for l, k, es in rows if k == kind and (lang == "all" or l == lang) for e in es]
            out[kind][lang] = summary(errs)
    return out


def table(title, res, kind):
    print(f"\n{title} — {kind} onsets")
    print("| ref | lang | n | median | ≤0.1 | ≤0.2 | ≤0.3 | ≤0.5 | ≤1.0 |")
    print("|---|---|---|---|---|---|---|---|---|")
    for ref, by in res.items():
        for lang in LANGS + ["all"]:
            s = (by.get(kind) or {}).get(lang)
            if s:
                print(f"| {ref} | {lang} | {s['n']} | {s['median']:.3f} | {s['le01']:.0%} | {s['le02']:.0%} | "
                      f"{s['le03']:.0%} | {s['le05']:.0%} | {s['le10']:.0%} |")


def centre(c):
    """Remove one constant per (track, ref): the reference's lead-in convention, not aligner error."""
    off = statistics.median([e for e, _ in c["line"]]) if c["line"] else 0
    return {**c, "line": [(e - off, li) for e, li in c["line"]], "word": [(e - off, li) for e, li in c["word"]]}


def score_pred(name, per_track, offset=False):
    tracks, refs, fetch = load()
    preds = json.loads((ROOT / f"data/pred/{name}.json").read_text(encoding="utf-8"))
    res, tr = {}, {}
    for ref in REFS:
        rows = []
        for t in tracks:
            p = preds.get(str(t["id"]))
            r = usable(refs[str(t["id"])].get(ref), t, fetch, word_level=ref != "lrclib")
            if not p or not r:
                continue
            c = compare(r, p["lines"])
            c = centre(c) if offset else c
            rows.append((t["lang"], "line", [e for e, _ in c["line"]]))
            if ref != "lrclib":
                rows.append((t["lang"], "word", [e for e, _ in c["word"]]))
            tr.setdefault(t["id"], {})[ref] = {
                "line_med": statistics.median([abs(e) for e, _ in c["line"]]) if c["line"] else None,
                "line_signed": statistics.median([e for e, _ in c["line"]]) if c["line"] else None,
                "word_med": statistics.median([abs(e) for e, _ in c["word"]]) if c["word"] and ref != "lrclib" else None,
                "matched": c["matched"], "same_text": ref == p["text_src"]}
        res[ref] = pool(rows)
    if per_track:
        for t in tracks:
            cells = " ".join(f"{k}:{v['line_med']:.2f}/{v['word_med'] or 0:.2f}({v['line_signed']:+.2f})"
                             for k, v in tr.get(t["id"], {}).items() if v["line_med"] is not None)
            print(t["id"], t["lang"], t["track"], cells)
    return res, tr


def score_refs(offset=False):
    """Every ordered pair of word-timed references on the same track: how much do they disagree?"""
    tracks, refs, fetch = load()
    res = {}
    for a in REFS:
        for b in REFS:
            if a >= b:
                continue
            rows = []
            for t in tracks:
                ra = usable(refs[str(t["id"])].get(a), t, fetch, word_level=a != "lrclib")
                rb = usable(refs[str(t["id"])].get(b), t, fetch, word_level=b != "lrclib")
                if not ra or not rb:
                    continue
                c = compare(ra, rb)
                if c["matched"] < 0.5:
                    continue
                c = centre(c) if offset else c
                rows.append((t["lang"], "line", [e for e, _ in c["line"]]))
                if "lrclib" not in (a, b):
                    rows.append((t["lang"], "word", [e for e, _ in c["word"]]))
            res[f"{a}~{b}"] = pool(rows)
    return res


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("name", nargs="?")
    ap.add_argument("--refs", action="store_true")
    ap.add_argument("--tracks", action="store_true")
    ap.add_argument("--offset", action="store_true")
    a = ap.parse_args()
    if a.refs:
        res, name, tr = score_refs(a.offset), "refs", None
    else:
        (res, tr), name = score_pred(a.name, a.tracks, a.offset), a.name
    name += "_offset" if a.offset else ""
    table(name, res, "line")
    table(name, res, "word")
    (ROOT / f"results/score_{name}.json").write_text(json.dumps({"pooled": res, "per_track": tr}, indent=1), encoding="utf-8")


if __name__ == "__main__":
    main()
