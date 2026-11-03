//! Regex cleaning of video-style titles: "Artist - Title (Official Video)",
//! "【MV】Title", "Artist「Title」". Already-clean titles pass through unchanged.
//! A dash only splits off the artist when the artist is known to be on one
//! side of it; "雪国 - Snow Country" is a bilingual title, not "Artist - Title".
//!
//! One pass, in order: drop noise brackets, find a quoted/bracketed title,
//! cut description tails, split off the artist, drop credits, tidy.

use regex::Regex;
use std::sync::LazyLock;

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).unwrap()
}

/// Bracket contents that are packaging, not part of the title. "(feat. X)" is
/// a credit, so it goes too; version markers like "(Acoustic)" stay.
static NOISE: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?i)official|video|audio|\bmv\b|lyric|visuali[sz]er|\b4k\b|\bhd\b|remaster|playlist|\bsub\b|中文|中字|字幕|歌詞|官方|公式|\bprod\b|from the series|bgm|插曲|片尾曲|片頭曲|original|オリジナル|原创|原創|翻唱|歌ってみた|self cover|^\s*(?:feat|ft|featuring)\b")
});
/// (...), [...] and 【...】 groups; fullwidth parentheses are normalized first.
static BRACKETS: LazyLock<Regex> = LazyLock::new(|| re(r"\(([^()]*)\)|\[([^\[\]]*)\]|【([^【】]*)】"));
/// Japanese/Chinese title quotes: the song name in a video title.
static QUOTED: LazyLock<Regex> = LazyLock::new(|| re(r"「([^」]+)」|『([^』]+)』|《([^》]+)》"));
/// Packaging after the title: "Official Music Video", "- YouTube Music", ".mp4".
static TAIL: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?i)(?:\s*-\s*youtube(?:\s*music)?|(?:\s*-)?\s+(?:official\s+|music\s+|lyrics?\s+|官方版\s*)*(?:video|mv|audio)\b[\s-]*|\.(?:flv|mp4|avi|mov|wmv|mkv)|を?\s*歌ってみた.*)$")
});
/// Credits outside brackets: "feat. X" up to the next bracket, a trailing "cover".
static CREDIT: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)\s+(?:feat|ft|featuring)\b\.?[^(\[]*|\s+cover$"));
/// What follows a slash when it is a credit rather than part of the title.
static SLASH_CREDIT: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)\bfeat|\bft\.|×|\bcover\b|first take|歌ってみた"));
/// "'Title' Suffix" or "\"Title\" (Suffix)".
static QUOTED_ASCII: LazyLock<Regex> = LazyLock::new(|| re(r#"^(?:'([^']+)'|"([^"]+)")\s*(.*)$"#));
static SPACES: LazyLock<Regex> = LazyLock::new(|| re(r"\s+"));

/// Best guess at the song title when the artist is unknown: "A - B" is
/// taken to be "Artist - Title".
pub fn clean_track_name_sync(title: &str) -> String {
    clean_title(title, "")
}

/// Best guess at the song title in `title` by `artist`. With an artist, a
/// separator splits off only a part that names the artist; otherwise the
/// title stays whole ("雪国 - Snow Country" by Yorushika).
pub fn clean_title(title: &str, artist: &str) -> String {
    let s = title.replace('（', " (").replace('）', ") ");
    let s = drop_noise_brackets(&SPACES.replace_all(&s, " "));
    let s = match title_in_brackets(&s) {
        Some(t) => t,
        None => {
            let s = drop_tag_brackets(TAIL.replace(&s, "").trim());
            match QUOTED.captures(&s) {
                Some(c) => first_group(&c).to_string(),
                None => drop_artist(cut_description(&s), artist).to_string(),
            }
        }
    };
    let s = drop_slash_credit(&s);
    let s = CREDIT.replace_all(&s, "");
    let s = TAIL.replace(s.trim(), "");
    tidy(s.trim())
}

/// Lowercase words, with fullwidth ASCII folded and punctuation dropped:
/// "ＤＡＯＫＯ", "Daoko" and "daoko!" are all "daoko".
pub fn normalize_name(s: &str) -> String {
    let folded: String = s
        .chars()
        .map(|c| match c as u32 {
            0xff01..=0xff5e => char::from_u32(c as u32 - 0xfee0).unwrap_or(c),
            _ => c,
        })
        .flat_map(char::to_lowercase)
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    SPACES.replace_all(folded.trim(), " ").into_owned()
}

/// Whether a title part names the artist: equal, or one holds the other
/// ("Tsumiki feat. KAFU" / "Tsumiki", "ヨルシカ" / "ヨルシカ / n-buna Official").
fn names_artist(part: &str, artist: &str) -> bool {
    let (p, a) = (normalize_name(part).replace(' ', ""), normalize_name(artist).replace(' ', ""));
    let long_enough = |s: &str| s.chars().count() >= 2;
    !p.is_empty() && !a.is_empty() && (p == a || (long_enough(&a) && p.contains(&a)) || (long_enough(&p) && a.contains(&p)))
}

fn first_group<'a>(c: &regex::Captures<'a>) -> &'a str {
    c.iter().skip(1).flatten().next().map_or("", |m| m.as_str()).trim()
}

fn drop_noise_brackets(s: &str) -> String {
    BRACKETS
        .replace_all(s, |c: &regex::Captures| {
            if NOISE.is_match(first_group(c)) { " ".to_string() } else { c[0].to_string() }
        })
        .trim()
        .to_string()
}

/// "魏如萱【晚安晚安】 Official Music Video": a bracket between the artist and
/// the packaging holds the title.
fn title_in_brackets(s: &str) -> Option<String> {
    BRACKETS.captures_iter(s).find_map(|c| {
        let m = c.get(0)?;
        let (before, after) = (s[..m.start()].trim(), s[m.end()..].trim());
        let boxed = !m.as_str().starts_with('(');
        (boxed && !before.is_empty() && !after.is_empty() && TAIL.replace(&format!(" {after}"), "").trim().is_empty())
            .then(|| first_group(&c).to_string())
    })
}

/// A leading 【tag】 or a trailing 【singer】 / [credit] is not the title.
fn drop_tag_brackets(s: &str) -> String {
    let mut s = s.to_string();
    while let Some(m) = BRACKETS.find(&s).filter(|m| m.start() == 0 && !m.as_str().starts_with('(')) {
        s = s[m.end()..].trim().to_string();
    }
    if let Some(m) = BRACKETS.find_iter(&s).last().filter(|m| m.end() == s.len() && m.start() > 0 && !m.as_str().starts_with('(')) {
        s = s[..m.start()].trim().to_string();
    }
    s
}

/// "Title | album | track 1", "Title // Official Music Video": the rest is description.
fn cut_description(s: &str) -> &str {
    let end = ["|", "∣", " // "]
        .iter()
        .filter_map(|sep| split_outside_brackets(s, sep, |_, _| true))
        .map(|(first, _)| first.len())
        .min();
    end.map_or(s, |n| s[..n].trim_end_matches(['-', '–', '—', ' ']))
}

/// "Artist - Title" -> "Title". With an artist, only a part naming it goes.
/// Without one, three or more parts are "Title - Artist - extra".
fn drop_artist<'a>(s: &'a str, artist: &str) -> &'a str {
    let dash = |prev: Option<char>, next: Option<char>| {
        next.is_some_and(char::is_whitespace) || prev.is_some_and(is_cjk) || next.is_some_and(is_cjk)
    };
    let split = [" - ", " – ", " — ", " · ", " × ", " • "]
        .iter()
        .find_map(|sep| split_outside_brackets(s, sep, |_, _| true).map(|p| (p, *sep)))
        .or_else(|| ["-", "–", "—"].iter().find_map(|sep| split_outside_brackets(s, sep, dash).map(|p| (p, *sep))));
    let Some(((first, second), sep)) = split else {
        return s;
    };
    if !artist.trim().is_empty() {
        return match (names_artist(first, artist), names_artist(second, artist)) {
            (true, _) => second,
            (_, true) => first,
            _ => s,
        };
    }
    if split_outside_brackets(second, sep, |_, _| true).is_some() {
        return first;
    }
    // "私じゃなかったんだね。 - Watashijyanakattandane.": a romanization, not a title.
    let romaji = second.len() >= 12 && !second.contains(' ') && second.is_ascii();
    if romaji && first.chars().any(is_cjk) {
        first
    } else {
        second
    }
}

/// "夜明けと蛍 / ナブナ", "Title／cover Singer": keep what precedes a credit.
/// "Television / So Far So Good" and "ハロ／ハワユ" are titles and stay.
fn drop_slash_credit(s: &str) -> String {
    let split = [" / ", "／"].iter().find_map(|sep| split_outside_brackets(s, sep, |_, _| true));
    let Some((first, second)) = split else {
        return s.to_string();
    };
    let lower = second.to_lowercase();
    if lower.ends_with(" cover") && !lower.starts_with("cover") {
        // "STAY / Japanese Cover" is a version of the song.
        return format!("{first} ({second})");
    }
    let spaced = s.contains(" / ") || s.contains(" ／ ");
    let cjk_pair = first.chars().any(is_cjk) && second.chars().any(is_cjk);
    if SLASH_CREDIT.is_match(second) || (spaced && cjk_pair) {
        first.to_string()
    } else {
        s.to_string()
    }
}

/// First split at `sep` outside brackets with text on both sides, where
/// `ok(char before, char after)` accepts the position.
fn split_outside_brackets<'a>(
    s: &'a str,
    sep: &str,
    ok: impl Fn(Option<char>, Option<char>) -> bool,
) -> Option<(&'a str, &'a str)> {
    s.match_indices(sep)
        .filter(|&(i, _)| bracket_depth(&s[..i]) == 0)
        .filter(|&(i, _)| ok(s[..i].chars().next_back(), s[i + sep.len()..].chars().next()))
        .map(|(i, _)| (s[..i].trim(), s[i + sep.len()..].trim()))
        .find(|(a, b)| !a.is_empty() && !b.is_empty())
}

fn bracket_depth(prefix: &str) -> usize {
    prefix.chars().fold(0, |depth, c| match c {
        '(' | '[' | '【' | '「' | '『' | '《' => depth + 1,
        ')' | ']' | '】' | '」' | '』' | '》' => depth.saturating_sub(1),
        _ => depth,
    })
}

fn is_cjk(c: char) -> bool {
    matches!(c as u32, 0x3040..=0x30ff | 0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xac00..=0xd7af)
}

/// Unwrap quotes ("'Title' Remix" -> "Title (Remix)"), one space before "(".
fn tidy(s: &str) -> String {
    let s = s.trim_matches(['「', '」']).trim();
    let s = match QUOTED_ASCII.captures(s) {
        Some(c) => {
            let (title, rest) = (first_group(&c), c[3].trim());
            match rest {
                "" => title.to_string(),
                r if r.starts_with(['(', '[']) => format!("{title} {r}"),
                r => format!("{title} ({r})"),
            }
        }
        None => s.to_string(),
    };
    let s = s.replace('(', " (").replace("( ", "(");
    SPACES.replace_all(&s, " ").replace(" )", ")").trim().to_string()
}
