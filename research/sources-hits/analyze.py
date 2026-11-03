"""Hit rates and miss causes from a harness run.

    python -X utf8 analyze.py before [after]

A row is one (case, mode, provider). The pipeline answer for a case is the
first provider, in QQ -> NetEase -> Kugou order, whose result was accepted.
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
    """accepted-right / accepted-wrong / the miss reason."""
    if row["outcome"] != "accepted":
        if row["outcome"] == "rejected":
            return "rejected-" + row["reject"] + ("-song" if row.get("pick_is_song") else "-other")
        return row["outcome"]
    pick = tuple(row["pick"])
    ok = row.get("pick_is_song") and pick not in WRONG_RECORDING
    return "right" if ok else "wrong"


def table(rows, modes):
    by = defaultdict(list)
    for r in rows:
        by[(r["mode"], r["provider"])].append(verdict(r))
    print("| mode | provider | right | wrong | of |")
    print("|---|---|---|---|---|")
    for mode in modes:
        for p in PROVIDERS:
            v = by[(mode, p)]
            if v:
                print(f"| {mode} | {p} | {v.count('right')} | {v.count('wrong')} | {len(v)} |")
        answers = pipeline(rows, mode)
        c = Counter(answers.values())
        print(f"| {mode} | **first accepted** | **{c['right']}** | **{c['wrong']}** | {len(answers)} |")


def pipeline(rows, mode):
    out = {}
    for r in rows:
        if r["mode"] != mode:
            continue
        if r["id"] not in out:
            out[r["id"]] = "miss"
        if out[r["id"]] == "miss" and r["outcome"] == "accepted":
            out[r["id"]] = verdict(r)
    return out


def causes(rows, mode):
    """Why each provider missed (no accepted result), by reason."""
    c = Counter()
    for r in rows:
        if r["mode"] == mode and r["outcome"] != "accepted":
            v = verdict(r)
            if v == "no_same_length":
                deltas = r.get("song_deltas") or []
                v += " (song at other length)" if deltas else " (song not listed)"
            c[v] += 1
    return c


def length_detail(rows):
    print("\nSong listed only outside +-3 s (case, mode, provider: deltas in s):")
    for r in rows:
        d = r.get("song_deltas") or []
        if r["outcome"] == "no_same_length" and d:
            print(f"  #{r['id']} {r['mode']} {r['provider']}: {sorted(round(x, 1) for x in d)}")


def main():
    for label in sys.argv[1:]:
        rows = load(label)
        modes = sorted({r["mode"] for r in rows})
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
