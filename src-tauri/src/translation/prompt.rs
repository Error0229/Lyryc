//! The translation prompt.
//!
//! The lyrics go in numbered and only the translations come back, numbered
//! the same way. The model never writes the original out again: asking for
//! "original<sep>translation" pairs read to some models as reproducing the
//! whole lyrics, which they refuse (Sonnet answered with a summary instead),
//! and it doubled the output for nothing, since the app already has the
//! original. Numbers also make the reply easy to line up.
//!
//! The rules favour fidelity: an earlier prompt that asked for "light
//! paraphrase" got embellished translations (imagery and emotion the line
//! didn't have) from small models.

use super::language::get_language_name;

/// Instructions for one song: what the job is, the style, the output format.
pub fn system_prompt(target_lang: &str, style: &str, custom_prompt: Option<&str>) -> String {
    let tl = get_language_name(target_lang);
    let style = match custom_prompt.map(str::trim).filter(|c| !c.is_empty()) {
        Some(custom) => custom.to_string(),
        None => style_rules(style, tl),
    };
    let script = script_rule(target_lang);
    format!(
        "You translate song lyrics for a lyrics app. The listener is playing the song, and the \
app already shows its original lyrics line by line; you supply only the {tl} translation shown \
under each line.

- Read the whole song first; keep one speaker, one set of pronouns and consistent tense throughout.
- A phrase the song repeats gets the same translation each time it repeats.
- Keep names, symbols and wordplay as they are meant (a price tag stays a price tag).
- Keep the original's quotation marks; add none of your own.
- A line already in {tl}, or a pure vocalization (la la, oh), is repeated unchanged.{script}

{style}

The lines are numbered. Answer with one line per input line, in order, each starting with its number:
1. translation of line 1
2. translation of line 2
Nothing else: no notes, headings or blank lines."
    )
}

/// The lyrics, numbered from 1.
pub fn user_prompt(lines: &[&str], source_lang: &str, target_lang: &str) -> String {
    let numbered: Vec<String> = lines.iter().enumerate().map(|(i, l)| format!("{}. {}", i + 1, l.trim())).collect();
    format!(
        "Translate each numbered {} line into {}:\n\n{}",
        get_language_name(source_lang),
        get_language_name(target_lang),
        numbered.join("\n")
    )
}

fn style_rules(style: &str, tl: &str) -> String {
    match style {
        "literal" => format!(
            "Style: close. Stay with the original's words and word order as far as {tl} grammar \
allows. Add nothing, leave nothing out."
        ),
        "rhyming" => format!(
            "Style: singable. Let lines rhyme in {tl} where the original's do, and keep a rhythm \
that can be sung; rephrase as much as that needs, but keep each line's meaning."
        ),
        "poetic" => format!(
            "Style: poetic. Choose evocative {tl} phrasing and keep the song's imagery and \
emotional arc; keep each line's meaning, and add nothing the original doesn't suggest."
        ),
        _ => format!(
            "Style: natural. Translate the meaning faithfully, in natural {tl} grammar and word \
order; rephrase only where a literal version would read awkwardly, never to embellish. Add no \
imagery, emphasis or emotion the original doesn't have, and leave nothing out. Consecutive lines \
should read as one flowing text."
        ),
    }
}

/// Which Chinese script, said plainly: models drift into the other one.
fn script_rule(target_lang: &str) -> &'static str {
    match target_lang {
        "zh-tw" => "\n- Write Traditional Chinese as used in Taiwan (學習, 電腦, 發生, 著), never Simplified, with full-width punctuation (，。？！：).",
        "zh-hk" => "\n- Write Traditional Chinese as used in Hong Kong (學習, 電腦, 發生, 著), never Simplified, with full-width punctuation (，。？！：).",
        "zh" | "zh-cn" => "\n- Write Simplified Chinese (学习, 电脑, 发生), never Traditional, with full-width punctuation (，。？！：).",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lyrics_go_in_numbered_and_the_original_is_never_asked_back() {
        let user = user_prompt(&["  一行目", "二行目"], "ja", "en");
        assert!(user.ends_with("1. 一行目\n2. 二行目"), "{user}");
        let system = system_prompt("en", "natural", None);
        assert!(system.contains("starting with its number"));
        assert!(!system.contains("<sep>") && !system.to_lowercase().contains("original line"));
    }

    #[test]
    fn a_custom_prompt_replaces_the_style_but_not_the_format() {
        let system = system_prompt("en", "natural", Some("Translate like a pirate."));
        assert!(system.contains("Translate like a pirate.") && !system.contains("Style: natural"));
        assert!(system.contains("starting with its number"));
    }

    #[test]
    fn chinese_targets_name_their_script() {
        assert!(system_prompt("zh-tw", "natural", None).contains("Traditional Chinese as used in Taiwan"));
        assert!(system_prompt("zh-cn", "natural", None).contains("Simplified Chinese"));
        assert!(!system_prompt("ja", "natural", None).contains("Chinese"));
    }
}
