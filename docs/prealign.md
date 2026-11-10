# Word-level timing for YouTube Music (pre-align)

Opt-in and experimental. Off by default. Study and numbers:
[`docs/research/prealign/REPORT.md`](research/prealign/REPORT.md), and for the
model in use, [`ALIGNERS.md`](research/prealign/ALIGNERS.md). Code: `src-tauri/src/prealign/`.

## What it does

1. The extension's queue reader (`extension/youtube-queue.js`) reports the next
   three YouTube Music videoIds that will play (`UPCOMING_TRACKS`). The playing
   videoId comes with every track message (the page URL).
2. One background worker takes one song at a time: the playing track first (it
   preempts a lookahead job), then the queue in play order, never more than
   three ahead.
3. For each song:
   - Lyrics come through the normal lookup: cache, then YouTube Music's own
     lyrics for that videoId when "Extra lyric sources" is on, then LRCLIB
     within its request cap and etiquette.
   - `yt-dlp.exe -f 140` downloads that exact video's AAC audio.
   - symphonia decodes it and the app resamples to 16 kHz mono. The file is
     deleted right after decoding.
   - echora-kara-300m aligns the text: int8 ONNX on ONNX Runtime, CPU,
     min(8, physical cores) threads, no CPU memory arena. The encoder runs in
     15 s chunks + 3 s context, the text head in 25 s windows, and our decoder
     (`decode.rs`, from the clean-room spec
     [`echora-spec.md`](research/prealign/echora-spec.md)) places one token per
     word or CJK character. Japanese lines are read with UniDic first; the
     dictionary is dropped before the model runs.
4. The result goes through the report's reject rule:
   - Drop the whole track if more than 50% of its lines have confidence below
     0.20 (recalibrated for echora; MMS used 60% below 0.15, see ALIGNERS.md).
   - Drop a line's words if its 3-line median confidence is below 0.10, or if it
     is more than 0.7 s off the source's real line timing (after removing the
     track's median offset).
   - A rejected line keeps the source's line time, or the aligner's start when
     the source had only an estimate.
5. The raw alignment is cached in `lyrics_cache.db`, table `aligned_cache`, keyed
   by videoId + SHA-256 of the aligner's name and the lyric text, so an
   alignment from another model (MMS, before) is never served. A whole-track
   reject is cached as NULL so the song is never downloaded again.
6. `fetch_lyrics_timed` serves a cache hit as `source: "aligned"`, with `words`
   on each kept line. When the playing track finishes aligning later, the app
   pushes `lyrics-timing-updated`.

echora's decoder places every line; it cannot leave out lines the recording
does not sing (MMS's `<star>` path could). A short version or a cut verse is
caught by the track rule when confidence collapses (most simulated cases in
ALIGNERS.md), otherwise its late lines are squeezed, and lines that drift off
LRCLIB timing lose their words.

Any failure (yt-dlp, model, decode, length mismatch, low confidence) leaves
today's timing in place and logs at debug level (`RUST_LOG=lyryc_lib=debug`).

## Timing pasted lyrics (the saved-lyrics editor)

Settings → Lyrics & sync → Saved lyrics → a song → **Paste lyrics** / **Edit as
text** takes lyrics with no times at all and has the same worker time them
(`prealign_align`):

- The audio is the YouTube Music video the song was last seen as. Every
  lookup that comes with a videoId (the playing track's, the lookahead's)
  writes it to the cache's `song_video` table under the song's (artist,
  title) key (`lyrics_cache::remember_video`), and the command reads it from
  there: the editor passes only the song's names. That is the one source: a
  song that never played on YouTube Music has no audio, and the editor says
  to play it there once. With the feature off it says so too, and nothing
  runs.
- The job goes ahead of the queue and stops the job that is running, which
  runs again afterwards. The playing track does not preempt it. A newer ask
  replaces an older one.
- The lines come back with the aligner's line starts (the same `reject::apply`
  as an estimate: there is no source timing to check against) and fill the
  editor's rows, unsaved. A whole-track reject, or any failure, is shown as
  the reason and changes nothing.
- The alignment is cached as usual (videoId + text hash). Once the rows are
  saved with the same words, the song gets its word timing from the cache
  the next time it is served, without aligning again. That holds for a song
  retimed with the words it already had too: a save deletes a song's
  alignments only when its words change (new times can't change what the
  aligner hears).

## Downloads (on first enable, into `%LOCALAPPDATA%\<app id>\prealign\`)

| file | source | size | check |
|---|---|---|---|
| `echora_enc.int8.onnx` | `ENCODER_URL` in `assets.rs` | 356 MB | SHA-256 and size pinned, re-verified once per session |
| `echo_text.int8.onnx` | `TEXT_HEAD_URL` | 23 MB | same |
| `echo_frame.int8.onnx` | `FRAME_HEAD_URL` | 18 MB | same |
| `onnxruntime.dll` | Microsoft's `onnxruntime-win-x64-1.22.0.zip` (GitHub release) | 72 MB download, 12 MB kept | SHA-256 of the zip pinned |
| `yt-dlp.exe` | `github.com/yt-dlp/yt-dlp/releases/latest` | ~18 MB | release `SHA2-256SUMS` |

The three model files are hosted at
[`error0229/echora-kara-300m-onnx`](https://huggingface.co/error0229/echora-kara-300m-onnx)
(CC BY-NC 4.0, credit hcxlabs), pinned to commit `5880efe`. They are built by
`bench/export_onnx.py echora` and `bench/echo_head.py` in `research/prealign/`.

- `yt-dlp -U` runs at most once a day, before a download.
- "Remove downloaded files" in Settings deletes the folder and turns the
  feature off. Cached timings stay: they are tiny and hold no audio.
- On start the app deletes the old MMS model (`mms_fa.int8.onnx`) if an earlier
  version left it in the folder.
- Windows only for now: the runtime download is wired for Windows.
- ONNX Runtime is loaded at run time (`ort` `load-dynamic`), so the app binary
  carries none of it. Static linking also fails here: the prebuilt static
  library needs a newer MSVC STL than VS 17.10.

### The model files, and how to use your own

The model is [`hcxlabs/echora-kara-300m`](https://huggingface.co/hcxlabs/echora-kara-300m)
(CC BY-NC 4.0, snapshot `64087e1`), converted to ONNX: the wav2vec2 encoder
(outputs `logits` [1, T, 30] and `hidden` [1, T, 1024]) and the text head, split
in two so the text side runs once per song. All three are MatMul-only dynamic
int8. echora's own code (AGPL) is not used; the head and decoder are ours,
written from the spec, and the decoder matches echora's token times to the
millisecond on the spec's test vectors (`decode::tests::spec_vectors`).

To make the files:

```sh
cd research/prealign
uv sync
uv run python -X utf8 bench/export_onnx.py echora   # -> models/echora_enc.int8.onnx (needs transformers)
uv run python -X utf8 bench/echo_head.py            # -> models/echo_text.int8.onnx, models/echo_frame.int8.onnx
```

**Put them in the prealign dir** (`%LOCALAPPDATA%\com.lyryc.app\prealign\`,
with those names) to skip the download. Each is still checked against its
pinned SHA-256 before first use, so a file that differs from the pinned build
needs its hash set as below; a file that fails the check is deleted.

**Or host them** and set, per file, both variables before starting the app
(any https URL; `file://` is not supported):

```
LYRYC_PREALIGN_ENCODER_URL=https://…/echora_enc.int8.onnx
LYRYC_PREALIGN_ENCODER_SHA256=<sha256>
LYRYC_PREALIGN_TEXT_HEAD_URL=https://…/echo_text.int8.onnx
LYRYC_PREALIGN_TEXT_HEAD_SHA256=<sha256>
LYRYC_PREALIGN_FRAME_HEAD_URL=https://…/echo_frame.int8.onnx
LYRYC_PREALIGN_FRAME_HEAD_SHA256=<sha256>
```

This is a developer setting, not a supported one. A file whose two variables
are set uses them (its SHA-256 is still checked, its size is not); the others
keep the pinned defaults. The URL is only fetched when the file is
missing, so for a hand-placed file of your own build, set its two variables
with any https URL and its real hash.

## Measured (2026-11-07, this machine: Windows 11, 16 logical cores)

Accuracy: line / word onset median vs QQ, scored with `bench/evaluate.py`'s
matcher. Python is the reference run `run_echo.py --head onnx-int8`
(`echo_clean`); Rust is `prealign::bench::parity`.

| track | lang | Python line / word (s) | Rust, study WAV | Δ line / word | Rust, yt-dlp 140 + symphonia | Δ line / word |
|---|---|---|---|---|---|---|
| Shape of You | en | 0.074 / 0.085 | 0.070 / 0.085 | −0.003 / −0.000 | 0.078 / 0.086 | +0.005 / +0.001 |
| bad guy | en | 0.166 / 0.135 | 0.173 / 0.135 | +0.007 / −0.001 | – | – |
| アイドル | ja | 0.068 / 0.058 | 0.064 / 0.055 | −0.004 / −0.003 | 0.067 / 0.054 | −0.001 / −0.003 |
| Lemon | ja | 0.091 / 0.075 | 0.086 / 0.070 | −0.005 / −0.005 | – | – |
| 晴天 | zh | 0.094 / 0.067 | 0.098 / 0.067 | +0.005 / −0.001 | – | – |
| 告白氣球 | zh | 0.068 / 0.066 | 0.070 / 0.066 | +0.003 / −0.001 | 0.073 / 0.065 | +0.005 / −0.001 |
| 밤편지 | ko | 0.109 / 0.091 | 0.109 / 0.091 | +0.000 / +0.000 | 0.115 / 0.090 | +0.006 / −0.001 |
| How You Like That | ko | 0.072 / 0.053 | 0.076 / 0.052 | +0.004 / −0.000 | 0.076 / 0.052 | +0.004 / −0.001 |
| pooled | | 0.089 / 0.070 (8 tracks) | 0.089 / 0.068 | | 0.077 / 0.064 (5 tracks; Python 0.075 / 0.066) | |

On the study WAVs, 82% of Rust's word onsets equal Python's to the
millisecond and 93% are within one frame (20 ms). The rest are near-ties that
flip on tiny lattice differences (f64 vs f32 normalization, thread order).

The study's WAVs came from opus + ffmpeg; the app path is AAC format 140. YouTube's
m4a carries a 1600-sample (36 ms) edit-list priming that symphonia 0.5 does not
apply; `audio.rs` trims it.

Speed and memory, per track (release build, 8 threads):

| step | value |
|---|---|
| yt-dlp download, format 140 (3–4 MB) | 11–15 s |
| decode + resample to 16 kHz | 0.45–0.75 s |
| alignment, no CPU arena (shipped) | RTF 0.28–0.32 (52–80 s per track); the encoder is nearly all of it, the text head ~2 s, the decoder ~1–2 s |
| alignment, default arena | RTF 0.25 |
| model load (all three sessions) | 1.2–1.4 s |
| peak RSS, shipped config | 1.02–1.03 GB (en/zh/ko), 1.27–1.30 GB (ja, UniDic) |
| peak RSS, default arena | 1.71 GB (en), 2.18 GB (ja) |
| resident between jobs | ~0.45 GB, ~0.72 GB after a Japanese song; all three sessions are dropped when the queue is idle |

MMS, for comparison, ran at RTF 0.23–0.25 with peak RSS 0.80 / 1.07 GB. The
arena would buy 15–20% speed for 0.7–0.9 GB more memory; a background job should
be the light one, so it stays off.

The arena accounts for the whole gap to the Python study (RTF 0.23). The study
ran ONNX Runtime with its default arena, and the Rust encoder with the arena on
matches it (42.5 s vs 40.5 s on a 181 s track). Other options measured
(`Pre-align: encoder …, head …, decode …` in the debug log gives the split):

| change | effect |
|---|---|
| ONNX Runtime 1.31 instead of 1.22 | none (encoder 45.6 s vs 44.2 s, arena on) |
| `onnxruntime.dll` from the PyPI wheel instead of the release zip (both 1.22.0) | encoder ~8% faster; within run-to-run noise of ±10% |
| arena on the encoder only | RTF 0.24–0.27, peak 1.53 GB (en), 1.88 GB (ja); +0.3 GB already at load |
| arena with `session.use_device_allocator_for_initializers` | saves ~80 MB; the arena's memory is activations, not weights |
| arena on, prepacking off | loses the speed, saves ~70 MB |

## Parity bench

The Rust aligner is checked against the Python study with the study's own scorer:

```sh
# 1. The study's aligner input for some tracks (needs research/prealign/data from the study).
uv run python -X utf8 bench/rust_inputs.py . data/inputs.json 0,4,5,7,10,11,15,16
# 2. Align in Rust: from the study's WAVs, or from fresh yt-dlp downloads.
#    PREALIGN_DIR holds the three model files, onnxruntime.dll and yt-dlp.exe
#    (copy them from research/prealign/models/ and the app's prealign dir).
#    Use Windows paths (C:/...): Git Bash does not convert the wav: one.
PREALIGN_DIR=<scratch dir> PREALIGN_INPUTS=data/inputs.json PREALIGN_OUT=data/pred/rust.json \
PREALIGN_AUDIO=ytdlp   # or wav:<research/prealign/data/audio>
cargo test --release --lib prealign::bench::parity -- --ignored --nocapture
# 3. Score against QQ, per track; compare with the Python reference, echo_clean.
uv run python -X utf8 bench/evaluate.py rust --tracks
```

`bench/rust_inputs.py` writes `{id: {text_src, lines, videoId, duration}}` from
`bench/run.py`'s `source_text`.

The decoder is also checked on its own against the spec's test vectors
(`research/prealign/data/spec_vectors`, git-ignored). Given their own inputs,
every token time must match echora's to the millisecond:

```sh
cargo test --lib prealign::decode -- --ignored --nocapture   # 0/748, 0/395, 0/286 mismatches
```
