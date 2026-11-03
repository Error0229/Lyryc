"""Can the aligner's own confidence reject bad alignments?

Line truth: the line's onset error against the first usable word-timed reference
(qq, kugou, netease), else LRCLIB. A line is *bad* if |err| > 1.0 s; a word is bad if
|err| > 0.3 s.

Line features (all from the alignment itself, no reference):
  conf   mean frame posterior of the line's tokens
  cps    text characters per second of the line span (too fast = squeezed)
Track features: median line conf, share of lines with conf below the line threshold.

Prints precision / recall of "reject" for a sweep of thresholds, and the accuracy of
what is kept. Usage: uv run python -X utf8 bench/confidence.py fp32
"""
import json, pathlib, statistics, sys
from evaluate import load, usable
from score import compare, stream

ROOT = pathlib.Path(__file__).resolve().parent.parent
ORDER = ["qq", "kugou", "netease", "lrclib"]


def rows(name):
    tracks, refs, fetch = load()
    preds = json.loads((ROOT / f"data/pred/{name}.json").read_text(encoding="utf-8"))
    out = []
    for t in tracks:
        p = preds.get(str(t["id"]))
        if not p:
            continue
        for src in ORDER:
            r = usable(refs[str(t["id"])].get(src), t, fetch, word_level=src != "lrclib")
            if r:
                break
        else:
            continue
        # Map each reference line's error onto the prediction line that holds its first char.
        c = compare(r, p["lines"])
        b = stream(p["lines"])
        a = stream(r)
        import difflib
        sm = difflib.SequenceMatcher(None, [x[0] for x in a], [x[0] for x in b], autojunk=False)
        m = {i + k: j + k for i, j, n in sm.get_matching_blocks() for k in range(n)}
        first = {}
        for i, x in enumerate(a):
            if x[4] and i in m:
                first[x[2]] = b[m[i]][2]
        line_err = {}
        for e, li in c["line"]:
            if li in first:
                line_err[first[li]] = e
        for pi, l in enumerate(p["lines"]):
            if pi not in line_err or l["start"] is None:
                continue
            ntok = sum(len(w["text"]) for w in l["words"])
            span = max(0.02, (l["end"] or l["start"]) - l["start"])
            out.append({"track": t["id"], "lang": t["lang"], "conf": l["conf"], "cps": ntok / span,
                        "err": abs(line_err[pi]), "src": src})
    return out


def sweep(xs, key, bad, ths, lower_is_bad=True):
    print(f"\n| {key} threshold | rejected | precision (rejected are bad) | recall (bad caught) | kept ≤1.0 s | kept n |")
    print("|---|---|---|---|---|---|")
    nbad = sum(bad(x) for x in xs)
    for th in ths:
        rej = [x for x in xs if (x[key] < th if lower_is_bad else x[key] > th)]
        kept = [x for x in xs if x not in rej]
        tp = sum(bad(x) for x in rej)
        print(f"| {th} | {len(rej) / len(xs):.0%} | {tp / max(1, len(rej)):.0%} | {tp / max(1, nbad):.0%} | "
              f"{sum(x['err'] <= 1.0 for x in kept) / max(1, len(kept)):.0%} | {len(kept)} |")


def main():
    name = sys.argv[1] if len(sys.argv) > 1 else "fp32"
    xs = rows(name)
    bad = lambda x: x["err"] > 1.0
    print(f"lines: {len(xs)}, bad (>1 s): {sum(map(bad, xs))} ({sum(map(bad, xs)) / len(xs):.0%})")
    sweep(xs, "conf", bad, [0.05, 0.1, 0.15, 0.2, 0.25, 0.3, 0.4])
    sweep(xs, "cps", bad, [15, 20, 25, 30, 40], lower_is_bad=False)

    # Track level: the decision the app actually makes (cache it, or fall back).
    by = {}
    for x in xs:
        by.setdefault(x["track"], []).append(x)
    tr = []
    for tid, ls in by.items():
        tr.append({"track": tid, "lang": ls[0]["lang"], "med_conf": statistics.median(x["conf"] for x in ls),
                   "low_share": sum(x["conf"] < 0.15 for x in ls) / len(ls),
                   "bad_share": sum(map(bad, ls)) / len(ls), "med_err": statistics.median(x["err"] for x in ls)})
    print("\n| track | lang | median conf | share conf<0.15 | median line err | share >1 s |")
    print("|---|---|---|---|---|---|")
    for x in sorted(tr, key=lambda x: x["med_conf"]):
        print(f"| {x['track']} | {x['lang']} | {x['med_conf']:.2f} | {x['low_share']:.0%} | {x['med_err']:.2f} | {x['bad_share']:.0%} |")
    (ROOT / f"results/confidence_{name}.json").write_text(json.dumps({"lines": xs, "tracks": tr}, indent=1), encoding="utf-8")


if __name__ == "__main__":
    main()
