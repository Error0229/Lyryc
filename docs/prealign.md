# Word-level timing for YouTube Music (pre-align)

Opt-in and experimental. Off by default. Study and numbers:
[`docs/research/prealign/REPORT.md`](research/prealign/REPORT.md). Code: `src-tauri/src/prealign/`.

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
   - The MMS CTC model aligns the text: int8 ONNX on ONNX Runtime, CPU, 15 s
     chunks + 3 s context, min(8, physical cores) threads, no CPU memory arena.
     Japanese lines are read with UniDic first; the dictionary is dropped
     before the model runs.
4. The result goes through the report's reject rule:
   - Drop the whole track if more than 60% of its lines have confidence below 0.15.
   - Drop a line's words if its 3-line median confidence is below 0.10, or if it
     is more than 0.7 s off the source's real line timing (after removing the
     track's median offset).
   - A rejected line keeps the source's line time, or the aligner's start when
     the source had only an estimate.
5. The raw alignment is cached in `lyrics_cache.db`, table `aligned_cache`, keyed
   by videoId + SHA-256 of the lyric text. A whole-track reject is cached as NULL
   so the song is never downloaded again.
6. `fetch_lyrics_timed` serves a cache hit as `source: "aligned"`, with `words`
   on each kept line. When the playing track finishes aligning later, the app
   pushes `lyrics-timing-updated`.

Any failure (yt-dlp, model, decode, length mismatch, low confidence) leaves
today's timing in place and logs at debug level (`RUST_LOG=lyryc_lib=debug`).

## Downloads (on first enable, into `%LOCALAPPDATA%\<app id>\prealign\`)

| file | source | size | check |
|---|---|---|---|
| `mms_fa.int8.onnx` | `MODEL_URL` in `assets.rs` (Hugging Face, commit-pinned) | 356 MB | SHA-256 pinned, re-verified once per session |
| `onnxruntime.dll` | Microsoft's `onnxruntime-win-x64-1.22.0.zip` (GitHub release) | 72 MB download, 12 MB kept | SHA-256 of the zip pinned |
| `yt-dlp.exe` | `github.com/yt-dlp/yt-dlp/releases/latest` | ~18 MB | release `SHA2-256SUMS` |

- `yt-dlp -U` runs at most once a day, before a download.
- "Remove downloaded files" in Settings deletes the folder and turns the
  feature off. Cached timings stay: they are tiny and hold no audio.
- Windows only for now: the runtime download is wired for Windows.
- ONNX Runtime is loaded at run time (`ort` `load-dynamic`), so the app binary
  carries none of it. Static linking also fails here: the prebuilt static
  library needs a newer MSVC STL than VS 17.10.

### Which model file, and how to use your own

The model is the MMS forced aligner (`MahmoudAshraf/mms-300m-1130-forced-aligner`)
exported to ONNX with **MatMul-only dynamic int8 and float convolutions**, the
study's recipe. Two exports were measured against the study's own on the same
track, machine and settings (How You Like That, 8 threads, 15 + 3 s):

| file | size | RTF | word median (QQ) |
|---|---|---|---|
| study's `export_onnx.py` output | 356 MB | 0.173 | 0.053 s |
| `onnx-community/…-ONNX` `onnx/model_int8.onnx` | 317 MB | **0.621** | 0.05 s |
| `romara-labs/…-ONNX` `model.q8.onnx` (default) | 356 MB | 0.24–0.31* | 0.05 s |

\* Measured while other work shared the machine. Its graph is op-for-op the
study's: Conv, LayerNormalization and MatMulInteger, plus an extra
`attention_mask` input, which the app feeds as all ones.

The Hugging Face-run `onnx-community` int8 quantizes the conv layers to
`ConvInteger`, which ONNX Runtime runs slowly on CPU. So the default is the
third-party export with the study's recipe, pinned by commit and SHA-256. To
use your own file instead:

```sh
cd research/prealign
uv sync
uv run python -X utf8 bench/export_onnx.py      # -> models/mms_fa.int8.onnx
```

Host or copy that file somewhere and set both variables before starting the app:

```
LYRYC_PREALIGN_MODEL_URL=https://…/mms_fa.int8.onnx      (file:// is not supported; any https URL)
LYRYC_PREALIGN_MODEL_SHA256=<sha256 of the file>
```

To skip the download entirely, put the file at `prealign\mms_fa.int8.onnx` in
the app data dir. Its hash is still checked against the variables, or against
the pinned default if they are unset.

## Measured (2026-11-02, this machine: Windows 11, 16 logical cores)

Accuracy: word / line onset median vs QQ, scored with `bench/evaluate.py`'s matcher.

| track | lang | Python prod line / word (s) | Rust, study WAV | Δ word | Rust, yt-dlp 140 + symphonia | Δ word |
|---|---|---|---|---|---|---|
| Shape of You | en | 0.091 / 0.096 | 0.079 / 0.093 | −0.003 | 0.107 / 0.104 | +0.008 |
| bad guy | en | 0.178 / 0.126 | 0.178 / 0.126 | +0.001 | – | – |
| アイドル | ja | 0.060 / 0.062 | 0.060 / 0.060 | −0.002 | 0.062 / 0.063 | +0.000 |
| Lemon | ja | 0.051 / 0.061 | 0.053 / 0.060 | −0.002 | – | – |
| 晴天 | zh | 0.104 / 0.057 | 0.103 / 0.056 | −0.002 | – | – |
| 告白氣球 | zh | 0.052 / 0.065 | 0.048 / 0.061 | −0.004 | 0.053 / 0.065 | +0.000 |
| 밤편지 | ko | 0.105 / 0.098 | 0.097 / 0.096 | −0.002 | 0.103 / 0.096 | −0.002 |
| How You Like That | ko | 0.076 / 0.053 | 0.078 / 0.054 | +0.001 | 0.081 / 0.073 | +0.020 |
| pooled word median | | 0.070 (8 tracks) | 0.067 | | 0.073 (5 tracks; Python 0.069) | |

The study's WAVs came from opus + ffmpeg; the app path is AAC format 140. YouTube's
m4a carries a 1600-sample (36 ms) edit-list priming that symphonia 0.5 does not
apply. Untrimmed, every onset was +0.041 s late; `audio.rs` now trims it.

Speed and memory, per track (release build, 8 threads):

| step | value |
|---|---|
| yt-dlp download, format 140 (3–4 MB) | 10–14 s (one outlier 21 s) |
| decode + resample to 16 kHz | 0.4–0.9 s |
| alignment, no CPU arena (shipped) | RTF 0.23–0.25 (42–63 s per track) |
| alignment, default arena | RTF 0.16–0.17 |
| model load | 1.7–2.1 s |
| peak RSS, shipped config | 0.80 GB (en/zh/ko), 1.06–1.08 GB (ja, UniDic) |
| peak RSS, default arena | 1.69–1.88 GB |
| resident between jobs | ~0.40 GB, ~0.66 GB after a Japanese song; the model (0.38 GB) is dropped when the queue is idle |

## Parity bench

The Rust aligner is checked against the Python study with the study's own scorer:

```sh
# 1. The study's aligner input for some tracks (needs research/prealign/data from the study).
uv run python -X utf8 bench/rust_inputs.py . data/inputs.json 0,4,5,7,10,11,15,16
# 2. Align in Rust: from the study's WAVs, or from fresh yt-dlp downloads.
PREALIGN_DIR=<scratch dir> PREALIGN_INPUTS=data/inputs.json PREALIGN_OUT=data/pred/rust.json \
PREALIGN_AUDIO=ytdlp   # or wav:<research/prealign/data/audio>
cargo test --release --lib prealign::bench -- --ignored --nocapture
# 3. Score.
uv run python -X utf8 bench/evaluate.py rust --tracks
```

`bench/rust_inputs.py` writes `{id: {text_src, lines, videoId, duration}}` from
`bench/run.py`'s `source_text`.
