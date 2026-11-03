"""Score approach B (identification -> LRCLIB) and approach A (lyrics from the model).

Inputs: data/cases.json, results/regex_clean.json, results/b_identify.jsonl,
results/a_lyrics.jsonl (+ results/raw/ for A's text). LRCLIB answers are cached in
data/.http, so a rerun costs no requests. Writes results/lookup.json and
results/summary.md.

LRCLIB strategies per case (all with the player's duration, as lyrics.rs does):
  raw      /api/get with the raw title and artist
  app      /api/get(regex-cleaned title), /api/get(raw title), then one /api/search;
           an approximation of lyrics.rs's exact path + first search stage
  B        /api/get for each identity.queries() pair, then one /api/search
  app+B    app, then B only when app found nothing (the proposed integration)
"""
import difflib, json, re, statistics, unicodedata
from pathlib import Path

import lrclib

HERE = Path(__file__).parent


def jl(p):
    p = HERE / p
    return [json.loads(l) for l in p.read_text(encoding="utf-8").splitlines() if l.strip()] if p.exists() else []


def norm(s):
    return re.sub(r"[\W_]+", "", unicodedata.normalize("NFKC", s or "").lower())


def same(name, aliases):
    return bool(name) and any(norm(name) == norm(a) for a in aliases)


def contains(name, aliases):
    n = norm(name)
    return bool(n) and any(norm(a) and (norm(a) in n or n in norm(a)) for a in aliases)


def pick(results, duration):
    ok = [r for r in results or [] if duration is None or (r.get("duration") and abs(r["duration"] - duration) <= 2)]
    return next((r for r in ok if r.get("syncedLyrics") or r.get("instrumental")), ok[0] if ok else None)


def lookup(pairs, duration, search_pair):
    for track, artist in pairs:
        if duration and track and artist:
            rec = lrclib.get_exact(track, artist, duration)
            if rec:
                return rec, f"get {track} / {artist}"
    if search_pair and search_pair[0]:
        rec = pick(lrclib.search(*search_pair), duration)
        if rec:
            return rec, f"search {search_pair[0]} / {search_pair[1]}"
    return None, None


def outcome(rec, case):
    if not rec:
        return "miss"
    if case["title"] is None:
        return "wrong"  # nothing is the right answer for a non-song
    # LRCLIB often files the same recording under a romanized or English name.
    titles, artists = case["title"] + case.get("alt_title", []), case["artist"] + case.get("alt_artist", [])
    if not (contains(rec["trackName"], titles) and contains(rec["artistName"], artists)):
        return "wrong"
    return "synced" if rec.get("syncedLyrics") else "instrumental" if rec.get("instrumental") else "plain"


def identity_queries(i):
    titles = [i["title_native"], i.get("title_romanized")]
    artists = [i["artist_native"], i.get("artist_romanized")]
    out = []
    for t in titles:
        for a in artists:
            if t and a and (t, a) not in out:
                out.append((t, a))
    return out


def run_lookups(cases, regex, b_rows):
    rows = []
    for c in cases:
        d, rt, ra = c["duration"], c["raw_title"], c["raw_artist"]
        cleaned = regex.get(c["id"], rt)
        row = {"id": c["id"], "kind": c["kind"]}
        found = lambda rec: f"{rec['trackName']} / {rec['artistName']}" if rec else None
        rec, how = lookup([(rt, ra)], d, None)
        row["raw"] = (outcome(rec, c), how, found(rec))
        pairs = [(cleaned, ra)] + ([(rt, ra)] if cleaned != rt else [])
        rec, how = lookup(pairs, d, (cleaned, ra))
        row["app"] = (outcome(rec, c), how, found(rec))
        for b in b_rows:
            if b["id"] != c["id"]:
                continue
            tag = f"B_{'search' if b['search'] else 'nosearch'}_{b['model']}"
            ident = b.get("identity")
            if ident:
                rec, how = lookup(identity_queries(ident), d, (ident["title_native"], ident["artist_native"]))
                row[tag] = (outcome(rec, c), how, found(rec))
            else:
                row[tag] = ("miss" if c["title"] is not None else "miss", "no identity")
        rows.append(row)
        print(c["id"], {k: v[0] for k, v in row.items() if isinstance(v, tuple)}, flush=True)
    return rows


def ident_accuracy(cases, b_rows):
    by_case = {c["id"]: c for c in cases}
    out = {}
    for b in b_rows:
        c = by_case[b["id"]]
        tag = f"B_{'search' if b['search'] else 'nosearch'}_{b['model']}"
        reply = b.get("reply") if isinstance(b.get("reply"), dict) else {}
        s = out.setdefault(tag, {"n": 0, "title": 0, "artist": 0, "both": 0, "kept": 0, "kept_wrong": 0,
                                 "latency": [], "errors": 0, "isrc_given": 0, "details": []})
        if "error" in b:
            s["errors"] += 1
            continue
        s["latency"].append(b["latency_ms"])
        if c["title"] is None:
            s["details"].append((c["id"], f"n/a (found={reply.get('found')})", reply.get("confidence"),
                                 reply.get("title_native"), reply.get("artist_native")))
            continue
        s["n"] += 1
        t = same(reply.get("title_native"), c["title"])
        a = same(reply.get("artist_native"), c["artist"])
        s["title"] += t
        s["artist"] += a
        s["both"] += t and a
        s["isrc_given"] += bool(reply.get("isrc"))
        if b.get("identity"):
            s["kept"] += 1
            s["kept_wrong"] += not (t and a)
        s["details"].append((c["id"], "ok" if t and a else "WRONG", reply.get("confidence"), reply.get("title_native"), reply.get("artist_native")))
    return out


# --- approach A ---------------------------------------------------------

TS = re.compile(r"\[(\d+):(\d+(?:\.\d+)?)\]")
CREDIT = re.compile(r"(作词|作曲|編曲|编曲|作詞|lyrics by|composed by|written by)", re.I)


def lines_of(text):
    out = []
    for line in (text or "").splitlines():
        line = TS.sub("", line).strip()
        if line and not CREDIT.search(line):
            out.append(line)
    return out


def lrc_times(text):
    out = []
    for line in (text or "").splitlines():
        m = list(TS.finditer(line))
        t = TS.sub("", line).strip()
        if m and t and not CREDIT.search(t):
            out.append((int(m[0].group(1)) * 60 + float(m[0].group(2)), t))
    return out


def best_ratio(line, pool):
    n = norm(line)
    return max((difflib.SequenceMatcher(None, n, norm(p)).ratio() for p in pool), default=0.0)


def score_a(cases, a_rows):
    by_case = {c["id"]: c for c in cases}
    out = []
    for r in a_rows:
        c = by_case[r["id"]]
        raw_path = HERE / f"results/raw/a-{r['id']}-{'search' if r['search'] else 'nosearch'}-{r['model']}.json"
        raw = json.loads(raw_path.read_text(encoding="utf-8")) if raw_path.exists() else {}
        cand = (raw.get("candidates") or [{}])[0]
        text = "".join(p.get("text", "") for p in cand.get("content", {}).get("parts", []) if not p.get("thought"))
        try:
            reply = json.loads(text) if text else {}
        except json.JSONDecodeError:
            reply = {}
        row = {"id": r["id"], "kind": c["kind"], "status": r["status"], "finish": r["finish"],
               "latency_ms": r["latency_ms"], "tokens": r["tokens"], "found": reply.get("found")}
        model_lines = lines_of(reply.get("plain_lyrics")) or lines_of(reply.get("synced_lrc"))
        row["model_lines"] = len(model_lines)
        row["has_lrc"] = len(lrc_times(reply.get("synced_lrc"))) >= 5
        ref = lrclib.by_id(c["ref_id"]) if c.get("ref_id") else None
        ref_lines = lines_of(ref.get("plainLyrics")) if ref and not ref.get("instrumental") else []
        row["ref_lines"] = len(ref_lines)
        if ref_lines and model_lines:
            uniq_ref = list(dict.fromkeys(ref_lines))
            uniq_model = list(dict.fromkeys(model_lines))
            row["recall"] = sum(best_ratio(l, uniq_model) >= 0.8 for l in uniq_ref) / len(uniq_ref)
            row["precision"] = sum(best_ratio(l, uniq_ref) >= 0.8 for l in uniq_model) / len(uniq_model)
            if row["has_lrc"] and ref.get("syncedLyrics"):
                ref_t = lrc_times(ref["syncedLyrics"])
                errs = []
                for t, line in lrc_times(reply["synced_lrc"]):
                    best = max(ref_t, key=lambda x: difflib.SequenceMatcher(None, norm(line), norm(x[1])).ratio())
                    if difflib.SequenceMatcher(None, norm(line), norm(best[1])).ratio() >= 0.8:
                        errs.append(abs(t - best[0]))
                row["timing_within_1s"] = sum(e <= 1 for e in errs) / len(errs) if errs else 0.0
        out.append(row)
    return out


def pct(a, b):
    return f"{a}/{b} ({100 * a / b:.0f}%)" if b else "-"


def main():
    cases = json.loads((HERE / "data/cases.json").read_text(encoding="utf-8"))
    regex = {r["id"]: r["cleaned"] for r in json.loads((HERE / "results/regex_clean.json").read_text(encoding="utf-8"))}
    b_rows = [r for r in jl("results/b_identify.jsonl")]
    a_rows = jl("results/a_lyrics.jsonl")

    lookups = run_lookups(cases, regex, b_rows)
    acc = ident_accuracy(cases, b_rows)
    a_scores = score_a(cases, [r for r in a_rows if r["status"] == 200])
    (HERE / "results/lookup.json").write_text(json.dumps(
        {"lookups": lookups, "identification": acc, "a": a_scores}, ensure_ascii=False, indent=1), encoding="utf-8")

    md = ["# Summary (generated by evaluate.py)", ""]
    songs = [c for c in cases if c["title"] is not None]
    strategies = [k for k in lookups[0] if k not in ("id", "kind")]
    md += ["## LRCLIB lookup, songs with a reference (n=%d)" % len(songs), "",
           "| strategy | synced | plain | instrumental | wrong song | miss |", "|---|---|---|---|---|---|"]
    for s in strategies:
        rows = [r for r in lookups if r["id"] in {c["id"] for c in songs} and s in r]
        cnt = {k: sum(r[s][0] == k for r in rows) for k in ("synced", "plain", "instrumental", "wrong", "miss")}
        md.append(f"| {s} | " + " | ".join(pct(cnt[k], len(rows)) for k in cnt) + " |")
    # app then B fallback
    for s in [k for k in strategies if k.startswith("B_")]:
        rows = [r for r in lookups if r["id"] in {c["id"] for c in songs}]
        good = lambda o: o in ("synced", "plain", "instrumental")
        comb = sum(good(r["app"][0]) or (r["app"][0] == "miss" and good(r.get(s, ("miss",))[0])) for r in rows)
        md.append(f"| app, then {s} on miss | {pct(comb, len(rows))} correct (any timing) | | | | |")
    md += ["", "Per case:", "", "| id | kind | " + " | ".join(strategies) + " |", "|---|---|" + "---|" * len(strategies)]
    for r in lookups:
        md.append(f"| {r['id']} | {r['kind']} | " + " | ".join(r.get(s, ("-",))[0] for s in strategies) + " |")

    md += ["", "## B identification accuracy", ""]
    for tag, s in acc.items():
        lat = s["latency"]
        md += [f"### {tag}", "",
               f"- title_native exact: {pct(s['title'], s['n'])}; artist_native exact: {pct(s['artist'], s['n'])}; both: {pct(s['both'], s['n'])}",
               f"- passed the confidence gate: {s['kept']}, of which wrong: {s['kept_wrong']}",
               f"- ISRC filled in: {s['isrc_given']}/{s['n']} (unverified)",
               f"- errors: {s['errors']}; latency median {statistics.median(lat):.0f} ms, max {max(lat)} ms" if lat else "",
               "", "| id | verdict | confidence | title_native | artist_native |", "|---|---|---|---|---|"]
        md += [f"| {d[0]} | {d[1]} | {d[2]} | {d[3]} | {d[4]} |" for d in s["details"]]
        md.append("")

    if a_scores:
        md += ["## A: lyrics from the model (no search)", "",
               "| id | kind | finish | found | lines (model/ref) | recall | precision | LRC | timing ≤1 s | latency |",
               "|---|---|---|---|---|---|---|---|---|---|"]
        for r in a_scores:
            f = lambda k: f"{r[k]:.2f}" if isinstance(r.get(k), float) else "-"
            md.append(f"| {r['id']} | {r['kind']} | {r['finish']} | {r['found']} | {r['model_lines']}/{r['ref_lines']} | "
                      f"{f('recall')} | {f('precision')} | {'yes' if r['has_lrc'] else 'no'} | {f('timing_within_1s')} | {r['latency_ms']} |")
    (HERE / "results/summary.md").write_text("\n".join(md) + "\n", encoding="utf-8")
    print("lrclib", lrclib.stats)


if __name__ == "__main__":
    main()
