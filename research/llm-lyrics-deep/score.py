"""Score every answer in results/runs.jsonl against LRCLIB. Metrics only, no lyric text.

    python score.py [--detail TAG_SUBSTRING]

Writes results/scores.jsonl (one row per answer) and results/summary.md (one row per config).
"""
import argparse, json, re, statistics
from collections import defaultdict

from common import RAW, RESULTS, cases, jl, lines_of, lrclib, lrclib_retry, score_text, score_timing

ENGLISH = {21, 22, 31, 33}
NO_LYRICS = {27, 28, 34, 35, 36}   # instrumentals + the non-song stream
SKIP = {3}                         # unverifiable reference
USABLE = dict(completeness=0.9, hallucination=0.1)
TEXT_OK = 0.9   # whole-text similarity: right words even when the line breaks differ
SYNC_OK = 0.8   # share of matched lines starting within 1 s


def reply_of(raw):
    content = ((raw.get("choices") or [{}])[0].get("message") or {}).get("content") or ""
    m = re.search(r"\{.*\}", content, re.S)
    if not m:
        return None, content
    try:
        return json.loads(m.group(0)), content
    except json.JSONDecodeError:
        return None, content


def reference(case):
    if not case.get("ref_id") or case["id"] in NO_LYRICS:
        return None
    rec = lrclib_retry(lrclib.by_id, case["ref_id"])
    return rec if rec and not rec.get("instrumental") else None


def score_row(run, case, ref):
    out = {"id": run["id"], "tag": run["tag"], "model": run["model"], "status": run["status"],
           "latency_ms": run["latency_ms"], "cost": run.get("cost") or 0, "finish": run.get("finish"),
           "searched": bool(run.get("searches") or run.get("citations")), "searches": run.get("searches") or 0,
           "citations": run.get("citations") or 0, "group": "none" if case["id"] in NO_LYRICS else "en" if case["id"] in ENGLISH else "cjk"}
    if run["status"] != 200:
        out["outcome"] = "error"
        return out
    raw = json.loads((RAW / run["tag"] / f"{run['id']}.json").read_text(encoding="utf-8"))
    reply, content = reply_of(raw)
    out["truncated"] = run.get("finish") == "length"
    if reply is None:
        out["outcome"] = "truncated" if out["truncated"] else "unparsed" if content.strip() else "empty"
        return out
    ans = lines_of(reply.get("lyrics") if isinstance(reply.get("lyrics"), (list, str)) else "")
    out.update(found=bool(reply.get("found")), ans_lines=len(ans), source_urls=len(reply.get("source_urls") or []))
    if case["id"] in NO_LYRICS:
        out["invented"] = len(ans) >= 4
        out["outcome"] = "invented" if out["invented"] else "correct-none"
        return out
    if not ans:
        out["outcome"] = "declined"
        return out
    out.update(score_text(ans, lines_of(ref.get("plainLyrics") or ref.get("syncedLyrics"))))
    if run["cfg"].get("lrc") and ref.get("syncedLyrics"):
        out.update(score_timing(reply.get("lyrics") if isinstance(reply.get("lyrics"), list) else str(reply.get("lyrics")).splitlines(),
                                ref["syncedLyrics"]))
        out["timed_source"] = reply.get("timed_source")
    out["text_ok"] = out["text_sim"] >= TEXT_OK and (out["hallucination"] or 0) <= USABLE["hallucination"]
    out["synced_ok"] = out["text_ok"] and (out.get("ts_within_1s") or 0) >= SYNC_OK
    good = out["completeness"] >= USABLE["completeness"] and (out["hallucination"] or 0) <= USABLE["hallucination"]
    out["outcome"] = "usable" if good else "partial" if out["completeness"] >= 0.5 else "wrong"
    return out


def mean(xs):
    xs = [x for x in xs if x is not None]
    return sum(xs) / len(xs) if xs else None


def agg(rows):
    songs = [r for r in rows if r["group"] != "none"]
    nolyr = [r for r in rows if r["group"] == "none"]
    f = lambda k, rs=songs: mean([r.get(k, 0.0 if r.get("outcome") in ("declined", "unparsed", "truncated", "empty", "error") and k in ("completeness", "text_sim") else None) for r in rs])
    lat = [r["latency_ms"] for r in rows if r["status"] == 200]
    return {
        "n": len(songs), "usable": sum(r.get("outcome") == "usable" for r in songs),
        "text_ok": sum(bool(r.get("text_ok")) for r in songs), "synced_ok": sum(bool(r.get("synced_ok")) for r in songs),
        "completeness": f("completeness"), "accuracy": f("accuracy"), "hallucination": f("hallucination"),
        "text_sim": f("text_sim"),
        "declined": sum(r.get("outcome") == "declined" for r in songs),
        "broken": sum(r.get("outcome") in ("unparsed", "empty", "error") for r in rows),
        "truncated": sum(bool(r.get("truncated")) for r in rows),
        "invented": f"{sum(bool(r.get('invented')) for r in nolyr)}/{len(nolyr)}" if nolyr else "-",
        "searched": mean([float(r["searched"]) for r in rows]), "citations": mean([r["citations"] for r in rows]),
        "lat_s": statistics.median(lat) / 1000 if lat else None, "cost": mean([r["cost"] for r in rows]),
        "ts_within_1s": mean([r.get("ts_within_1s") for r in songs]), "ts_median_err": mean([r.get("ts_median_err") for r in songs]),
    }


def fmt(v):
    return "-" if v is None else f"{v:.2f}" if isinstance(v, float) else str(v)


COLS = ["n", "usable", "text_ok", "completeness", "accuracy", "hallucination", "text_sim", "declined", "broken", "truncated",
        "invented", "searched", "citations", "lat_s", "cost"]


def table(title, groups, cols=COLS):
    md = [f"## {title}", "", "| config | " + " | ".join(cols) + " |", "|---|" + "---|" * len(cols)]
    for tag, a in groups:
        md.append(f"| `{tag}` | " + " | ".join(fmt(a[c]) for c in cols) + " |")
    return md + [""]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--detail")
    args = ap.parse_args()
    by_id = {c["id"]: c for c in cases()}
    latest = {}
    for r in jl(RESULTS / "runs.jsonl"):
        if r["id"] in SKIP:
            continue
        prev = latest.get((r["tag"], r["id"]))
        if prev is None or prev["status"] != 200:
            latest[(r["tag"], r["id"])] = r
    refs = {i: reference(c) for i, c in by_id.items() if i not in SKIP}
    scored = [score_row(r, by_id[r["id"]], refs[r["id"]]) for r in latest.values()]
    with open(RESULTS / "scores.jsonl", "w", encoding="utf-8") as f:
        for s in scored:
            f.write(json.dumps(s, ensure_ascii=False) + "\n")
    tags = defaultdict(list)
    for s in scored:
        tags[s["tag"]].append(s)
    order = sorted(tags, key=lambda t: (-(agg(tags[t])["completeness"] or 0)))
    md = ["# Summary (generated by score.py; metrics only)", "",
          f"usable = completeness >= {USABLE['completeness']} and hallucination <= {USABLE['hallucination']}; "
          f"text_ok = text_sim >= {TEXT_OK} and hallucination <= {USABLE['hallucination']}; "
          f"synced_ok = text_ok and >= {SYNC_OK:.0%} of matched lines start within 1 s. "
          "completeness/text_sim count declined or broken answers as 0.", ""]
    md += table("All songs with lyrics", [(t, agg(tags[t])) for t in order])
    md += table("CJK songs only", [(t, agg([r for r in tags[t] if r["group"] != "en"])) for t in order])
    md += table("English songs only", [(t, agg([r for r in tags[t] if r["group"] != "cjk"])) for t in order])
    lrc = [t for t in order if "lrc=True" in t]
    if lrc:
        md += table("Timestamps (lrc=True)", [(t, agg(tags[t])) for t in lrc],
                    ["n", "text_ok", "synced_ok", "ts_within_1s", "ts_median_err"])
    (RESULTS / "summary.md").write_text("\n".join(md) + "\n", encoding="utf-8")
    for t in order:
        a = agg(tags[t])
        print(f"{t:60s} n={a['n']:2d} usable={a['usable']:2d} txt={a['text_ok']:2d} comp={fmt(a['completeness'])} acc={fmt(a['accuracy'])} "
              f"hall={fmt(a['hallucination'])} sim={fmt(a['text_sim'])} decl={a['declined']} broken={a['broken']} "
              f"trunc={a['truncated']} inv={a['invented']} srch={fmt(a['searched'])} lat={fmt(a['lat_s'])} ${fmt(a['cost'])}")
    if args.detail:
        for s in sorted(scored, key=lambda s: (s["tag"], s["id"])):
            if args.detail in s["tag"]:
                keep = {k: (round(v, 2) if isinstance(v, float) else v) for k, v in s.items() if k not in ("tag", "model")}
                print(s["tag"], keep)


if __name__ == "__main__":
    main()
