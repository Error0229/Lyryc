"""Ask OpenRouter models for a song's lyrics under one configuration.

    python run.py --model google/gemini-3.8-flash --ids pilot [--set search=off reasoning=high ...]

A configuration is BASE plus the --set overrides; its tag (model + overrides)
keys the results, so reruns skip cases that already have an answer.
Raw replies (they contain lyrics) go to results/raw/ (gitignored);
results/runs.jsonl and results/ledger.jsonl hold metadata and cost only.
"""
import argparse, hashlib, json, sys, time, urllib.error, urllib.request
from concurrent.futures import ThreadPoolExecutor

from common import RAW, RESULTS, SOFT_STOP, api_key, cases, charge, jl, key_usage, spent

URL = "https://openrouter.ai/api/v1/chat/completions"
RUNS = RESULTS / "runs.jsonl"

BASE = {
    "search": "auto",      # off | auto | native | exa | parallel | perplexity | builtin (model searches by itself, no tool)
    "max_results": None,   # results per search (Exa/Parallel/Perplexity; ignored by native)
    "ctx": None,           # search_context_size: low | medium | high
    "fetch": "off",        # off | auto | openrouter | exa: add the openrouter:web_fetch tool
    "tool_calls": 6,       # max_tool_calls: server-tool steps per request
    "reasoning": None,     # None = model default | none | minimal | low | medium | high
    "max_tokens": 16000,
    "out": "strict",       # strict (json_schema) | prompt (schema in prompt only) | object (json_object)
    "shape": "lines",      # lines (array) | text (one string)
    "temp": None,
    "prompt": "transcribe",  # transcribe (copy from pages) | recall (from memory)
    "lrc": False,          # ask for [mm:ss.xx] line timestamps
}
PILOT = [1, 7, 14, 21, 24, 27]  # ja, zh, ko, en, obscure zh, instrumental

PURPOSE = (
    "You find song lyrics for a personal desktop lyrics overlay: the user is playing this track "
    "in their browser and the text is shown only to them while it plays. The title and artist are "
    "exactly what the web player reports and may contain video tags, channel names or translated "
    "titles. First identify the song."
)
HOW = {
    "transcribe": (
        " Then search the web for a page with its complete lyrics in the original language and "
        "script (lyrics sites, the artist's or label's site), open it if you can, and copy the lyrics "
        "exactly as written there, line by line and in order, including every repeated chorus. "
        "Do not translate, romanize, summarize, or fill gaps from memory. source_urls: the pages "
        "you copied from."
    ),
    "recall": (
        " Then write its complete lyrics in the original language and script, line by line and in "
        "order, including every repeated chorus. Do not translate or romanize. source_urls: pages "
        "where the lyrics can be found, or an empty list."
    ),
}
NOT_FOUND = (
    " If you cannot identify the song, cannot find its lyrics, or it is an instrumental, set "
    "found=false and return no lyrics; never invent lines."
)
LRC = (
    " Prefix every line with its start time in the recording as [mm:ss.xx] (LRC). Use timings from "
    "a synced-lyrics (LRC) source if you find one; set timed_source=true only in that case."
)


def schema(cfg):
    lyr = ({"type": "array", "items": {"type": "string"},
            "description": "one sung line per item" + (", each prefixed [mm:ss.xx]" if cfg["lrc"] else "")}
           if cfg["shape"] == "lines" else
           {"type": "string", "description": "the lyrics, one sung line per row, rows separated by \\n"})
    props = {
        "found": {"type": "boolean"},
        "title": {"type": "string"},
        "artist": {"type": "string"},
        "language": {"type": "string", "description": "ISO 639-1"},
        "lyrics": lyr,
        "source_urls": {"type": "array", "items": {"type": "string"}},
    }
    if cfg["lrc"]:
        props["timed_source"] = {"type": "boolean"}
    return {"type": "object", "properties": props, "required": list(props), "additionalProperties": False}


def system_prompt(cfg):
    how = HOW["recall" if cfg["search"] == "off" else cfg["prompt"]]
    s = PURPOSE + how + NOT_FOUND + (LRC if cfg["lrc"] else "")
    if cfg["out"] != "strict":
        s += ("\n\nReply with only a JSON object, no prose and no code fence, matching this JSON Schema:\n"
              + json.dumps(schema(cfg), ensure_ascii=False))
    return s


def body(model, cfg, case):
    dur = f"{case['duration']} s" if case.get("duration") else "unknown"
    b = {
        "model": model,
        "messages": [{"role": "system", "content": system_prompt(cfg)},
                     {"role": "user", "content": f"Title: {case['raw_title']}\nArtist: {case['raw_artist']}\nDuration: {dur}"}],
        "max_tokens": cfg["max_tokens"],
    }
    if cfg["temp"] is not None:
        b["temperature"] = cfg["temp"]
    if cfg["reasoning"]:
        b["reasoning"] = {"effort": cfg["reasoning"], "exclude": True}
    if cfg["out"] == "strict":
        b["response_format"] = {"type": "json_schema", "json_schema": {"name": "lyrics", "strict": True, "schema": schema(cfg)}}
    elif cfg["out"] == "object":
        b["response_format"] = {"type": "json_object"}
    tools = []
    if cfg["search"] not in ("off", "builtin"):
        p = {k: v for k, v in {"engine": cfg["search"], "max_results": cfg["max_results"],
                               "search_context_size": cfg["ctx"]}.items() if v is not None}
        tools.append({"type": "openrouter:web_search", "parameters": p})
    if cfg["fetch"] != "off":
        tools.append({"type": "openrouter:web_fetch", "parameters": {"engine": cfg["fetch"], "max_uses": 3, "max_content_tokens": 20000}})
    if tools:
        b["tools"] = tools
        b["max_tool_calls"] = cfg["tool_calls"]
    return b


def tag_of(model, over):
    keys = "".join(f",{k}={over[k]}" for k in sorted(over))
    return model.split("/")[-1] + keys


def call(key, b):
    req = urllib.request.Request(URL, data=json.dumps(b).encode(), headers={
        "Authorization": f"Bearer {key}", "Content-Type": "application/json",
        "HTTP-Referer": "https://github.com/Error0229/Lyryc", "X-Title": "Lyryc lyrics research"})
    t0 = time.time()
    try:
        raw, status = json.loads(urllib.request.urlopen(req, timeout=600).read()), 200
    except urllib.error.HTTPError as e:
        txt = e.read() or b"{}"
        try:
            raw = json.loads(txt)
        except json.JSONDecodeError:
            raw = {"error": {"message": txt[:300].decode("utf-8", "replace")}}
        status = e.code
    except Exception as e:  # timeouts, resets
        raw, status = {"error": {"message": f"{type(e).__name__}: {e}"[:300]}}, 0
    return status, raw, int((time.time() - t0) * 1000)


def meta(raw):
    ch = (raw.get("choices") or [{}])[0]
    msg = ch.get("message") or {}
    u = raw.get("usage") or {}
    ann = [a for a in msg.get("annotations") or [] if a.get("type") == "url_citation"]
    tools = u.get("server_tool_use_details") or {}
    return {
        "finish": ch.get("finish_reason"), "native_finish": ch.get("native_finish_reason"),
        "provider": raw.get("provider"),
        "cost": u.get("cost"), "prompt_tokens": u.get("prompt_tokens"), "completion_tokens": u.get("completion_tokens"),
        "reasoning_tokens": (u.get("completion_tokens_details") or {}).get("reasoning_tokens"),
        "searches": tools.get("web_search_requests"), "tool_calls": tools.get("tool_calls_executed"),
        "citations": len({(a.get("url_citation") or {}).get("url") for a in ann}),
        "content_chars": len(msg.get("content") or ""), "refusal": bool(msg.get("refusal")),
        "error": ((raw.get("error") or {}).get("message") or "")[:200] or None,
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--model", required=True)
    ap.add_argument("--ids", default="pilot", help="pilot | all | 1,2,3")
    ap.add_argument("--set", nargs="*", default=[], help="factor=value overrides of BASE")
    ap.add_argument("--workers", type=int, default=4)
    args = ap.parse_args()
    over = {}
    for kv in args.set:
        k, v = kv.split("=", 1)
        if k not in BASE:
            sys.exit(f"unknown factor {k}")
        over[k] = None if v == "None" else int(v) if v.isdigit() else float(v) if v.replace(".", "", 1).isdigit() else v == "true" if v in ("true", "false") else v
    cfg = {**BASE, **over}
    tag = tag_of(args.model, over)
    allc = cases()
    ids = PILOT if args.ids == "pilot" else [c["id"] for c in allc] if args.ids == "all" else [int(i) for i in args.ids.split(",")]
    done = {r["id"] for r in jl(RUNS) if r["tag"] == tag and r["status"] == 200}
    todo = [c for c in allc if c["id"] in ids and c["id"] not in done]
    key = api_key()
    billed = key_usage(key)
    print(f"{tag}: {len(todo)} to run; ledger ${spent():.4f}, key usage ${billed:.4f}")
    if max(billed, spent()) > SOFT_STOP:
        sys.exit("budget reached")
    (RAW / tag).mkdir(parents=True, exist_ok=True)

    def one(case):
        if spent() > SOFT_STOP:
            print(f"budget: {spent():.4f} spent, not starting {tag} #{case['id']}")
            return
        b = body(args.model, cfg, case)
        status, raw, ms = call(key, b)
        (RAW / tag / f"{case['id']}.json").write_text(json.dumps(raw, ensure_ascii=False), encoding="utf-8")
        row = {"id": case["id"], "tag": tag, "model": args.model, "cfg": cfg, "status": status, "latency_ms": ms,
               "req_hash": hashlib.sha1(json.dumps(b, sort_keys=True).encode()).hexdigest()[:10], **meta(raw)}
        total = charge({"tag": tag, "id": case["id"], "status": status, "cost": row["cost"] or 0})
        with open(RUNS, "a", encoding="utf-8") as f:
            f.write(json.dumps(row, ensure_ascii=False) + "\n")
        print(f"{tag} #{case['id']} {status} {row['finish']} {ms}ms ${row['cost'] or 0:.4f} "
              f"s={row['searches']} c={row['citations']} rt={row['reasoning_tokens']} chars={row['content_chars']} "
              f"total=${total:.4f} {row['error'] or ''}", flush=True)

    with ThreadPoolExecutor(args.workers) as ex:
        list(ex.map(one, todo))


if __name__ == "__main__":
    main()
