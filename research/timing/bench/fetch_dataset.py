"""Fetch LRCLIB ground truth: entries that carry both syncedLyrics and plainLyrics.

Usage: python bench/fetch_dataset.py   (writes data/dataset.json)
"""
import json, time, urllib.error, urllib.parse, urllib.request, pathlib

ROOT = pathlib.Path(__file__).resolve().parent.parent
UA = {"User-Agent": "LyrycTimingResearch/0.1 (https://github.com/Error0229/Lyryc)"}


CACHE = ROOT / "data/.http"


def get(url, tries=8):
    import hashlib
    CACHE.mkdir(exist_ok=True)
    hit = CACHE / (hashlib.sha1(url.encode()).hexdigest() + ".json")
    if hit.exists():
        return json.loads(hit.read_text(encoding="utf-8"))
    for i in range(tries):
        try:
            req = urllib.request.Request(url, headers=UA)
            with urllib.request.urlopen(req, timeout=20) as r:
                data = json.load(r)
                hit.write_text(json.dumps(data, ensure_ascii=False), encoding="utf-8")
                return data
        except urllib.error.HTTPError as e:
            if e.code < 500 or i == tries - 1:
                raise
            time.sleep(5 * (i + 1))


def search(track, artist):
    for q in ({"track_name": track, "artist_name": artist}, {"q": f"{track} {artist}"}):
        res = get("https://lrclib.net/api/search?" + urllib.parse.urlencode(q))
        ok = [r for r in res if r.get("syncedLyrics") and r.get("plainLyrics") and r.get("duration")]
        if ok:
            # Prefer the canonical (most common) duration so we get the full
            # album version, not a TV-size edit or a live take.
            votes = lambda r: sum(abs((o.get("duration") or 0) - r["duration"]) <= 2 for o in res)
            return max(ok, key=votes)
    return None


def main():
    out = []
    for raw in (ROOT / "data/tracks.txt").read_text(encoding="utf-8").splitlines():
        if not raw.strip() or raw.startswith("#"):
            continue
        lang, track, artist, tag, *pin = raw.split("|")
        try:
            # A pinned LRCLIB id overrides search when the top hit is a bad
            # entry (live take, embedded translations, ...).
            rec = get(f"https://lrclib.net/api/get/{pin[0]}") if pin else search(track, artist)
        except Exception as e:  # LRCLIB throws sporadic 503s; rerun to fill gaps
            print("ERR ", track, artist, e)
            continue
        if not rec:
            print("MISS", track, artist)
            continue
        out.append({"id": len(out), "lang": lang, "tags": tag.split(","), "query": [track, artist],
                    "lrclib_id": rec["id"], "track": rec["trackName"], "artist": rec["artistName"],
                    "album": rec.get("albumName"), "duration": rec["duration"],
                    "plain": rec["plainLyrics"], "synced": rec["syncedLyrics"]})
        print("OK  ", lang, rec["trackName"], "-", rec["artistName"], rec["duration"])
        time.sleep(0.3)
    (ROOT / "data/dataset.json").write_text(json.dumps(out, ensure_ascii=False, indent=1), encoding="utf-8")
    print(len(out), "tracks")


if __name__ == "__main__":
    main()
