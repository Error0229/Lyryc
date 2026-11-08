"""Candidate reject rules, scored on results/confidence_<name>.json (from confidence.py).

A rule rejects lines (their word timing is hidden and the line falls back to line-level
or unsynced display). Precision = rejected lines that really are > 1 s off; recall =
bad lines caught; then the accuracy of what is kept.

  python bench/reject.py prod [line low share]     # thresholds, default the MMS rule 0.10 0.15 0.6
  python bench/reject.py echo_clean sweep          # the combined rule over a grid of thresholds
"""
import json, pathlib, statistics, sys

ROOT = pathlib.Path(__file__).resolve().parent.parent


def smooth(vals, k=1):
    return [statistics.median(vals[max(0, i - k):i + k + 1]) for i in range(len(vals))]


def rules(lines, line=0.10, low_th=0.15, share=0.6):
    by = {}
    for x in lines:
        by.setdefault(x["track"], []).append(x)
    out = {}
    for tid, ls in by.items():
        c = [x["conf"] for x in ls]
        sm = smooth(c)
        med = statistics.median(c)
        low = sum(v < low_th for v in c) / len(c)
        for x, v, s in zip(ls, c, sm):
            out.setdefault(f"conf<{line}", []).append((x, v < line))
            out.setdefault(f"smoothed3 conf<{line}", []).append((x, s < line))
            out.setdefault("conf<0.4×track median", []).append((x, v < 0.4 * med))
            out.setdefault(f"track: >{share:.0%} lines conf<{low_th}", []).append((x, low > share))
            out.setdefault(f"smoothed3<{line} OR track >{share:.0%} low", []).append((x, s < line or low > share))
            if x.get("lrc_dis") is not None:
                out.setdefault(f"[LRCLIB subset] conf<{line}", []).append((x, v < line))
                out.setdefault("[LRCLIB subset] off LRCLIB line by >0.7 s", []).append((x, x["lrc_dis"] > 0.7))
                out.setdefault(f"[LRCLIB subset] smoothed3<{line} OR track low OR off LRCLIB >0.7 s", []).append(
                    (x, s < line or low > share or x["lrc_dis"] > 0.7))
    return out


def row(rule, xs):
    rej = [x for x, r in xs if r]
    kept = [x for x, r in xs if not r]
    bad = sum(x["err"] > 1 for x, _ in xs)
    print(f"| {rule} | {len(rej) / len(xs):.0%} | {sum(x['err'] > 1 for x in rej) / max(1, len(rej)):.0%} | "
          f"{sum(x['err'] > 0.5 for x in rej) / max(1, len(rej)):.0%} | {sum(x['err'] > 1 for x in rej) / max(1, bad):.0%} | "
          f"{sum(x['err'] <= 0.3 for x in kept) / max(1, len(kept)):.0%} | {sum(x['err'] <= 1 for x in kept) / max(1, len(kept)):.0%} |")


def main():
    name = sys.argv[1] if len(sys.argv) > 1 else "prod"
    lines = json.loads((ROOT / f"results/confidence_{name}.json").read_text(encoding="utf-8"))["lines"]
    print("| rule | rejected | precision (>1 s) | precision (>0.5 s) | recall (>1 s) | kept ≤0.3 s | kept ≤1.0 s |")
    print("|---|---|---|---|---|---|---|")
    print(f"| none | 0% | – | – | – | {sum(x['err'] <= 0.3 for x in lines) / len(lines):.0%} | {sum(x['err'] <= 1 for x in lines) / len(lines):.0%} |")
    if sys.argv[2:3] == ["sweep"]:
        for line in (0.10, 0.12, 0.15, 0.18, 0.20):
            for low in (0.15, 0.20, 0.25):
                for share in (0.5, 0.6):
                    key = f"smoothed3<{line} OR track >{share:.0%} low"
                    row(f"{key} (<{low})", rules(lines, line, low, share)[key])
        return
    th = [float(v) for v in sys.argv[2:5]]
    for rule, xs in rules(lines, *th).items():
        row(rule, xs)


if __name__ == "__main__":
    main()
