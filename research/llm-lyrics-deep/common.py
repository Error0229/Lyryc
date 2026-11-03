"""Shared bits: key, ledger and budget, LRCLIB access, text scoring.

Lyrics never leave this process except into results/raw/ (gitignored).
"""
import difflib, json, os, re, sys, threading, time, unicodedata
from pathlib import Path

HERE = Path(__file__).parent
RESULTS = HERE / "results"
RAW = RESULTS / "raw"
LEDGER = RESULTS / "ledger.jsonl"
BUDGET = 5.00      # USD, hard
SOFT_STOP = 4.70   # stop starting new calls past this
ENV_FILE = Path(os.environ.get("LYRYC_ENV", r"C:\Users\login\Lyryc\.env"))

sys.path.insert(0, str(HERE.parent / "llm-search"))
import lrclib  # noqa: E402  (cached, sequential, stops on 429)

_lock = threading.Lock()


def api_key():
    if os.environ.get("OPENROUTER_API_KEY"):
        return os.environ["OPENROUTER_API_KEY"]
    for line in ENV_FILE.read_text(encoding="utf-8").splitlines():
        m = re.match(r'\s*OPENROUTER_API_KEY\s*=\s*["\']?([^"\'\s]+)', line)
        if m:
            return m.group(1)
    raise SystemExit("OPENROUTER_API_KEY not found")


def key_usage(key):
    """What OpenRouter has billed this key so far: the authoritative spend."""
    import urllib.request
    req = urllib.request.Request("https://openrouter.ai/api/v1/key", headers={"Authorization": f"Bearer {key}"})
    return json.loads(urllib.request.urlopen(req, timeout=30).read())["data"]["usage"]


def jl(path):
    path = Path(path)
    if not path.exists():
        return []
    return [json.loads(l) for l in path.read_text(encoding="utf-8").splitlines() if l.strip()]


def spent():
    return sum(r.get("cost") or 0 for r in jl(LEDGER))


def charge(row):
    """Append one billed call to the ledger; returns the new running total."""
    with _lock:
        total = spent() + (row.get("cost") or 0)
        row = {**row, "total": round(total, 6), "at": time.strftime("%Y-%m-%dT%H:%M:%S")}
        with LEDGER.open("a", encoding="utf-8") as f:
            f.write(json.dumps(row, ensure_ascii=False) + "\n")
        return total


def lrclib_retry(fn, *a, tries=6):
    for i in range(tries):
        try:
            return fn(*a)
        except lrclib.RateLimited:
            raise
        except Exception:  # timeouts during LRCLIB's overload spells
            if i == tries - 1:
                raise
            time.sleep(10 * (i + 1))


def cases():
    return json.loads((HERE / "data/cases.json").read_text(encoding="utf-8"))


# --- text scoring --------------------------------------------------------

try:
    import opencc
    _t2s = opencc.OpenCC("t2s").convert
except ImportError:  # scoring then treats traditional/simplified as different
    _t2s = lambda s: s

TS = re.compile(r"\[(\d+):(\d+(?:[.:]\d+)?)\]")
CREDIT = re.compile(r"(作词|作曲|編曲|编曲|作詞|词：|曲：|lyrics by|composed by|written by|produced by)", re.I)
SECTION = re.compile(r"^[\[(（【]?\s*(verse|chorus|pre-chorus|bridge|outro|intro|hook|refrain|間奏|间奏)[^\])）】]*[\])）】]?$", re.I)


def norm(s):
    """Case, width, punctuation and Chinese script folded away."""
    s = unicodedata.normalize("NFKC", s or "").lower()
    return re.sub(r"[\W_]+", "", _t2s(s))


def lines_of(text_or_lines):
    raw = text_or_lines if isinstance(text_or_lines, list) else (text_or_lines or "").splitlines()
    out = []
    for line in raw:
        line = TS.sub("", str(line)).strip()
        if line and not CREDIT.search(line) and not SECTION.match(line) and norm(line):
            out.append(line)
    return out


def lrc_times(lines):
    out = []
    for line in lines if isinstance(lines, list) else (lines or "").splitlines():
        m = TS.match(str(line).strip())
        text = TS.sub("", str(line)).strip()
        if m and norm(text) and not CREDIT.search(text):
            out.append((int(m.group(1)) * 60 + float(m.group(2).replace(":", ".")), text))
    return out


def ratio(a, b):
    return difflib.SequenceMatcher(None, a, b, autojunk=False).ratio()


def score_text(ans_lines, ref_lines):
    """Line-level completeness/accuracy/hallucination plus a line-break-proof text similarity."""
    ref = list(dict.fromkeys(norm(l) for l in ref_lines))
    ans = list(dict.fromkeys(norm(l) for l in ans_lines))
    if not ref:
        return {}
    best_ref = [max((ratio(r, a) for a in ans), default=0.0) for r in ref]
    best_ans = [max((ratio(a, r) for r in ref), default=0.0) for a in ans]
    matched = [b for b in best_ref if b >= 0.6]
    whole_ref, whole_ans = "".join(norm(l) for l in ref_lines), "".join(norm(l) for l in ans_lines)
    return {
        "completeness": sum(b >= 0.8 for b in best_ref) / len(ref),
        "accuracy": sum(matched) / len(matched) if matched else None,
        "hallucination": sum(b < 0.5 for b in best_ans) / len(ans) if ans else None,
        "text_sim": ratio(whole_ref, whole_ans) if whole_ans else 0.0,
        "ref_lines": len(ref), "ans_lines": len(ans),
    }


def score_timing(ans_lrc, ref_synced):
    ref = [(t, norm(x)) for t, x in lrc_times(ref_synced)]
    errs = []
    for t, line in lrc_times(ans_lrc):
        n = norm(line)
        good = [(rt, ratio(n, rx)) for rt, rx in ref]
        good = [rt for rt, r in good if r >= 0.8]
        if good:  # repeated lines: credit the nearest occurrence (optimistic)
            errs.append(min(abs(t - rt) for rt in good))
    if not errs:
        return {"timed_lines": 0}
    errs.sort()
    return {"timed_lines": len(errs), "ts_median_err": errs[len(errs) // 2],
            "ts_within_1s": sum(e <= 1 for e in errs) / len(errs)}
