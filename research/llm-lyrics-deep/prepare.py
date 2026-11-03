"""data/cases.json = the first study's 28 cases + obscure songs + instrumentals.

Durations of the new songs come from their pinned LRCLIB record, as in the
first study. Prints metadata only.
"""
import json

from common import HERE, lrclib, lrclib_retry

EXTRA = [
    # id, kind, raw title, raw artist, LRCLIB ref id, accepted names
    (29, "obscure", "deca joins - 海浪 (Official Music Video)", "deca joins", 35996062, ["海浪"], ["deca joins"]),
    (30, "obscure", "SE SO NEON - NAN CHUN", "SE SO NEON", 15636581, ["난춘", "난춘(亂春)", "NAN CHUN"], ["새소년", "SE SO NEON"]),
    (31, "obscure", "Treehouse", "Alex G", 36058966, ["Treehouse"], ["Alex G"]),
    (32, "obscure", "Regal Lily - Rickenbacker", "Regal Lily", 3096479, ["リッケンバッカー"], ["リーガルリリー"]),
    (33, "obscure", "Inside Out", "Duster", 14875527, ["Inside Out"], ["Duster"]),
    (34, "instrumental", "Aruarian Dance", "Nujabes", 36237886, ["Aruarian Dance"], ["Nujabes"]),
    (35, "instrumental", "Ludovico Einaudi - Experience", "Ludovico Einaudi", 36315549, ["Experience"], ["Ludovico Einaudi"]),
    (36, "instrumental", "Summer (from Kikujiro)", "Joe Hisaishi", 36148240, ["Summer", "菊次郎の夏"], ["Joe Hisaishi", "久石譲"]),
]


def main():
    base = json.loads((HERE.parent / "llm-search/data/cases.json").read_text(encoding="utf-8"))
    out = list(base)
    for cid, kind, rt, ra, ref, titles, artists in EXTRA:
        rec = lrclib_retry(lrclib.by_id, ref)
        out.append({"id": cid, "kind": kind, "raw_title": rt, "raw_artist": ra, "ref_id": ref,
                    "title": titles, "artist": artists, "duration": round(rec["duration"])})
        state = "instrumental" if rec.get("instrumental") else "synced" if rec.get("syncedLyrics") else "plain"
        print(cid, kind, ref, state, round(rec["duration"]))
    (HERE / "data/cases.json").write_text(json.dumps(out, ensure_ascii=False, indent=1), encoding="utf-8")
    print(len(out), "cases", lrclib.stats)


if __name__ == "__main__":
    main()
