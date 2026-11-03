# LLM + web search for lyrics lookup

Experiment behind [`docs/research/llm-search/REPORT.md`](../../docs/research/llm-search/REPORT.md).
It compares two approaches:
- **A**: ask the model for the lyrics.
- **B**: ask the model only to identify the song, then use Lyryc's normal LRCLIB lookup.

Lyrics are not committed. `data/.http/` (the LRCLIB cache) and `results/raw/` (model replies) are gitignored.

## Keys

The runners use the Gemini key saved in Lyryc's `llm-settings.json`:
- default location: `%APPDATA%\com.lyryc.app\`;
- override with `LYRYC_LLM_SETTINGS=<path>`.

They read the key into a request header only and never print it.

The Rust runner loads a temporary copy of the file, so the app's own file is never moved or rewritten.

## Run

```sh
cd research/llm-search
python prepare.py              # cases.src.json -> data/cases.json (durations from the pinned LRCLIB records)

cd ../../src-tauri             # approach B goes through the real Rust code (llm::identify)
cargo test --lib search_experiment::dump_regex_clean -- --ignored            # regex-cleaner baseline
LYRYC_SEARCH=off cargo test --lib search_experiment::identify_cases -- --ignored --nocapture
LYRYC_SEARCH=on  cargo test --lib search_experiment::identify_cases -- --ignored --nocapture   # needs grounding quota

cd ../research/llm-search
python run_a.py                # approach A without search
python run_a.py --search       # approach A with google_search (needs grounding quota)
python evaluate.py             # LRCLIB lookups (cached) + scoring -> results/summary.md, results/lookup.json
```

All runners resume: cases that already have a result are skipped, so a rerun spends no calls on them.

LRCLIB access is sequential, at least 2 s apart. It backs off on 503 and stops on 429.

## Files

| path | what |
|---|---|
| `data/cases.src.json` | 28 cases: the raw title/artist the player reports, the pinned LRCLIB reference, and accepted canonical names |
| `lrclib.py` | gentle cached LRCLIB client |
| `gemini_key.py` | reads the key from Lyryc's settings |
| `run_a.py` | approach A runner; uses the same Gemini request shape as `dialect.rs` |
| `../../src-tauri/src/llm/search_experiment.rs` | approach B runner (ignored tests) |
| `evaluate.py` | hit rates, identification accuracy, A's text and timing scores |
| `results/b_identify.jsonl`, `results/a_lyrics.jsonl` | one row per live call (A's rows hold metadata only) |
| `results/calls.log` | every live LLM request, including failed probes |
