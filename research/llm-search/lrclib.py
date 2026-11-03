"""Gentle LRCLIB client: sequential, on-disk cache, backs off on 503, stops on 429."""
import hashlib, json, sys, time, urllib.error, urllib.parse, urllib.request
from pathlib import Path

UA = {"User-Agent": "LyrycLlmSearchResearch/0.1 (https://github.com/Error0229/Lyryc)"}
CACHE = Path(__file__).parent / "data/.http"
GAP = 2.0  # seconds between live requests
_last = [0.0]
stats = {"live": 0, "cached": 0}


class RateLimited(Exception):
    pass


def _get(url, tries=10):
    CACHE.mkdir(parents=True, exist_ok=True)
    hit = CACHE / (hashlib.sha1(url.encode()).hexdigest() + ".json")
    if hit.exists():
        stats["cached"] += 1
        return json.loads(hit.read_text(encoding="utf-8"))
    for i in range(tries):
        wait = _last[0] + GAP - time.time()
        if wait > 0:
            time.sleep(wait)
        _last[0] = time.time()
        stats["live"] += 1
        try:
            body = json.loads(urllib.request.urlopen(urllib.request.Request(url, headers=UA), timeout=20).read())
        except urllib.error.HTTPError as e:
            if e.code == 429:
                raise RateLimited(url)
            if e.code == 404:
                body = None
            elif e.code in (500, 502, 503, 504) and i < tries - 1:
                time.sleep(min(30, max(float(e.headers.get("Retry-After") or 0), 5 * (i + 1))))
                continue
            else:
                raise
        hit.write_text(json.dumps(body, ensure_ascii=False), encoding="utf-8")
        return body


def get_exact(track, artist, duration):
    """`/api/get` as lyrics.rs calls it: name + artist + rounded duration (LRCLIB allows +-2 s)."""
    q = urllib.parse.urlencode({"track_name": track, "artist_name": artist, "duration": round(duration)})
    return _get("https://lrclib.net/api/get?" + q)


def search(track=None, artist=None, q=None):
    params = {"q": q} if q else {"track_name": track, **({"artist_name": artist} if artist else {})}
    return _get("https://lrclib.net/api/search?" + urllib.parse.urlencode(params)) or []


def by_id(i):
    return _get(f"https://lrclib.net/api/get/{i}")
