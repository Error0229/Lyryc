# Pre-aligning YouTube Music lyrics before they play: research report

Branch `research/prealign`, 2026-11-02. Code and run instructions: [`research/prealign/`](../../../research/prealign/README.md). Queue-reader prototype: [`archive/extension-prototypes/queue/`](../../../archive/extension-prototypes/queue/README.md) (shipped as `extension/youtube-queue.js`). Follows [`docs/research/timing/REPORT.md`](../timing/REPORT.md).

## Headline

**Recommendation: worth doing.** It solves a real problem: the timing study's error came mostly from aligning against a different upload than the user hears. On YouTube Music the page URL gives the exact `videoId`, so we can fetch the audio that actually plays and align it before it plays.

- **Line level: median 0.11 s; 80% of lines within 0.3 s; 89% within 1 s.** Against QQ references, 20 tracks, 1,135 lines. The timing study got 0.49 s from random uploads.
- **Word level (Latin words, CJK and kana characters, hangul syllable blocks): median 0.09 s; 75% within 0.2 s; 85% within 0.3 s.** 8,475 words. That is the same as the disagreement between two independent human references (NetEase vs QQ: 0.088 s median once each track's constant offset is removed). **The aligner is now about as good as the ground truth can show.** Character and syllable timing for ja/zh/ko works: ja median 0.066 s, zh 0.080 s, ko 0.074 s.
- **Cost per 4-minute track: about 56 s wall time.** Download 4.7 s (median), decode 1.3 s, alignment 50 s. Config: ONNX Runtime int8, 8 threads, 15 s chunks + 3 s context. Peak RSS 1.4 GB, 356 MB model on disk. That is realtime factor 0.21, so it runs about 4× faster than playback and stays ahead of a 2–3 track queue. With 2–4 threads it takes about 90 s per track, still 2.5× faster than playback.
- **Demucs vocal separation does not pay.** It costs about 4× the compute (192 s per track for separation alone) and gives no consistent accuracy gain.
- **Reject rule:** reject a line if its 3-line-smoothed confidence is below 0.10, or reject the whole track if more than 60% of its lines are below 0.15. Where LRCLIB line timing exists, also reject lines that are more than 0.7 s away from LRCLIB. Lines kept within 1 s: 89% → 95% (99% with the LRCLIB check). Precision is honest but modest; see below.
- **Integration: Rust `ort` + int8 ONNX, with the standalone self-updating `yt-dlp.exe` as the only external binary.** No Python sidecar. The real porting cost is romanization (Japanese kanji readings), not the model.
- **The frontend already supports word timing** (`LyricLine.words?: LyricWord[]`, rendered by `tokenize`). The Rust `LyricLine` needs one optional field.

## Setup

- **Tracks.** 20 tracks: 5 en, 5 ja, 5 zh, 5 ko. They include rap or fast songs (Lose Yourself, アイドル, 雙截棍, How You Like That, 봄날), ballads, a long intro (晴天), a repetitive song (Billie Jean), a whispered one (bad guy), and en/ko mixed lines.
- **Audio.** Each track was resolved to its YouTube Music **song** id (`MUSIC_VIDEO_TYPE_ATV`, the id `music.youtube.com/watch?v=` carries) with `ytmusicapi`. It was then fetched with yt-dlp `bestaudio` (opus, about 130 kbps, 4.2 MB median). All 20 downloads succeeded, sequentially, with a 2 s gap.
- **Aligner.** MMS wav2vec2 CTC (`torchaudio` MMS_FA) with uroman, plus pykakasi for Japanese. The `<star>` wildcard soaks up intros and ad-libs. Each line is split into pieces: a Latin word, or one CJK, kana or hangul character. Each piece's span comes straight from the CTC path. The aligner gets the reference's plain text (QQ first, then Kugou, NetEase, LRCLIB) and has to find the times, which is what the app would do with any plain text.
- **References.** QQ QRC (clean-room QQ 3DES), Kugou KRC and NetEase YRC: all word or syllable timed. LRCLIB is line-level only. A reference is dropped for a track if its duration is off by more than 2 s. Scoring matches characters with difflib, so small text differences are fine.
- "**Offset-removed**" means each (track, reference) median line offset is subtracted. A constant offset means the reference was timed against a different master or edit. It is a property of the reference, not of the aligner.

## Line-level accuracy (production config: ONNX int8, 8 threads, 15 + 3 s)

Line onset error, aligner vs reference. Raw / offset-removed.

| ref | lang | lines | median (s) | ≤0.1 s | ≤0.2 s | ≤0.3 s | ≤1.0 s | median, offset-removed |
|---|---|---|---|---|---|---|---|---|
| QQ | en | 370 | 0.221 | 26% | 47% | 64% | 80% | 0.121 |
| QQ | ja | 319 | 0.066 | 66% | 90% | 95% | 97% | 0.049 |
| QQ | zh | 247 | 0.097 | 51% | 74% | 86% | 96% | 0.079 |
| QQ | ko | 199 | 0.096 | 53% | 69% | 75% | 83% | 0.086 |
| **QQ** | **all** | **1135** | **0.106** | **47%** | **69%** | **80%** | **89%** | **0.076** |
| Kugou | all | 726 | 0.172 | 36% | 56% | 67% | 85% | 0.095 |
| NetEase | all (en/ja/zh) | 320 | 0.116 | 45% | 60% | 68% | 87% | 0.048 |
| LRCLIB | all | 850 | 0.235 | 26% | 45% | 59% | 82% | 0.108 |

English is worse because of two tracks:
- **Billie Jean:** line median 0.83 s, low confidence. The rule below rejects the track.
- **Lose Yourself:** a constant −0.24 s offset against QQ (0.07 s once removed).

The other three English tracks are at 0.08–0.18 s. LRCLIB is the noisiest reference. Its raw 0.235 s median is mostly LRCLIB's own line-level slop, not the aligner's error.

## Word-level accuracy (same run)

Word onset error. A "word" is a Latin word, or one CJK or kana character, or one hangul syllable block.

| ref | lang | words | median (s) | ≤0.1 s | ≤0.2 s | ≤0.3 s | median, offset-removed |
|---|---|---|---|---|---|---|---|
| QQ | en | 2656 | 0.171 | 33% | 56% | 71% | 0.114 |
| QQ | ja | 3085 | 0.066 | 67% | 87% | 95% | 0.059 |
| QQ | zh | 1843 | 0.080 | 59% | 81% | 89% | 0.089 |
| QQ | ko | 891 | 0.074 | 62% | 78% | 84% | 0.071 |
| **QQ** | **all** | **8475** | **0.090** | **54%** | **75%** | **85%** | **0.078** |
| Kugou | en | 2653 | 0.176 | 31% | 56% | 71% | 0.118 |
| Kugou | ja | 1813 | 0.077 | 62% | 87% | 94% | 0.076 |
| Kugou | zh | 301 | 0.125 | 37% | 82% | 96% | 0.053 |
| Kugou | ko | 730 | **0.450** | 14% | 25% | 33% | 0.183 |
| NetEase | en | 1762 | 0.054 | 63% | 68% | 69% | 0.040 |
| NetEase | ja | 465 | 0.062 | 66% | 87% | 92% | 0.081 |
| NetEase | zh | 399 | 0.287 | 0% | 1% | 61% | 0.049 |

- NetEase zh is a constant +0.29 s reference offset; 91% of words are within 0.1 s once it is removed.
- Kugou ko is a reference problem; see the next section.
- LRCLIB has no word data.

**Model precision does not matter.** fp32 torch with 30 s chunks scores 0.093 s word / 0.111 s line (QQ, all). int8 ONNX with 15 + 3 s scores 0.090 s / 0.106 s. On the 2-track speed subset, fp32 ONNX and int8 ONNX at 15 + 3 s are equal: 0.076 vs 0.075 s word median.

## How good are the references? (read before trusting any word number)

| check | result |
|---|---|
| QQ lines (≥3 words) with every word gapless | **88%** (Kugou 89%, NetEase 74%) |
| lines with evenly spread (synthetic) word durations | 0–2% |
| Kugou word onsets identical to QQ (±10 ms) | **100% on 6 of 13 tracks** (Shape of You, Lose Yourself, Billie Jean, bad guy, 紅蓮華, 밤편지), 0–8% on the rest |
| LRCLIB search hits with synced lyrics / with enhanced `<mm:ss.xx>` word tags | 315 / **0** |

- **Word onsets are usable; word ends are not.** About 88% of QQ/Kugou lines are karaoke "fill": each word ends exactly where the next begins. So word *end* times in the references are fill, not vocal offsets. I score onsets only, and the app should only trust onsets plus "until the next word".
- **Kugou is not an independent second opinion.** It and QQ are both Tencent Music, and on 6 of 13 tracks Kugou is a byte-identical copy of QQ. Where they agree, it proves nothing.
- **Independent pairs agree to about 0.09 s.** NetEase vs QQ word onsets: 0.21 s raw, **0.088 s offset-removed**. Kugou vs NetEase: 0.092 s offset-removed. The aligner vs QQ (0.078–0.090 s) sits **at this noise floor**. Below about 0.1 s the references cannot separate a good aligner from a better one.
- **Korean:** QQ vs aligner is 0.07 s. Kugou vs aligner is 0.45 s, and Kugou vs QQ is 0.44 s raw but 0.074 s offset-removed. **Kugou's Korean timings are offset**, probably timed against a different master. They are not wrong word by word.
- **LRCLIB cannot be used as word-level ground truth:** 0 enhanced-LRC hits across 315 synced entries. Its line timing is also the noisiest: 0.25 s vs QQ raw, 0.09 s offset-removed.

## Speed and cost

Realtime factor (RTF) = compute seconds / audio seconds. The machine is Windows 11, 16 logical cores, 16 GB, CPU only, and was shared with other agents during the runs. Thread-scaling numbers beyond 2–4 threads are noisy, so read them as ranges.

| config | backend | threads | chunk + ctx (s) | tracks | median RTF | s per 4-min track | load (s) | peak RSS (MB) |
|---|---|---|---|---|---|---|---|---|
| fp32 (timing-study config) | torch | 8 | 30 + 0 | 20 | 0.342 | 82 | 10.6 | 2025 |
| fp32 | ONNX | 8 | 15 + 3 | 2 | 0.363 | 87 | 4.7 | 2261 |
| int8 | ONNX | 2 | 15 + 3 | 2 | 0.381 | 92 | 1.8 | 1405 |
| int8 | ONNX | 4 | 30 + 5 | 2 | 0.388 | 93 | 2.1 | 2316 |
| int8 | ONNX | 8 | 30 + 5 | 2 | 0.237 | 57 | 2.1 | 2318 |
| int8 | ONNX | 8 | 20 + 4 | 2 | 0.229 | 55 | 2.2 | 1915 |
| **int8 (recommended)** | **ONNX** | **8** | **15 + 3** | **20** | **0.207** (0.17–0.23) | **50** | **2.6** | **1437** |
| int8 | ONNX | 16 | 30 + 5 | 2 | 0.202 | 48 | 2.4 | 2322 |
| Demucs htdemucs, separation only | torch | 8 | – | 6 | 0.799 | 192 | – | 1496 |

| step | median (range), 20 tracks |
|---|---|
| yt-dlp download (opus about 130 kbps, 4.2 MB) | 4.7 s (3.2–12.7) |
| ffmpeg decode to 16 kHz mono | 1.3 s |
| CTC emissions | about 98% of alignment time |
| Viterbi + word spans | 0.3–1.8 s |

- **Shorter chunks are faster and lighter, and no less accurate.** 15 + 3 s beats 30 + 5 s on time and memory, because attention cost grows with chunk length. Accuracy is unchanged: 0.075 vs 0.076 s word median on the subset.
- **Going from 8 to 16 threads buys nothing.** In the background, 2–4 threads (about 90 s per track) is the polite setting and still runs 2.5× faster than playback.
- Peak RSS here includes a Python process with torch imported. A Rust `ort` process would carry the model (356 MB) plus activations, so expect well under 1 GB. That figure is not measured.

**Does Demucs pay off? No.** On the 6 tracks run both ways (QQ ref, line / word median):

| track | plain mix | vocals stem |
|---|---|---|
| Shape of You | 0.09 / 0.10 | 0.11 / 0.12 |
| Lose Yourself (rap) | 0.27 / 0.25 | 0.27 / 0.26 |
| Billie Jean | 0.83 / 0.30 | 0.53 / 0.25 |
| 雙截棍 (rap) | 0.17 / 0.13 | 0.22 / 0.13 |
| How You Like That | 0.08 / 0.05 | 0.09 / 0.07 |
| LOVE DIVE | 0.31 / 0.09 | 0.30 / 0.09 |

Only Billie Jean improves, and even then it is still bad. The separated run costs 4–5× the compute and adds 84 MB of weights. Not worth it.

## Robustness and the reject rule

What fails:
- **Billie Jean** (repetitive, long instrumental stretches): line median 0.83 s, 47% of lines more than 1 s off. Its median confidence is 0.095, the lowest in the set, and 70% of its lines are below 0.15. The model knows it is lost.
- **Constant offsets** (Lose Yourself −0.24 s, NetEase zh +0.29 s) are reference-version effects, not alignment failures.
- **Line-onset outliers on mixed en/ko lines** (LOVE DIVE line 0.31 s, word 0.09 s): the first token of a line is an ad-lib or a romanized English word that the wildcard swallows. Words after it are fine.
- **Rap and fast songs are not a problem.** アイドル 0.06 s, 雙截棍 0.13 s word, How You Like That 0.05 s, 봄날 0.08 s.
- There is a small systematic early bias: signed medians run from −0.03 to −0.05 s. The app's global offset can absorb it.

Confidence is the mean frame posterior of a line's tokens. Rules were scored on 1,210 lines of the production run. A line counts as bad if it is more than 1 s off its best word-timed reference (else LRCLIB).

| rule | rejected | precision (>1 s) | precision (>0.5 s) | recall (>1 s) | kept ≤0.3 s | kept ≤1.0 s |
|---|---|---|---|---|---|---|
| none | 0% | – | – | – | 79% | 89% |
| conf < 0.10 | 16% | 40% | 51% | 58% | 86% | 94% |
| track: > 60% of lines conf < 0.15 | 8% | 47% | 60% | 35% | 83% | 92% |
| **smoothed3 conf < 0.10 OR track > 60% low** | **20%** | **36%** | **47%** | **62%** | **87%** | **95%** |
| *LRCLIB subset:* off LRCLIB line by > 0.7 s | 11% | 79% | 87% | 86% | 91% | 98% |
| ***LRCLIB subset:* conf < 0.10 OR off LRCLIB > 0.7 s** | **18%** | **53%** | **62%** | **91%** | **92%** | **99%** |

**Proposed rule:**
1. If more than 60% of a track's lines have confidence below 0.15, drop the alignment for the whole track and keep whatever timing we had.
2. Otherwise, mark a line *unaligned* if the median confidence of it and its two neighbours is below 0.10, or if an LRCLIB synced line exists and the aligned onset is more than 0.7 s away from it.
3. An unaligned line keeps its line time (LRCLIB's, if any) and drops its `words`.

**Honest read:**
- Confidence alone has poor precision: about 1 in 3 rejected lines was actually fine. Rejecting only costs word highlighting on those lines, so that is cheap.
- The LRCLIB cross-check is the strong signal: 79% precision on its own.
- The app usually has LRCLIB timing already, so the combined rule is the realistic one: 99% of kept lines within 1 s.

## Queue reader (prototype, not wired in)

`archive/extension-prototypes/queue/`. It needs no new permission.

- **How it reads the queue.** A MAIN-world script reads YTM's own Redux store, `document.querySelector("ytmusic-player-queue").getState().queue`, once per second. When anything changes it posts `{current, position, upNext[3], ad}`, and a bridge forwards that as `UPCOMING_TRACKS`.
- **Why not the DOM or the player API.** Queue DOM items carry no videoId, and the player's `getPlaylist()` is empty on YTM.
- **Verified live on music.youtube.com, 2026-11-02, logged out:**
  - the store holds the queue with the panel closed and while an ad plays (`#movie_player.ad-showing`);
  - after an ad, `items` = [current] + `automixItems` (50);
  - after "next", `selectedItemIndex` = 1 and the next id is `automixItems[0]`.
- **Pitfall:** in video mode (`OMV_PREFERRED`) the queue holds music-video ids (`MUSIC_VIDEO_TYPE_OMV`), whose audio often has a different intro. Prefetch the id that will actually play.
- **Failure mode:** if YTM renames the store, the reader goes silent and the app simply doesn't prefetch, which is today's behaviour.

## Integration: sidecar vs Rust `ort`

Both options need yt-dlp, and yt-dlp breaks whenever YouTube changes its player. Both need romanization tables and the 356 MB int8 model as an opt-in download.

| | Python sidecar (embedded Python + onnxruntime + uroman + pykakasi + yt-dlp) | Rust `ort` + `yt-dlp.exe` |
|---|---|---|
| Ship size | Python runtime about 25 MB + onnxruntime/numpy about 40 MB + pykakasi/uroman about 30 MB + ffmpeg about 80 MB (do not ship torch: +200 MB, and it is not needed with ONNX) | `onnxruntime.dll` about 12 MB + `yt-dlp.exe` about 18 MB, fetched on first use. No ffmpeg: request `-f 140` (m4a/AAC) and decode with `symphonia` (not tested here; the bench used opus + ffmpeg). Japanese reading dictionary (lindera/IPADIC) 15–50 MB. |
| Model | 356 MB int8 ONNX, opt-in download | same |
| Startup | Interpreter + imports 1–3 s per spawn (keep it resident: +100–200 MB idle) | Model load about 2 s in-process; nothing resident when idle |
| yt-dlp updates | `pip install -U yt-dlp` in the sidecar's venv | `yt-dlp.exe -U` (self-updating official binary), run on failure and weekly |
| Other updates | Python security updates, wheel ABI churn, AV false positives on frozen exes | Rust deps through the normal app release |
| Porting work | none (this bench is the sidecar) | CTC Viterbi plus pieces → spans: about 200 lines. Romanization: hangul is algorithmic, pinyin via the `pinyin` crate, kana is a table, Latin via `deunicode`. **Kanji readings need a morphological dictionary (lindera).** That is the real cost. |

**Pick: Rust `ort` + standalone `yt-dlp.exe`.**
- The sidecar ships a whole runtime to run what is about 300 lines of logic. The only thing it would update separately is yt-dlp, and the standalone binary updates itself anyway.
- Do the port as a straight translation. Validate it by writing this bench's `data/pred/<name>.json` format from Rust and running `bench/evaluate.py` on it. Accept it when it matches the 0.09 s word median.
- Japanese romanization parity is the one thing to watch.

Pipeline in the app:
1. `UPCOMING_TRACKS` arrives.
2. For each id that is not cached and not playing: fetch lyrics text (existing sources), run yt-dlp, decode, align at 2–4 threads at low priority, apply the reject rule, and cache by `videoId` forever.
3. On play, a cache hit replaces the line timing and adds `words`.

## Output format

**The frontend already supports it:** `src/lib/types.ts` defines

```ts
interface LyricWord { time: number; text: string; duration?: number }
interface LyricLine { time: number; text: string; duration?: number | null; words?: LyricWord[] }
```

`tokenize` / `wordKnots` in `src/lib/timeline.ts` find each word in the line text with `indexOf`, so per-character CJK words work without spaces. The overlay renders them when `wordHighlight` is on.

- **Proposal: use that shape, not a new `{start, end, text}`.** `time` = onset; `duration` = end − onset. The bench emits `start`/`end` only internally.
- **Rust side** (`src-tauri/src/types.rs`): add `#[serde(default, skip_serializing_if = "Option::is_none")] pub words: Option<Vec<LyricWord>>` to `LyricLine`. Existing payloads and caches deserialize unchanged.
- `translation/types.rs` has its own `LyricLine` and can ignore `words`.
- Add a `LyricsSource` value such as `"aligned"` so the UI can tell aligned timing from fetched timing.
- **Word ends:** since the references themselves are about 88% gapless fill, the overlay should treat `duration` as "until the next word" for the sweep and not promise a vocal offset.

## Risks

- **YouTube ToS.** Downloading YouTube audio with yt-dlp breaches YouTube's Terms of Service, even for a transient, local, never-redistributed copy. This is the biggest risk and needs your call before shipping. Mitigations: opt-in, off by default; delete the audio right after alignment and keep only timings; nothing leaves the machine. It still is what it is.
- **yt-dlp breakage.** YouTube changes its player or PO-token rules every few weeks. Expect failures until the next `yt-dlp -U`, and occasionally days with no fix. The fallback is today's behaviour, so a failure costs only the feature.
- **Third-party lyric sources** (QQ/Kugou/NetEase) are unofficial, as in the timing study. The aligner only needs plain text, so LRCLIB plain lyrics are enough input. The word-timed sources were needed only for *evaluation*.
- **CPU and battery.** About 50 CPU-seconds × 8 threads per track. That is fine on desktop. On laptops, use 2 threads and only prefetch on AC power.
- **Disk.** 356 MB model. Cached timings are tiny; audio is not kept.
- **Version mismatch is back in video mode:** OMV ids, as noted above.

## What I could not measure

- **Rust `ort` memory and speed.** The numbers are ONNX Runtime from Python. Inference should be identical; RSS should be lower. Not measured.
- **Thread scaling at 4/16 threads with the 15 + 3 config**, and a clean thread sweep in general. The machine was shared, so 2–4 thread numbers vary by ±30%.
- **Korean word level against a second independent reference.** NetEase had no Korean YRC for these tracks, and Kugou ko is offset, so ko word accuracy rests on QQ alone.
- **Chinese against Kugou:** only 1 track (小幸運) had KRC.
- **Logged-in or Premium YTM sessions**, queue editing (drag and drop, "play next"), and radio; only logged-out automix was verified.
- **Long-term yt-dlp failure rate.** 20/20 downloads succeeded on one day; that says nothing about next month.
- **Whether the residual 0.08–0.1 s error is the aligner or the references.** That is the reference noise floor, and nothing in this data can resolve it.
