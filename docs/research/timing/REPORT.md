# Timing for plain lyrics: research report

Branch `rebuild/timing`, 2026-11-02. Benchmark code and how to run it: [`research/timing/`](../../../research/timing/README.md).

## Decisions needed from you (read this first)

No method is both accurate and cheap to ship, so I did not integrate an aligner. There are two good options, and each needs a decision from you.

1. **Third-party synced sources (QQ Music / NetEase / Kugou).** These are the most accurate option without audio: median error 0.39–0.51 s, the same as the disagreement between human-made LRC files. They cover 47% (14/30) of the tracks where LRCLIB has only plain lyrics. Download is zero and latency about 2 s. **Risk:** they are undocumented, unofficial endpoints. Their terms do not permit this use (NetEase's best-known API wrapper was archived "to protect copyright"), and they can break or be blocked at any time. *Decision: ship them as an opt-in "extra sources" setting, or not at all?* Not integrated.
2. **Audio forced alignment (MMS wav2vec2 CTC).** This is the best timing when the audio matches: median 0.13 s and 72% of lines within 0.3 s once the version offset is removed. It works for every language in the set. It needs **both** of these:
   - an audio path. Either a Chrome permission change (`tabCapture` + `offscreen`, which re-prompts every user and needs one click per session), or WASAPI loopback (`cpal`), which picks up all system audio.
   - a **320 MB** int8 model (1.26 GB fp32), well over the ~200 MB budget, plus about 45–55 CPU-seconds per track.

   On a first listen it can only follow the song, about 1–2 s behind each line. It is exact on replays. *Decision: is a one-time 320 MB optional download plus capture permission acceptable?* Prototyped, not integrated.

**What I did integrate:** ToS-clean, zero-download changes to the plain-lyrics path only.
- A plain-only LRCLIB hit now looks for a synced LRCLIB entry of the same recording (length within 3 s).
- Failing that, lines are spread over the track by syllables instead of 5 s per line. Mean error drops from 39.3 s to 10.9 s. That is still useless for highlighting, so the UI should treat these lines as unsynced.
- Estimated lines are no longer cached, so a later play can pick up real timing.

## Problem and constraints (short)

`fetch_lyrics` returns `LyricLine[]`. When LRCLIB has only `plainLyrics`, the old code used 5 s per line (`lyrics.rs`) or a length-weighted spread (`src/services/textAlign.ts`). The app has no audio. Music plays in a browser tab and the extension reports only title, artist, duration and currentTime. Users run consumer Windows machines with no Python or CUDA. The track mix is heavy on Japanese and Chinese, with English and Korean.

## Benchmark

**Ground truth:** 27 LRCLIB entries with both `syncedLyrics` and `plainLyrics`, pinned by id: 8 en, 8 ja, 6 zh, 5 ko, 1,254 lines. The set includes long intros (Hotel California, Stairway, 晴天, 稻香), fast rap (Lose Yourself, アイドル), slow ballads and tempo changes (Bohemian Rhapsody).

I filtered credit lines and the `^translation` suffix out of the truth. Two LRCLIB search hits had to be pinned by hand: Lemon's top hit had Vietnamese translations interleaved, and Marigold's was a live take. **LRCLIB's own synced data is noisy.**

**Inputs:** each method gets the line texts and the track length and returns one start time per line.

**Audio:** for audio methods, a YouTube upload within 3 s of the LRCLIB length, fetched with yt-dlp and stored as 16 kHz mono. This is only for research; see the ToS notes below.

**Machine:** 16 logical cores, CPU only (8 threads used), 16 GB RAM with about 1.5 GB free during runs.

**Metrics:**
- Absolute line-start error, pooled over all lines of the covered tracks.
- Percentage of all lines within 0.3, 0.5 and 1.0 s.
- Coverage: the share of tracks with any result.
- Median time per track.
- Download size: the shippable artifact, e.g. a whisper.cpp q5_1 file.
- Peak RSS of the Python prototype.

**Noise floor.** Independent human LRCs of the same song disagree by a **median of 0.13–0.51 s**. Only 32–65% of lines fall within 0.3 s of each other (`bench/agreement.py`). So "≤0.3 s against LRCLIB" can't get much above about 60% for any method. Read differences below 0.3 s as noise.

**Oracle offset.** Some of the downloaded audio is a different master or upload from the one the LRCLIB timing was made against:
- Lemon is shifted +1.9 s, 後來 +5.7 s and アイドル +34 s (the upload is a different cut).
- Six tracks are off by more than 1 s.

`--oracle-offset` subtracts each track's median signed error, a single constant. That isolates aligner error from version mismatch. It uses ground truth, so treat it as a diagnostic.
- When Lyryc aligns the stream the user is actually hearing, there is no version mismatch, so oracle-offset approximates that case.
- When Lyryc pre-fetches "the same song" from YouTube, the raw number is the realistic one.

### Results: all 27 tracks (raw)

| method | coverage | mean err (s) | median (s) | ≤0.3s | ≤0.5s | ≤1.0s | time/track (s) | download (MB) | peak RAM (MB) |
|---|---|---|---|---|---|---|---|---|---|
| h0 5 s/line (current Rust) | 100% | 39.29 | 25.48 | 1% | 1% | 2% | 0 | 0 | – |
| h1 textAlign.ts (current TS) | 100% | 14.01 | 9.97 | 2% | 3% | 5% | 0 | 0 | – |
| h2 syllable spread (**integrated**) | 100% | 10.87 | 7.89 | 2% | 3% | 7% | 0 | 0 | – |
| s_netease | 89% | 1.10 | 0.39 | 41% | 57% | 75% | 2.0 | 0 | – |
| s_qq | 93% | 1.03 | 0.40 | 44% | 57% | 76% | 2.5 | 0 | – |
| s_kugou (KRC, word-level) | 81% | 1.46 | 0.51 | 38% | 50% | 70% | 1.6 | 0 | – |
| a_mms_fa (fp32) | 100% | 2.70 | 0.49 | 37% | 50% | 69% | 55 | 1260 | 1996 |
| a_mms_fa_int8 | 100% | 3.07 | 0.52 | 36% | 49% | 67% | 43 | 320 | 2266* |
| a_whisper_base + text match | 100% | 12.81 | 0.90 | 21% | 32% | 53% | 30 | 60 | 812 |
| a_whisper_small + text match | 100% | 7.08 | 0.69 | 24% | 38% | 63% | 53 | 190 | 1225 |

\* The prototype loads fp32 and then quantizes, so its RAM peak is fp32's. An int8 ONNX file would load about 0.4 GB of weights; that figure is an estimate I did not measure.

### Results: all 27 tracks (oracle offset, i.e. audio matches the reference)

| method | mean err (s) | median (s) | ≤0.3s | ≤0.5s | ≤1.0s |
|---|---|---|---|---|---|
| h2 syllable spread | 7.71 | 5.34 | 6% | 9% | 15% |
| s_qq | 0.78 | 0.14 | 71% | 77% | 83% |
| s_netease | 0.91 | 0.19 | 64% | 74% | 81% |
| s_kugou | 1.11 | 0.16 | 66% | 73% | 80% |
| **a_mms_fa** | 1.79 | **0.13** | **72%** | **80%** | **84%** |
| a_mms_fa_int8 | 2.15 | 0.14 | 69% | 78% | 82% |
| a_whisper_base | 8.81 | 0.58 | 35% | 47% | 58% |
| a_whisper_small | 5.36 | 0.33 | 48% | 61% | 74% |

The sources gain from the offset correction too. Human LRCs also differ by a constant lead-in convention.

### By language (raw, ≤1.0 s / median)

| method | en (471 lines) | ja (309) | zh (253) | ko (221) |
|---|---|---|---|---|
| h2 syllable spread | 6% / 9.05 | 3% / 9.68 | 4% / 7.55 | 15% / 3.39 |
| s_qq | 79% / 0.39 (88% cov) | 74% / 0.55 (88%) | 81% / 0.18 | 71% / 0.41 |
| s_netease | 72% / 0.44 | 89% / 0.30 (88%) | 83% / 0.32 (83%) | 51% / 0.60 (80%) |
| a_mms_fa | 77% / 0.42 | 53% / 0.96 † | 68% / 0.43 † | 76% / 0.32 |
| a_whisper_small | 66% / 0.59 | 54% / 0.93 | 63% / 0.71 | 69% / 0.47 |

† Japanese and Chinese raw numbers are dominated by version-mismatched uploads (アイドル, Lemon, First Love, 後來, 小幸運). With the offset removed, MMS's per-track median is 0.05–0.21 s on 13 of those 14 tracks. アイドル is the exception, a different cut.

### Vocal separation (htdemucs, 8 hard tracks: 3, 5, 6, 7, 13, 16, 22, 25)

| method | median raw | ≤1.0s raw | median oracle | ≤1.0s oracle | extra CPU |
|---|---|---|---|---|---|
| a_mms_fa, full mix | 0.54 | 68% | 0.25 | 71% | – |
| a_mms_fa, Demucs vocals | 0.60 | 70% | 0.22 | 72% | +65–180 s/track (≈0.45× realtime), 84 MB model |

This is a marginal gain for the cost. One caveat: the stems were separated from 16 kHz audio upsampled to 44.1 kHz, so this understates what Demucs does on the real stream. **Not worth it as a default.**

### Coverage on the real failure set (plain-only LRCLIB entries)

The table above uses tracks where LRCLIB already has timing. The real question is what happens when it doesn't. I sampled 30 plain-only LRCLIB entries with generic search terms, which skews towards the long tail (`bench/plain_only_coverage.py`).

| | n | other LRCLIB entry synced | netease | qq | kugou | any |
|---|---|---|---|---|---|---|
| all | 30 | 7% | 33% | 33% | 20% | **47%** |
| en | 12 | 0% | 25% | 8% | 8% | 25% |
| ja | 11 | 9% | 36% | 45% | 36% | 55% |
| ko | 5 | 20% | 40% | 60% | 0% | 80% |
| zh | 2 | 0% | 50% | 50% | 50% | 50% |

Accuracy can't be measured on this set. Coverage there means "text matches ≥50% of lines and the length is within 3 s".

## Methods tried

**Heuristics (h0–h2), no audio.**
- h0 (5 s/line) is off by 39 s on average.
- h1 (the textAlign.ts port) is off by 14 s.
- h2 skips an 8% intro and outro and weights lines by syllables (CJK/kana/hangul characters, Latin vowel groups). It is untuned and off by 10.9 s.

No audio-free heuristic can know where the intro ends or where instrumental breaks fall, and only 2–7% of lines land within 1 s. **Heuristic timing should never drive karaoke highlighting.** It is only good for a slow auto-scroll.

**Third-party synced sources.**
- They return their own LRC. I map it onto the LRCLIB line list with a monotonic bigram-similarity DP (`bench/align_text.py`) and interpolate unmatched lines.
- Candidates are chosen by length (±3 s) first, then title/artist similarity with a cover/karaoke penalty.
- Results: QQ has 93% coverage and 0.40 s median, NetEase 89% and 0.39 s, Kugou 81% and 0.51 s. Kugou's KRC is word-level after XOR + zlib decoding.
- Their median error equals the human-vs-human noise floor. Only 0–9 lines per track carry the identical timestamp, so LRCLIB is mostly not a copy of these sources.
- Weaknesses:
  - Traditional-vs-simplified metadata hurts Kugou's search (晴天 / 後來 found nothing).
  - Credit blocks have to be filtered.
  - Covers rank above originals unless penalised.
- ToS and stability risk: high. All three are reverse-engineered endpoints (Lyricify, syncedlyrics and LyricsX use the same ones), with no published terms for this use. NetEase's popular API wrapper was archived "to protect copyright" [1].

**MMS CTC forced alignment.**
- Model: the MMS forced-alignment wav2vec2 checkpoint (300M parameters, 1,130 languages, uroman character vocabulary [2][3]), loaded from its Hugging Face mirror [4]. Alignment uses torchaudio `forced_align` with a `<star>` wildcard token before each line, so intros, ad-libs and breaks that aren't in the text get absorbed.
- Romanization: Japanese goes through pykakasi first, because uroman reads kanji as Mandarin. In Lyryc this would reuse the readings the furigana module already computes. Other scripts go through uroman.
- Results: raw median 0.49 s. With the offset removed it is 0.13 s with 72% of lines ≤0.3 s, which beats the human noise floor. That matches published lyrics aligners (0.15–0.22 s AAE [5][6]).
- Speed: about 55 s per 4-minute track on 8 CPU threads, 2 GB RAM in Python.
- int8 dynamic quantization costs about 1–3 points and cuts the shippable file to about 320 MB.
- Failures: Billie Jean (29 of 71 lines off by more than 5 s) and How You Like That. Both have long, highly repetitive choruses, where the star token lets whole repeats slide.
- Shipping cost: there is no official ONNX export, so we'd export it ourselves. The `ort` crate is still a release candidate [7]. `uroman-rs` exists [8]. For Japanese romanization the app already has kana readings from furigana.

**Whisper word timestamps + text matching.**
- Method: faster-whisper int8 with the language hint and the first four lyric lines as the prompt. The transcript and the lyrics are matched as romanized character streams (difflib), and unmatched lines are interpolated.
- Results:
  - small (190 MB as whisper.cpp q5_1 [9]): median 0.69 s raw and 0.33 s oracle.
  - base (60 MB): 0.90 s and 0.58 s.
- It is bimodal. About 70% of tracks land at 0.2–0.3 s; the rest fail completely, because Whisper hallucinates or loops on music. Some tracks have 46 of 50 lines off by more than 1 min. 後來 took 35 minutes with base because of a repetition loop.
- Matching the known text after transcription is the wrong tool: a forced aligner can't hallucinate. whisper.cpp's `--dtw` token timestamps [10] don't fix the recognition failures.
- Not recommended.

**Not run.**
- stable-ts `align()` and WhisperX. Both are Whisper plus wav2vec2, already covered by the two families above, and both are Python-only.
- Musixmatch richsync, Spotify color-lyrics and Apple TTML. All three need a user session token (`sp_dc` or `media-user-token`) and are forbidden by the services' terms [11][12][13], so I chose not to scrape them.
- YouTube Music timed lyrics. youtubei.js `getLyrics()` returns plain text only [14]. ytmusicapi has `timestamps=True` [15], but that is the same ToS class as above.
- MDX/RoFormer separation, given Demucs' marginal result.
- dl.fbaipublicfiles.com is unreachable from this machine, so torchaudio's own MMS_FA bundle and Demucs weights came from Hugging Face mirrors of the same checkpoints.

## Recommendation

**Tiered and honest.** Real timing from the most legitimate source first, audio alignment only as an opt-in, and never present guessed timing as if it were synced.

1. **LRCLIB, then LRCLIB same-recording synced alternative.** Integrated. Free and clean, but adds little: 7% of plain-only tracks.
2. **Optional "extra lyric sources" setting**, if you accept the ToS risk: QQ → NetEase → Kugou, length-matched and mapped onto the LRCLIB text. This doubles synced coverage of the long tail at no download cost. It belongs in its own module with a kill switch, because these endpoints break.
3. **Optional "align from audio" setting**, if you accept the 320 MB download and the capture permission: MMS int8 ONNX in-process via `ort`, fed by the extension's tab capture.
   - On first listen it follows the song.
   - It caches a full alignment for replays.
   - Skip vocal separation.
   - Avoid pre-fetching YouTube audio. It breaks YouTube's ToS, and it brings in version mismatch: in our data 6 of 27 duration-matched uploads were off by more than 1 s, and one by 34 s.
4. **Last resort:** syllable estimate, flagged `estimated`, shown as unsynced. Integrated; the flag still needs a contract change, see below.

Trade-offs:

| | accuracy (median) | first-listen latency | download | ToS/legal | maintenance |
|---|---|---|---|---|---|
| LRCLIB alt | exact LRC | ~0.3 s | 0 | clean (documented API) | none |
| QQ/NetEase/Kugou | 0.4 s | ~2 s | 0 | high | high (endpoints change, anti-bot) |
| MMS align, live stream | ~0.13 s* | follows, ~1–2 s behind | 320 MB + onnxruntime DLL (official zip 83 MB; DLL size not measured) | low (user's own playback) | medium (ONNX export, ort rc) |
| MMS align, YouTube prefetch | 0.49 s | before playback (~45 s) | 320 MB | high (YouTube ToS) | high |
| Whisper small | 0.69 s, bimodal | follows | 190 MB | low | medium |
| heuristic | 8 s | instant | 0 | clean | none |

\* Oracle-offset number; the live stream has no version mismatch.

## Integration design (for when option 2 or 3 is approved)

**Who runs what.** Everything stays in the Tauri process, with no sidecar and no Python.
- The `timing` module owns a background worker: a `tokio::task::spawn_blocking` pool of size 1.
- The aligner sits behind a trait: `trait Aligner { fn align(&self, pcm: &[f32], lines: &[String], lang: Lang) -> Vec<Option<f64>> }`.
- MMS goes through `ort` with `load-dynamic`, with `onnxruntime.dll` shipped next to the exe. The model is downloaded on first opt-in into `%LOCALAPPDATA%\Lyryc\models\mms-fa-int8.onnx`, with its SHA-256 pinned.

**Where the audio comes from.**
- Preferred: the extension prototype in [`extension/prototype-tabcapture/`](../../../extension/prototype-tabcapture/README.md). An offscreen document captures the tab and sends 16 kHz mono PCM16 in 1 s frames over the existing `ws://localhost:8765`. Each frame has an `audio_chunk` header carrying `capturedAt`. The app maps wall time to track position using the `currentTime` updates it already gets.
  - Requires `tabCapture` + `offscreen` permissions and a user click on the action [16].
  - Not wired into `manifest.json` on this branch.
- Alternative with no extension change: `cpal` WASAPI loopback [17]. It captures everything the system plays (notifications, calls), so it needs a clear UI toggle.

**Streaming vs align-once.**
- Buffer PCM per track, keyed by track id plus position, and drop it on seek or track change.
- Every ~5 s, run emissions on the new audio only (wav2vec2 conv/attention over 30 s chunks; emissions concatenate). Re-run the Viterbi over everything heard so far against the full line list. That is cheap: about 0.2× realtime on this CPU.
- A line is "confirmed" once the alignment has moved two lines past it. Push confirmed lines as they settle.
- At track end, with at least 90% of the audio heard, run the final full alignment and cache it.
- A first listen can't be ahead of the music without pre-fetching, and pre-fetching is rejected above.

**Cache format.**
- Additive columns on `lyrics_cache`: `timing_source TEXT` (`lrclib` | `lrclib_alt` | `qq` | `netease` | `kugou` | `aligned:mms-int8@1` | `estimated`), `timing_confidence REAL`, `audio_duration REAL`.
- Rule: never let a lower-ranked source overwrite a higher one.
- Estimated timing is not cached; this is already true on this branch.
- Cache aligned results under (artist, track, duration rounded to the nearest second) so a different master doesn't reuse the wrong timing.

**UI while timing is pending.** For the frontend stream:
- `estimated`: show the full lyric list as an unsynced, slowly auto-scrolling sheet (h2 times are fine for scroll position), with no current-line highlight and no word sweep. A small "timing…" badge.
- `aligning`: highlight confirmed lines only. Keep the next unconfirmed line dimmed "up next".
- `synced` / `aligned`: normal karaoke.

**Pushing improved lines.** `fetch_lyrics` keeps its signature. When better timing appears, the backend emits an additive event:

```ts
// event "lyrics-timing-updated"
{ track: { title, artist }, lines: LyricLine[], source: "lrclib_alt" | "qq" | "aligned" | ..., final: boolean }
```

The contract already says lines may be swapped mid-song. Line count and text stay identical, because alignment maps onto the same plain lines, so the UI can update times in place without re-rendering. **Contract change to agree on:** a way for `fetch_lyrics` callers to know the result is `estimated`. Without a `LyricLine` change that is impossible today. Options: (a) add the `timing_source` to the event above and have the backend always emit it once after `fetch_lyrics`, or (b) add an optional field to `LyricLine` (`types.rs`, shared). I'd take (a). It is purely additive and needs no change to `LyricLine`.

## What changed on this branch

- `src-tauri/src/timing/mod.rs` (new): `estimate_plain` (syllable spread), `pick_synced_alternative` and `find_synced_alternative` (LRCLIB same-recording lookup), plus unit tests.
- `src-tauri/src/lyrics.rs`, plain path only:
  - A plain-only winner carries its LRCLIB track, artist and duration.
  - `finish_lyrics` tries the synced alternative, otherwise returns the estimate, and does not cache estimates.
  - The dead `convert_plain_lyrics_to_lines` is removed.
  - `execute_staged_search` returns the full result.
- `src-tauri/src/lib.rs`: `mod timing;` (additive).
- `extension/prototype-tabcapture/`: capture prototype, not referenced by the manifest.
- `research/timing/`: dataset pins, scripts, per-line predictions (`results/*.json`), `results/TABLES.md`.
- `alignment/` (the old whisperx stub) is untouched. It was never implemented and this work supersedes it, so I'd delete it.

Behaviour change to be aware of:
- Plain-only tracks are no longer cached. Each play repeats the LRCLIB search plus at most one extra `/api/search` request (LRCLIB allows about 90 requests/min [18]).
- Their line times now start after an estimated intro instead of at 0 s.

## Sources

1. NeteaseCloudMusicApi (archived 2024-04-16): https://github.com/Binaryify/NeteaseCloudMusicApi. Endpoint survey in Lyricify Lyrics Helper: https://github.com/WXRIW/Lyricify-Lyrics-Helper, and syncedlyrics: https://github.com/moehmeni/syncedlyrics
2. Pratap et al., *Scaling Speech Technology to 1,000+ Languages* (MMS), 2023: https://arxiv.org/abs/2305.13516. torchaudio MMS_FA: https://docs.pytorch.org/audio/main/generated/torchaudio.pipelines.MMS_FA.html. forced_align kept in torchaudio 2.9+: https://github.com/pytorch/audio/issues/3902
3. uroman: https://github.com/isi-nlp/uroman
4. MMS forced-aligner weights (HF): https://huggingface.co/MahmoudAshraf/mms-300m-1130-forced-aligner. ctc-forced-aligner: https://github.com/MahmoudAshraf97/ctc-forced-aligner
5. Gupta, Yılmaz, Li, *Automatic lyrics alignment and transcription in polyphonic music*, ICASSP 2020 (0.18–0.22 s AE): https://arxiv.org/abs/1909.10200. Stoller et al., ICASSP 2019 (0.35 s): https://arxiv.org/abs/1902.06797
6. Durand et al., *Contrastive learning-based audio to lyrics alignment*, ICASSP 2023 (0.15 s AAE on Jamendo): https://arxiv.org/abs/2306.07744. Jam-ALT benchmark: https://arxiv.org/abs/2311.13987. DALI: https://arxiv.org/abs/1906.10606
7. `ort` (ONNX Runtime for Rust, 2.0.0-rc): https://github.com/pykeio/ort, https://ort.pyke.io/setup/linking
8. uroman-rs: https://github.com/o24s/uroman-rs
9. whisper.cpp models and sizes: https://github.com/ggml-org/whisper.cpp, https://huggingface.co/ggerganov/whisper.cpp. faster-whisper: https://github.com/SYSTRAN/faster-whisper
10. whisper.cpp DTW token timestamps (PR #1485): https://github.com/ggml-org/whisper.cpp/pull/1485
11. Spotify User Guidelines: https://www.spotify.com/us/legal/user-guidelines/. color-lyrics wrapper ("probably against Spotify TOS"): https://github.com/akashrchandran/spotify-lyrics-api
12. Apple Media Services terms: https://www.apple.com/legal/internet-services/itunes/us/terms.html
13. Musixmatch endpoints used by syncedlyrics (`track.richsync.get`): https://github.com/moehmeni/syncedlyrics
14. youtubei.js Music client: https://ytjs.dev/api/namespaces/Clients/classes/Music
15. ytmusicapi `get_lyrics(timestamps=True)`: https://ytmusicapi.readthedocs.io/en/stable/reference/browsing.html
16. chrome.tabCapture (MV3, offscreen, user gesture): https://developer.chrome.com/docs/extensions/reference/api/tabCapture
17. cpal WASAPI loopback: https://github.com/RustAudio/cpal/issues/251
18. LRCLIB API docs: https://lrclib.net/docs.md. Demucs (archived 2025): https://github.com/facebookresearch/demucs
