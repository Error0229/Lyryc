# LLM + web search for lyrics lookup

Date: 2026-11-02. Branch `research/llm-search`. Runner and raw results: [`research/llm-search/`](../../../research/llm-search/).

## Headline

**Use the LLM to identify the song (approach B), never to supply lyrics (approach A).**

- **B works, even without search.** The model gets the raw player title, artist and duration and returns canonical names. Lyryc then does its normal LRCLIB `/api/get`. On 26 songs:
  - correct LRCLIB hits rise from **17/26 (65%)** with Lyryc's exact + first-search path to **25/26 (96%)**;
  - no wrong song is ever returned;
  - cost is about 1.1 s and USD 0.0006 per *unique* song on the user's existing route (`gemini-3.5-flash-lite`, no search).
- **A is unusable.** The model never refused (0/26), but it fabricated:
  - only 2/25 lyric texts matched the reference (two English megahits);
  - none of the 23 CJK songs matched: 5 came back in romaji, and the other 18 had at most half their lines right (median 0);
  - it emitted LRC timestamps for 23 songs, and none were usable: almost no line is within 1 s.
- **Search is the escalation, not the default, and could not be measured live.** The only key available, the user's Gemini key, is on the free tier, and Google Search grounding returns `429 RESOURCE_EXHAUSTED` for it on every Gemini 3 model. The 2.5 models are closed to new users (404).
  - The search request shapes for all five providers are implemented and unit-tested against the documented examples.
  - Search should only run when the unsearched identity's lookups miss. The one remaining miss (wrong artist taken from the YouTube channel) is exactly the kind of case search should fix.
- **Nobody seems to do this yet.** I found no lyrics app that uses an LLM with web search for either lookup or identification. The nearest relatives are beets plugins that normalise tags with an LLM (no search).

## Approach B: identify, then the normal lookup

`gemini-3.5-flash-lite`, search **off** (the key has no grounding quota), the real `llm::identify` code, 28 cases.

| metric | value |
|---|---|
| title_native exactly right (native script) | 25/26 (96%) |
| artist_native exactly right | 25/26 (96%) |
| both right | 24/26 (92%) |
| wrong answers that passed the 0.6 confidence gate | 2/2 (confidence 0.99 and 1.0) |
| non-song input (`lofi hip hop radio …`) rejected | yes (`found=false`) |
| refusals / errors (after pacing to the free-tier RPM limit) | 0 |
| latency median / max | 1.1 s / 2.4 s |
| cost per song (estimate, see below) | ≈ USD 0.0006 without search; + USD 0.014–0.04 with Gemini search |

LRCLIB `/api/get` with the player's duration (±2 s), 26 songs with a reference:

| strategy | synced | plain | instrumental | wrong song | miss | correct |
|---|---|---|---|---|---|---|
| raw names | 9 | 2 | 1 | 0 | 14 | 12/26 (46%) |
| Lyryc today, approx.: regex-cleaned + raw `/api/get`, then one `/api/search` | 14 | 2 | 1 | 0 | 9 | 17/26 (65%) |
| B identity: `/api/get` per `queries()`, then one `/api/search` | 22 | 2 | 1 | 0 | 1 | 25/26 (96%) |
| Lyryc today, then B on a miss (the proposed wiring) | | | | | | 25/26 (96%) |

What B fixed that the regex cleaner can't:

| case | raw (player) | regex cleaner → | B → |
|---|---|---|---|
| 1 | `雪国 - Snow Country` / `Yorushika` | `Snow Country` | 雪国 / ヨルシカ ✓ |
| 7 | `Qing Tian` / `Jay Chou` | `Qing Tian` | 晴天 / 周杰倫 ✓ |
| 10 | `Itte.` / `YORUSHIKA` | `Itte.` | 言って。 / ヨルシカ ✓ |
| 13 | `周杰倫 Jay Chou【稻香 Rice Field】-Official Music Video` | `稻香 Rice Field` | 稻香 / 周杰倫 ✓ |
| 14 | `[MV] IU(아이유) _ Blueming(블루밍)` / `1theK (원더케이)` | `IU (아이유) _ Blueming(블루밍)` | Blueming / 아이유 ✓ |
| 17 | `Lemon` / `Kenshi Yonezu` | (plain-only hit) | Lemon / 米津玄師, synced ✓ |
| 23–25 | ZUTOMAYO MV title, 草東沒有派對 MV title, `Kawaki wo Ameku` / `Minami` | partial / unchanged | ミラーチューン, 山海, カワキヲアメク ✓ |

Where B failed or regressed:
- **Case 2** `【MV】私じゃなかったんだね。(Official Video)` / `マカロニえんぴつ`: the title was right, but the model kept the wrong artist from the channel (confidence 0.99). The song is by りりあ。. This is the only miss, and it is exactly what search grounding exists for. Unverified here.
- **Case 5**: the model wrote the title as 回回奇譚 instead of 廻廻奇譚, at confidence 1.0. The romanized query `Kaikai Kitan` / `Eve` still hit, which is why `queries()` tries both forms.
- **Case 19**: the native pair found a plain-only LRCLIB record, while the romanized raw name had found a synced one. The existing plain → third-party upgrade path covers this.
- **Confidence is uncalibrated.** It is 0.95–1.0 on every answer, including both wrong ones. Don't gate on it. The lookup itself is the verifier: `/api/get` must match the name *and* the duration within ±2 s, and that is why no wrong song got through.
- **Metadata beyond the names is fabricated.** Don't use it:
  - albums were wrong in at least 3/26 (エルマ, 伸びしろ, SAE BYOK);
  - `duration_s` just echoes the input;
  - ISRC was filled in 26/26 and is unverified;
  - the "evidence URLs" are invented Spotify/Apple links.

## Approach A: "give me the lyrics"

`gemini-3.5-flash-lite`, search off (same quota problem), raw names, a JSON schema that asks for `{found, title, artist, language, synced_lrc, plain_lyrics, source_urls, confidence}`.

| metric | value |
|---|---|
| refusals / RECITATION blocks | 0/26 |
| `found=true` on the unverifiable `SYNC` / `Daoko` case | yes (4 lines, confidence 0.5) |
| text matches the LRCLIB reference (line recall ≥ 0.8) | 2/25 (Blinding Lights, Bohemian Rhapsody) |
| CJK songs with any matching text | 0/23: 5 in romaji, 18 with ≤ 50% of lines right (median 0%) |
| answered in romaji instead of the native script | 5/23 (Lemon is the real lyrics in romaji; still useless for the overlay and furigana) |
| emitted "synced" LRC | 23/25 |
| LRC lines within 1 s of the reference timing | ≈ 0 (best song: 43% of its matched lines) |
| latency median / p90 | 3.4 s / 4.8 s |
| tokens in / out (avg) | 156 / 1024, ≈ USD 0.0026 per song |

The expected failure was refusal; the real one is worse. The model confidently invents plausible lyrics and timestamps, with `confidence` ≥ 0.85 almost everywhere. With search it could quote real pages, but:
- web pages almost never carry LRC timing;
- reproducing full lyrics is precisely what providers block (Gemini's `RECITATION` finish reason exists for this);
- Lyryc already has real text and timing sources.

**Not worth keeping in code**; the runner stays in `research/llm-search/run_a.py`.

## Provider support: search + structured output in one request

Researched 2026-11-02 from the official docs. "Implemented" refers to `llm::complete_with` (`dialect.rs`).

| provider (preset) | search tool | schema in the same request? | what Lyryc sends | price of search | live |
|---|---|---|---|---|---|
| OpenAI (`openai`) | Responses API `tools:[{"type":"web_search"}]` ([docs](https://developers.openai.com/api/docs/guides/tools-web-search)) | Not stated either way ([structured outputs](https://developers.openai.com/api/docs/guides/structured-outputs)) | `POST /responses` + `text.format` json_schema (strict). On a 400 naming `text.format`, the schema moves into `instructions` | $10 / 1k calls + search content tokens ([pricing](https://developers.openai.com/api/docs/pricing)) | no key |
| Anthropic (`anthropic`) | `web_search_20250305` server tool, `max_uses` 3 ([docs](https://platform.claude.com/docs/en/agents-and-tools/tool-use/web-search-tool)) | **Treated as no.** Web search always cites, and citations reject `output_config.format` with a 400 ([structured outputs](https://platform.claude.com/docs/en/build-with-claude/structured-outputs), [citations](https://platform.claude.com/docs/en/build-with-claude/citations)) | tool + schema spelled out in `system`; the reply is the text after the last `web_search_tool_result` | $10 / 1k searches | no key |
| Google Gemini (`gemini`) | `tools:[{"google_search":{}}]` ([docs](https://aistudio.google.com/docs/google-search)) | **Gemini 3 only, preview** ("Gemini 3 lets you combine Structured Outputs with built-in tools…", [docs](https://aistudio.google.com/docs/structured-output)); 2.5 rejects a JSON mime type next to a tool | `responseJsonSchema` + tool; a 400 naming json/mime/schema falls back to the schema in `systemInstruction` | Gemini 3: 5,000 free/month (paid tier), then $14 / 1k queries; **not available on the free tier** ([pricing](https://aistudio.google.com/docs/pricing)) | **429 on this key**; the schema-only request (nullable type arrays in `responseJsonSchema`) was answered 56× without error |
| OpenRouter (`openrouter`) | `tools:[{"type":"openrouter:web_search"}]`; `plugins:[{id:"web"}]` / `:online` are deprecated ([docs](https://openrouter.ai/docs/guides/features/server-tools/web-search)) | Not documented ([structured outputs](https://openrouter.ai/docs/features/structured-outputs)) | server tool + `response_format` json_schema | Exa $0.007 / request, native search passed through ([docs](https://openrouter.ai/docs/features/web-search)) | no key |
| xAI (`xai`) | Live Search (`search_parameters`) is gone (HTTP 410). Responses API `tools:[{"type":"web_search"}]` ([docs](https://docs.x.ai/docs/guides/tools/search-tools)) | **Yes**, Grok 4 family ([docs](https://docs.x.ai/docs/guides/structured-outputs)) | same as OpenAI: `POST /responses` + `text.format` | $5 / 1k calls ([pricing](https://docs.x.ai/developers/pricing)) | no key |
| Perplexity (no preset) | Sonar Chat Completions support ended 2026-09-27; the Agent API (`/v1/agent`) has a `web_search` tool ([migration](https://docs.perplexity.ai/docs/agent-api/migrate-from-sonar/overview.md)) | Yes, `response_format` json_schema; the first request with a new schema can take 10–30 s ([docs](https://docs.perplexity.ai/guides/structured-outputs)) | not implemented: no preset, and a fourth request shape | $0.0025 / invocation + tokens ([pricing](https://docs.perplexity.ai/docs/getting-started/pricing.md)) | no key |
| Groq, DeepSeek, Mistral, Ollama, LM Studio, custom | none | - | `complete_with` fails before the network: "… has no built-in web search" | - | unit test |

Design notes:
- `Tools { web_search, schema }` is an additive argument; `ChatRequest` and `complete()` are unchanged, so the cleaning and translation callers are untouched.
- The search mechanism is per-preset data (`Preset::web_search`), not per-dialect, because OpenAI and OpenRouter share a dialect but not a search API.
- Search is never silently dropped. Only the *schema* can degrade, from native to prompt-enforced, and `identify::parse` reads the outermost JSON object either way.
- `Completion` returns the grounding sources and the number of searches the provider reports, which is the billing unit.

## What ran live and what didn't

- **Ran live (Gemini `gemini-3.5-flash-lite`, user's key):**
  - B without search on all 28 cases, through the Rust code;
  - A without search on all 28;
  - probes showing grounding returns 429 on 3.5-flash-lite and 3.8-flash, and 404 on 2.5-flash and 2.5-flash-lite.
- **Not live:**
  - any search-grounded answer;
  - every non-Gemini provider (no keys);
  - the Gemini 2.5 schema-plus-search 400 fallback (those models are gone for this account).
  
  These are covered by unit tests built from the documented request and response examples (`dialect.rs` tests, `identify.rs` tests).
- **Calls:** 71 `generateContent` requests: 56 answered, 13 × 429 (including the transport's automatic retry), 2 × 404; see [`results/calls.log`](../../../research/llm-search/results/calls.log).
  - Billed: USD 0 (free tier). At paid list prices that is about USD 0.09 in total.
  - Token prices are USD 0.30 / 2.50 per 1M in/out, from third-party listings, e.g. [anotherwrapper](https://anotherwrapper.com/llm-pricing/gemini-3.5-flash-lite) and [pricepertoken](https://pricepertoken.com/pricing-page/model/google-gemini-3.5-flash-lite); confirm on Google's pricing page.
  - The B per-song token cost is estimated from prompt size (~400 in / ~200 out), because the Rust runner did not record usage.
- **LRCLIB:** 134 distinct requests, sequential and at least 2 s apart, about 250 counting retries through long `503 ServerOverloaded` spells. No 429.

**Caveats.**
- n = 26 is small, and the set leans towards songs a model has seen. Truly obscure songs are where unsearched identification breaks and search should matter.
- The "Lyryc today" row approximates `lookup_lrclib`: the exact path plus the first search stage. It leaves out the later LLM-cleaning stages, so the real current hit rate is somewhat higher than 65%.

## Integration plan (after the lookup-pipeline refactor lands)

`lyrics.rs` stays untouched here. The plan is written in terms of the pipeline stages so it survives the refactor:

1. LRCLIB `/api/get` with the raw and regex-cleaned title (exists).
2. One LRCLIB `/api/search` (exists).
3. **New: on a miss, `identify`**, keyed by `(raw_title, raw_artist, round(duration))` and cached in `lyrics_cache`, including negative results. Each unique song costs at most one call, ever.
   - Use the cleaning route, with search **off** first: cheap, about 1 s, and works on free-tier keys.
   - Escalate to search **on** only if (a) the stage-4/5 lookups with the unsearched identity also miss, (b) the routed preset has `web_search`, and (c) the user enabled "Use web search to identify songs".
4. `/api/get` for each `SongIdentity::queries()` pair, native first, with the *player's* duration. Never use the model's `duration_s`.
5. Third-party sources (QQ / NetEase / Kugou) with `track: title_native, artist: artist_native`. Today the `Lrclib::Missing` arm of `resolve()` passes `clean_track_name_sync(&req.title)` (lyrics.rs:797), which is the "Snow Country" problem again. These sources index native names, so this is where the identity helps most.
6. Otherwise not found, cached as today.

Rules:
- **Gate on the lookup, not the model.** Accept an identity only through a duration-matched hit. Ignore `confidence`, `album`, `isrc` and `evidence_urls` for matching.
- **Budget:** cap identification at about 15 s, since search turns take 3–10 s. Run it at most once per song, and never for `found=false`.
- **Quota errors:** a 429 whose body says quota or RESOURCE_EXHAUSTED should switch search off for the session. Today `transport.rs` retries every 429 once after 2 s, which only wastes 2 s on a quota error. Small follow-up: don't retry when the message names the quota.
- **Simplification to consider:** unsearched `identify` returns everything the current LLM cleaning prompt does (`cleaning/`), plus native-script names and the artist. If the pipeline owner wants one LLM call instead of two, `identify` could replace the cleaning call on the miss path. I did not measure the current LLM cleaner, so that needs its own A/B.

Regex-cleaner bugs this experiment surfaced, for the `cleaning/` owner:
- `Shinunoga E-Wa` → `Wa` (split on a bare `-` inside a word);
- `雪国 - Snow Country` → `Snow Country` (keeps the translation, not the native half);
- `Subtitle［Official Video］` keeps the full-width brackets;
- `IU(아이유) _ Blueming(블루밍)` is not split on ` _ `.

## Cost and privacy notes for users

- **BYOK: the user's key pays.** Without search, identification costs about USD 0.0006 per unique song, cached forever.
  - Search adds the provider's search fee per lookup: about $0.005–0.04 depending on provider and number of queries (Gemini $14 / 1k queries after 5,000 free per month on paid keys; OpenAI and Anthropic $10 / 1k; xAI $5 / 1k).
  - Example: 300 never-seen, LRCLIB-missing songs a month is about USD 0.20 without search, and up to about USD 10 if every one escalated to search.
- **Free-tier Gemini keys can't search.** Grounding is "not available" on the free tier, so the setting must explain that it fails there.
- **What leaves the machine:** the raw title, artist and duration, sent to the chosen provider. With search on, the provider also sends derived queries to its search backend (Google Search for Gemini).
  - No lyrics, listening history or account data are sent.
  - Free-tier Gemini traffic may be used by Google to improve its products (Gemini API terms), so say so next to the toggle.
- **Display terms (verify before shipping):** Google's grounding terms require showing the returned Search Suggestions (`searchEntryPoint`) when grounded results are shown ([search suggestions](https://ai.google.dev/gemini-api/docs/grounding/search-suggestions)). Lyryc shows only the lyrics found *afterwards*, not the model's answer, but check the terms before turning on Gemini search.

## Existing apps

I found none that use an LLM *with web search* to fetch lyrics or to identify a song before a lyrics lookup. Shallow web and GitHub search; LyricsX, Lyricify, SPlayer, lrcget, YesPlayMusic, Spicetify lyrics-plus and Picard plugins all checked.

The closest:
- [beets-ai-tagcleaner](https://github.com/abugeia/beets-ai-tagcleaner) sends raw artist, title and filename to a local Ollama model before MusicBrainz matching: the same "normalise first" idea, without search.
- [beets-aisauce](https://github.com/metasauce/beets-aisauce) corrects tags with an OpenAI-compatible LLM.
- [BetterLyrics](https://github.com/jayfunc/BetterLyrics) advertises metadata correction and future local-LLM support.

## Reproduce

See [`research/llm-search/README.md`](../../../research/llm-search/README.md). With a key that has grounding quota, run:

```sh
LYRYC_SEARCH=on cargo test --lib search_experiment::identify_cases -- --ignored --nocapture
python run_a.py --search
python evaluate.py
```

The search columns then appear next to the unsearched ones; done cases are skipped, so nothing is spent twice.
