"""Map timestamps from one lyric text onto another (same song, different text source).

This is the piece any "fetch synced lyrics elsewhere" integration needs: third-party
LRC files differ in credits, punctuation, script (trad/simp), repeats, so we align
lines monotonically by character-bigram similarity and interpolate the gaps.
"""
import re


def grams(s):
    s = re.sub(r"[\W_]+", "", s.lower())
    return {s[i:i + 2] for i in range(len(s) - 1)} or {s}


def dice(a, b):
    a, b = grams(a), grams(b)
    return 2 * len(a & b) / (len(a) + len(b)) if a and b else 0.0


def map_lines(target, source, min_sim=0.35):
    """target: [text]; source: [(t, text)] -> [t|None] for each target line (monotonic DP)."""
    n, m = len(target), len(source)
    S = [[dice(target[i], source[j][1]) for j in range(m)] for i in range(n)]
    D = [[0.0] * (m + 1) for _ in range(n + 1)]
    B = [[0] * (m + 1) for _ in range(n + 1)]
    for i in range(1, n + 1):
        for j in range(1, m + 1):
            s = S[i - 1][j - 1]
            opts = [(D[i - 1][j], 1), (D[i][j - 1], 2)]
            if s >= min_sim:
                opts.append((D[i - 1][j - 1] + s, 0))
            D[i][j], B[i][j] = max(opts)
    out = [None] * n
    i, j = n, m
    while i and j:
        if B[i][j] == 0:
            out[i - 1] = source[j - 1][0]
            i, j = i - 1, j - 1
        elif B[i][j] == 1:
            i -= 1
        else:
            j -= 1
    return out


def interpolate(times, duration):
    """Fill None gaps linearly between known anchors (what the app would display)."""
    known = [(i, t) for i, t in enumerate(times) if t is not None]
    if not known:
        return times
    out = list(times)
    anchors = [(-1, 0.0)] + known + [(len(times), duration)]
    for (i0, t0), (i1, t1) in zip(anchors, anchors[1:]):
        for k in range(i0 + 1, i1):
            out[k] = t0 + (t1 - t0) * (k - i0) / (i1 - i0)
    return out
