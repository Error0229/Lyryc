# Extra sources: hit rate by name, length and guard

Why QQ Music, NetEase and Kugou rarely found anything, and what changed.
The code under test is `src-tauri/src/sources/`; the harness is the ignored test
`sources::diag::hit_rate`.

## Run

```sh
python -X utf8 build_cases.py      # cases.json from ../llm-search + the extra cases below
cd ../../src-tauri
LYRYC_DIAG=after LYRYC_DIAG_BUDGET=20 cargo test --lib sources::diag -- --ignored --nocapture
cd ../research/sources-hits
python -X utf8 analyze.py before after
```

Every response is cached in `.http/` (gitignored: it holds lyrics). A rerun only
pays for URLs it has not seen. Requests are sequential, 1 s apart, and capped
by the budget. A host that fails 3 times in a row is dropped for the run.
`results/before.jsonl` came from the harness at commit c8031b7, which ran the old
code with one search per provider. Mode `a` is today's query (regex-cleaned raw
title + raw artist); mode `b` is the identified native names.

## Cases

There are 36 cases, and 34 of them have a length. The app never asks the sources
without one, so `SYNC` and the lofi stream are skipped.
- 26 cases come from `../llm-search`, each with Gemini's search-off identification.
- 8 cases are new, with hand-written identities. They cover romanized CJK titles,
  songs with many covers (紅蓮華, 告白氣球, 海闊天空, 밤양갱) and music-video uploads.
  Album lengths come from iTunes, because LRCLIB answered 503.

The playing length is the LRCLIB reference length, except for music-video uploads.
For those it is the YouTube video's length (`yt_lengths.py`):
- #11: 226 s video vs 214 s album;
- #14: 222 vs 217;
- #15: 322 vs 306;
- #16: 277 vs 271;
- #24: 311 vs 252;
- #36: 203 vs 205;
- #37: 236 vs 239.

## Diagnosis (before, today's query)

Today the sources get 13 cases right and 4 wrong (covers or other recordings).
That leaves 21 cases that are not right:

| cause | cases | share |
|---|---|---|
| Wrong names: romanized/translated/garbled query (7 of the 10 had the right song picked, then thrown away by the title guard; 2 also needed Traditional ↔ Simplified) | #4 #7 #9 #13 #19 #29 #30 #31 #33 #38 | 10/21 |
| Music-video length differs from every listing by 4.4–13 s | #11 #14 #16 #23 | 4/21 |
| Music-video length coincides with a live recording (wrong recording accepted) | #15 | 1/21 |
| Test artifact: LRCLIB's reference length is 5–30 s off the release length that all three providers and iTunes list (#6 191 vs 185.6, #8 283 vs 253.3, #10 262 vs 242.2, #25 270 vs 251.9) | #6 #8 #10 #25 | 4/21 |
| Title-only guard let a same-title cover through (other artist) | #5 | 1/21 |
| Instrumental: no lyrics exist (a correct miss) | #27 | 1/21 |

"One query, one page" was not a cause. With native names, every case's song
was listed by at least one provider. With the player's names, only #6 and #10
were missing from all three.

Wrong-song acceptances today, all with no LRCLIB text:
- covers with the exact title: #4 "Odoriko" by nihmune, #5 "Kaikai Kitan" by Will Stetson;
- other versions: #9 "Spring Day (Japanese ver.)", #15 "Subtitle (... LIVE ...)";
- with native names: #7 "晴天 (原唱 周杰伦)" by RyaVocal, #24 "山海 (Live)" by 华晨宇 (another song).

## Changes

- **Order** (`lyrics.rs`): when LRCLIB misses, the LLM identifies the song (one
  cached call), LRCLIB is retried with the native names, and only then do the
  sources run. Without an LLM, the order and queries are unchanged.
- **Names** (`sources::Query`): the sources get every name: native first, the
  player's (regex-cleaned) second, then the other identified spellings. The first
  two distinct names are searched, each across QQ → NetEase → Kugou, stopping at
  the first accepted result. That is at most 2 searches per provider per song.
- **Guards**: with no LRCLIB text, a candidate must:
  - carry a title from any of the song's names;
  - carry an artist from any of them, matched by credit part, so
    "Ayase / YOASOBI" matches YOASOBI and "米津玄師 (よねづ けんし)" matches 米津玄師;
  - have no version marker (live, cover, ver., 伴奏, 原唱, …) that the song's
    titles lack.

  The guards now apply when picking the candidate, so an original further down
  the list wins over a cover on top.
- **Script**: names are compared with Traditional Chinese and Japanese shinjitai
  folded to Simplified, using OpenCC's character tables (Apache-2.0). For example,
  告白氣球 = 告白气球 and 茅原実里 = 茅原实里.
- **Length**: unchanged at ±3 s. A longer music video could only be shifted by a
  known constant offset. On this path LRCLIB has no text, and even LRCLIB plain
  text carries no times, so nothing can estimate that offset. The extra is an
  intro in some videos and an outro in others: #24's 59 s, or the 12 s in #11.
  A guessed offset would put every line early or late, so these 4 cases (12%)
  stay misses.

## Before → after (same 34 cases, same cached responses)

| | QQ | NetEase | Kugou | answer (first accepted) |
|---|---|---|---|---|
| before, today's query (a) | 10 right / 2 wrong | 10 / 4 | 7 / 1 | **13 / 4** |
| before, native names only (b) | 18 / 2 | 17 / 2 | 15 / 1 | 20 / 2 |
| **after, with LLM** | 21 / 0 | 17 / 0 | 19 / 1 | **23 / 1** |
| after, no LLM | 9 / 0 | 6 / 0 | 7 / 1 | 11 / 1 |

The remaining wrong answer, in every row, is #15. Kugou lists the 2025 stadium
live recording as plain "Subtitle" at 323 s, the same length as the 322 s video.
Its timestamps are identical to QQ's "Subtitle (... LIVE at STADIUM 2025)", but
nothing in its name or length tells it apart. No other song is accepted wrongly
after the change.

Without an LLM, the artist guard costs 3 right answers (#1 ヨルシカ vs
"Yorushika", #17 米津玄師 vs "Kenshi Yonezu", #2 where the player credits the
channel). In return it removes 3 wrong ones (#4, #5, #9). #5 also becomes right,
because Kugou's original is then picked.

Cases still missing after the change: 4 music-video lengths, 4 test-reference
lengths, the instrumental, and #2.

## Requests

| what | requests |
|---|---|
| QQ / NetEase / Kugou, baseline (2 runs) | 200 + 40 |
| QQ / NetEase / Kugou, after | 14 |
| YouTube search (video lengths) | 23 |
| iTunes Search (album lengths) | 14 |
| LRCLIB | 3 (2 answered 503; stopped) |

That makes 254 provider requests in total. 2 NetEase searches failed on the
network and were retried in the next run; no provider refused.
