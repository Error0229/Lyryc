# Lyric timing benchmark

Reproducible benchmark for assigning timestamps to plain lyrics. Findings are in
[`docs/research/timing/REPORT.md`](../../docs/research/timing/REPORT.md).

Ground truth: 27 LRCLIB entries that have both `syncedLyrics` and `plainLyrics`, pinned
by id in `data/tracks.txt` (8 en, 8 ja, 6 zh, 5 ko). Credit lines (`作词 :`, `Lyrics by:`)
and the translation after `^` in bilingual lines are stripped from the truth. Each method
gets the truth's line texts and must return one start time per line.

Lyrics and audio are not committed. The scripts re-download them.

## Requirements

- Python 3.11 and [uv](https://docs.astral.sh/uv/)
- `yt-dlp` and `ffmpeg` on PATH (audio methods only)
- Network: lrclib.net, huggingface.co, and for the source methods music.163.com,
  c.y.qq.com and lyrics.kugou.com

## Run

```sh
cd research/timing
uv sync                                         # CPU-only torch 2.7.1, faster-whisper, demucs
python bench/fetch_dataset.py                   # -> data/dataset.json (rerun if LRCLIB 503s)
python bench/fetch_audio.py                     # -> data/audio/<id>.wav, 16 kHz mono

python methods/heuristics.py                    # h0 (current Rust), h1 (textAlign.ts), h2
python methods/sources.py                       # s_netease, s_qq, s_kugou
uv run python methods/mms_fa.py                 # a_mms_fa        (~1 min/track, 2 GB RAM)
uv run python methods/mms_fa.py --int8          # a_mms_fa_int8
uv run python methods/whisper_match.py --model base   # a_whisper_base
uv run python methods/whisper_match.py --model small  # a_whisper_small
uv run python bench/separate.py --ids 3,5,6,7,13,16,22,25   # htdemucs vocals -> data/stems
uv run python methods/mms_fa.py --stems         # a_mms_fa_vocals (subset only)

python bench/report.py --by-lang                # benchmark tables
python bench/report.py --ids 3,5,6,7,13,16,22,25   # same subset, for the vocals comparison
python bench/per_track.py a_mms_fa              # per-track offset diagnostic
python bench/agreement.py                       # how much human LRC sources disagree
python bench/plain_only_coverage.py             # fallback coverage on plain-only entries
```

`results/*.json` holds the committed per-line predictions. `report.py` needs only
those files and `data/dataset.json`.

## Layout

| path | what |
|---|---|
| `bench/common.py` | dataset loader, credit filter, LRC parser |
| `bench/align_text.py` | monotonic line mapper (third-party LRC → LRCLIB lines) |
| `bench/sources.py` | NetEase / QQ / Kugou (KRC) clients: unofficial endpoints, research only |
| `bench/romanize.py` | pykakasi (ja) + uroman, for the MMS character vocabulary |
| `methods/*.py` | one script per method family; each writes `results/<method>.json` |
