"""Candidate reject rules, scored on results/confidence_<name>.json (from confidence.py).

A rule rejects lines (their word timing is hidden and the line falls back to line-level
or unsynced display). Precision = rejected lines that really are > 1 s off; recall =
bad lines caught; then the accuracy of what is kept. Usage: python bench/reject.py prod
"""
import json, pathlib, statistics, sys

ROOT = pathlib.Path(__file__).resolve().parent.parent


def smooth(vals, k=1):
    return [statistics.median(vals[max(0, i - k):i + k + 1]) for i in range(len(vals))]


def rules(lines):
    by = {}
    for x in lines:
        by.setdefault(x["track"], []).append(x)
    out = {}
    for tid, ls in by.items():
        c = [x["conf"] for x in ls]
        sm = smooth(c)
        med = statistics.median(c)
        low = sum(v < 0.15 for v in c) / len(c)
        for x, v, s in zip(ls, c, sm):
            out.setdefault("conf<0.10", []).append((x, v < 0.10))
            out.setdefault("smoothed3 conf<0.10", []).append((x, s < 0.10))
            out.setdefault("smoothed3 conf<0.12", []).append((x, s < 0.12))
            out.setdefault("conf<0.4×track median", []).append((x, v < 0.4 * med))
            out.setdefault("track: >60% lines conf<0.15", []).append((x, low > 0.6))
            out.setdefault("smoothed3<0.10 OR track >60% low", []).append((x, s < 0.10 or low > 0.6))
            if x.get("lrc_dis") is not None:
                out.setdefault("[LRCLIB subset] conf<0.10", []).append((x, v < 0.10))
                out.setdefault("[LRCLIB subset] off LRCLIB line by >0.7 s", []).append((x, x["lrc_dis"] > 0.7))
                out.setdefault("[LRCLIB subset] conf<0.10 OR off LRCLIB >0.7 s", []).append((x, v < 0.10 or x["lrc_dis"] > 0.7))
    return out


def main():
    name = sys.argv[1] if len(sys.argv) > 1 else "prod"
    lines = json.loads((ROOT / f"results/confidence_{name}.json").read_text(encoding="utf-8"))["lines"]
    print("| rule | rejected | precision (>1 s) | precision (>0.5 s) | recall (>1 s) | kept ≤0.3 s | kept ≤1.0 s |")
    print("|---|---|---|---|---|---|---|")
    base = [x for x in lines]
    print(f"| none | 0% | – | – | – | {sum(x['err'] <= 0.3 for x in base) / len(base):.0%} | {sum(x['err'] <= 1 for x in base) / len(base):.0%} |")
    for rule, xs in rules(lines).items():
        rej = [x for x, r in xs if r]
        kept = [x for x, r in xs if not r]
        bad = sum(x["err"] > 1 for x, _ in xs)
        print(f"| {rule} | {len(rej) / len(xs):.0%} | {sum(x['err'] > 1 for x in rej) / max(1, len(rej)):.0%} | "
              f"{sum(x['err'] > 0.5 for x in rej) / max(1, len(rej)):.0%} | {sum(x['err'] > 1 for x in rej) / max(1, bad):.0%} | "
              f"{sum(x['err'] <= 0.3 for x in kept) / len(kept):.0%} | {sum(x['err'] <= 1 for x in kept) / len(kept):.0%} |")


if __name__ == "__main__":
    main()
