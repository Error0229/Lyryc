"""Which sites answers were copied from: domains of source_urls and url_citation annotations."""
import collections, json, sys
from urllib.parse import urlparse
from common import RAW
from score import reply_of


def domains(path):
    raw = json.loads(path.read_text(encoding="utf-8"))
    reply, _ = reply_of(raw)
    msg = ((raw.get("choices") or [{}])[0].get("message") or {})
    cited = {urlparse((a.get("url_citation") or {}).get("url") or "").netloc for a in msg.get("annotations") or []}
    used = {urlparse(u).netloc for u in (reply or {}).get("source_urls") or [] if isinstance(u, str)}
    return used - {""}, cited - {""}


if __name__ == "__main__":
    used_c, cited_c = collections.Counter(), collections.Counter()
    for pat in sys.argv[1:]:
        for p in RAW.glob(pat):
            u, c = domains(p)
            used_c.update(d.removeprefix("www.") for d in u)
            cited_c.update(d.removeprefix("www.") for d in c)
    print("source_urls:", used_c.most_common(15))
    print("citations:", cited_c.most_common(15))
