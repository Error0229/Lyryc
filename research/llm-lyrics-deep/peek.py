"""Structure of raw replies, never their text: parse state, keys, line counts, refusal keywords."""
import json, re, sys
from common import RAW, lines_of
from score import reply_of

KW = re.compile(r"copyright|cannot|can't|can not|unable|not able|licens|reproduc|sorry|excerpt|snippet|partial", re.I)


def peek(path):
    raw = json.loads(path.read_text(encoding="utf-8"))
    reply, content = reply_of(raw)
    lyr = (reply or {}).get("lyrics")
    info = {"chars": len(content), "fence": "```" in content, "json": reply is not None}
    if reply is not None:
        info.update(keys=sorted(reply), found=reply.get("found"), lyr_type=type(lyr).__name__,
                    lines=len(lines_of(lyr if isinstance(lyr, (list, str)) else "")), urls=len(reply.get("source_urls") or []))
    info["kw"] = sorted({m.lower() for m in KW.findall(content)})
    return info


for pat in sys.argv[1:]:
    for p in sorted(RAW.glob(pat)):
        print(p.parent.name, p.stem, peek(p))
