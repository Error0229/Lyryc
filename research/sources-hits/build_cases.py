"""cases.json for the sources hit-rate harness (src-tauri/src/sources/diag.rs).

Takes the llm-search cases and Gemini's search-off identifications as they are,
adds the playing length (the music-video length for MV uploads, see
yt_lengths.json), and appends the extra cases below. No LLM calls: the extra
cases carry hand-written identities, marked `"identity_by": "hand"`.

    python -X utf8 build_cases.py
"""
import json
from pathlib import Path

HERE = Path(__file__).parent
LLM = HERE.parent / "llm-search"

# Raw title -> length of the YouTube upload (yt_lengths.py, picked by hand
# where the title differed only in bracket style).
MV = {
    11: 226,  # YOASOBI「アイドル」 Official Music Video (album 214)
    13: 224,  # 稻香 MV (album 224)
    14: 222,  # [MV] IU _ Blueming (album 217)
    15: 322,  # Official髭男dism - Subtitle [Official Video] (album 306)
    16: 277,  # ヨルシカ - 晴る（OFFICIAL VIDEO） (album 271)
    23: 254,  # ZUTOMAYO - MIRROR TUNE (Music Video) (album 254)
    24: 311,  # 草東沒有派對 - 山海 Wayfarer【Official Music Video】 (album 252)
}

# Romanized / translated titles of CJK songs, songs with many covers, and MV
# uploads. Album lengths from the iTunes Search API (LRCLIB answered 503).
# Hou Lai / Rene Liu and Mayonaka no Door / Miki Matsubara were dropped to keep
# the provider request total near 250.
EXTRA = [
    (29, "romanized", "Gurenge", "LiSA", 239, None, ["紅蓮華"], ["Gurenge"], ["LiSA"], [], "Gurenge", "LiSA"),
    (30, "romanized", "Zankyou Sanka", "Aimer", 185, None, ["残響散歌"], ["Zankyou Sanka", "Zankyosanka"], ["Aimer"], [], "Zankyou Sanka", "Aimer"),
    (31, "romanized", "Gao Bai Qi Qiu", "Jay Chou", 215, None, ["告白氣球", "告白气球"], ["Gao Bai Qi Qiu", "Love Confession"], ["周杰倫", "周杰伦"], ["Jay Chou"], "Gao Bai Qi Qiu", "Jay Chou"),
    (33, "romanized", "Hai Kuo Tian Kong", "Beyond", 325, None, ["海闊天空", "海阔天空"], ["Hai Kuo Tian Kong", "Boundless Oceans, Vast Skies"], ["Beyond"], [], "Hai Kuo Tian Kong", "Beyond"),
    (34, "romanized", "Bam Yang Gang", "BIBI", 146, None, ["밤양갱"], ["Bam Yang Gang"], ["비비", "BIBI"], [], "Bam Yang Gang", "BIBI"),
    (36, "yt-video", "【Ado】うっせぇわ", "Ado", 205, 203, ["うっせぇわ"], ["Usseewa"], ["Ado"], [], "Usseewa", "Ado"),
    (37, "yt-video", "LiSA『紅蓮華』-MUSiC CLiP-（テレビアニメ「鬼滅の刃」竈門炭治郎 立志編 オープニングテーマ）", "LiSA", 239, 236, ["紅蓮華"], ["Gurenge"], ["LiSA"], [], "Gurenge", "LiSA"),
    (38, "romanized", "Hakujitsu", "King Gnu", 275, None, ["白日"], ["Hakujitsu"], ["King Gnu"], [], "Hakujitsu", "King Gnu"),
]


def identities():
    """The last search-off identification per case id."""
    out = {}
    for line in (LLM / "results/b_identify.jsonl").read_text(encoding="utf-8").splitlines():
        row = json.loads(line)
        if not row["search"]:
            out[row["id"]] = row.get("identity")
    return out


def main():
    ids = identities()
    cases = []
    for c in json.loads((LLM / "data/cases.json").read_text(encoding="utf-8")):
        album = c["duration"]
        play = MV.get(c["id"], album)
        cases.append({
            "id": c["id"], "kind": c["kind"],
            "raw_title": c["raw_title"], "raw_artist": c["raw_artist"],
            "album": album, "play": play, "mv": c["id"] in MV,
            "titles": (c["title"] or []) + c["alt_title"],
            "artists": (c["artist"] or []) + c["alt_artist"],
            "identity": pick(ids.get(c["id"])), "identity_by": "gemini",
        })
    for (i, kind, raw_t, raw_a, album, mv, t, alt_t, a, alt_a, rom_t, rom_a) in EXTRA:
        cases.append({
            "id": i, "kind": kind, "raw_title": raw_t, "raw_artist": raw_a,
            "album": album, "play": mv or album, "mv": mv is not None,
            "titles": t + alt_t, "artists": a + alt_a,
            "identity": {"title_native": t[0], "artist_native": a[0],
                         "title_romanized": rom_t, "artist_romanized": rom_a},
            "identity_by": "hand",
        })
    out = HERE / "cases.json"
    out.write_text(json.dumps(cases, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    print(f"{len(cases)} cases -> {out}")


def pick(identity):
    if not identity:
        return None
    keys = ("title_native", "artist_native", "title_romanized", "artist_romanized")
    return {k: identity.get(k) for k in keys}


main()
