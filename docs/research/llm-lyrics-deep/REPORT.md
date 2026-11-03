# LLM + web search for lyrics, second pass

Date: 2026-11-02. Branch `research/llm-lyrics-deep`. Runner and metrics: [`research/llm-lyrics-deep/`](../../../research/llm-lyrics-deep/).
It follows [the first study](../llm-search/REPORT.md), which ran one model (`gemini-3.5-flash-lite`) with search off.

No lyric text appears in this report or in any committed file. Answers were scored inside the scripts against LRCLIB; the raw replies sit in a gitignored cache.

## Headline

**The user's hypothesis holds in large part, but the decisive factors are search and the model family, not reasoning.**

- **With search on and the right output config, a model can find lyrics, including CJK lyrics.** DeepSeek-v4.1-flash through OpenRouter's `openrouter:web_search` tool:
  - On the full set, the text is right for **25/30 songs** (whole-text similarity ≥ 0.9, ≤ 10% invented lines), **22/26 for CJK**. The first study got 2/25 overall and **0/23 for CJK**.
  - Matched lines are 96% accurate; 5% of answer lines match nothing.
  - **It invented no lyrics** for the 4 instrumentals or the non-song stream (0/5).
  - Cost is about USD 0.03 per song (median; max 0.094); latency is 16 s median and 69 s p90.
- **Timestamps are usable when the model copies an LRC page:** 21/30 songs have the right text *and* ≥ 80% of lines within 1 s of LRCLIB, and the median line-start error is 0.37 s.
  - The timing comes from third-party LRC sites (megalobiz, lyricdown, gecifang, rentanadviser …), not from LRCLIB itself.
  - It fails in two ways:
    - a different edit's timings (case 8: right text, every line about 25 s off);
    - made-up timings, which the model flagged itself in the one case it did (case 32, `timed_source=false`).
- **Most "incomplete or inaccurate" answers are configuration problems.** Effect on mean completeness, from the pilot:

  | change | effect on completeness |
  |---|---|
  | search off | −0.58 |
  | strict `json_schema` next to the search tool | −0.95 (6/6 replies came back as prose, unparseable) |
  | "recall" instead of "copy from the page" prompt | −0.27 |
  | `max_tokens` 1500 | −0.21 (truncation) |

- **Reasoning effort matters less, and in both directions:**
  - `none` gives −0.19 completeness, and the hallucinated-line share rises from 0.01 to 0.19;
  - `max` gives −0.37, because the reasoning eats the token budget and 2/5 replies are truncated;
  - `low` is not distinguishable from the default.
- **The model family dominates everything else, and no setting fixes it.** These models declined every lyrics request, at every reasoning level tried, often without searching: GPT-5.6-terra, GPT-5.6-luna, Claude Sonnet 5.5, Claude Haiku 5.5, Grok-4.7 and Perplexity Sonar-pro. Haiku says outright that it is copyright. Only Chinese-trained models answer reliably (DeepSeek, GLM, Kimi). Gemini answers sometimes and refuses others.
- **Approach B (identify, then LRCLIB) with search fixes identification completely on the hard cases.** It got 10/10 on the first study's one miss plus the obscure set, at about USD 0.015–0.05 per song. Each answer is a real, licensed-by-LRCLIB synced record, not text the model copied. **B stays the primary path. A is a last resort at most.**

## Setup

- **Route:** OpenRouter Chat Completions. Search is the current `openrouter:web_search` server tool; the `web` plugin and `:online` are deprecated per [the docs](https://openrouter.ai/docs/guides/features/server-tools/web-search). The engine is `auto`: native search where the provider has it (OpenAI, Anthropic, Google, xAI, Perplexity), else Exa at USD 0.007 per search. `max_tool_calls` is 6.
- **Cost:** every response's `usage.cost` goes into [`results/ledger.jsonl`](../../../research/llm-lyrics-deep/results/ledger.jsonl). The ledger matched OpenRouter's billed key usage to the cent at each check.
- **Prompt:**
  - The system prompt states the purpose: a personal overlay, shown only to the listener while the track plays.
  - It says: identify the song, search for a lyrics page in the original script, and copy it line by line without translating, romanizing or filling gaps from memory.
  - It requires `found=false` for instrumentals or when nothing is found.
  - The user turn carries the raw player title, artist and duration.
- **Output:** `{found, title, artist, language, lyrics[], source_urls[]}`, and `timed_source` when LRC is requested.
- **Test set:** 36 cases:
  - the first study's 28;
  - 5 obscure songs: zh `海浪` / deca joins, ko `난춘` / SE SO NEON, ja `リッケンバッカー` / リーガルリリー (given romanized), en Treehouse / Alex G, en Inside Out / Duster;
  - 3 instrumentals: Nujabes, Einaudi, Hisaishi.
  
  Case 3 (no verifiable reference) is excluded. References are the pinned LRCLIB records.
- **Metrics, per answer** (normalized for case, width, punctuation and traditional vs simplified Chinese):
  - *completeness*: the share of unique reference lines matched at ≥ 0.8 character similarity;
  - *accuracy*: the mean similarity of lines matched at ≥ 0.6;
  - *hallucination*: the share of answer lines below 0.5 against every reference line;
  - *text_sim*: the similarity of the whole concatenated text. This is robust to different line breaking. Several answers have text_sim ≈ 1.0 but low line completeness because they merged lines, as in case 15.
  - *usable*: completeness ≥ 0.9 and hallucination ≤ 0.1. *text_ok*: text_sim ≥ 0.9 and hallucination ≤ 0.1. *synced_ok*: text_ok and ≥ 80% of matched lines within 1 s.
  - *timing*: line-start error against LRCLIB synced. Repeated lines get the nearest occurrence, which is slightly optimistic.

## 1. Model screen (search on, prompt-enforced JSON, default reasoning)

Two songs first: Blinding Lights (en) and 晴天 (zh). Then the six-song pilot for the models that answered: ja 雪国, zh 晴天, ko Blueming, en Blinding Lights, obscure zh 山海, and an instrumental.

| model | answers lyrics? | pilot usable / text_ok (n=5) | completeness | hallucination | broken replies | $/song | latency |
|---|---|---|---|---|---|---|---|
| `deepseek/deepseek-v4.1-flash` | yes | **5 / 4** | **0.95** | 0.01 | 0 | 0.013 | 5.8 s |
| `deepseek/deepseek-v4-pro-0813` | yes | 4 / 4 | 0.88 | 0.01 | 1 truncated (instrumental, 16k tokens) | 0.027 | 7.6 s |
| `z-ai/glm-5.3` | yes | 4 / 3 | 0.84 | 0.02 | 0 | 0.037 | 18.7 s |
| `moonshotai/kimi-k3` | mostly | 4 / 3 | 0.75 | 0.00 | 1 declined | 0.025 | 15.4 s |
| `qwen/qwen3.8-max-0902` | sometimes | 2 / 2 | 0.38 | 0.00 | 2 declined, 1 stopped at `tool_calls` | 0.029 | 16.0 s |
| `google/gemini-3.8-flash` | sometimes | 1 / 2 | 0.35 | 0.00 | 2 refusals in prose ("copyright"), 1 upstream 504 | 0.031 | 9.3 s |
| `openai/gpt-5.6-terra`, `openai/gpt-5.6-luna` | **no**: `found=false` 0/2 | - | 0 | - | - | 0.012–0.019 | 3–7 s |
| `anthropic/claude-sonnet-5.5`, `anthropic/claude-haiku-5.5` | **no**: 0/2; Haiku cites copyright | - | 0 | - | - | 0.004–0.009 | 4–8 s |
| `x-ai/grok-4.7` | **no**: 0/2 (searched up to 5×, then declined) | - | 0 | - | - | 0.049 | 14 s |
| `perplexity/sonar-pro` | **no**: 0/2. OpenRouter rejects server tools for it (404 "no endpoints that support tool use"), so it ran with its built-in search | - | 0 | - | - | 0.008 | 2 s |

**Reasoning vs refusals** (two songs each, prompt JSON). None of these 12 answers contained lyrics: 10 were `found=false`, and Grok's 2 were refusals in prose ("can't", "licensed"):
- GPT-5.6-terra at `none` and `high`;
- Claude Haiku 5.5 at `none` and `high`;
- Grok-4.7 at `low` and `xhigh`.

Refusal is a provider policy, not a reasoning-depth effect.

## 2. Which factors matter (DeepSeek-v4.1-flash, one factor at a time, pilot)

Baseline: search `auto` (Exa), prompt-enforced JSON, lines array, default reasoning (`high` for this model), `max_tokens` 16000, default temperature, "transcribe" prompt. n = 5 songs plus 1 instrumental. **One song moves a mean by about 0.2, so only effects well beyond 0.2 are clear.**

| change from baseline | text_ok | completeness (Δ) | text_sim (Δ) | notes |
|---|---|---|---|---|
| baseline | 4/5 | 0.95 | 0.96 | $0.013, 5.8 s |
| **`response_format` json_schema strict** (next to the tool) | **0/5** | 0.00 (−0.95) | 0.00 | 6/6 replies were prose or markdown with no JSON object. OpenRouter accepts the strict schema but doesn't enforce it once a server tool runs. Gemini's single strict reply was JSON, so the effect is model-dependent. |
| `response_format` json_object | 3/5 | 0.92 (−0.04) | 0.94 | parseable, no gain |
| **search off** (recall prompt) | **1/5** | 0.37 (−0.58) | 0.28 | 1 declined, 1 truncated, 1 unparsed; one song recalled correctly |
| "recall" prompt with search on | 2/5 | 0.68 (−0.27) | 0.74 | the "copy from the page" wording matters |
| **reasoning `none`** | 4/5 | 0.76 (−0.19) | 0.81 | hallucinated lines 0.19 vs 0.01; one song fully wrong |
| reasoning `low` | 3/5 | 0.94 (−0.02) | 0.94 | noise |
| **reasoning `max`** | 3/5 | 0.58 (−0.37) | 0.59 | 2/5 truncated by reasoning; $0.05 and 62 s per song |
| `max_tokens` 1500 | 3/5 | 0.75 (−0.21) | 0.76 | 1 truncated (`finish_reason=length`), 1 cut short |
| temperature 0 | 3/5 | 0.85 (−0.10) | 0.94 | noise |
| one lyrics string instead of an array | 4/5 | 0.94 (−0.01) | 0.96 | no difference |
| search engine `exa` + `search_context_size=high` | 3/5 | 0.75 (−0.20) | 0.76 | 1 declined; no gain from more context |
| `max_results` 10 | 2/5 | 0.74 (−0.22) | 0.74 | 1 declined; no gain |
| + `openrouter:web_fetch` tool | 3/5 | 0.78 (−0.17) | 0.94 | no gain; fetches cost extra steps |
| **LRC requested** (`lrc=true`) | **5/5** | 0.96 (+0.01) | 0.99 | timestamps within 1 s on 98–100% of lines; 16 s per song |

Gemini-3.8-flash at reasoning `low` vs its default `medium` went from text_ok 2/5 to 2/4 (one fewer song). Its `high` setting failed upstream: all 4 calls were aborted with a 504 after about 11 s, and none was billed.

**How much each hypothesis explains:**
- **Configuration (2) + (3):** the large share. On the same model, the gap between a broken config (strict schema next to the tool, or search off) and the working one is 0.6–0.95 completeness. The output config alone (strict → prompt JSON) is the difference between 0/5 and 4/5 parseable answers.
- **Reasoning (1):** a moderate share, mostly at the extremes:
  - `none` loses about 0.2 completeness and multiplies invented lines by 19;
  - `max` loses about 0.4 through truncation unless `max_tokens` grows with it;
  - between low and high, no measurable effect.
- **Model family (4), not on the list:** the largest single factor. Half the frontier models refuse regardless of anything else.

## 3. Full set: the best config

`deepseek/deepseek-v4.1-flash`, search `auto` (Exa), prompt-enforced JSON, lines array, LRC requested, default reasoning, `max_tokens` 16000. 30 songs with lyrics + 5 without.

| subset | n | usable (line) | **text_ok** | **synced_ok** | completeness | accuracy | hallucination | text_sim | declined / truncated | $/song (mean) | latency (median) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| all songs | 30 | 17 | **25** | **21** | 0.79 | 0.96 | 0.05 | 0.89 | 1 / 1 | 0.037 | 16 s |
| **CJK** | 26 | 14 | **22** | **18** | 0.78 | 0.96 | 0.05 | 0.87 | 1 / 1 | 0.039 | 17 s |
| CJK, not obscure | 20 | 11 | 17 | 14 | 0.80 | 0.97 | 0.06 | 0.89 | 1 / 0 | 0.033 | 16 s |
| English | 4 | 3 | 3 | 3 | 0.85 | 0.95 | 0.04 | 0.99 | 0 / 0 | 0.020 | 12 s |
| obscure (8) | 8 | 4 | 6 | 5 | 0.73 | 0.94 | 0.03 | 0.86 | 0 / 1 | 0.048 | 19 s |
| the first study's 25 songs | 25 | 14 | **21** | 18 | 0.78 | 0.97 | 0.05 | 0.87 | 1 / 1 | 0.034 | 17 s |
| instrumentals + stream | 5 | | **invented 0/5** | | | | | | | 0.007 | 3 s |

Same 25 songs in the first study (Gemini flash-lite, no search): 2/25 matched, and 0/23 for CJK.

Failures on the full set:
- **case 11** (YOASOBI アイドル): wrong text (similarity 0.06; most likely another script or a translation);
- **case 9** (BTS Spring Day): declined without searching;
- **case 25**: 11 searches, then the reasoning hit `max_tokens`, so no answer ($0.094);
- **case 33** (Duster): wrong page, 43% of lines;
- **case 6**: 0.86 text similarity, just under the bar.

Searches per song: median 4, max 11. The cap of 6 tool calls was not enforced for Exa searches; OpenRouter counted up to 11.

**Timestamps** (27 answers carried LRC lines):
- the median song has 100% of matched lines within 1 s, and a median error of 0.37 s;
- 26/27 answers said `timed_source=true`. Two of those were still wrong: case 8 is about 25 s off, likely another edit; case 33 is 8.5 s off with partial text.
- The one `timed_source=false` (case 32) really was off (40 s).
- Timestamps are therefore **only trustworthy when they are checked against the audio**: a duration or first-line sanity check, or YouTube Music pre-alignment. Never trust the model's flag alone.

Where the text came from (domains of `source_urls` and citations, all runs): kkbox.com, mojim.org, uta-net.com, lyrics.az, yorushika.com, music-tw.line.me, colorcodedlyrics.com, music.bugs.co.kr, mw.genie.co.kr, gecifang.com, megalobiz.com and the like. These are ordinary lyrics sites. YouTube, Spotify and Apple Music pages appear in the citations, used for identification.

## 4. Approach B with search

`deepseek/deepseek-v4.1-flash`, the `llm::identify` system prompt read from `identify.rs`, schema in the prompt, `openrouter:web_search`. LRCLIB `/api/get` for each `queries()` pair, then one `/api/search`, as in the first study.

| case | raw names alone (LRCLIB) | B, search off | **B, search on** | searches | $ |
|---|---|---|---|---|---|
| 2 (the first study's miss: channel ≠ artist) | miss | no identity | **synced, right artist (りりあ。)** | 7 | 0.054 |
| 23, 24, 25 (the first study's obscure) | miss ×3 | 1/3 | **synced 3/3** | 2–4 | 0.015–0.029 |
| 29 deca joins, 30 SE SO NEON, 32 Regal Lily (romanized) | miss ×3 | 2/3 | **synced 3/3** | 2–4 | 0.015–0.031 |
| 31 Alex G, 33 Duster | synced ×2 | no identity | synced 2/2 | 6 | 0.044–0.046 |
| 28 lofi stream | miss | rejected | **rejected** (`found=false`) | 0 | 0.000 |
| 3 SYNC / Daoko (no reference) | miss | no identity | identity given; no LRCLIB record | 3 | 0.023 |

- With search, B identified **10/10** and every lookup returned the **synced** LRCLIB record, with no wrong song.
- DeepSeek without search says `found=false` when unsure: 3/10 here. That is honest, but it is why the unsearched route of the first study used a different model (Gemini flash-lite, 25/26).
- Median cost is about USD 0.025 per song; latency is 6–25 s.

## 5. App change (`src-tauri/src/llm/dialect.rs`)

The study found one config the app sends that breaks. `complete_with` sent OpenRouter's `openrouter:web_search` together with a strict `response_format: json_schema`, which is the combination that returned 6/6 prose replies.
- With OpenRouter search on, the schema now goes into the system prompt, the same path Anthropic search already takes. Without search the native strict schema stays.
- OpenRouter search requests now carry `max_tool_calls: 6`, a cost guard against the default of 30.
- Unit tests: `openrouter_search_puts_the_schema_in_the_prompt`, `openrouter_schema_without_search_is_native`, and the updated `openrouter_search_is_a_server_tool_on_chat_completions`.
- Nothing calls this path yet (`identify` is not wired in), so no user-visible behaviour changes.
- I did not add a reasoning-effort option. The model default was best, and `none` and `max` were both worse, so there is nothing to configure for the recommended route.

## 6. Recommendation

1. **Keep the first study's order:** LRCLIB → extra sources → B (identify, search off) → **B with search on**, behind the opt-in web search setting.
   - B with search is the cheapest way to fix the remaining misses: about USD 0.025 per unique song, cached.
   - It returns real synced records.
   - It solved every hard case here.
2. **Approach A (lyrics from the model) is viable only as an explicit, opt-in last resort**, when every source and B have missed, and only with:
   - a model that answers: today DeepSeek-v4.1-flash (best and cheapest) or GLM-5.3 / DeepSeek-v4-pro. **OpenAI, Anthropic, xAI and Perplexity models refuse**, so the setting must say which models work;
   - search on, the "copy from the page" prompt, the schema in the prompt (not strict), default reasoning, and `max_tokens` ≥ 8–16k;
   - LRC requested. Use the timestamps only after an audio check (the YouTube Music pre-alignment, or a duration or first-line check). Otherwise drop them and time the plain text with the syllable estimator or pre-alignment;
   - the source URL shown and the result cached locally. Treat `found=false` as final.
   - Cost: about USD 0.03–0.04 per song (up to 0.09); latency 15 s median and up to about 70 s. Fine for a background last resort, too slow for the first-render path.
3. **Expected yield of A in that slot is unmeasured.** Every song in this set (including the obscure ones, which I picked from LRCLIB) already had an LRCLIB record, and B found it. A only adds value for songs that LRCLIB and the extra sources truly lack. Those are exactly the songs whose lyrics pages are also rarest, so expect lower than the 83% text_ok here.

**Legal and copyright posture.**
- A makes the provider read third-party lyrics sites (mojim, kkbox, uta-net …) and reproduce the text.
- Lyryc only displays it to the person playing the track, on their own key, and doesn't redistribute it. That is the same posture as a browser showing those pages.
- But:
  - those sites' terms generally forbid automated copying;
  - several providers clearly treat lyric reproduction as off-limits, as the refusals show;
  - a feature that steers users to the models that don't refuse invites scrutiny.
- B avoids all of this: the model returns only names, and the text comes from LRCLIB.
- So: ship B; if A ships at all, ship it off by default, labelled, cached only locally, and never uploaded or shared.

## Caveats

- **n is small.** The factor pilot is 5 songs plus 1 instrumental, and Δ ≤ 0.2 is within one song. The full-set numbers are one model and one config, run once; I did not repeat calls to measure run-to-run variance.
- **Bias in the obscure songs:** the 5 new obscure songs were chosen from LRCLIB (they need a reference), so they are findable by construction.
- **Small model screen:** 2 songs per refusing model. Their refusals were uniform: 24/24 completed answers across models and reasoning levels had no lyrics, plus 2 Perplexity 404s. More samples were not worth the budget.
- **Gemini's high-reasoning failures were 504s from the upstream.** They are not a property of the config, and I did not retry them.
- **Not run, for budget:** a second model on the full set; the B strict-schema variant; Kimi and GLM reasoning sweeps.

## Spend

**USD 4.58 of the USD 5.00 budget**: 209 calls in `results/ledger.jsonl`, 202 of them billed. OpenRouter's key usage read USD 4.571 at the end; the ledger is slightly higher, presumably because the key's figure lags.

| phase | USD |
|---|---|
| model screen, refusal × reasoning checks, six-song pilot (65 calls) | 1.44 |
| one-factor sweep: 14 DeepSeek configs + 2 Gemini reasoning levels (92 calls) | 1.81 |
| full set, best config, the 30 cases not in the pilot (30 calls) | 1.01 |
| approach B, search on and off (22 calls) | 0.32 |

## Reproduce

See [`research/llm-lyrics-deep/README.md`](../../../research/llm-lyrics-deep/README.md). Runs resume by config tag, so a rerun spends nothing on cases already answered.
