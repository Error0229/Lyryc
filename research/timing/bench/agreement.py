"""Noise floor of human-made LRC: how much do independent sources disagree on line starts?

Compares each pair among {LRCLIB truth, QQ, NetEase, Kugou} on lines both cover
(after mapping onto the LRCLIB line list). Usage: python bench/agreement.py
"""
import itertools, statistics
from common import load
from results import load_all

if __name__ == "__main__":
    data, res = load(), load_all()
    series = {"lrclib": {str(t["id"]): [g for g, _ in t["truth"]] for t in data}}
    for k in ("s_qq", "s_netease", "s_kugou"):
        series[k[2:]] = {i: r["times"] for i, r in res[k]["tracks"].items() if r["times"]}
    print("| pair | tracks | median abs diff (s) | within 0.3 s |\n|---|---|---|---|")
    for a, b in itertools.combinations(series, 2):
        diffs, n = [], 0
        for i in series[a].keys() & series[b].keys():
            n += 1
            diffs += [abs(x - y) for x, y in zip(series[a][i], series[b][i])]
        print(f"| {a} vs {b} | {n} | {statistics.median(diffs):.2f} | {sum(d <= .3 for d in diffs) / len(diffs):.0%} |")
