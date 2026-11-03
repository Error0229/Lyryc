"""Download the exact YouTube Music audio for each videoId and time it -> data/audio/<id>.wav.

One yt-dlp call per track, sequential, with the venv's pinned yt-dlp. Records the
download wall time, the stream format and its size, and the ffmpeg decode time to
16 kHz mono, in data/audio/fetch.json. Research only (YouTube ToS: see REPORT.md).
Usage: uv run python -X utf8 bench/fetch_audio.py [--ids 0,1]
"""
import argparse, json, pathlib, subprocess, sys, time

ROOT = pathlib.Path(__file__).resolve().parent.parent
OUT = ROOT / "data/audio"


def fetch(t):
    url = f"https://music.youtube.com/watch?v={t['videoId']}"
    t0 = time.perf_counter()
    p = subprocess.run([sys.executable, "-m", "yt_dlp", "-q", "--no-playlist", "-f", "bestaudio", "--print-json",
                        "--no-simulate", "-o", str(OUT / f"{t['id']}.%(ext)s"), url],
                       capture_output=True, text=True, encoding="utf-8")
    dl = time.perf_counter() - t0
    if p.returncode:
        return {"error": p.stderr.strip()[-400:], "download_s": dl}
    info = json.loads(p.stdout.splitlines()[-1])
    src = OUT / f"{t['id']}.{info['ext']}"
    t0 = time.perf_counter()
    subprocess.run(["ffmpeg", "-v", "error", "-y", "-i", str(src), "-ac", "1", "-ar", "16000", str(OUT / f"{t['id']}.wav")],
                   check=True)
    return {"download_s": dl, "decode_s": time.perf_counter() - t0, "format_id": info.get("format_id"),
            "acodec": info.get("acodec"), "abr": info.get("abr"), "bytes": src.stat().st_size,
            "duration": info.get("duration"), "title": info.get("title"), "channel": info.get("channel")}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--ids", default=None)
    a = ap.parse_args()
    OUT.mkdir(parents=True, exist_ok=True)
    log = OUT / "fetch.json"
    res = json.loads(log.read_text(encoding="utf-8")) if log.exists() else {}
    for t in json.loads((ROOT / "tracks.json").read_text(encoding="utf-8")):
        if a.ids and str(t["id"]) not in a.ids.split(","):
            continue
        if str(t["id"]) in res and "error" not in res[str(t["id"])]:
            continue
        r = res[str(t["id"])] = fetch(t)
        print(t["id"], t["track"], r.get("error") or f"dl {r['download_s']:.1f}s dec {r['decode_s']:.1f}s "
              f"{r['acodec']} {r['bytes'] / 1e6:.1f} MB {r['duration']}s", flush=True)
        log.write_text(json.dumps(res, ensure_ascii=False, indent=1), encoding="utf-8")
        time.sleep(2)


if __name__ == "__main__":
    main()
