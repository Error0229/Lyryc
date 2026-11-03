"""Speed table from results/speed_*.json: realtime factor (compute s / audio s), s per 4-min track.
Usage: python bench/speed.py"""
import json, pathlib, statistics

ROOT = pathlib.Path(__file__).resolve().parent.parent
print("| run | backend | threads | chunk+ctx (s) | tracks | median RTF | range | s per 4-min track | load (s) | peak RSS (MB) |")
print("|---|---|---|---|---|---|---|---|---|---|")
for p in sorted((ROOT / "results").glob("speed_*.json")):
    d = json.loads(p.read_text())
    if "backend" not in d or p.stem == "speed_smoke":
        continue
    r = [t["rtf"] for t in d["tracks"].values()]
    print(f"| {p.stem[6:]} | {d['backend']} | {d['threads']} | {d['chunk']}+{d.get('ctx', 0)} | {len(r)} | {statistics.median(r):.3f} | "
          f"{min(r):.2f}–{max(r):.2f} | {statistics.median(r) * 240:.0f} | {d['load_s']:.1f} | {d['peak_rss_mb']} |")
d = json.loads((ROOT / "results/speed_demucs.json").read_text())
r = [t["rtf"] for t in d["tracks"].values()]
print(f"| demucs htdemucs (separation only) | torch | 8 | – | {len(r)} | {statistics.median(r):.3f} | {min(r):.2f}–{max(r):.2f} | "
      f"{statistics.median(r) * 240:.0f} | – | {d['peak_rss_mb']} |")
