"""Resolve each seed track to the YouTube Music *song* (ATV) videoId -> tracks.json.

This is the id the YTM page URL carries when the user plays the song (not a music
video upload). Uses ytmusicapi's unauthenticated search, one request per track.
Usage: uv run python bench/resolve.py
"""
import csv, json, pathlib, time
from ytmusicapi import YTMusic

ROOT = pathlib.Path(__file__).resolve().parent.parent


def main():
    yt = YTMusic()
    out = []
    for row in csv.DictReader(open(ROOT / "tracks.seed.tsv", encoding="utf-8"), delimiter="\t"):
        hits = yt.search(f"{row['track']} {row['artist']}", filter="songs", limit=5)
        h = hits[0]
        out.append({
            "id": len(out), "lang": row["lang"], "tags": [t for t in row["tags"].split(",") if t],
            "track": row["track"], "artist": row["artist"],
            "videoId": h["videoId"], "ytm_title": h["title"],
            "ytm_artist": ", ".join(a["name"] for a in h.get("artists", [])),
            "duration": h.get("duration_seconds"), "videoType": h.get("videoType"),
        })
        print(out[-1]["id"], row["track"], "->", h["videoId"], h["title"], out[-1]["ytm_artist"], h.get("duration"))
        time.sleep(1)
    (ROOT / "tracks.json").write_text(json.dumps(out, ensure_ascii=False, indent=1), encoding="utf-8")


if __name__ == "__main__":
    main()
