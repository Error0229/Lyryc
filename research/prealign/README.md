# YouTube Music pre-alignment bench

Findings: [`docs/research/prealign/REPORT.md`](../../docs/research/prealign/REPORT.md). Research only. Audio, lyric text and models are not committed (`data/`, `models/` are git-ignored); the scripts re-fetch them. Committed `results/` hold numbers only.

Needs `uv`, `ffmpeg` on PATH, Python 3.11/3.12. All commands from this directory; `-X utf8` matters on Windows.

```sh
uv sync
uv run python bench/resolve.py                      # tracks.seed.tsv -> tracks.json (YTM song videoIds)
uv run python -X utf8 bench/refs.py                 # QQ / Kugou / NetEase / LRCLIB references -> data/refs.json
uv run python -X utf8 bench/fetch_audio.py          # yt-dlp, sequential -> data/audio/<id>.wav + fetch.json
uv run python -X utf8 bench/export_onnx.py          # -> models/mms_fa.onnx, models/mms_fa.int8.onnx

# production config: ONNX int8, 8 threads, 15 s chunks + 3 s context
uv run python -X utf8 bench/run.py --backend onnx --onnx models/mms_fa.int8.onnx --threads 8 --chunk 15 --ctx 3 --name prod
uv run python -X utf8 bench/evaluate.py prod              # -> results/score_prod.json
uv run python -X utf8 bench/evaluate.py prod --offset     # per-track offset removed
uv run python -X utf8 bench/evaluate.py --refs            # reference vs reference (noise floor)
uv run python -X utf8 bench/confidence.py prod            # -> results/confidence_prod.json
uv run python -X utf8 bench/reject.py prod                # reject-rule table
uv run python -X utf8 bench/refquality.py                 # gapless / Kugou-copies-QQ / LRCLIB word tags
uv run python -X utf8 bench/speed.py                      # speed table from results/speed_*.json

# vocal separation (Demucs) variant
uv run python -X utf8 bench/separate.py --ids 0,2,3,12,16,19
uv run python -X utf8 bench/run.py --backend onnx --onnx models/mms_fa.int8.onnx --threads 8 --chunk 15 --ctx 3 --stems --ids 0,2,3,12,16,19 --name prod_vocals
```

`bench/align.py` is the aligner (pieces = Latin words, CJK/kana/hangul characters); `bench/qqdes.py` is the QRC cipher; `bench/score.py` the character-level matcher used for scoring.
