"""Aggregate results/*.json into the benchmark table.

Usage: python bench/report.py [--by-lang] [--ids 3,5,6] [--oracle-offset]
  --oracle-offset : subtract each track's median signed error first. This uses ground
                    truth, so it is a diagnostic: it isolates aligner error from a constant
                    shift between the downloaded audio and the LRCLIB timeline.
"""
import statistics, sys
from common import load, score
from results import load_all


def pooled(data, tracks, oracle=False):
    """Line-pooled metrics over tracks the method covered, plus coverage."""
    errs, n, covered, secs = [], 0, 0, []
    for t in data:
        r = tracks.get(str(t["id"]))
        if not r or r.get("times") is None:
            continue
        covered += 1
        secs.append(r["secs"])
        d = [p - gt if p is not None else None for (gt, _), p in zip(t["truth"], r["times"])]
        off = statistics.median(x for x in d if x is not None) if oracle else 0.0
        n += len(d)
        errs += [abs(x - off) for x in d if x is not None]
    if not errs:
        return None
    return {"cov": covered / len(data), "mean": statistics.fmean(errs), "median": statistics.median(errs),
            "le03": sum(e <= .3 for e in errs) / n, "le05": sum(e <= .5 for e in errs) / n,
            "le10": sum(e <= 1 for e in errs) / n, "secs": statistics.median(secs)}


def row(name, info, m):
    if not m:
        return f"| {name} | 0% | – | – | – | – | – | – | {info.get('download_mb', '?')} | {info.get('ram_mb', '?')} |"
    return (f"| {name} | {m['cov']:.0%} | {m['mean']:.2f} | {m['median']:.2f} | {m['le03']:.0%} | {m['le05']:.0%} "
            f"| {m['le10']:.0%} | {m['secs']:.2f} | {info.get('download_mb', '?')} | {info.get('ram_mb', '?')} |")


HEAD = ("| method | coverage | mean abs err (s) | median (s) | ≤0.3s | ≤0.5s | ≤1.0s | time/track (s, median) "
        "| download (MB) | peak RAM (MB) |\n|---|---|---|---|---|---|---|---|---|---|")

if __name__ == "__main__":
    data = load()
    allres = load_all()
    if "--ids" in sys.argv:
        ids = sys.argv[sys.argv.index("--ids") + 1].split(",")
        data = [t for t in data if str(t["id"]) in ids]
    oracle = "--oracle-offset" in sys.argv
    groups = [("all", data)]
    if "--by-lang" in sys.argv:
        groups += [(lg, [t for t in data if t["lang"] == lg]) for lg in ("en", "ja", "zh", "ko")]
    for g, subset in groups:
        print(f"\n**{g}** ({len(subset)} tracks, {sum(len(t['truth']) for t in subset)} lines)\n")
        print(HEAD)
        for name, r in allres.items():
            print(row(name, r["info"], pooled(subset, r["tracks"], oracle)))
