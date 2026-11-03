"""Download duration-matched audio for each dataset track (yt-dlp + ffmpeg on PATH).

Prefers YouTube "Topic" (album audio) uploads whose length is within 3 s of the
LRCLIB duration, then converts to 16 kHz mono WAV in data/audio/<id>.wav.
Usage: python bench/fetch_audio.py
"""
import json, subprocess, sys
from common import ROOT, load

AUDIO = ROOT / "data/audio"


def pick(track, artist, duration):
    q = f"ytsearch8:{artist} {track} audio"
    out = subprocess.run(["yt-dlp", "--flat-playlist", "-J", q], capture_output=True, text=True, encoding="utf-8")
    entries = json.loads(out.stdout).get("entries", [])
    ok = [e for e in entries if e.get("duration") and abs(e["duration"] - duration) <= 3]
    ok.sort(key=lambda e: (not (e.get("channel") or "").endswith("Topic"), abs(e["duration"] - duration)))
    return ok[0] if ok else None


def main():
    AUDIO.mkdir(parents=True, exist_ok=True)
    manifest_path = AUDIO / "manifest.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8")) if manifest_path.exists() else {}
    for t in load():
        wav = AUDIO / f"{t['id']}.wav"
        if wav.exists():
            continue
        e = pick(t["track"], t["artist"], t["duration"])
        if not e:
            print("NO MATCH", t["id"], t["track"], file=sys.stderr)
            continue
        src = AUDIO / f"{t['id']}.src"
        subprocess.run(["yt-dlp", "-q", "-f", "bestaudio", "-o", str(src), e["url"]], check=True)
        subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-i", str(src), "-ac", "1", "-ar", "16000", str(wav)], check=True)
        src.unlink()
        manifest[str(t["id"])] = {"url": e["url"], "title": e.get("title"), "channel": e.get("channel"), "duration": e["duration"]}
        manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, indent=1), encoding="utf-8")
        print("OK", t["id"], e.get("channel"), e.get("title"), e["duration"], "vs", t["duration"])


if __name__ == "__main__":
    main()
