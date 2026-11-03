"""Approach A, "give me the lyrics": raw player title/artist/duration in, lyrics out.

Same request shape as `src-tauri/src/llm/dialect.rs` builds for Gemini
(generateContent + responseJsonSchema, + google_search when --search).
Raw replies (they contain lyrics) go to the gitignored results/raw/;
results/a_lyrics.jsonl keeps metadata only. Resumable: done cases are skipped.

    python run_a.py [--search] [--model gemini-3.5-flash-lite] [--ids 1,2]
"""
import argparse, json, time, urllib.error, urllib.request
from pathlib import Path

from gemini_key import gemini_key

HERE = Path(__file__).parent
BASE = "https://generativelanguage.googleapis.com/v1beta"
GAP = 5.0  # free-tier keys allow only a few requests per minute

SYSTEM = (
    "You find song lyrics for a desktop lyrics overlay. The title and artist are exactly what a "
    "web music player reports and may contain video tags, channel names, or translated titles. "
    "Identify the song and return its complete lyrics in the original language. If you know a "
    "time-synced version, put it in synced_lrc as LRC lines ([mm:ss.xx] text), else null. "
    "plain_lyrics: the full lyrics, one line per sung line, else null. found=false if you cannot "
    "identify the song or it has no lyrics. language: ISO 639-1 code. confidence: 0..1."
)
OPT = {"type": ["string", "null"]}
SCHEMA = {
    "type": "object", "additionalProperties": False,
    "properties": {
        "found": {"type": "boolean"}, "title": {"type": "string"}, "artist": {"type": "string"},
        "language": OPT, "synced_lrc": OPT, "plain_lyrics": OPT,
        "source_urls": {"type": "array", "items": {"type": "string"}},
        "confidence": {"type": "number"},
    },
    "required": ["found", "title", "artist", "language", "synced_lrc", "plain_lyrics",
                 "source_urls", "confidence"],
}


def call(key, model, case, search):
    duration = f"{case['duration']} s" if case.get("duration") else "unknown"
    body = {
        "contents": [{"role": "user", "parts": [{"text":
            f"Title: {case['raw_title']}\nArtist: {case['raw_artist']}\nDuration: {duration}"}]}],
        "systemInstruction": {"parts": [{"text": SYSTEM}]},
        "generationConfig": {"maxOutputTokens": 8192, "responseMimeType": "application/json",
                             "responseJsonSchema": SCHEMA},
    }
    if search:
        body["tools"] = [{"google_search": {}}]
    req = urllib.request.Request(f"{BASE}/models/{model}:generateContent", data=json.dumps(body).encode(),
                                 headers={"x-goog-api-key": key, "Content-Type": "application/json"})
    t0 = time.time()
    try:
        raw = json.loads(urllib.request.urlopen(req, timeout=120).read())
        status = 200
    except urllib.error.HTTPError as e:
        raw, status = json.loads(e.read() or b"{}"), e.code
    return status, raw, int((time.time() - t0) * 1000)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--search", action="store_true")
    ap.add_argument("--model", default="gemini-3.5-flash-lite")
    ap.add_argument("--ids")
    args = ap.parse_args()
    key = gemini_key()
    cases = json.loads((HERE / "data/cases.json").read_text(encoding="utf-8"))
    ids = {int(i) for i in args.ids.split(",")} if args.ids else None
    out = HERE / "results/a_lyrics.jsonl"
    raw_dir = HERE / "results/raw"
    raw_dir.mkdir(parents=True, exist_ok=True)
    done = {(r["id"], r["search"], r["model"]) for r in map(json.loads, out.read_text(encoding="utf-8").splitlines())} if out.exists() else set()
    for case in cases:
        if (ids and case["id"] not in ids) or (case["id"], args.search, args.model) in done:
            continue
        status, raw, ms = call(key, args.model, case, args.search)
        tag = f"{case['id']}-{'search' if args.search else 'nosearch'}-{args.model}"
        (raw_dir / f"a-{tag}.json").write_text(json.dumps(raw, ensure_ascii=False), encoding="utf-8")
        cand = (raw.get("candidates") or [{}])[0]
        usage = raw.get("usageMetadata", {})
        row = {"id": case["id"], "search": args.search, "model": args.model, "status": status, "latency_ms": ms,
               "finish": cand.get("finishReason"), "block": raw.get("promptFeedback", {}).get("blockReason"),
               "error": raw.get("error", {}).get("message", "")[:160] or None,
               "searches": len(cand.get("groundingMetadata", {}).get("webSearchQueries", [])),
               "tokens": {k: usage.get(k) for k in ("promptTokenCount", "candidatesTokenCount", "thoughtsTokenCount", "toolUsePromptTokenCount")}}
        print(json.dumps(row, ensure_ascii=False))
        with out.open("a", encoding="utf-8") as f:
            f.write(json.dumps(row, ensure_ascii=False) + "\n")
        if status == 429:
            print("429, stopping"); break
        time.sleep(GAP)


if __name__ == "__main__":
    main()
