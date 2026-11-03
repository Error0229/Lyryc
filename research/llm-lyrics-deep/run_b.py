"""Approach B with search: identify the song (llm/identify.rs prompt and schema), then LRCLIB.

    python run_b.py --model deepseek/deepseek-v4.1-flash --ids 2,23,24,25,29,30,31,32,33 [--search off]

The schema goes in the prompt (strict json_schema next to a server tool came back
as prose in the lyrics runs). Writes results/b_runs.jsonl (names, metadata and
the LRCLIB outcome; never lyrics).
"""
import argparse, json, re, sys

from common import HERE, RAW, RESULTS, SOFT_STOP, api_key, cases, charge, jl, key_usage, lrclib_retry, spent
from run import call, meta

sys.path.insert(0, str(HERE.parent / "llm-search"))
import evaluate  # noqa: E402  (lookup, identity_queries, outcome from the first study)

SRC = (HERE.parent.parent / "src-tauri/src/llm/identify.rs").read_text(encoding="utf-8")
SYSTEM = re.search(r'const SYSTEM: &str = "(.*?)";', SRC, re.S).group(1).replace("\\\n", "")
OPT = {"type": ["string", "null"]}
SCHEMA = {"type": "object", "additionalProperties": False, "properties": {
    "found": {"type": "boolean"}, "title_native": {"type": "string"}, "title_romanized": OPT,
    "artist_native": {"type": "string"}, "artist_romanized": OPT, "album": OPT,
    "duration_s": {"type": ["number", "null"]}, "isrc": OPT, "confidence": {"type": "number"},
    "evidence_urls": {"type": "array", "items": {"type": "string"}}}}
SCHEMA["required"] = list(SCHEMA["properties"])
OUT = RESULTS / "b_runs.jsonl"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--model", default="deepseek/deepseek-v4.1-flash")
    ap.add_argument("--ids", required=True)
    ap.add_argument("--search", default="auto")
    ap.add_argument("--strict", action="store_true", help="json_schema response_format, as dialect.rs sends today")
    args = ap.parse_args()
    key = api_key()
    if max(spent(), key_usage(key)) > SOFT_STOP:
        sys.exit("budget reached")
    tag = f"B-{args.model.split('/')[-1]},search={args.search}" + (",strict" if args.strict else "")
    done = {r["id"] for r in jl(OUT) if r["tag"] == tag and r["status"] == 200}
    (RAW / tag).mkdir(parents=True, exist_ok=True)
    ids = {int(i) for i in args.ids.split(",")}
    for case in [c for c in cases() if c["id"] in ids and c["id"] not in done]:
        system = SYSTEM if args.strict else (
            SYSTEM + "\n\nReply with only a JSON object, no prose and no code fence, matching this JSON Schema:\n" + json.dumps(SCHEMA))
        dur = f"{case['duration']} s" if case.get("duration") else "unknown"
        b = {"model": args.model, "max_tokens": 4096, "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": f"Title: {case['raw_title']}\nArtist: {case['raw_artist']}\nDuration: {dur}"}]}
        if args.strict:
            b["response_format"] = {"type": "json_schema", "json_schema": {"name": "song_identity", "strict": True, "schema": SCHEMA}}
        if args.search != "off":
            b["tools"] = [{"type": "openrouter:web_search", "parameters": {"engine": args.search}}]
            b["max_tool_calls"] = 6
        status, raw, ms = call(key, b)
        (RAW / tag / f"{case['id']}.json").write_text(json.dumps(raw, ensure_ascii=False), encoding="utf-8")
        m = meta(raw)
        charge({"tag": tag, "id": case["id"], "status": status, "cost": m["cost"] or 0})
        content = ((raw.get("choices") or [{}])[0].get("message") or {}).get("content") or ""
        found = re.search(r"\{.*\}", content, re.S)
        try:
            ident = json.loads(found.group(0)) if found else None
        except json.JSONDecodeError:
            ident = None
        row = {"id": case["id"], "tag": tag, "status": status, "latency_ms": ms, "cost": m["cost"],
               "searches": m["searches"], "citations": m["citations"], "parsed": ident is not None}
        if ident and ident.get("found") and ident.get("title_native") and ident.get("artist_native"):
            row["identity"] = {k: ident.get(k) for k in ("title_native", "title_romanized", "artist_native", "artist_romanized", "confidence")}
            rec, how = lrclib_retry(evaluate.lookup, evaluate.identity_queries(ident), case["duration"],
                                    (ident["title_native"], ident["artist_native"]))
            row["lrclib"] = evaluate.outcome(rec, case) if case.get("title") else ("wrong" if rec else "miss")
            row["right_name"] = bool(case.get("title")) and evaluate.same(ident["title_native"], case["title"] + case.get("alt_title", [])) \
                and evaluate.same(ident["artist_native"], case["artist"] + case.get("alt_artist", []))
        else:
            row["lrclib"] = "no identity"
        # the raw names alone, as Lyryc tries first
        rec, _ = lrclib_retry(evaluate.lookup, [(case["raw_title"], case["raw_artist"])], case["duration"], (case["raw_title"], case["raw_artist"]))
        row["lrclib_raw"] = evaluate.outcome(rec, case) if case.get("title") else ("wrong" if rec else "miss")
        print(json.dumps(row, ensure_ascii=False), flush=True)
        with OUT.open("a", encoding="utf-8") as f:
            f.write(json.dumps(row, ensure_ascii=False) + "\n")


if __name__ == "__main__":
    main()
