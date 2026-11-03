# LLM + web search for lyrics, second pass

Experiment behind [`docs/research/llm-lyrics-deep/REPORT.md`](../../docs/research/llm-lyrics-deep/REPORT.md).
It re-tests "ask the LLM for the lyrics" through OpenRouter, varying the model, reasoning effort, search and output configuration.

Lyrics are never committed. `results/raw/` (model replies) is gitignored; the LRCLIB cache is the first study's `../llm-search/data/.http/`, also gitignored.
Every committed file holds metrics, hashes, domains or song names only.

## Key and budget

- `OPENROUTER_API_KEY` comes from the environment, else from `C:\Users\login\Lyryc\.env` (override the path with `LYRYC_ENV`). It goes into a request header only and is never printed.
- Every call's `usage.cost` is appended to `results/ledger.jsonl` with a running total.
  - A runner refuses to start past USD 4.70 (`SOFT_STOP` in `common.py`), checking both the ledger and the key's billed usage (`GET /api/v1/key`).
  - `run.py` re-checks the ledger before each call.

## Run

```sh
python prepare.py                      # data/cases.json: the first study's 28 cases + 5 obscure songs + 3 instrumentals
python run.py --model deepseek/deepseek-v4.1-flash --ids pilot --set out=prompt
python run.py --model deepseek/deepseek-v4.1-flash --ids all --set out=prompt lrc=true
python run_b.py --ids 2,23,24,25,29,30,31,32,33              # approach B: identify with search, then LRCLIB
python score.py                        # results/scores.jsonl + results/summary.md
python peek.py "<tag>/*.json"          # reply structure (JSON? keys? line count?), never the text
python domains.py "<tag>/*.json"       # which sites answers were copied from
```

A configuration is `BASE` in `run.py` plus `--set factor=value` overrides. Its tag (model + overrides) keys the results, so a rerun skips cases that already have an answer.

## Files

| path | what |
|---|---|
| `common.py` | key, ledger, LRCLIB access with retries, text and timing scoring |
| `run.py` | lyrics runner (approach A) on OpenRouter Chat Completions |
| `run_b.py` | identification runner (approach B), with the prompt read from `src-tauri/src/llm/identify.rs` |
| `score.py` | per-answer metrics and per-config tables |
| `results/runs.jsonl` | one row per call: config, status, finish reason, tokens, searches, citations, cost |
| `results/b_runs.jsonl` | approach B: identity (names only) and the LRCLIB outcome |
| `results/scores.jsonl`, `results/summary.md` | scored answers |
| `results/ledger.jsonl` | spend |
