"""List LRCLIB candidates to pick a pinned id: python bench/candidates.py TRACK ARTIST"""
import sys, urllib.parse
from fetch_dataset import get

res = get("https://lrclib.net/api/search?" + urllib.parse.urlencode({"track_name": sys.argv[1], "artist_name": sys.argv[2]}))
for r in res:
    if r.get("syncedLyrics") and r.get("plainLyrics"):
        lines = r["syncedLyrics"].splitlines()
        print(r["id"], r["trackName"][:25], "|", (r["albumName"] or "")[:20], r["duration"], len(lines), lines[1][:50])
