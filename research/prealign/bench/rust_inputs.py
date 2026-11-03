# Dump the study's aligner input (run.py source_text) per track -> inputs.json.
import json, sys, pathlib
sys.path.insert(0, sys.argv[1] + "/bench")
from run import source_text
root = pathlib.Path(sys.argv[1])
tracks = json.loads((root / "tracks.json").read_text(encoding="utf-8"))
refs = json.loads((root / "data/refs.json").read_text(encoding="utf-8"))
ids = sys.argv[3].split(",")
out = {}
for t in tracks:
    if str(t["id"]) not in ids: continue
    src, lines = source_text(refs[str(t["id"])], t)
    out[str(t["id"])] = {"text_src": src, "lines": lines, "videoId": t["videoId"], "duration": t["duration"], "lang": t["lang"], "track": t["track"]}
pathlib.Path(sys.argv[2]).write_text(json.dumps(out, ensure_ascii=False, indent=1), encoding="utf-8")
print({k: (v["text_src"], len(v["lines"])) for k, v in out.items()})
