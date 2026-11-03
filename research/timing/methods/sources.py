"""Synced lyrics from third-party sources, mapped onto the LRCLIB line list.

Usage: python methods/sources.py   (needs network; responses cached in data/.http)
"""
import sys, time, pathlib
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent / "bench"))
from common import load, parse_lrc
from results import save
from align_text import map_lines, interpolate
from sources import PROVIDERS


def run(name, fetch, data):
    res = {}
    for t in data:
        track, artist = t["query"]  # what the browser extension would report
        t0 = time.perf_counter()
        try:
            lrc, meta = fetch(track, artist, t["duration"])
        except Exception as e:
            lrc, meta = None, f"error: {e}"
        secs = time.perf_counter() - t0
        src = parse_lrc(lrc, t["track"], t["artist"]) if lrc else []
        if len(src) < 3:
            res[t["id"]] = {"times": None, "secs": secs, "meta": meta if not lrc else "unsynced/too short"}
            continue
        mapped = map_lines(t["lines"], src)
        matched = sum(x is not None for x in mapped)
        if matched < 0.5 * len(mapped):  # wrong song or wrong language version
            res[t["id"]] = {"times": None, "secs": secs, "meta": f"text mismatch {matched}/{len(mapped)}"}
            continue
        exact = sum(x is not None and abs(x - gt) < 0.02 for x, (gt, _) in zip(mapped, t["truth"]))
        res[t["id"]] = {"times": interpolate(mapped, t["duration"]), "secs": secs,
                        "meta": {**meta, "matched": matched, "identical_ts": exact}}
        print(name, t["id"], t["track"], f"matched {matched}/{len(mapped)} identical {exact}")
    return res


if __name__ == "__main__":
    data = load()
    for name, fetch in PROVIDERS.items():
        save(f"s_{name}", run(name, fetch, data), {"download_mb": 0, "ram_mb": 0, "needs_audio": False})
