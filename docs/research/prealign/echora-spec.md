# echora-kara-300m lead aligner: behavioural specification

Status: clean-room spec, written from a reading of echora-kara (AGPL-3.0) and checked by running it.
This document contains no echora code. It describes what is computed, as equations, tables and
examples. Two independent re-implementations written from this text alone (a numpy decoder and a
torch-functional text head) reproduce echora's output on three bench tracks. Every token time
matches to the millisecond. The text-share matrix matches to within 2e-5 in log-probability (see §8).

Weights: `hcxlabs/echora-kara-300m`, snapshot `64087e1078b1aaf39cc1e6a9fc93874ca2f60e22`
(CC BY-NC 4.0). `config.json` → `text_aligner` = `{dim 512, heads 8, text_layers 6, cross_layers 4,
ff_mult 4, frame_dim 1024, letter_dim 30, letters 32, max_letters 16, max_lines 256,
max_line_positions 64, factorized true, line_embeddings true, qk_norm true, dropout 0.1}`,
`song_text_version` = 1.

## 0. Scope

**In scope:**
- The text-conditioned head ("text aligner").
- The lead-line decoder with these fixed options:
  - `min_token_ms 100`, `min_letter_ms 40`, `duration_pace_ratio 0.9`
  - `vocal_activity_weight 2.0`, `vocal_activity_source energy`
  - `exact_frame_clock`, `blank_free_tokens`
  - `head_correct 0`, `tail_correct 0`, `timing_offset_ms 0`
  - text bonus β = 0.4
  - in-line gap cost 0
  - no sub-frame refinement, no onset offsets, audio speed 1

**Out of scope:** the encoder (ONNX, given), MuQ fusion, the edge, background, VAD and F0 heads,
label rules, the lyric normaliser, LID, espeak, LRC/ASS output and head/tail correction. §7 lists what
can be dropped and why.

Notation:
- `T`: number of frames in the stitched lattice.
- `t`: frame index, 0-based.
- `K`: number of tokens.
- `k`: token index.
- `L`: total number of letters.
- `j`: flat letter index over the whole song.
- `ℓ(j)`: the vocabulary id of letter `j`.
- `own(j)`: the token that owns letter `j`.
- All logs are natural.

## 1. Data contract

### 1.1 Inputs

| Name | Shape / type | Meaning |
|---|---|---|
| `x` | float32 `[N]` | Song audio, mono, 16 kHz. Multichannel audio is averaged over channels; other rates are resampled to 16 kHz first. This waveform feeds both the encoder and the energy feature (§4.1). |
| `E` | float32 `[T, 30]` | Per-frame CTC **log-probabilities**: `E = log_softmax(logits)` over the 30 vocabulary entries, stitched over the song (§1.3). |
| `Hd` | float32 `[T, 1024]` | Last hidden state of the encoder (the input of its CTC linear layer), stitched the same way. |
| tokens | list of `K` letter-id lists, grouped into lines | Each token is a non-empty list of vocabulary ids, in sung order. Lines are consecutive runs of tokens. Empty lines do not exist: drop them before numbering. |
| `frame_times` | float64 `[T]` | Receptive-field-centre time of each stitched frame, in seconds (§1.3). |

Vocabulary (from `vocab.json`): `|`=0, `a`..`z`=1..26, `'`=27, `[UNK]`=28, **blank `[PAD]`=29**.
The lyric pipeline turns a romanised string into ids one character at a time, with no lower-casing:
- a space becomes 0;
- an upper-case letter, a digit or punctuation other than `'` becomes 28.

On the bench tracks our labels produce only `a`..`z` and `'`; the `'` (27) occurs on track 18. **Vowel ids** = {1, 5, 9, 15, 21, 25} (a e i o u y).

### 1.2 Output

For every token `k`:
- `start_k`, `end_k` in seconds (§5.6);
- the same values in integer milliseconds: `round(1000·s)`. Every time taken from the clock is
  ≡ 0.46875 (mod 1) ms, so rounding never ties there. The exception is an end extrapolated past the
  last frame (§5.6).

The benchmark (`run_echora.py`) consumed the display times, after the blank hold of §5.5. The pre-hold
("acoustic") times are also defined, in §5.5.

### 1.3 Lattice layout and frame clock (fixed by echora's chunking)

The decoder needs to know which audio instant each lattice row stands for. echora builds the lattice
as follows (the encoder internals are out of scope; only the layout matters here):

- **Chunks.** Chunk length `C` = 45 s = 720000 samples. Overlap `O` = 2 s = 32000 samples.
  Step = `C − O` = 688000. Chunk starts are `s_c = c·688000` for c = 0, 1, …, `s_c < N`. A start is
  kept if it is the first one or if `N − s_c > 32000`. Chunk `c` covers `[s_c, min(N, s_c + C))`, of
  length `n_c`.
- **Frames per chunk.** `f_c = ⌊(n_c − 400)/320⌋ + 1`. This is the wav2vec2 conv stack with kernels
  (10,3,3,3,3,2,2) and strides (5,2,2,2,2,2,2). A full chunk gives 2249 frames.
- **Trim.** `o_c = round(f_c · 32000 / n_c)`, which is 100 for every chunk long enough to matter.
  Keep local frames `[a_c, f_c − b_c)`, where:
  - `a_c = ⌊o_c/2⌋` for every chunk except the first, which uses 0;
  - `b_c = o_c − ⌊o_c/2⌋` for every chunk except the last, which uses 0.

  Concatenate the kept frames in order. `T` is the total.
- **Per-chunk normalisation.** Each chunk's waveform is normalised to zero mean and unit variance
  (`(x−μ)/sqrt(σ²+1e-7)`, the HF `Wav2Vec2FeatureExtractor` with `do_normalize`) before the encoder.
  This is encoder-side; it is listed here because it is per chunk, not per song.
- **Exact clock.** The kept local frame `i` of chunk `c` has time
  `(s_c + 199.5 + 320·i) / 16000` s. Here 199.5 samples is the centre of the 400-sample receptive
  field of a chunk's first frame. Example: the first frame is at 0.01246875 s. Each chunk seam skips
  one 20 ms slot: from frame 2198 to 2199 the clock jumps by 40 ms (703559.5 → 704199.5 samples).
- **Legacy shift.** `Δ = (N/16000) / T` seconds per frame, slightly above 0.02 (0.0200088 on track 5).
  The clock does not use it. It still sets several frame counts (§4.3, §5.5) and extrapolates past
  the last frame.

The test vectors were made with our harness, which also windows each 45 s chunk internally for the
int8 ONNX encoder (`--sub 15,3`). It pads each chunk to exactly `f_c` frames, so the layout above
holds.

## 2. Text head: architecture

`d` = 512, `h` = 8 heads, `d_h` = 64, FF width 2048. Every LayerNorm has learned weight and bias and
eps = 1e-5. GELU is the exact (erf) form. Dropout is inactive (inference). Linear layers compute
`y = xWᵀ + b` with `W` stored `[out, in]`.

**Sinusoid** `P(p) ∈ R^512` for an integer position `p`:
- `ω_i = exp(−ln(10000)·i/256)` for i = 0..255;
- `P(p) = [sin(p·ω_0..255), cos(p·ω_0..255)]`, all sines first, then all cosines.

### 2.1 Text side (once per song, over all K tokens)

1. **Letter embedding.** Each token has a padded letter row `ids ∈ {0..32}^16` (§3). Look up
   `tokens.letters.weight[ids]`, giving `[16, 512]`. Row 32 is the padding row; it is all zeros in the
   checkpoint.
2. **Convolution.** Conv1d over the 16 letter positions: 512→512 channels, kernel 3, zero-padding 1
   on each side, with bias, then GELU. The padded positions take part in the convolution as
   neighbours.
3. **Masked mean.** Average the conv outputs over the real (non-pad) letter positions only. The
   divisor is `max(1, count)`. The result is `e_k ∈ R^512`.
4. **Additive embeddings.**
   - `x_k = e_k + voice.weight[0] + P(k) + line.weight[min(line_k, 255)] + line_position.weight[min(pos_k, 63)]`.
   - `k` is the token's global index in the song, 0..K−1. All our tokens are lead, so the voice id
     is 0.
5. **Text encoder.** 6 standard pre-norm Transformer encoder layers. Each layer does:
   - `x ← x + MHA(LN₁(x))`: self-attention over all K tokens, no mask, no qk-norm, scale `1/√64`,
     packed q/k/v projection;
   - then `x ← x + W₂·GELU(W₁·LN₂(x))`.
6. **Output.** `text = LN_text(x)`, shape `[K, 512]`.

**Attention, in general.** `in_proj_weight` `[1536, 512]` is split into rows 0..511 (q), 512..1023 (k)
and 1024..1535 (v). `in_proj_bias` is split the same way. q, k and v are each split into 8 heads of
64 dimensions.

Per head:
`A = softmax(q·kᵀ / 8) · v`.

The heads are concatenated in head order and passed through `out_proj`.

### 2.2 Frame side (per window of W ≤ 1250 frames, §3.2)

1. **Input.** `u_t = [Hd_t (1024) ‖ E_t (30)] ∈ R^1054`. Note that `E` is the raw log-probabilities,
   before any vocal-activity penalty.
2. **Projection.** `f_t = Linear_{1054→512}(LN_{1054}(u_t)) + P(t_local)`. `t_local` = 0..W−1 is the
   frame index **within the window**.
3. **Cross layers.** 4 cross layers. Each one does:
   - a. `f ← f + SelfAttnQK(LN_self(f))`. Self-attention over the window's frames, with no mask.
   - b. `f ← f + CrossAttnQK(LN_cross(f), text)`. The queries come from frames; keys and values come
     from the K token vectors `text`. These are used as-is: no extra norm, no mask.
   - c. `f ← f + W₂·GELU(W₁·LN_ff(f))`.

   **qk-norm attention** works like the general attention above, with one difference. After the q
   and k projections and the head split, each head's 64-dim query and key vectors pass through a
   LayerNorm(64), and only then are the scores taken:
   - queries use `*_q_norm` and keys use `*_k_norm`;
   - the norm weights are shared across heads;
   - scale `1/√64`; `v` is not normalised.
4. **Output.** `g = LN_frame(f)`, shape `[W, 512]`.

### 2.3 Factorised output and the "text share"

The lead voice uses projection set 0:

- `q_t = query.0(g_t)`, `κ_k = key.0(text_k)`.
- **Letter evidence.**
  - `m_k ∈ R^30` is the token's letter histogram over vocabulary ids 0..29, normalised to sum to 1.
    Pad positions and ids ≥ 30 are ignored; the divisor is `max(1, count)`.
  - `ev[t,k] = ln( max(1e-6, Σ_c exp(E[t,c]) · m_k[c]) )`.
- **Scores.** `z[t,k] = (q_t·κ_k)/√512 + λ·ev[t,k]`, with `λ = letter_scale[0]` (= 0.3603 in this
  checkpoint).
- **Text share.**

  `S[t,k] = log_softmax_k z[t,k]`, normalised over the song's lead tokens.

`S` is log P(token k | frame t is singing), shape `[T, K]`. It is the only output of the head that the
decoder uses.

The head also produces two things that are **not used** here:
- the "singing" logit `none.0(g_t)`, which would give P(singing) = σ(·) and P(none) = σ(−·);
- everything with index 1 (backing voice): `query.1`, `key.1`, `none.1`, `letter_scale[1]`.

Columns of other voices or of padding would be masked to −1e4 before the softmax. With lead-only,
unpadded input, no masking occurs.

### 2.4 Weight map (`model.safetensors`, prefix `text_aligner.`, all F32)

There are 203 tensors and 38,297,152 parameters. `{i}` = 0..3 for `cross`, 0..5 for `text_encoder.layers`.

| Tensor | Shape | Count | Role (§) |
|---|---|---|---|
| `tokens.letters.weight` | [33, 512] | 1 | letter embedding, row 32 = pad (2.1.1) |
| `tokens.conv.weight` / `.bias` | [512, 512, 3] / [512] | 1 | letter conv `[out, in, kernel]` (2.1.2) |
| `voice.weight` | [2, 512] | 1 | row 0 = lead (2.1.4) |
| `line.weight` | [256, 512] | 1 | line index embedding (2.1.4) |
| `line_position.weight` | [64, 512] | 1 | position-in-line embedding (2.1.4) |
| `text_encoder.layers.{i}.norm1.weight` / `.bias` | [512] | 6 | LN₁ (pre-attention) |
| `text_encoder.layers.{i}.self_attn.in_proj_weight` | [1536, 512] | 6 | packed q,k,v |
| `text_encoder.layers.{i}.self_attn.in_proj_bias` | [1536] | 6 | |
| `text_encoder.layers.{i}.self_attn.out_proj.weight` / `.bias` | [512, 512] / [512] | 6 | |
| `text_encoder.layers.{i}.norm2.weight` / `.bias` | [512] | 6 | LN₂ (pre-FF) |
| `text_encoder.layers.{i}.linear1.weight` / `.bias` | [2048, 512] / [2048] | 6 | W₁ |
| `text_encoder.layers.{i}.linear2.weight` / `.bias` | [512, 2048] / [512] | 6 | W₂ |
| `text_norm.weight` / `.bias` | [512] | 1 | LN_text |
| `frame_in.0.weight` / `.bias` | [1054] | 1 | LN over [hidden ‖ log-probs] |
| `frame_in.1.weight` / `.bias` | [512, 1054] / [512] | 1 | frame projection |
| `cross.{i}.self_norm.weight` / `.bias` | [512] | 4 | LN_self |
| `cross.{i}.self_attention.in_proj_weight` / `_bias` | [1536, 512] / [1536] | 4 | packed q,k,v |
| `cross.{i}.self_attention.out_proj.weight` / `.bias` | [512, 512] / [512] | 4 | |
| `cross.{i}.self_q_norm.weight` / `.bias` | [64] | 4 | per-head query LN |
| `cross.{i}.self_k_norm.weight` / `.bias` | [64] | 4 | per-head key LN |
| `cross.{i}.cross_norm.weight` / `.bias` | [512] | 4 | LN_cross (frames only) |
| `cross.{i}.cross_attention.in_proj_weight` / `_bias` | [1536, 512] / [1536] | 4 | q from frames; k, v from text |
| `cross.{i}.cross_attention.out_proj.weight` / `.bias` | [512, 512] / [512] | 4 | |
| `cross.{i}.cross_q_norm.weight` / `.bias` | [64] | 4 | |
| `cross.{i}.cross_k_norm.weight` / `.bias` | [64] | 4 | |
| `cross.{i}.ff_norm.weight` / `.bias` | [512] | 4 | LN_ff |
| `cross.{i}.ff.0.weight` / `.bias` | [2048, 512] / [2048] | 4 | W₁ |
| `cross.{i}.ff.3.weight` / `.bias` | [512, 2048] / [512] | 4 | W₂ |
| `frame_norm.weight` / `.bias` | [512] | 1 | LN_frame |
| `query.0.weight` / `.bias` | [512, 512] / [512] | 1 | lead query (2.3) |
| `key.0.weight` / `.bias` | [512, 512] / [512] | 1 | lead key (2.3) |
| `letter_scale` | [2] | 1 | [0] = λ lead = 0.3603; [1] unused |
| `query.1.*`, `key.1.*` | as above | 1 | **unused** (backing voice) |
| `none.0.weight` / `.bias`, `none.1.*` | [1, 512] / [1] | 1 | **unused** ("singing" logits) |

## 3. Text head: input packing and windowing

### 3.1 Packing (song_text_version 1: the input order is kept as is)

For each token `k`, in input order:

| Field | Value |
|---|---|
| letters | The token's first 16 ids, then padded with 32 up to length 16. Longer tokens are **truncated for the head only**; the decoder still uses every letter. Track 18 has a token exactly at the limit, `ppajyeobeorineun` (16 letters). |
| voice | 0 |
| line | The token's line number. Lines are numbered 0, 1, 2, … over the non-empty lines only. Clipped to 255 at lookup. |
| position | The token's 0-based index within its line. It restarts at 0 whenever the line number differs from that of the previous token. Clipped to 63 at lookup. |
| global position | `k` (0..K−1), used by the text sinusoid. It is **not** clipped. |

There is no padding mask: one song is one sequence. Example (track 13): the first tokens are
`hou lai liu ruo ying rene liu | hou lai | wo …`. Their letter rows are `[8,15,21,32,…]`,
`[12,1,9,32,…]` and so on; `line` is `0,0,0,0,0,0,0,1,1,2`; `position` is `0,1,2,3,4,5,6,0,1,0`.

### 3.2 Frame windows and stitching

The head is run over frame windows of `W` = 1250 frames (25 s) that overlap by 250 frames, with a step
of 1000.

- **Starts.** `0, 1000, 2000, …`, every value `< max(1, T − 250)`.
- **Window extent.** Window `n` covers `[start_n, min(T, start_n + 1250))`.
- **Kept rows.** From each window's output keep local rows `[l_n, len_n − r_n)`, where:
  - `l_n = 125`, or 0 for the first window;
  - `r_n = 125`, or 0 for the last window.

Concatenating the kept rows gives exactly `T` rows.

The text side (§2.1) is computed once and shared by all windows. Every window restarts the frame
sinusoid at `t_local = 0`.

## 4. Decoder inputs

### 4.1 Vocal-activity energy feature

This step uses the full mix `x` at 16 kHz. It is not a vocal stem.

1. **Pad.** Zero-pad `x` at the end to at least `320·T + 400` samples.
2. **Frame.** Frame `t` takes samples `[320t, 320t + 400)` on the **nominal song grid**: the global
   sample index, with no chunk seams.
3. **Loudness.** `Ldb_t = 10·log10( mean(x²) + 1e-10 )`.
4. **Normalise.** Take `lo = Q_0.1(Ldb)` and `hi = Q_0.9(Ldb)`, quantiles with linear interpolation
   (numpy/torch default). Then

   `a_t = clip( (Ldb_t − lo) / max(hi − lo, 1e-6), 0, 1 )`.

This grid ignores the stitched frame clock. The two agree only within the first chunk. After each
seam (every 43 s) the energy window falls one more frame behind the lattice frame it is paired with:
it is 7 frames (140 ms) behind at the end of a 5.7-minute song. This is echora's behaviour, and the
test vectors include it (§9).

### 4.2 Penalised emissions

- Letters: `E′[t,c] = E[t,c] − 2.0·(1 − a_t)` for every c ≠ 29.
- Blank: `E′[t,29] = E[t,29]`.

### 4.3 Frame counts derived from Δ (§1.3)

| Symbol | Formula | Track 5 value |
|---|---|---|
| `letter_cap` | `max(1, round(0.040/Δ))` | 2 |
| `token_cap` | `max(1, round(0.100/Δ))` | 5 |
| `pause` | `round(0.4/Δ)` | 20 |
| `hold` | `⌊0.2/Δ⌋` | **9** (not 10, because Δ > 0.02) |

`round` is round-half-even. Ties do not occur at these values.

## 5. Decoder

### 5.1 Topology: blank-free, duration-floored CTC

The input is the song's `L` letters in order, each with a floor `m_j ≥ 1` frames, and a score matrix
`D` with one column per letter and one blank column. Build a left-to-right chain of states:

```
B₀ │ token 0: [letter j: m_j sub-states] [letter j+1: m_{j+1} sub-states] … │ B₁ │ token 1: … │ B₂ │ … │ token K−1 … │ B_K
```

- There is one **blank state** before the first token and one after each token, `K+1` in total.
  There is **no blank between letters of the same token** (blank-free tokens).
- Each letter `j` expands into `m_j` sub-states. All of them emit letter `j`'s column.
- `D[t, state]` is that state's column at frame `t`. Blank states emit the blank column.

Allowed moves from frame `t−1` to frame `t`:

| Move | Allowed when |
|---|---|
| **stay** (s→s) | s is a blank state, or the **last** sub-state of a letter |
| **advance** (s−1→s) | always (any state to the next one) |
| **skip** (s−2→s) | s is the first sub-state of a token's first letter (k ≥ 1), s−1 is the blank between the two tokens, and the **vocabulary id** of the previous token's last letter differs from that of this letter (`ℓ` values, not column numbers) |

These moves imply the following:
- Letter `j` lasts at least `m_j` consecutive frames.
- Letters inside a token follow each other directly.
- Between two tokens, the path takes zero or more blank frames. Zero is allowed only when the two
  letters on either side of the boundary have different ids; when they are the same, at least one
  blank frame is required (the CTC repeat rule).

**Start:** at `t = 0` the path is in `B₀` or in the first sub-state of letter 0.
**End:** at `t = T−1` the path is in `B_K` or in the last sub-state of letter `L−1`.

**Objective:** maximise `Σ_t D[t, state_t]` (max-product Viterbi, no transition scores).

**Tie-breaking (verified):**
- Per state and frame, a tie is broken in the order **stay, then advance, then skip**: the earliest
  wins.
- At the end, a tie is broken in favour of `B_K`.
- Scores are accumulated in float32. The reference does this, and the vectors match bit for bit under
  float32. Float64 can flip near-ties.

**Spans:** letter `j` occupies `[first frame in any of its sub-states, last such frame + 1)`. Token
`k` runs from its first letter's start to its last letter's end. All spans are in frames.

If `Σ m_j > T`, or no finite path exists, the final pass (§5.4) is re-run with every `m_j = 1`. This
did not happen on any bench track.

### 5.2 Pass 1: plain pass (used only to measure pace)

- Columns: letter `j` uses `E′[:, ℓ(j)]`, blank uses `E′[:, 29]`. There is no text bonus.
- Floors: all `m_j = 1`.
- Topology: as in §5.1.

If `T < L` this pass fails and there is no fallback. The song cannot be aligned.

### 5.3 Paced floors (pace ratio 0.9)

For each line, with tokens `k ∈ [a, b)`:

1. Take the pass-1 token spans `(s_k, e_k)` and sort them by start (they are already in order).
2. `total = max_k e_k − min_k s_k`.
3. `pauses` = sum, over consecutive sorted spans, of `s_{next} − e_{prev}`, counting a gap only if it
   is strictly greater than `pause` (20 frames). Gaps of 20 frames or less add nothing.
4. `sung = max(1, total − pauses)`.
5. `n_tok = b − a`, and `n_let` = the number of letters in the line.
6. Token floor: `F_tok = max(1, min(token_cap, ⌊0.9·sung / n_tok⌋))`.
7. Letter floor: `F_let = max(1, min(letter_cap, ⌊0.9·sung / max(1, n_let)⌋))`.
8. For each token in the line, give every letter `F_let` frames. If the token's total is below
   `F_tok`, hand the shortfall out one frame at a time, round-robin, to the token's **vowel** letters
   (ids {1,5,9,15,21,25}) in letter order. If the token has no vowel, every extra frame goes to its
   first letter. Extra frames are never taken away.

Example (track 5, line 0): `mu`, `teki`, … gets floors `[2,3]`, `[2,2,2,…]`, …

### 5.4 Pass 2: final pass with text bonus

- Columns: one column per letter `j`, plus the blank column. Ids are not shared, so two letters with
  the same id have separate columns.
  - Letter `j`: `D[t, j] = E′[t, ℓ(j)] + 0.4 · S[t, own(j)]`.
  - Blank: `D[t, blank] = E′[t, 29]`.

  The bonus adds the head's log P(token | singing) for the owning token to each letter. Every letter
  of a token gets the same bonus. Blank gets none.
- Floors: from §5.3.
- Topology: §5.1. The skip rule compares **vocabulary ids** `ℓ`, not the per-letter column indices.

This pass gives the **acoustic** token spans `(s_k, e_k)`.

There is no star or garbage state, and no special handling between lines: lines affect only the
pace (§5.3) and the blank hold (§5.5). Silence and instrumental parts are absorbed by the blank
states. The vocal-activity penalty makes letter frames costly where the mix is quiet.

### 5.5 Blank-hold display extension

For each token `k < K−1` that is **not** the last token of its line:
- if `s_{k+1} > e_k`, set `e_k ← min(s_{k+1}, e_k + hold)`, with `hold` = 9 (§4.3).

Starts never change. Line-final tokens are never extended. The ends before this step are the
"acoustic" times; the ends after it are the "display" times.

### 5.6 Frame index to seconds

`τ(f) = frame_times[f]` for `f < T`; for `f ≥ T`, `τ(f) = frame_times[T−1] + (f − T + 1)·Δ`.

- `start_k = τ(s_k)`, the centre of the first occupied frame.
- `end_k = τ(e_k)`, the centre of the frame **after** the last occupied frame.

Timing offset is 0 and speed is 1, so nothing else is added.

## 6. Constants and options

| Name | Value | Where |
|---|---|---|
| sample rate | 16000 | everything |
| frame hop / window | 320 / 400 samples | lattice (§1.3), energy (§4.1) |
| receptive-field centre | 199.5 samples | exact clock (§1.3) |
| chunk / overlap (`ECHORA_KARA_CHUNK_OVERLAP_SECONDS` default 2) | 45 s / 2 s | lattice and clock (§1.3) |
| chunk trim | ⌊o/2⌋ left, o − ⌊o/2⌋ right, o = round(f·32000/n) | §1.3 |
| blank id | 29 | decoder |
| vowel ids | {1,5,9,15,21,25} | floors (§5.3) |
| head window / overlap / keep margin | 1250 / 250 / 125 frames | §3.2 |
| d, heads, d_h, FF, text layers, cross layers | 512, 8, 64, 2048, 6, 4 | §2 |
| letters/pad id, max letters | 32, 16 | §3.1 |
| max lines / line positions | 256 / 64 (clip to 255 / 63) | §3.1 |
| LayerNorm eps | 1e-5 | §2 |
| sinusoid base | 10000, sin half then cos half | §2 |
| letter-evidence floor | 1e-6 | §2.3 |
| letter_scale[0] (λ) | 0.3603 (learned) | §2.3 |
| `ECHORA_KARA_TEXT_BONUS` β | 0.4 | §5.4 |
| `vocal_activity_weight` | 2.0 | §4.2 |
| `vocal_activity_source` | energy | §4.1 |
| loudness eps / quantiles / range floor | 1e-10 / 0.1, 0.9 / 1e-6 | §4.1 |
| `min_letter_ms` | 40 → `letter_cap` | §4.3, §5.3 |
| `min_token_ms` | 100 → `token_cap` | §4.3, §5.3 |
| `duration_pace_ratio` | 0.9 | §5.3 |
| pace pause threshold | 0.4 s → 20 frames | §5.3 |
| `blank_free_tokens` | on | §5.1 |
| `exact_frame_clock` | on | §5.6 |
| `ECHORA_KARA_MAX_BLANK_HOLD_SECONDS` | 0.2 s → 9 frames | §5.5 |
| `ECHORA_KARA_IN_LINE_GAP_COST` | 0 (off) | none |
| `edge_weight` | 0 (off) | none |
| `timing_offset_ms`, `head_correct`, `tail_correct` | 0 | none |
| audio speed | 1 | none |
| `song_text_version` | 1 (input order kept) | §3.1 |
| MuQ fusion | `gate0` (adds nothing; the trained gate is −2.2e-5 anyway) | none |

## 7. What can be omitted with these flags

- **Encoder internals and MuQ.** Use `Hd` and `E` as given. With `gate0` the hidden state is the plain
  encoder's.
- **Head outputs other than the lead token log-softmax.** The "singing" logit, all backing-voice
  projections, the −1e4 masking, and the combined `[none, tokens]` output are not used.
- **Background decoding.** The CLI flag routes a line to the background head only if the *entire*
  line is one parenthesised span. None of the three test tracks has such a line. For lead-only use,
  never route any line.
- **Edge head, VAD/F0 aux heads** (not shipped with this snapshot), **external activity, in-line gap
  cost, onset offsets, sub-frame refinement, source priors, the semi-Markov duration-model decoder,
  head/tail silence correction, the timing offset, time-stretching, LRC/ASS writers, label rules,
  normaliser, LID and espeak.** All are off or unreachable with these flags.
- **Per-token "score"** (mean probability). It is reported, but it does not affect times.
- **Background-voice block decoding** and the second lead decode that follows it. These happen only
  when some line is routed to the background head.

## 8. Verification

The spec author's own checks reproduced echora on tracks 5, 13 and 18:
- identical pass-1 letter spans, identical floors and identical final letter spans;
- **0 / 748, 0 / 395 and 0 / 286 token-millisecond mismatches**;
- energy within 3e-7;
- `S` within a maximum |Δ log p| of 1.9e-5, with argmax agreement of 1.000.

Those scripts were written by someone who had read echora's source, so they are **not** part of the
clean side and have been removed from the repo tree. A clean implementation must write its own
checks against the vectors in §9.

## 9. Test vectors

`research/prealign/data/spec_vectors/`. They were produced by running echora's own CLI
in-process (the generator script is kept outside the repo) with:
- the int8 ONNX encoder (`models/echora_enc.int8.onnx`, `--sub 15,3`);
- `--labels ours` (the records from `run_echora.our_labels`);
- `--fusion gate0`;
- `ECHORA_KARA_TEXT_BONUS=0.4`, `ECHORA_KARA_IN_LINE_GAP_COST=0`;
- the §0 flags, without `--background_decoding`.

The tracks are 5 (ja, 214 s), 13 (zh, 341 s) and 18 (ko/en, 179 s).

**`track_<id>.npz`** (float32 unless noted):

| Key | Shape | Content |
|---|---|---|
| `logits` | [T, 30] | Stitched raw CTC logits (§1.3). `log_softmax(logits) == log_probs` exactly. |
| `log_probs` | [T, 30] | `E`, the decoder and head input. |
| `hidden` | [T, 1024] | `Hd`, stitched last hidden state. |
| `frame_times` | [T] float64 | Exact clock, in seconds. |
| `activity` | [T] | `a_t` (§4.1). |
| `text_share` | [T, K] | `S` (§2.3). |
| `letters` | [K, 16] int64 | Packed letter ids, pad 32 (§3.1). |
| `line`, `position`, `voice` | [K] int64 | §3.1; voice is all 0. |

**`track_<id>.json`:**

| Field | Content |
|---|---|
| `n_samples`, `sample_rate`, `frames` (T), `frame_shift` (Δ) | Lattice facts. |
| `chunk_samples`, `chunk_frames` | `n_c`, `f_c` per 45 s chunk. |
| `tokens` | Romanised token strings. |
| `token_text` | Display text. |
| `encoded` | Letter-id lists, one per token (decoder input). |
| `line_ranges` | `[a, b)` token ranges per line. |
| `line_of_token` | Line number per token. |
| `plain_pass_letter_spans`, `plain_pass_token_spans` | §5.2 output, in frames, end exclusive. |
| `min_frames_per_letter` | `m_j` from §5.3. |
| `min_frames_fallback_used` | Whether the all-ones fallback ran (false everywhere). |
| `final_letter_spans`, `final_token_spans` | §5.4 output (acoustic), in frames. |
| `display_frame_duration` | Δ. |
| `token_times_acoustic_s` | `[τ(s_k), τ(e_k)]` before the hold. |
| `token_times_s` | After the hold (§5.5). |
| `token_times_ms` | Integer ms as written by echora: the reference answer. |

`run_<id>/` holds echora's own output directory: `o.alignment.json`, LRC and ASS. It is kept for
provenance.

## 10. Lyryc decisions (answers to the spec author's open questions)

1. **In-line gap cost is 0.** The ablation with 0 scored the same as 0.2, or slightly better (lines
   >1 s off: 4.0% vs 4.1%).
2. **Lattice.** Lyryc does **not** reproduce echora's 45 s chunk layout. It encodes the song as one
   continuous lattice in 15 s windows with 3 s of context on each side, and keeps each window's own
   frames. This is the shipped MMS scheme (`bench/align.py` `Model.emissions`). Its exact clock is
   `(199.5 + 320·t) / 16000` s for every frame `t`, and `T = ⌊(N − 400)/320⌋ + 1`.
3. **Energy grid.** With the continuous lattice, the energy grid in §4.1 lines up with the lattice by
   construction, so the seam drift goes away. The §9 vectors still carry echora's layout and drift.
   Use them to check the head and the decoder **given the vectors' own inputs**: `frame_times`,
   `activity` and `log_probs` from the `.npz`. Do not use them to check the end-to-end lattice.
4. **Output times.** Lyryc uses the display times, after the blank hold of §5.5. These are what the
   benchmark scored.
5. **Limits.** Truncate tokens to 16 letters for the head only, and clip line and position indices as
   in §3.1. The decoder always uses every letter.
