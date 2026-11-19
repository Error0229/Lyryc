//! The fonts installed on this machine, for the lyric font pickers: each
//! family with the scripts it is made for, so a language's picker lists only
//! fonts that can write it. Read from DirectWrite, so Windows only, like the
//! app's builds; elsewhere the list is empty. Read again each time the
//! Appearance page opens (once for all its pickers, ~0.2 s off the main
//! thread), so a font installed meanwhile shows up without a restart.

use serde::Serialize;

/// An installed font family, by the name the webview matches CSS
/// `font-family` against, and the scripts (`SCRIPTS` codes) it writes.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SystemFont {
    name: String,
    scripts: Vec<&'static str>,
}

/// A script a font can be made for. `code` is the language family the
/// frontend names lines by (`lineLanguage`, lib/fonts.ts FONT_LANGUAGES), or
/// "latin". A font writes it when it has a glyph for every `sample`
/// character; for the four CJK ones, whose fonts mostly carry each other's
/// characters too (a Chinese font has kana, Japanese fonts have most Han), it
/// must also be made for it: its OS/2 code page bit (`codepage`) is set, or it
/// claims none of the four.
struct Script {
    code: &'static str,
    sample: &'static str,
    codepage: Option<u32>,
}

const SCRIPTS: &[Script] = &[
    Script { code: "latin", sample: "AaZzéß", codepage: None },
    Script { code: "ja", sample: "あアの漢", codepage: Some(17) },
    Script { code: "zh", sample: "这们说汉", codepage: Some(18) },
    Script { code: "ko", sample: "가한글", codepage: Some(19) },
    Script { code: "zh-tw", sample: "這們說漢", codepage: Some(20) },
    Script { code: "ru", sample: "ЖЯжя", codepage: None },
    Script { code: "el", sample: "ΩΣωσ", codepage: None },
    Script { code: "he", sample: "אשת", codepage: None },
    Script { code: "ar", sample: "بعم", codepage: None },
    Script { code: "hi", sample: "कहा", codepage: None },
    Script { code: "th", sample: "กขค", codepage: None },
];

/// OS/2 ulCodePageRange1 bits of the four CJK code pages (932, 936, 949, 950).
const CJK_CODEPAGES: u32 = (1 << 17) | (1 << 18) | (1 << 19) | (1 << 20);
/// OS/2 ulCodePageRange1 "Symbol character set": its letters' codes draw
/// pictures (Wingdings), so it writes no script.
const SYMBOL_CODEPAGE: u32 = 1 << 31;

/// What one font says about itself: does it have a glyph for `c`, and its
/// OS/2 ulCodePageRange1 (0 when it has none).
trait Face {
    fn has(&self, c: char) -> bool;
    fn codepages(&self) -> u32;
}

/// OS/2's ulCodePageRange1 (big-endian at byte 78), which version 1 added;
/// 0 for a version 0 table, or none.
fn codepage_range(os2: &[u8]) -> u32 {
    let version = os2.get(0..2).map_or(0, |b| u16::from_be_bytes([b[0], b[1]]));
    match os2.get(78..82) {
        Some(b) if version >= 1 => u32::from_be_bytes([b[0], b[1], b[2], b[3]]),
        _ => 0,
    }
}

/// The scripts a font writes, in `SCRIPTS` order.
fn scripts_of(face: &impl Face) -> Vec<&'static str> {
    let pages = face.codepages();
    if pages & SYMBOL_CODEPAGE != 0 {
        return Vec::new();
    }
    let made_for = |bit: Option<u32>| bit.is_none_or(|b| pages & CJK_CODEPAGES == 0 || pages & (1 << b) != 0);
    SCRIPTS.iter().filter(|s| made_for(s.codepage) && s.sample.chars().all(|c| face.has(c))).map(|s| s.code).collect()
}

/// The list as the pickers show it: alphabetical whatever the case, each name
/// once, without fonts that write none of the scripts (symbols, icons).
/// (DirectWrite already lists a family's weights and widths as one family.)
fn listed(fonts: Vec<SystemFont>) -> Vec<SystemFont> {
    let mut fonts: Vec<SystemFont> = fonts.into_iter().filter(|f| !f.scripts.is_empty() && !f.name.trim().is_empty()).collect();
    fonts.sort_by_cached_key(|f| (f.name.to_lowercase(), f.name.clone()));
    fonts.dedup_by(|a, b| a.name == b.name);
    fonts
}

#[tauri::command]
pub async fn system_fonts() -> Result<Vec<SystemFont>, String> {
    tokio::task::spawn_blocking(|| listed(installed())).await.map_err(|e| e.to_string())
}

#[cfg(windows)]
mod directwrite {
    use super::{scripts_of, Face, SystemFont};
    use dwrote::{FontCollection, FontStretch, FontStyle, FontWeight};

    struct DwFace {
        face: dwrote::FontFace,
    }

    impl Face for DwFace {
        fn has(&self, c: char) -> bool {
            self.face.glyph_indices(&[c as u32]).is_ok_and(|g| g.first().is_some_and(|&g| g != 0))
        }

        fn codepages(&self) -> u32 {
            let os2 = self.face.font_table(u32::from_le_bytes(*b"OS/2")).ok().flatten().unwrap_or_default();
            super::codepage_range(&os2)
        }
    }

    /// Every family in DirectWrite's system collection, as Chromium reads it,
    /// judged by its regular face.
    pub fn installed() -> Vec<SystemFont> {
        let collection = FontCollection::system();
        collection
            .families_iter()
            .filter_map(|family| {
                let name = family.family_name().ok()?;
                let font = family.first_matching_font(FontWeight::Regular, FontStretch::Normal, FontStyle::Normal).ok()?;
                let scripts = scripts_of(&DwFace { face: font.create_font_face() });
                Some(SystemFont { name, scripts })
            })
            .collect()
    }
}

#[cfg(windows)]
use directwrite::installed;

#[cfg(not(windows))]
fn installed() -> Vec<SystemFont> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake {
        chars: &'static str,
        codepages: u32,
    }

    impl Face for Fake {
        fn has(&self, c: char) -> bool {
            self.chars.contains(c)
        }
        fn codepages(&self) -> u32 {
            self.codepages
        }
    }

    const LATIN: &str = "AaZzéß";
    /// Latin and the samples of all four CJK scripts, in one font.
    const LATIN_AND_HAN: &str = "AaZzéßあアの漢这们说這們說汉";

    #[test]
    fn a_font_writes_the_scripts_it_has_every_sample_of() {
        assert_eq!(scripts_of(&Fake { chars: LATIN, codepages: 1 }), ["latin"]);
        assert_eq!(scripts_of(&Fake { chars: "AaZz", codepages: 1 }), Vec::<&str>::new(), "no accents: not enough Latin");
        assert_eq!(scripts_of(&Fake { chars: "AaZzéßЖЯжя", codepages: 0 }), ["latin", "ru"]);
        assert_eq!(scripts_of(&Fake { chars: LATIN, codepages: 1 << 31 }), Vec::<&str>::new(), "a symbol font's letters are pictures");
    }

    #[test]
    fn a_cjk_font_is_listed_for_what_it_is_made_for_not_all_it_carries() {
        let jp = Fake { chars: LATIN_AND_HAN, codepages: 1 << 17 };
        assert_eq!(scripts_of(&jp), ["latin", "ja"], "a Japanese font with Chinese characters too");
        let sc = Fake { chars: LATIN_AND_HAN, codepages: (1 << 18) | 1 };
        assert_eq!(scripts_of(&sc), ["latin", "zh"]);
        let unsaid = Fake { chars: LATIN_AND_HAN, codepages: 0 };
        assert_eq!(scripts_of(&unsaid), ["latin", "ja", "zh", "zh-tw"], "claims none: what it carries");
    }

    fn font(name: &str, scripts: &[&'static str]) -> SystemFont {
        SystemFont { name: name.into(), scripts: scripts.to_vec() }
    }

    #[test]
    fn the_code_pages_are_read_only_from_a_table_that_has_them() {
        let mut os2 = vec![0u8; 86];
        os2[78..82].copy_from_slice(&(1u32 << 17).to_be_bytes());
        assert_eq!(codepage_range(&os2), 0, "version 0: those bytes are something else");
        os2[1] = 1;
        assert_eq!(codepage_range(&os2), 1 << 17);
        assert_eq!(codepage_range(&os2[..78]), 0, "cut short");
        assert_eq!(codepage_range(&[]), 0, "no table");
    }

    #[test]
    fn the_list_is_sorted_once_each_without_fonts_that_write_nothing() {
        let fonts = vec![font("Segoe UI", &["latin"]), font("Wingdings", &[]), font("arial", &["latin"]), font("Segoe UI", &["latin"])];
        let names: Vec<_> = listed(fonts).into_iter().map(|f| f.name).collect();
        assert_eq!(names, ["arial", "Segoe UI"]);
    }

    #[cfg(windows)]
    #[test]
    fn this_machine_lists_its_system_fonts_for_their_scripts() {
        let fonts = listed(installed());
        let scripts = |name: &str| fonts.iter().find(|f| f.name == name).map(|f| f.scripts.clone());
        assert!(scripts("Segoe UI").is_some_and(|s| s.contains(&"latin")));
        // The CJK fonts are optional on some Windows images: judged where installed.
        let judged = |name: &str, made_for: &str, not: &str| scripts(name).is_none_or(|s| s.contains(&made_for) && !s.contains(&not));
        assert!(judged("Yu Gothic", "ja", "zh"));
        assert!(judged("Microsoft YaHei", "zh", "ja"));
        assert!(judged("Malgun Gothic", "ko", "ja"));
        assert!(!fonts.iter().any(|f| ["Wingdings", "Webdings", "Symbol"].contains(&f.name.as_str())), "symbol fonts");
    }

    /// `cargo test --lib fonts::tests::dump -- --ignored --nocapture`: what this machine lists.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn dump() {
        let all = installed();
        let shown = listed(all.clone());
        println!("{} families, {} listed", all.len(), shown.len());
        for f in &shown {
            println!("{:40} {}", f.name, f.scripts.join(" "));
        }
        for code in SCRIPTS.iter().map(|s| s.code) {
            println!("{code}: {}", shown.iter().filter(|f| f.scripts.contains(&code)).count());
        }
    }
}
