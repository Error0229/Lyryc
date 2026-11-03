"""Word-timed lyric references: QQ (QRC), Kugou (KRC), NetEase (YRC), LRCLIB (LRC / enhanced LRC).

Unofficial endpoints, research only. Every response is cached in data/.http so a
rerun makes no requests. Each fetch_<src>(track) returns
  {"src", "meta", "lines": [{"start", "end", "text", "words": [{"start", "end", "text"}] | None}]}
or None. Times are seconds. Candidate choice is duration first (±3 s), then title/artist.

Usage: uv run python -X utf8 bench/refs.py            # all tracks -> data/refs.json
"""
import base64, hashlib, json, pathlib, re, time, urllib.error, urllib.parse, urllib.request, zlib
from Crypto.Cipher import AES
from qqdes import qrc_decrypt

ROOT = pathlib.Path(__file__).resolve().parent.parent
CACHE = ROOT / "data/.http"
UA = "Mozilla/5.0 (Windows NT 10.0; Win64; x64)"


def http(url, data=None, headers=None):
    CACHE.mkdir(parents=True, exist_ok=True)
    hit = CACHE / (hashlib.sha1(url.encode() + (data or b"")).hexdigest() + ".txt")
    if hit.exists():
        return hit.read_text(encoding="utf-8")
    req = urllib.request.Request(url, data=data, headers={"User-Agent": UA, **(headers or {})})
    body = urllib.request.urlopen(req, timeout=20).read().decode("utf-8", "replace")
    hit.write_text(body, encoding="utf-8")
    time.sleep(0.5)
    return body


def norm(s):
    return re.sub(r"[\W_]+", "", (s or "").lower())


def sim(a, b):
    a, b = set(norm(a)), set(norm(b))
    return len(a & b) / max(1, len(a | b))


COVER = re.compile(r"cover|翻唱|女声|男声|伴奏|instrumental|karaoke|remix|live|dj|钢琴|piano|原唱", re.I)


def best(cands, t):
    """cands: [(id, title, artist, duration_s, extra)]"""
    ok = [c for c in cands if c[3] and abs(c[3] - t["duration"]) <= 3]
    if not ok:
        return None
    names = [t["track"], t["ytm_title"]]
    return max(ok, key=lambda c: max(sim(c[1], n) for n in names) + max(sim(c[2], t["artist"]), sim(c[2], t["ytm_artist"]))
               - abs(c[3] - t["duration"]) / 30 - (0.5 if COVER.search(c[1]) else 0))


def query(t):
    return f"{t['track']} {t['artist']}"


# ---- word-timed formats -------------------------------------------------------------

QRC_WORD = re.compile(r"(?P<w>[^()]*?)\((?P<s>\d+),(?P<d>\d+)\)")   # word(abs_ms,dur)
YRC_WORD = re.compile(r"\((?P<s>\d+),(?P<d>\d+),\d+\)(?P<w>[^(]*)")  # (abs_ms,dur,0)word
KRC_WORD = re.compile(r"<(?P<s>\d+),(?P<d>\d+),\d+>(?P<w>[^<]*)")    # <rel_ms,dur,0>word
LINE = re.compile(r"^\[(\d+),(\d+)\](.*)$", re.M)


def parse_timed(body, rx, relative):
    lines = []
    for m in LINE.finditer(body):
        s, d, rest = int(m.group(1)), int(m.group(2)), m.group(3)
        words = []
        for w in rx.finditer(rest):
            ws = int(w.group("s")) + (s if relative else 0)
            words.append({"start": ws / 1000, "end": (ws + int(w.group("d"))) / 1000, "text": w.group("w")})
        text = "".join(w["text"] for w in words) if words else rest
        lines.append({"start": s / 1000, "end": (s + d) / 1000, "text": text.strip(), "words": words})
    return lines


LRC = re.compile(r"^\[(\d+):(\d+(?:\.\d+)?)\](.*)$")
ENH = re.compile(r"<(\d+):(\d+(?:\.\d+)?)>([^<]*)")


def parse_lrc(lrc):
    """Plain or enhanced (A2) LRC. Enhanced word tags give word starts; ends are the next start."""
    lines = []
    for raw in lrc.splitlines():
        m = LRC.match(raw.strip())
        if not m:
            continue
        start, rest = int(m.group(1)) * 60 + float(m.group(2)), m.group(3)
        tags = ENH.findall(rest)
        words = [{"start": int(a) * 60 + float(b), "end": None, "text": w} for a, b, w in tags]
        for w, nxt in zip(words, words[1:]):
            w["end"] = nxt["start"]
        words = [w for w in words if w["text"].strip()]
        text = ENH.sub(lambda x: x.group(3), rest) if tags else rest
        lines.append({"start": start, "end": None, "text": text.strip(), "words": words or None})
    for a, b in zip(lines, lines[1:]):
        a["end"] = b["start"]
    return lines


# ---- providers ----------------------------------------------------------------------

def fetch_qq(t):
    q = urllib.parse.urlencode({"w": query(t), "format": "json", "n": 10})
    lst = json.loads(http("https://c.y.qq.com/soso/fcgi-bin/client_search_cp?" + q, headers={"Referer": "https://y.qq.com/"}))
    lst = lst["data"]["song"]["list"]
    c = best([(s["songid"], s["songname"], " ".join(x["name"] for x in s["singer"]), s["interval"], None) for s in lst], t)
    if not c:
        return None
    body = json.dumps({"comm": {"ct": "19", "cv": "1859", "uin": "0"}, "req": {
        "module": "music.musichallSong.PlayLyricInfo", "method": "GetPlayLyricInfo",
        "param": {"songID": c[0], "qrc": 1, "crypt": 1, "trans": 0, "roma": 0}}}).encode()
    d = json.loads(http("https://u.y.qq.com/cgi-bin/musicu.fcg", body,
                        {"Referer": "https://y.qq.com/", "Content-Type": "application/json"}))
    hexed = d["req"]["data"].get("lyric")
    if not hexed:
        return None
    qrc = qrc_decrypt(hexed)
    m = re.search(r'LyricContent="(.*?)"\s*/>', qrc, re.S)
    return {"src": "qq", "meta": c[:4], "lines": parse_timed(m.group(1) if m else qrc, QRC_WORD, relative=False)}


KRC_KEY = bytes([64, 71, 97, 119, 94, 50, 116, 71, 81, 54, 49, 45, 206, 210, 110, 105])


def fetch_kugou(t):
    q = urllib.parse.urlencode({"ver": 1, "man": "yes", "client": "pc", "keyword": f"{t['artist']} - {t['track']}",
                                "duration": int(t["duration"] * 1000)})
    d = json.loads(http("https://lyrics.kugou.com/search?" + q))
    c = best([(c["id"], c.get("song", ""), c.get("singer", ""), c["duration"] / 1000, c["accesskey"])
              for c in d.get("candidates", [])], t)
    if not c:
        return None
    q = urllib.parse.urlencode({"ver": 1, "client": "pc", "id": c[0], "accesskey": c[4], "fmt": "krc", "charset": "utf8"})
    body = json.loads(http("https://lyrics.kugou.com/download?" + q))
    if not body.get("content"):
        return None
    raw = base64.b64decode(body["content"])[4:]
    krc = zlib.decompress(bytes(b ^ KRC_KEY[i % 16] for i, b in enumerate(raw))).decode("utf-8", "replace")
    return {"src": "kugou", "meta": c[:4], "lines": parse_timed(krc, KRC_WORD, relative=True)}


def eapi(path, params):
    """NetEase's eapi request envelope (AES-128-ECB of path + JSON + md5)."""
    text = json.dumps(params, separators=(",", ":"))
    dig = hashlib.md5(f"nobody{path}use{text}md5forencrypt".encode()).hexdigest()
    raw = f"{path}-36cd479b6b5-{text}-36cd479b6b5-{dig}".encode()
    pad = 16 - len(raw) % 16
    enc = AES.new(b"e82ckenh8dichen8", AES.MODE_ECB).encrypt(raw + bytes([pad]) * pad)
    return urllib.parse.urlencode({"params": enc.hex().upper()}).encode()


def fetch_netease(t):
    q = urllib.parse.urlencode({"s": query(t), "type": 1, "limit": 10})
    songs = json.loads(http("https://music.163.com/api/search/get?" + q)).get("result", {}).get("songs", [])
    c = best([(s["id"], s["name"], " ".join(a["name"] for a in s["artists"]), s["duration"] / 1000, None) for s in songs], t)
    if not c:
        return None
    p = {"id": str(c[0]), "cp": "false", "lv": "0", "kv": "0", "tv": "0", "rv": "0", "yv": "0", "ytv": "0", "yrv": "0"}
    d = json.loads(http("https://interface3.music.163.com/eapi/song/lyric/v1", eapi("/api/song/lyric/v1", p),
                        {"Content-Type": "application/x-www-form-urlencoded", "Cookie": "os=pc; appver=2.10.13"}))
    yrc = (d.get("yrc") or {}).get("lyric")
    if yrc:
        return {"src": "netease", "meta": c[:4], "lines": parse_timed(yrc, YRC_WORD, relative=False)}
    lrc = (d.get("lrc") or {}).get("lyric")
    return {"src": "netease", "meta": c[:4], "lines": parse_lrc(lrc), "line_only": True} if lrc else None


def fetch_lrclib(t):
    h = {"User-Agent": "Lyryc research"}
    q = urllib.parse.urlencode({"track_name": t["ytm_title"], "artist_name": t["ytm_artist"].split(",")[0],
                                "duration": t["duration"]})
    try:
        d = json.loads(http("https://lrclib.net/api/get?" + q, headers=h))
    except urllib.error.HTTPError:
        d = {}
    if not d.get("syncedLyrics"):
        q = urllib.parse.urlencode({"track_name": t["track"], "artist_name": t["artist"]})
        hits = json.loads(http("https://lrclib.net/api/search?" + q, headers=h))
        hits = [x for x in hits if x.get("syncedLyrics") and x.get("duration") and abs(x["duration"] - t["duration"]) <= 3]
        d = hits[0] if hits else {}
    if not d.get("syncedLyrics"):
        return None
    lines = parse_lrc(d["syncedLyrics"])
    return {"src": "lrclib", "meta": [d["id"], d["trackName"], d["artistName"], d["duration"]], "lines": lines,
            "enhanced": any(l["words"] for l in lines)}


PROVIDERS = {"qq": fetch_qq, "kugou": fetch_kugou, "netease": fetch_netease, "lrclib": fetch_lrclib}


def main():
    tracks = json.loads((ROOT / "tracks.json").read_text(encoding="utf-8"))
    out = {}
    for t in tracks:
        out[t["id"]] = {}
        for name, fn in PROVIDERS.items():
            try:
                r = fn(t)
            except Exception as e:  # endpoint drift: record and move on
                r = {"src": name, "error": repr(e)[:200]}
            out[t["id"]][name] = r
            words = sum(bool(l.get("words")) for l in (r or {}).get("lines", []))
            print(t["id"], t["track"], name,
                  "-" if not r else (r.get("error") or f"{len(r['lines'])} lines, {words} word-timed"), flush=True)
    (ROOT / "data/refs.json").write_text(json.dumps(out, ensure_ascii=False), encoding="utf-8")


if __name__ == "__main__":
    main()
