"""echora-kara lead-line decoder, numpy only, from docs/research/prealign/echora-spec.md §4-5.

decode(E, a, S, encoded, line_ranges, frame_times, delta) -> dict of spans (frames) and token times.
  E [T,30] CTC log-probs, a [T] vocal activity (activity()), S [T,K] head text share,
  encoded: per-token letter-id lists, line_ranges: [a, b) token ranges per (non-empty) line,
  frame_times [T] seconds, delta = (N/16000)/T.
"""
import math
import numpy as np

BLANK, VOWELS, BETA, VA_WEIGHT = 29, {1, 5, 9, 15, 21, 25}, 0.4, 2.0
NEG = np.float32(-np.inf)


def activity(x, T):
    """§4.1: energy of 400-sample frames at hop 320 on the nominal grid, robustly scaled to [0, 1]."""
    n = 320 * T + 400
    x = np.pad(np.asarray(x, np.float32), (0, max(0, n - len(x))))
    fr = np.lib.stride_tricks.sliding_window_view(x, 400)[::320][:T]
    ldb = 10 * np.log10((fr.astype(np.float64) ** 2).mean(1) + 1e-10)
    lo, hi = np.quantile(ldb, [0.1, 0.9])
    return np.clip((ldb - lo) / max(hi - lo, 1e-6), 0, 1).astype(np.float32)


def penalise(E, a):
    """§4.2: letters pay 2·(1 − a_t); blank is untouched."""
    Ep = E.astype(np.float32, copy=True)
    Ep[:, :BLANK] -= (np.float32(VA_WEIGHT) * (np.float32(1) - a))[:, None]
    return Ep


def caps(delta):
    """§4.3: letter_cap, token_cap, pause, hold in frames."""
    return max(1, round(0.040 / delta)), max(1, round(0.100 / delta)), round(0.4 / delta), math.floor(0.2 / delta)


def viterbi(D, cols, blank_col, ids, tok_len, floors):
    """§5.1 blank-free, duration-floored Viterbi. D [T, C] float32; cols[j] is letter j's column.

    Returns letter spans [L, 2] (frames, end exclusive) or None if no path exists.
    """
    T = len(D)
    col, let, stay, skip, j = [], [], [], [], 0
    for k, n in enumerate(tok_len):
        col.append(blank_col); let.append(-1); stay.append(True); skip.append(False)
        for jj in range(j, j + n):
            for r in range(floors[jj]):
                col.append(cols[jj]); let.append(jj); stay.append(r == floors[jj] - 1)
                skip.append(r == 0 and jj == j and k > 0 and ids[j - 1] != ids[j])
        j += n
    col.append(blank_col); let.append(-1); stay.append(True); skip.append(False)
    col, let = np.array(col), np.array(let)
    N, L = len(col), j
    if N - len(tok_len) - 1 > T:
        return None
    si, ki = np.flatnonzero(stay), np.flatnonzero(skip)
    stay_won = np.zeros((T, len(si)), bool)
    skip_won = np.zeros((T, len(ki)), bool)
    score = np.full(N, NEG, np.float32)
    score[:2] = D[0, col[:2]]
    best = np.empty(N, np.float32)
    for t in range(1, T):
        best[0] = NEG
        best[1:] = score[:-1]                                   # advance
        st, ad = score[si], best[si]
        w = st >= ad                                            # stay wins ties
        stay_won[t] = w
        best[si] = np.where(w, st, ad)
        sk, cur = score[ki - 2], best[ki]
        w = sk > cur                                            # skip only if strictly better
        skip_won[t] = w
        best[ki] = np.where(w, sk, cur)
        score = best + D[t, col]
    if score[N - 1] == NEG and score[N - 2] == NEG:
        return None
    s = N - 1 if score[N - 1] >= score[N - 2] else N - 2
    spos, kpos = np.full(N, -1), np.full(N, -1)
    spos[si], kpos[ki] = np.arange(len(si)), np.arange(len(ki))
    path = np.empty(T, np.int64)
    for t in range(T - 1, 0, -1):
        path[t] = s
        if kpos[s] >= 0 and skip_won[t, kpos[s]]:
            s -= 2
        elif not (spos[s] >= 0 and stay_won[t, spos[s]]):
            s -= 1
    path[0] = s
    lf = let[path]
    tt, ll = np.flatnonzero(lf >= 0), lf[lf >= 0]
    r = np.arange(L)
    return np.stack([tt[np.searchsorted(ll, r, "left")], tt[np.searchsorted(ll, r, "right") - 1] + 1], 1)


def token_spans(letter_spans, tok_len):
    first = np.cumsum([0] + list(tok_len[:-1]))
    last = first + np.asarray(tok_len) - 1
    return np.stack([letter_spans[first, 0], letter_spans[last, 1]], 1)


def paced_floors(tok, encoded, line_ranges, letter_cap, token_cap, pause):
    """§5.3: per-letter frame floors from the pass-1 pace of each line."""
    floors = []
    for a, b in line_ranges:
        sp = tok[a:b][np.argsort(tok[a:b, 0], kind="stable")]
        gaps = sp[1:, 0] - sp[:-1, 1]
        sung = max(1, int(sp[:, 1].max() - sp[:, 0].min() - gaps[gaps > pause].sum()))
        n_let = sum(len(encoded[k]) for k in range(a, b))
        f_tok = max(1, min(token_cap, math.floor(0.9 * sung / (b - a))))
        f_let = max(1, min(letter_cap, math.floor(0.9 * sung / max(1, n_let))))
        for k in range(a, b):
            ids = encoded[k]
            m = [f_let] * len(ids)
            vow = [i for i, c in enumerate(ids) if c in VOWELS] or [0]
            for x in range(max(0, f_tok - f_let * len(ids))):
                m[vow[x % len(vow)]] += 1
            floors += m
    return floors


def decode(E, a, S, encoded, line_ranges, frame_times, delta):
    T = len(E)
    ids = np.array([c for tok in encoded for c in tok])
    tok_len = [len(tok) for tok in encoded]
    own = np.repeat(np.arange(len(encoded)), tok_len)
    letter_cap, token_cap, pause, hold = caps(delta)
    Ep = penalise(E, a)
    plain = viterbi(Ep, ids, BLANK, ids, tok_len, [1] * len(ids))           # §5.2
    if plain is None:
        raise ValueError(f"cannot align {len(ids)} letters in {T} frames")
    floors = paced_floors(token_spans(plain, tok_len), encoded, line_ranges, letter_cap, token_cap, pause)
    D = np.empty((T, len(ids) + 1), np.float32)                              # §5.4
    D[:, :-1] = Ep[:, ids] + np.float32(BETA) * S[:, own]
    D[:, -1] = Ep[:, BLANK]
    cols = np.arange(len(ids))
    final = viterbi(D, cols, len(ids), ids, tok_len, floors)
    fallback = final is None
    if fallback:
        final = viterbi(D, cols, len(ids), ids, tok_len, [1] * len(ids))
    acoustic = token_spans(final, tok_len)
    display = acoustic.copy()                                                # §5.5
    last = {b - 1 for _, b in line_ranges}
    for k in range(len(encoded) - 1):
        if k not in last and acoustic[k + 1, 0] > acoustic[k, 1]:
            display[k, 1] = min(acoustic[k + 1, 0], acoustic[k, 1] + hold)
    tau = lambda f: float(frame_times[f]) if f < T else float(frame_times[T - 1]) + (f - T + 1) * delta  # §5.6
    times = [[tau(s), tau(e)] for s, e in display]
    return {"plain_letter_spans": plain, "floors": floors, "fallback": fallback, "final_letter_spans": final,
            "acoustic": acoustic, "display": display, "times_s": times,
            "times_ms": [[round(1000 * s), round(1000 * e)] for s, e in times]}
