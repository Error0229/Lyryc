# Pre-align: other aligner models

2026-11-07. Follows [REPORT.md](REPORT.md) and uses the same 20 tracks, references and scorer (`research/prealign/bench/evaluate.py`). Unless a row says otherwise, numbers are pooled against QQ.

## Headline

**Replace MMS with echora-kara-300m (hcxlabs), run through our own clean-room decoder and text head.**

- Median timing stays at the reference noise floor: line median 0.105 s, word median 0.090 s.
- The large errors mostly go away:
  - Lines off by more than 1 s drop from 10.2% to 4.0%.
  - Mean line error drops from 0.96 s to 0.30 s.
  - Billie Jean improves from 0.65 s to 0.26 s.
- Speed is the same: RTF about 0.23 in Python with int8 ONNX, 8 threads.
- Size is about the same: 380 MiB of int8 models, against 356 MiB for MMS.

| config | line median | word median | lines > 1 s off | mean line error | en lines ≤ 1 s | Billie Jean | RTF |
|---|---|---|---|---|---|---|---|
| MMS (shipped) | 0.104 | 0.090 | 10.2% | 0.96 | 81% | 0.65 | 0.23–0.25 (Rust) |
| echora CLI, MuQ + text head, fp32 | 0.111 | 0.090 | 3.8% | 0.28 | 95% | 0.27 | 0.49 |
| echora CLI, int8 encoder, 15 + 3 s windows | 0.110 | 0.090 | 4.1% | 0.30 | 93% | 0.24 | 0.26 |
| **clean rewrite (`run_echo.py`), int8** | **0.105** | **0.090** | **4.0%** | **0.30** | 93% | 0.26 | **0.23** |

## Candidates

- **Qwen3-ForcedAligner-0.6B** (`run_qwen.py`): ruled out. It has a 5-minute limit, 80 ms slots, and peak RAM of 3.3–3.8 GB. It was worse than MMS on every track tried, even with windows cut using known line times (for example アイドル 0.26 s vs 0.06 s, Billie Jean 3.0 s). It is trained on speech, not singing.
- **echora-kara-300m:**
  - The encoder is a wav2vec2 initialised from omniASR CTC 300M v2, fine-tuned on sung lyrics. The vocab is a–z, `'`, `|`, and blank = 29.
  - It has extra heads: a text head (38M), MuQ fusion, edge, and background.
  - It needs its own decoder. Running the encoder in our plain Viterbi made it 0.2–0.5 s late (`echo_sep` / `echo_nosep`).

## What echora's gain is made of

These ablations ran echora's own CLI (`run_echora.py`) on the shippable form: int8 encoder, 15 + 3 s windows.

| removed | lines > 1 s off | mean | notes |
|---|---|---|---|
| nothing (baseline) | 4.1% | 0.30 | |
| text head (MuQ on or off) | 8.5% | 0.56 | fp32 runs; the text head carries most of the gain |
| MuQ fusion | 3.8% (= with MuQ) | 0.28 | the trained gate is −2.2e-5, so MuQ adds nothing |
| minimum-duration floors | 7.5% | 1.25 | core |
| vocal-activity penalty | 4.4% | 0.28 | line median 0.110 → 0.119, ja 0.084 → 0.105 |
| blank-free tokens | 4.2% | 0.30 | en 0.168 → 0.185 |
| in-line gap cost | 4.0% | 0.27 | dropped |
| v3-sung → v1 labels | 4.4% | 0.33 | label rules barely matter |
| echora labels → ours (`align.py`) | 4.3% | 0.34 | **our labeler is enough** |

The int8 encoder (dynamic quantization, MatMul weights) is within noise of fp32: 3.7% vs 3.8%. The 45 s chunks echora uses run at RTF 0.26 for the encoder alone. With MMS-style 21 s windows it is 0.15.

## Licensing and the clean room

- **Weights:** CC BY-NC 4.0. Lyryc is non-commercial, so this is fine, with attribution in the README.
- **echora code:** AGPL-3.0. We do not ship or port it.
- **How the clean room was run:**
  - One agent read the source and wrote [echora-spec.md](echora-spec.md). It contains equations, tables, constants and test vectors, but no code.
  - A separate agent, which never saw the source, wrote `bench/echo_head.py` and `bench/echo_decode.py` from the spec.
  - Given the vectors' inputs, the decoder reproduces echora's token times to the millisecond (0 mismatches in 1,429 tokens). The fp32 head is within 6e-5 log-probability.
- **Not needed:** labels, espeak (GPL) and language detection. We use our own `align.py` pieces.

## Reproduce

```
uv run python -X utf8 bench/run_echo.py --name echo_clean --head onnx-int8   # needs models/ from echo_head.py export + echora_enc.int8.onnx
uv run python -X utf8 bench/check_echo.py                                    # needs data/spec_vectors/
uv run python -X utf8 bench/evaluate.py echo_clean
```

To produce the encoder ONNX, run `run_echora.py export` in a venv with transformers. That module only imports echora when it runs the CLI.
