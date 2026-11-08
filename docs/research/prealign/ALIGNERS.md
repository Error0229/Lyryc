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

## Reject rule on echora

The reject rule ([REPORT.md](REPORT.md), "Robustness and the reject rule") was tuned on MMS posteriors. Line confidence is still the mean posterior of a line's letters over their spans (`run_echo.py` `conf`), but echora's scale differs. It has fewer very low lines: 10th percentile 0.147 against MMS's 0.057, and median 0.316 against 0.343. The tables below come from `confidence.py` and `reject.py` on `echo_clean`, with the same 1,210 lines and the same "bad" definition (more than 1 s off).

**MMS rule on each model** (smoothed3 < 0.10, or the track has more than 60% of its lines below 0.15):

| run | rejected | precision (>1 s) | recall | kept ≤0.3 s | kept ≤1.0 s |
|---|---|---|---|---|---|
| MMS, no rule | 0% | – | – | 79% | 89% |
| MMS, rule | 20% | 36% | 62% | 87% | 95% |
| MMS, rule + LRCLIB 0.7 s (LRCLIB subset) | 21% | 45% | 92% | 92% | 99% |
| echora, no rule | 0% | – | – | 88% | 96% |
| echora, MMS rule | 3% | 32% | 25% | 89% | 97% |
| echora, MMS rule + LRCLIB 0.7 s (LRCLIB subset) | 6% | 42% | 69% | 92% | 99% |

**Line threshold:** it stays at 0.10. On echora, raising it only adds false rejects: smoothed3 < 0.15 or < 0.20 has 26% and 18% precision on the LRCLIB subset, against 43% at 0.10, and the same 98.8–98.9% kept within 1 s. The "sure" override in `reject.rs` (a line with confidence 0.4 or more is believed over LRCLIB) changes no outcome on echora at 0.4, 0.6 or 0.8, so it stays at 0.4.

**Track threshold:** this changes to **more than 50% of lines below 0.20**. The MMS constants never fire on echora: Billie Jean, the track they were made for, sits at exactly 60%. No study track is "lost" under echora, so the rule was checked on lost alignments made on purpose (`bench/lost.py`). Echora has no line skipping, so short versions now go through this rule too.

| lost alignment | median conf | lines < 0.15 | lines < 0.20 | MMS rule | new rule |
|---|---|---|---|---|---|
| lyrics of 1 on audio of 0 (en) | 0.135 | 71% | 92% | reject | reject |
| lyrics of 7 on audio of 5 (ja) | 0.252 | 7% | 22% | keep | keep |
| lyrics of 11 on audio of 10 (zh) | 0.191 | 28% | 55% | keep | reject |
| lyrics of 17 on audio of 16 (ko) | 0.097 | 76% | 91% | reject | reject |
| track 0 cut to 45%, full lyrics | 0.075 | 81% | 85% | reject | reject |
| track 5 cut to 45% | 0.167 | 40% | 60% | keep | reject |
| track 11 cut to 45% | 0.145 | 51% | 62% | keep | reject |
| track 15 cut to 60% | 0.255 | 30% | 41% | keep | keep |

Of the 20 real tracks, the new rule rejects only Billie Jean (84% of its lines are below 0.20, and 23% are more than 1 s off). The next highest is 37%.

| echora, new rule | rejected | precision (>1 s) | recall | kept ≤0.3 s | kept ≤1.0 s |
|---|---|---|---|---|---|
| smoothed3 < 0.10 or track > 50% below 0.20 | 9% | 21% | 45% | 91% | 97% |
| + LRCLIB 0.7 s (LRCLIB subset) | 11% | 22% | 69% | 92% | 99% |

With LRCLIB timing, the new track rule costs Billie Jean's good lines their word timing and buys nothing measurable here, because the LRCLIB check already catches its bad lines. It is there for lyrics that only have estimated timing, where a lost alignment would otherwise be shown. Same-language wrong lyrics (ja on ja) and mildly cut versions still pass. Confidence cannot see them.

**Another confidence definition, tried and dropped:** the head's own token probability `exp(S[t, k])` over the same letter spans (`run_echo.py` `conf_head`). It is worse at every threshold. At 0.10 it rejects 15% at 13% precision, against 5% at 28% for `conf`.

## Reproduce

```
uv run python -X utf8 bench/run_echo.py --name echo_clean --head onnx-int8   # needs models/ from echo_head.py export + echora_enc.int8.onnx
uv run python -X utf8 bench/check_echo.py                                    # needs data/spec_vectors/
uv run python -X utf8 bench/evaluate.py echo_clean
uv run python -X utf8 bench/confidence.py echo_clean                        # -> results/confidence_echo_clean.json
uv run python -X utf8 bench/reject.py echo_clean 0.10 0.20 0.5              # line, track-low, track-share thresholds
uv run python -X utf8 bench/lost.py                                         # deliberately lost alignments
```

To produce the encoder ONNX, run `run_echora.py export` in a venv with transformers. That module only imports echora when it runs the CLI.
