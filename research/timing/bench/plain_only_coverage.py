"""Coverage on the real failure population: LRCLIB entries that have plainLyrics only.

For each sampled plain-only entry, would a fallback find synced timing?
  lrclib_alt : another LRCLIB entry of the same song (|dur| <= 3 s) that is synced
  netease/qq/kugou : third-party synced LRC whose text matches >= 50% of the lines
Accuracy cannot be measured here (no ground truth); this is coverage only.
Usage: python bench/plain_only_coverage.py
"""
import json, re, urllib.parse
from fetch_dataset import get as lrclib_get
from common import ROOT, parse_lrc
from align_text import map_lines
from sources import PROVIDERS

QUERIES = ["love", "night", "heart", "rain", "summer", "夜", "愛", "恋", "君", "爱", "雨", "我", "사랑", "밤", "너"]
LANG = [("ja", re.compile(r"[぀-ヿ]")), ("ko", re.compile(r"[가-힯]")),
        ("zh", re.compile(r"[一-鿿]")), ("en", re.compile(r"[a-z]", re.I))]


def lang_of(text):
    return next((lg for lg, rx in LANG if rx.search(text)), "other")


def sample():
    seen, out = set(), []
    for q in QUERIES:
        try:
            res = lrclib_get("https://lrclib.net/api/search?" + urllib.parse.urlencode({"q": q}))
        except Exception as e:
            print("ERR", q, e)
            continue
        for r in res:
            key = (r["trackName"].lower(), r["artistName"].lower())
            if r.get("syncedLyrics") or not r.get("plainLyrics") or r.get("instrumental") or not r.get("duration") or key in seen:
                continue
            seen.add(key)
            out.append(r)
    return out


def lrclib_alt(r):
    q = urllib.parse.urlencode({"track_name": r["trackName"], "artist_name": r["artistName"]})
    try:
        res = lrclib_get("https://lrclib.net/api/search?" + q)
    except Exception:
        return None
    return any(x.get("syncedLyrics") and abs((x.get("duration") or 0) - r["duration"]) <= 3 for x in res)


def main():
    rows = []
    for r in sample():
        lines = [l.strip() for l in r["plainLyrics"].splitlines() if l.strip()]
        row = {"track": r["trackName"], "artist": r["artistName"], "duration": r["duration"],
               "lang": lang_of(r["plainLyrics"]), "lrclib_alt": lrclib_alt(r)}
        for name, fetch in PROVIDERS.items():
            try:
                lrc, _ = fetch(r["trackName"], r["artistName"], r["duration"])
            except Exception:
                lrc = None
            src = parse_lrc(lrc) if lrc else []
            ok = len(src) >= 3 and sum(x is not None for x in map_lines(lines, src)) >= 0.5 * len(lines)
            row[name] = ok
        row["any_source"] = any(row[k] for k in ("lrclib_alt", *PROVIDERS))
        rows.append(row)
        print(row, flush=True)
    (ROOT / "results/plain_only_coverage.json").write_text(json.dumps(rows, ensure_ascii=False, indent=0), encoding="utf-8")
    keys = ["lrclib_alt", *PROVIDERS, "any_source"]
    print(f"\n| lang | n | " + " | ".join(keys) + " |\n|---|---|" + "---|" * len(keys))
    for lg in ["all", "en", "ja", "zh", "ko"]:
        sub = [x for x in rows if lg == "all" or x["lang"] == lg]
        if sub:
            print(f"| {lg} | {len(sub)} | " + " | ".join(f"{sum(bool(x[k]) for x in sub) / len(sub):.0%}" for k in keys) + " |")


if __name__ == "__main__":
    main()
