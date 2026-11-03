"""cases.src.json -> cases.json: fill each case's duration from its pinned LRCLIB record.

The reference lyrics stay in the gitignored HTTP cache (data/.http), as in research/timing.
"""
import json
from pathlib import Path
import lrclib

HERE = Path(__file__).parent
cases = json.loads((HERE / "data/cases.src.json").read_text(encoding="utf-8"))
for c in cases:
    if c["ref_id"]:
        r = lrclib.by_id(c["ref_id"])
        c["duration"] = round(r["duration"])
        print(c["id"], r["trackName"], "|", r["artistName"], "|", r["albumName"], r["duration"],
              "synced" if r["syncedLyrics"] else ("instrumental" if r["instrumental"] else "plain"))
(HERE / "data/cases.json").write_text(json.dumps(cases, ensure_ascii=False, indent=1), encoding="utf-8")
print("lrclib", lrclib.stats)
