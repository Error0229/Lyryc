"""How trustworthy is each word-timed reference?

Per source:
  - synthetic lines: >= 3 words whose durations are all equal (±15 ms), i.e. the line
    time was spread evenly instead of being timed by ear (QRC credit/title lines look so);
  - gapless lines: every word ends exactly where the next starts (karaoke-style "fill");
  - identical-to-QQ share for Kugou (both are Tencent Music: one dataset or two?);
  - LRCLIB enhanced LRC: how many LRCLIB search hits for our tracks carry <mm:ss.xx> word tags.
Usage: uv run python -X utf8 bench/refquality.py
"""
import json, pathlib, statistics, urllib.parse
from refs import http, parse_lrc
from score import clean, compare

ROOT = pathlib.Path(__file__).resolve().parent.parent


def line_kinds(lines):
    syn = gapless = n = 0
    for l in lines:
        ws = [w for w in l.get("words") or [] if w["text"].strip()]
        if len(ws) < 3:
            continue
        n += 1
        d = [w["end"] - w["start"] for w in ws]
        syn += max(d) - min(d) <= 0.015
        gapless += all(abs(a["end"] - b["start"]) < 0.005 for a, b in zip(ws, ws[1:]))
    return n, syn, gapless


def main():
    tracks = json.loads((ROOT / "tracks.json").read_text(encoding="utf-8"))
    refs = json.loads((ROOT / "data/refs.json").read_text(encoding="utf-8"))
    print("| source | tracks | lines (≥3 words) | evenly spread | gapless |")
    print("|---|---|---|---|---|")
    for src in ("qq", "kugou", "netease"):
        tot = [0, 0, 0]
        k = 0
        for t in tracks:
            r = refs[str(t["id"])].get(src)
            if not r or r.get("error") or r.get("line_only"):
                continue
            k += 1
            for i, x in enumerate(line_kinds(clean(r["lines"], t["track"], t["artist"]))):
                tot[i] += x
        print(f"| {src} | {k} | {tot[0]} | {tot[1] / max(1, tot[0]):.0%} | {tot[2] / max(1, tot[0]):.0%} |")

    same = []
    for t in tracks:
        a, b = refs[str(t["id"])].get("qq"), refs[str(t["id"])].get("kugou")
        if not a or not b or a.get("error") or b.get("error"):
            continue
        c = compare(clean(a["lines"], t["track"], t["artist"]), clean(b["lines"], t["track"], t["artist"]))
        errs = [abs(e) for e, _ in c["word"]]
        same.append((t["track"], t["lang"], sum(e < 0.011 for e in errs) / max(1, len(errs))))
    print("\nKugou word onsets identical to QQ (±10 ms), per track:")
    print(", ".join(f"{n} ({l}) {s:.0%}" for n, l, s in same))

    enh = total = 0
    for t in tracks:
        q = urllib.parse.urlencode({"track_name": t["track"], "artist_name": t["artist"]})
        try:
            hits = json.loads(http("https://lrclib.net/api/search?" + q, headers={"User-Agent": "Lyryc research"}))
        except Exception:
            continue
        for h in hits:
            if h.get("syncedLyrics"):
                total += 1
                enh += any(l["words"] for l in parse_lrc(h["syncedLyrics"]))
    print(f"\nLRCLIB search hits with synced lyrics: {total}; with enhanced <mm:ss> word tags: {enh}")


if __name__ == "__main__":
    main()
