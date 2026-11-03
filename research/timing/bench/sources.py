"""Clients for third-party synced-lyrics sources (unofficial endpoints, research only).

Each `fetch_<name>(track, artist, duration)` returns (lrc_text, meta) or (None, reason).
Candidate choice is duration-first (|d - duration| <= 3 s) then title/artist overlap,
which is what an integration would do too.
"""
import base64, json, re, urllib.parse, urllib.request, zlib, hashlib, time
from common import ROOT

UA = {"User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64)"}
CACHE = ROOT / "data/.http"


def get(url, headers=None, cache=True):
    CACHE.mkdir(parents=True, exist_ok=True)
    hit = CACHE / (hashlib.sha1(url.encode()).hexdigest() + ".txt")
    if cache and hit.exists():
        return hit.read_text(encoding="utf-8")
    req = urllib.request.Request(url, headers={**UA, **(headers or {})})
    body = urllib.request.urlopen(req, timeout=15).read().decode("utf-8", "replace")
    hit.write_text(body, encoding="utf-8")
    time.sleep(0.2)
    return body


def norm(s):
    return re.sub(r"[\W_]+", "", (s or "").lower())


def sim(a, b):
    a, b = set(norm(a)), set(norm(b))
    return len(a & b) / max(1, len(a | b))


def best(cands, track, artist, duration):
    """cands: [(id, title, artist, duration_s, extra)]"""
    ok = [c for c in cands if c[3] and abs(c[3] - duration) <= 3]
    if not ok:
        return None
    cover = re.compile(r"cover|翻唱|女声|男声|伴奏|instrumental|karaoke|remix|live|dj", re.I)
    return max(ok, key=lambda c: sim(c[1], track) + sim(c[2], artist) - abs(c[3] - duration) / 30
               - (0.5 if cover.search(c[1]) else 0))


def fetch_netease(track, artist, duration):
    q = urllib.parse.urlencode({"s": f"{track} {artist}", "type": 1, "limit": 10})
    songs = json.loads(get("https://music.163.com/api/search/get?" + q)).get("result", {}).get("songs", [])
    cands = [(s["id"], s["name"], " ".join(a["name"] for a in s["artists"]), s["duration"] / 1000, None) for s in songs]
    c = best(cands, track, artist, duration)
    if not c:
        return None, f"no duration match among {len(cands)}"
    d = json.loads(get(f"https://music.163.com/api/song/lyric?id={c[0]}&lv=1&kv=1&tv=-1"))
    lrc = (d.get("lrc") or {}).get("lyric") or ""
    return (lrc or None), {"id": c[0], "title": c[1], "artist": c[2], "duration": c[3]}


def fetch_qq(track, artist, duration):
    ref = {"Referer": "https://y.qq.com/"}
    q = urllib.parse.urlencode({"w": f"{track} {artist}", "format": "json", "n": 10})
    lst = json.loads(get("https://c.y.qq.com/soso/fcgi-bin/client_search_cp?" + q, ref))["data"]["song"]["list"]
    cands = [(s["songmid"], s["songname"], " ".join(x["name"] for x in s["singer"]), s["interval"], None) for s in lst]
    c = best(cands, track, artist, duration)
    if not c:
        return None, f"no duration match among {len(cands)}"
    d = json.loads(get(f"https://c.y.qq.com/lyric/fcgi-bin/fcg_query_lyric_new.fcg?songmid={c[0]}&format=json&nobase64=1", ref))
    return (d.get("lyric") or None), {"id": c[0], "title": c[1], "artist": c[2], "duration": c[3]}


KRC_KEY = bytes([64, 71, 97, 119, 94, 50, 116, 71, 81, 54, 49, 45, 206, 210, 110, 105])


def decode_krc(b64):
    raw = base64.b64decode(b64)[4:]  # strip "krc1"
    return zlib.decompress(bytes(b ^ KRC_KEY[i % 16] for i, b in enumerate(raw))).decode("utf-8", "replace")


def krc_to_lrc(krc):
    """[start_ms,dur_ms]<off,dur,0>word... -> LRC lines (word timing dropped here)."""
    out = []
    for m in re.finditer(r"^\[(\d+),(\d+)\](.*)$", krc, re.M):
        t = int(m.group(1)) / 1000
        text = re.sub(r"<\d+,\d+,\d+>", "", m.group(3))
        out.append(f"[{int(t // 60):02d}:{t % 60:05.2f}]{text}")
    return "\n".join(out)


def fetch_kugou(track, artist, duration):
    q = urllib.parse.urlencode({"ver": 1, "man": "yes", "client": "pc", "keyword": f"{artist} - {track}",
                                "duration": int(duration * 1000)})
    d = json.loads(get("https://lyrics.kugou.com/search?" + q))
    cands = [(c["id"], c.get("song", ""), c.get("singer", ""), c["duration"] / 1000, c["accesskey"]) for c in d.get("candidates", [])]
    c = best(cands, track, artist, duration)
    if not c:
        return None, f"no duration match among {len(cands)}"
    q = urllib.parse.urlencode({"ver": 1, "client": "pc", "id": c[0], "accesskey": c[4], "fmt": "krc", "charset": "utf8"})
    body = json.loads(get("https://lyrics.kugou.com/download?" + q))
    if not body.get("content"):
        return None, "empty download"
    krc = decode_krc(body["content"])
    return krc_to_lrc(krc), {"id": c[0], "title": c[1], "artist": c[2], "duration": c[3],
                             "word_level": bool(re.search(r"<\d+,\d+,\d+>", krc))}


PROVIDERS = {"netease": fetch_netease, "qq": fetch_qq, "kugou": fetch_kugou}

if __name__ == "__main__":
    import sys
    lrc, meta = PROVIDERS[sys.argv[1]](sys.argv[2], sys.argv[3], float(sys.argv[4]))
    print(meta)
    print((lrc or "")[:400])
