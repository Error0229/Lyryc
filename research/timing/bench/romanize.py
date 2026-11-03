"""Romanize lyric text for a Latin-alphabet CTC aligner (MMS_FA).

Japanese goes through kana first (pykakasi) because uroman reads kanji as Mandarin.
In Lyryc this step would reuse the reading data the furigana module already computes.
"""
import re
import pykakasi
import uroman

_kks = pykakasi.kakasi()
_ur = uroman.Uroman()
KANA = re.compile(r"[぀-ヿ]")


def romanize(text, lang):
    if lang == "ja" or KANA.search(text):
        text = " ".join(x["hepburn"] for x in _kks.convert(text))
    text = _ur.romanize_string(text).lower()
    text = re.sub(r"[^a-z' ]+", " ", text.replace("’", "'"))
    return text.split()
