"""Per-track diagnostics for one method: median signed error (constant offset = audio
version mismatch, not aligner error) and median abs error after removing that offset.
The offset-removed number uses ground truth, so it is an oracle diagnostic only.
Usage: python bench/per_track.py a_mms_fa
"""
import statistics, sys
from common import load
from results import load_all

if __name__ == "__main__":
    r = load_all()[sys.argv[1]]["tracks"]
    rows = []
    for t in load():
        x = r.get(str(t["id"]))
        if not x or x["times"] is None:
            print(f"{t['id']:>2} {t['lang']} {t['track'][:22]:<22} -- no result")
            continue
        d = [p - g for (g, _), p in zip(t["truth"], x["times"]) if p is not None]
        off = statistics.median(d)
        raw = statistics.median(abs(e) for e in d)
        fixed = statistics.median(abs(e - off) for e in d)
        rows.append((off, raw, fixed))
        print(f"{t['id']:>2} {t['lang']} {t['track'][:22]:<22} offset {off:+6.2f}  med|err| {raw:5.2f}  "
              f"offset-removed {fixed:5.2f}  >5s {sum(abs(e) > 5 for e in d):>2}/{len(d)}")
    print("median offset", round(statistics.median(o for o, _, _ in rows), 2))
