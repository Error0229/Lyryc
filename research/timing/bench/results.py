"""Result files: results/<method>.json = {"info": {...}, "tracks": {id: {times, secs, meta}}}."""
import json
from common import ROOT

RES = ROOT / "results"


def save(name, tracks, info):
    RES.mkdir(exist_ok=True)
    (RES / f"{name}.json").write_text(json.dumps({"info": info, "tracks": tracks}, ensure_ascii=False, indent=0),
                                      encoding="utf-8")


def load_all():
    return {p.stem: json.loads(p.read_text(encoding="utf-8")) for p in sorted(RES.glob("*.json"))}
