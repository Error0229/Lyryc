//! Lyric text → alignment pieces: a Latin word, or one CJK / kana / hangul
//! character, each with the romanized letters the CTC model is asked to find.
//!
//! A port of the study's romanizer (uroman + pykakasi, `research/prealign/bench/align.py`):
//! - Latin: lowercase, accents folded, `[a-z']` kept.
//! - Hangul: uroman's jamo table (initial + vowel + final), exact.
//! - Chinese: toneless pinyin.
//! - Japanese (any line of a song that has kana): kana by Hepburn; kanji readings
//!   from the UniDic tokenizer autoruby already ships, split over the kanji by
//!   matching the okurigana.

use lindera_core::mode::Mode;
use lindera_dictionary::{DictionaryConfig, DictionaryKind};
use lindera_tokenizer::tokenizer::{Tokenizer, TokenizerConfig};
use pinyin::ToPinyin;

/// One unit of word timing: `chars` characters of the line from character `pos`.
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    pub pos: usize,
    pub len: usize,
    pub rom: String,
}

/// The study's CJK class: kana (incl. ー and small kana), CJK ideographs, hangul syllables.
pub fn is_cjk(c: char) -> bool {
    matches!(c, '\u{3040}'..='\u{30FF}' | '\u{3400}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}' | '\u{AC00}'..='\u{D7AF}')
}

fn is_kana(c: char) -> bool {
    matches!(c, '\u{3040}'..='\u{30FF}')
}

fn is_hangul(c: char) -> bool {
    matches!(c, '\u{AC00}'..='\u{D7A3}')
}

fn is_latin(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '\u{C0}'..='\u{24F}' | '\'' | '\u{2019}')
}

/// Does this song's text call for Japanese readings? (The study used the track's
/// language; any kana in the lyrics is the app's stand-in for it.)
pub fn is_japanese<'a>(lines: impl IntoIterator<Item = &'a str>) -> bool {
    lines.into_iter().any(|l| l.chars().any(|c| matches!(c, '\u{3041}'..='\u{30FA}')))
}

/// Pieces for every line of a song. The UniDic tokenizer (~550 MB resident) is
/// built only if some line needs Japanese readings, and dropped on return.
pub fn plan(lines: &[String]) -> Vec<Vec<Piece>> {
    let japanese = is_japanese(lines.iter().map(String::as_str));
    let tokenizer = (japanese || lines.iter().any(|l| l.chars().any(is_kana))).then(tokenizer).flatten();
    lines.iter().map(|l| pieces_with(l, japanese, tokenizer.as_ref())).collect()
}

/// Pieces of one line. `japanese`: read kanji as Japanese, not Mandarin.
fn pieces_with(text: &str, japanese: bool, tokenizer: Option<&Tokenizer>) -> Vec<Piece> {
    let chars: Vec<char> = text.chars().collect();
    if japanese || chars.iter().copied().any(is_kana) {
        return ja_pieces(&chars, tokenizer);
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if is_latin(c) {
            let start = i;
            while i < chars.len() && is_latin(chars[i]) {
                i += 1;
            }
            out.push(latin_piece(&chars, start, i));
        } else {
            if is_cjk(c) {
                out.push(Piece { pos: i, len: 1, rom: cjk_roman(c) });
            }
            i += 1;
        }
    }
    out
}

fn latin_piece(chars: &[char], start: usize, end: usize) -> Piece {
    let word: String = chars[start..end].iter().collect();
    Piece { pos: start, len: end - start, rom: latin_roman(&word) }
}

/// uroman for Latin: fold accents, keep letters and apostrophes.
pub fn latin_roman(word: &str) -> String {
    let mut out = String::new();
    for c in word.chars().flat_map(char::to_lowercase) {
        match c {
            'a'..='z' | '\'' => out.push(c),
            '\u{2019}' => out.push('\''),
            _ => out.push_str(fold(c)),
        }
    }
    out
}

/// Latin-1 Supplement and Latin Extended-A/B letters to ASCII.
fn fold(c: char) -> &'static str {
    match c {
        'à'..='å' | 'ā' | 'ă' | 'ą' | 'ǎ' => "a",
        'æ' => "ae",
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => "c",
        'ð' | 'ď' | 'đ' => "d",
        'è'..='ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => "e",
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => "g",
        'ĥ' | 'ħ' => "h",
        'ì'..='ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' | 'ǐ' => "i",
        'ĵ' => "j",
        'ķ' => "k",
        'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => "l",
        'ñ' | 'ń' | 'ņ' | 'ň' => "n",
        'ò'..='ö' | 'ø' | 'ō' | 'ŏ' | 'ő' | 'ǒ' => "o",
        'œ' => "oe",
        'ŕ' | 'ŗ' | 'ř' => "r",
        'ß' => "ss",
        'ś' | 'ŝ' | 'ş' | 'š' => "s",
        'ţ' | 'ť' | 'ŧ' => "t",
        'þ' => "th",
        'ù'..='ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' | 'ǔ' | 'ǖ' | 'ǘ' | 'ǚ' | 'ǜ' => "u",
        'ŵ' => "w",
        'ý' | 'ÿ' | 'ŷ' => "y",
        'ź' | 'ż' | 'ž' => "z",
        _ => "",
    }
}

/// A hangul syllable or a Han character, outside Japanese.
fn cjk_roman(c: char) -> String {
    if is_hangul(c) {
        return hangul_roman(c);
    }
    match c.to_pinyin() {
        Some(p) => p.plain().replace(['ü', 'v'], "u"),
        None => String::new(),
    }
}

// uroman's hangul table, checked against uroman on 3000 random syllables.
const INITIAL: [&str; 19] = ["g", "gg", "n", "d", "dd", "r", "m", "b", "bb", "s", "ss", "", "j", "jj", "c", "k", "t", "p", "h"];
const VOWEL: [&str; 21] = [
    "a", "ae", "ya", "yae", "eo", "e", "yeo", "ye", "o", "wa", "wai", "oe", "yo", "u", "weo", "we", "wi", "yu", "eu", "yi", "i",
];
const FINAL: [&str; 28] = [
    "", "g", "gg", "gs", "n", "nj", "nh", "d", "l", "lg", "lm", "lb", "ls", "lt", "lp", "lh", "m", "b", "bs", "s", "ss", "ng",
    "j", "c", "k", "t", "p", "h",
];

pub fn hangul_roman(c: char) -> String {
    let s = c as u32 - 0xAC00;
    [INITIAL[(s / 588) as usize], VOWEL[(s % 588 / 28) as usize], FINAL[(s % 28) as usize]].concat()
}

// ---------------------------------------------------------------------------
// Japanese.

fn tokenizer() -> Option<Tokenizer> {
    let config = TokenizerConfig {
        dictionary: DictionaryConfig { kind: Some(DictionaryKind::UniDic), path: None },
        user_dictionary: None,
        mode: Mode::Normal,
    };
    Tokenizer::from_config(config).map_err(|e| log::warn!("UniDic tokenizer unavailable: {e}")).ok()
}

/// UniDic's kana reading of a token (発音形出現形 in katakana), if it has one.
const READING_FIELD: usize = 9;

fn ja_pieces(chars: &[char], tokenizer: Option<&Tokenizer>) -> Vec<Piece> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let start = i;
        if is_ja(chars[i]) {
            while i < chars.len() && is_ja(chars[i]) {
                i += 1;
            }
            out.extend(ja_run(&chars[start..i], start, tokenizer));
        } else if is_latin(chars[i]) {
            while i < chars.len() && is_latin(chars[i]) {
                i += 1;
            }
            out.push(latin_piece(chars, start, i));
        } else {
            i += 1;
        }
    }
    out
}

/// Kana, kanji and the iteration / closing marks (々 〆 〇) that read as part of a word.
fn is_ja(c: char) -> bool {
    is_cjk(c) || matches!(c, '\u{3005}'..='\u{3007}')
}

/// A run of Japanese text: tokenized for kanji readings, then romanized as one
/// sequence so a small tsu doubles the next consonant even across tokens.
fn ja_run(run: &[char], offset: usize, tokenizer: Option<&Tokenizer>) -> Vec<Piece> {
    let text: String = run.iter().collect();
    let tokens: Vec<(Vec<char>, Option<String>)> = tokenizer
        .and_then(|t| t.tokenize(&text).ok())
        .map(|mut toks| {
            toks.iter_mut()
                .map(|tok| {
                    let reading = tok.get_details().and_then(|d| d.get(READING_FIELD).map(|s| s.to_string()));
                    (tok.text.chars().collect(), reading.filter(|r| r.chars().all(|c| is_kana(c) || c == 'ー')))
                })
                .collect()
        })
        .unwrap_or_else(|| vec![(run.to_vec(), None)]);

    let mut shares: Vec<Vec<char>> = Vec::with_capacity(run.len());
    for (surface, reading) in &tokens {
        shares.extend(token_shares(surface, reading.as_deref()));
    }
    if shares.len() != run.len() {
        shares = token_shares(run, None); // the tokenizer did not cover the text exactly
    }
    let roms = romaji_shares(&shares);

    let mut out: Vec<Piece> = Vec::new();
    let mut carry = String::new();
    for (i, &c) in run.iter().enumerate() {
        if is_cjk(c) {
            let mut rom = std::mem::take(&mut carry);
            rom.push_str(&roms[i]);
            out.push(Piece { pos: offset + i, len: 1, rom });
        } else if let Some(last) = out.last_mut() {
            last.rom.push_str(&roms[i]); // 々 and the like: their sound belongs to the previous character
        } else {
            carry.push_str(&roms[i]);
        }
    }
    out
}

/// One token's characters with their share of its kana reading.
fn token_shares(chars: &[char], reading: Option<&str>) -> Vec<Vec<char>> {
    let kana: Vec<char> = chars.iter().map(|&c| to_katakana(c)).collect();
    let all_kana = chars.iter().all(|&c| is_kana(c));
    // Kana are read as written (as pykakasi does); only kanji need the dictionary.
    let reading: Vec<char> = match reading {
        Some(r) if !all_kana => r.chars().collect(),
        _ => kana.iter().copied().filter(|&c| is_kana(c)).collect(),
    };
    split_reading(&kana, &reading)
}

/// Which slice of `reading` each surface character stands for. Kana match
/// themselves; each run of kanji takes what lies between, shared evenly.
fn split_reading(surface: &[char], reading: &[char]) -> Vec<Vec<char>> {
    let mut shares = vec![Vec::new(); surface.len()];
    let mut r = 0;
    let mut i = 0;
    while i < surface.len() {
        if is_kana(surface[i]) {
            if r < reading.len() && same_kana(reading[r], surface[i]) {
                shares[i].push(reading[r]);
                r += 1;
            } else if r < reading.len() && surface[i] != 'ー' {
                // Reading disagrees with the text (e.g. は read ワ): use the text.
                shares[i].push(surface[i]);
                r += 1;
            } else {
                shares[i].push(surface[i]);
            }
            i += 1;
            continue;
        }
        // A kanji run: up to the next kana of the surface found in the reading.
        let run_end = (i..surface.len()).find(|&j| is_kana(surface[j])).unwrap_or(surface.len());
        let stop = if run_end < surface.len() {
            // The last occurrence that still leaves room for the rest of the surface kana.
            let rest = surface[run_end..].iter().filter(|c| is_kana(**c)).count();
            (r + 1..reading.len())
                .filter(|&k| same_kana(reading[k], surface[run_end]) && reading.len() - k >= rest)
                .min()
                .unwrap_or(reading.len().saturating_sub(rest).max(r))
        } else {
            reading.len()
        };
        let part = &reading[r..stop.max(r)];
        let n = run_end - i;
        for (k, share) in even_split(part, n).into_iter().enumerate() {
            shares[i + k] = share;
        }
        r = stop.max(r);
        i = run_end;
    }
    shares
}

/// `part` cut into `n` contiguous pieces, earlier ones a mora longer if uneven.
/// Small kana stay with the mora before them.
fn even_split(part: &[char], n: usize) -> Vec<Vec<char>> {
    let mut morae: Vec<Vec<char>> = Vec::new();
    for &c in part {
        match morae.last_mut() {
            Some(m) if is_small(c) => m.push(c),
            _ => morae.push(vec![c]),
        }
    }
    let (base, extra) = (morae.len() / n, morae.len() % n);
    let mut out = Vec::with_capacity(n);
    let mut it = morae.into_iter();
    for k in 0..n {
        let take = base + usize::from(k < extra);
        out.push(it.by_ref().take(take).flatten().collect());
    }
    out
}

fn is_small(c: char) -> bool {
    matches!(to_katakana(c), 'ァ' | 'ィ' | 'ゥ' | 'ェ' | 'ォ' | 'ャ' | 'ュ' | 'ョ' | 'ヮ')
}

fn to_katakana(c: char) -> char {
    match c {
        '\u{3041}'..='\u{3096}' => char::from_u32(c as u32 + 0x60).unwrap_or(c),
        _ => c,
    }
}

fn same_kana(a: char, b: char) -> bool {
    to_katakana(a) == to_katakana(b)
}

/// Romaji for each share, with っ doubling the next consonant and ー repeating
/// the previous vowel across share boundaries.
fn romaji_shares(shares: &[Vec<char>]) -> Vec<String> {
    let flat: Vec<(usize, char)> = shares.iter().enumerate().flat_map(|(i, s)| s.iter().map(move |&c| (i, c))).collect();
    let mut out = vec![String::new(); shares.len()];
    let mut prev_vowel: Option<char> = None;
    let mut geminate = false;
    for (k, &(owner, c)) in flat.iter().enumerate() {
        let c = to_katakana(c);
        let next_small = flat.get(k + 1).map(|&(_, n)| to_katakana(n));
        let mut rom = match c {
            'ッ' => {
                geminate = true;
                continue;
            }
            'ー' => prev_vowel.map(String::from).unwrap_or_default(),
            _ if is_small(c) => small_romaji(c, k.checked_sub(1).map(|p| to_katakana(flat[p].1))),
            _ => {
                let base = kana_romaji(c).to_string();
                match (base.as_str(), next_small) {
                    // Digraphs: the consonant here, the vowel with the small kana.
                    (b, Some('ャ' | 'ュ' | 'ョ')) if b.ends_with('i') && b.len() > 1 => b[..b.len() - 1].to_string(),
                    (b, Some('ァ' | 'ィ' | 'ェ' | 'ォ')) if matches!(c, 'フ' | 'ヴ') => b[..b.len() - 1].to_string(),
                    ("chi", Some('ェ')) => "ch".to_string(),
                    _ => base,
                }
            }
        };
        if geminate {
            geminate = false;
            if let Some(first) = rom.chars().next().filter(|f| !"aeioun".contains(*f)) {
                let lead = if rom.starts_with("ch") { 't' } else { first };
                rom.insert(0, lead);
            }
        }
        prev_vowel = rom.chars().last().filter(|v| "aeiou".contains(*v)).or(prev_vowel);
        out[owner].push_str(&rom);
    }
    out
}

/// A small kana after its big one: the vowel or y-glide it adds.
fn small_romaji(c: char, prev: Option<char>) -> String {
    let after_sh = matches!(prev, Some('シ' | 'チ' | 'ジ' | 'ヂ'));
    match c {
        'ャ' if after_sh => "a".into(),
        'ュ' if after_sh => "u".into(),
        'ョ' if after_sh => "o".into(),
        'ャ' => "ya".into(),
        'ュ' => "yu".into(),
        'ョ' => "yo".into(),
        'ァ' => "a".into(),
        'ィ' => "i".into(),
        'ゥ' => "u".into(),
        'ェ' => "e".into(),
        'ォ' => "o".into(),
        'ヮ' => "wa".into(),
        _ => String::new(),
    }
}

fn kana_romaji(c: char) -> &'static str {
    match c {
        'ア' => "a", 'イ' => "i", 'ウ' => "u", 'エ' => "e", 'オ' => "o",
        'カ' | 'ヵ' => "ka", 'キ' => "ki", 'ク' => "ku", 'ケ' | 'ヶ' => "ke", 'コ' => "ko",
        'ガ' => "ga", 'ギ' => "gi", 'グ' => "gu", 'ゲ' => "ge", 'ゴ' => "go",
        'サ' => "sa", 'シ' => "shi", 'ス' => "su", 'セ' => "se", 'ソ' => "so",
        'ザ' => "za", 'ジ' => "ji", 'ズ' => "zu", 'ゼ' => "ze", 'ゾ' => "zo",
        'タ' => "ta", 'チ' => "chi", 'ツ' => "tsu", 'テ' => "te", 'ト' => "to",
        'ダ' => "da", 'ヂ' => "ji", 'ヅ' => "zu", 'デ' => "de", 'ド' => "do",
        'ナ' => "na", 'ニ' => "ni", 'ヌ' => "nu", 'ネ' => "ne", 'ノ' => "no",
        'ハ' => "ha", 'ヒ' => "hi", 'フ' => "fu", 'ヘ' => "he", 'ホ' => "ho",
        'バ' => "ba", 'ビ' => "bi", 'ブ' => "bu", 'ベ' => "be", 'ボ' => "bo",
        'パ' => "pa", 'ピ' => "pi", 'プ' => "pu", 'ペ' => "pe", 'ポ' => "po",
        'マ' => "ma", 'ミ' => "mi", 'ム' => "mu", 'メ' => "me", 'モ' => "mo",
        'ヤ' => "ya", 'ユ' => "yu", 'ヨ' => "yo",
        'ラ' => "ra", 'リ' => "ri", 'ル' => "ru", 'レ' => "re", 'ロ' => "ro",
        'ワ' => "wa", 'ヰ' => "i", 'ヱ' => "e", 'ヲ' => "wo", 'ン' => "n", 'ヴ' => "vu",
        _ => "",
    }
}

/// One line's pieces with a tokenizer shared across tests.
#[cfg(test)]
pub fn pieces(text: &str, japanese: bool) -> Vec<Piece> {
    static TOK: std::sync::LazyLock<Option<Tokenizer>> = std::sync::LazyLock::new(tokenizer);
    pieces_with(text, japanese, TOK.as_ref())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roms(text: &str, ja: bool) -> Vec<(String, String)> {
        let chars: Vec<char> = text.chars().collect();
        pieces(text, ja).into_iter().map(|p| (chars[p.pos..p.pos + p.len].iter().collect(), p.rom)).collect()
    }

    fn joined(text: &str, ja: bool) -> String {
        pieces(text, ja).into_iter().map(|p| p.rom).collect()
    }

    #[test]
    fn latin_words_fold_accents_and_keep_apostrophes() {
        assert_eq!(
            roms("Café naïve, don’t stop 123", false),
            [("Café", "cafe"), ("naïve", "naive"), ("don’t", "don't"), ("stop", "stop"), ("123", "")]
                .map(|(a, b)| (a.to_string(), b.to_string()))
        );
    }

    #[test]
    fn hangul_matches_uroman() {
        // Expected strings are uroman's output (research venv).
        assert_eq!(joined("사랑해요 너를", false), "saranghaeyoneoreul");
        assert_eq!(joined("봄날 그리워", false), "bomnalgeuriweo");
        assert_eq!(joined("닭 값 읽다 앉아", false), "dalggabsilgdaanja");
        assert_eq!(roms("좋아", false)[0].1, "joh");
        assert_eq!(pieces("How You Like That 뚜두뚜두", false).len(), 8, "one piece per word and per syllable");
    }

    #[test]
    fn chinese_is_toneless_pinyin_per_character() {
        assert_eq!(joined("我爱你 女人 绿色 雙截棍 晴天 小幸運", false), "woaininurenluseshuangjiegunqingtianxiaoxingyun");
        assert_eq!(pieces("告白氣球", false).len(), 4);
    }

    #[test]
    fn kana_follow_hepburn() {
        // pykakasi's Hepburn, as the study used it.
        assert_eq!(joined("あいしてる", true), "aishiteru");
        assert_eq!(joined("ラーメン", true), "raamen");
        assert_eq!(joined("キャッチー", true), "kyatchii");
        assert_eq!(joined("ちょっと", true), "chotto");
        assert_eq!(joined("がっこう", true), "gakkou");
        assert_eq!(joined("ファン", true), "fan");
        assert_eq!(joined("アイドル", true), "aidoru");
        assert_eq!(joined("きっ", true), "ki", "a trailing small tsu is silence");
        let p = roms("かった", true);
        assert_eq!(p, [("か", "ka"), ("っ", ""), ("た", "tta")].map(|(a, b)| (a.to_string(), b.to_string())));
    }

    #[test]
    fn kanji_take_the_reading_between_their_okurigana() {
        let p = roms("振り返る", true);
        assert_eq!(p.iter().map(|x| x.1.as_str()).collect::<String>(), "furikaeru");
        assert_eq!(p[1], ("り".to_string(), "ri".to_string()));
        assert_eq!(p[3], ("る".to_string(), "ru".to_string()));
        let p = roms("夜に駆ける", true);
        assert_eq!(p[0].1, "yoru");
        assert_eq!(p[2].1, "ka");
        // One piece per character, and a kanji-only line is still read as Japanese.
        assert_eq!(pieces("紅蓮華", true).len(), 3);
        assert!(joined("世界", true).starts_with("se"), "{}", joined("世界", true));
    }

    #[test]
    fn mixed_lines_keep_latin_words_whole() {
        let p = roms("I love you 愛してる", true);
        assert_eq!(p[0], ("I".to_string(), "i".to_string()));
        assert_eq!(p[2], ("you".to_string(), "you".to_string()));
        assert_eq!(p.len(), 3 + 4);
    }

    #[test]
    fn a_song_is_japanese_if_it_has_kana() {
        assert!(is_japanese(["夜空", "きみと"]));
        assert!(!is_japanese(["晴天", "故事的小黃花"]));
    }
}
