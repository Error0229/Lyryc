"""Hit rates and miss causes from a harness run.

    python -X utf8 analyze.py before [after]

A row is one search: (case, mode, provider, name). A provider's verdict is
its accepted search, else its first miss. The answer for a case is the
real find_with's (after.jsonl's "-pipeline" rows), or for before.jsonl,
with one search per provider, the first accepted in QQ -> NetEase -> Kugou.
"Right" means the accepted candidate names the case's song (the harness's
`pick_is_song`) and is not in WRONG_RECORDING below.
"""
import json, sys
from collections import Counter, defaultdict
from pathlib import Path

HERE = Path(__file__).parent
PROVIDERS = ["qq", "netease", "kugou"]

# Picks that name the right song but are another recording, checked by hand:
# (title, artist, duration) as the provider lists them.
WRONG_RECORDING = {
    # Japanese-language version of 봄날: other lyrics.
    ("Spring Day (Japanese ver.)", "BTS (防弹少年团)", 276.693),
    # Same timestamps as QQ's "Subtitle (... LIVE at STADIUM 2025)" lyric:
    # Kugou's unmarked entry is the live recording, not the 322 s video.
    ("Subtitle", "Official髭男dism", 323.0),
}


def load(label):
    return [json.loads(l) for l in (HERE / "results" / f"{label}.jsonl").read_text(encoding="utf-8").splitlines()]


def verdict(row):
    """right / wrong / the miss reason."""
    if row["outcome"] == "rejected":  # before.jsonl's name for a failed guard
        return "rejected-" + row["reject"] + ("-song" if row.get("pick_is_song") else "-other")
    if row["outcome"] != "accepted":
        return row["outcome"]
    pick = tuple(row["pick"])
    ok = row.get("pick_is_song") and pick not in WRONG_RECORDING
    return "right" if ok else "wrong"


def per_provider(rows, mode):
    """{(id, provider): verdict}: the accepted search if any, else the first miss."""
    out = {}
    for r in rows:
        if r["mode"] != mode:
            continue
        key = (r["id"], r["provider"])
        v = verdict(r)
        if key not in out or v in ("right", "wrong"):
            out[key] = v
    return out


def pipeline(rows, mode):
    """{id: verdict} of the answer the app would give."""
    piped = [r for r in rows if r["mode"] == mode + "-pipeline"]
    per = per_provider(rows, mode)
    if piped:  # the harness ran the real find_with
        return {r["id"]: per[(r["id"], r["provider"])] if r["provider"] else "miss" for r in piped}
    out = {}
    for (i, p), v in per.items():  # one search per provider: first accepted in order
        if out.get(i, "miss") == "miss":
            out[i] = v if v in ("right", "wrong") else "miss"
    return out


def table(rows, modes):
    print("| mode | provider | right | wrong | of |")
    print("|---|---|---|---|---|")
    for mode in modes:
        per = per_provider(rows, mode)
        for p in PROVIDERS:
            v = [x for (i, q), x in per.items() if q == p]
            print(f"| {mode} | {p} | {v.count('right')} | {v.count('wrong')} | {len(v)} |")
        c = Counter(pipeline(rows, mode).values())
        print(f"| {mode} | **first accepted** | **{c['right']}** | **{c['wrong']}** | {sum(c.values())} |")


def causes(rows, mode):
    """Why each search missed, by reason."""
    c = Counter()
    for r in rows:
        if r["mode"] == mode and r["outcome"] != "accepted":
            v = verdict(r)
            if v == "no_same_length":
                v += " (song at other length)" if r.get("song_deltas") else " (song not listed)"
            c[v] += 1
    return c


def length_detail(rows):
    print("\nSong listed only outside +-3 s (case, mode, provider: deltas in s):")
    for r in rows:
        d = r.get("song_deltas") or []
        if r.get("outcome") == "no_same_length" and d:
            print(f"  #{r['id']} {r['mode']} {r['provider']}: {sorted(set(round(x, 1) for x in d))}")


def main():
    for label in sys.argv[1:]:
        rows = load(label)
        modes = sorted({r["mode"] for r in rows if not r["mode"].endswith("-pipeline")})
        print(f"\n## {label}\n")
        table(rows, modes)
        for mode in modes:
            print(f"\nMisses by reason, mode {mode}:")
            for k, n in causes(rows, mode).most_common():
                print(f"  {n:3} {k}")
        print("\nPer case, first accepted:")
        per = {m: pipeline(rows, m) for m in modes}
        for i in sorted(per[modes[0]]):
            print(f"  #{i}: " + "  ".join(f"{m}={per[m].get(i, '-')}" for m in modes))
        length_detail(rows)


main()
