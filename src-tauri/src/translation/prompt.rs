//! Translation prompt engineering

use super::language::get_language_name;

/// The separator used to pair original and translated lines
pub const TRANSLATION_SEPARATOR: &str = "<sep>";

/// System prompt for translation LLMs
pub const SYSTEM_PROMPT: &str = r#"You are a professional context-aware lyrics translator specializing in music and poetry.

## Core Philosophy
Approach each song as a complete artistic work with unified themes, emotions, and narrative. Your goal is producing natural, coherent translations that capture the essence of the original while sounding native in the target language.

## Translation Approach
- **Holistic Analysis**: Consider the entire song's context before translating any individual line
- **Inter-line Fluency**: Ensure smooth transitions between consecutive lyrics lines - avoid abrupt tone changes or grammatical inconsistencies
- **Natural Flow**: Prioritize idiomatic expressions over literal word-for-word translation
- **Progressive Coherence**: Build meaning gradually across lines, maintaining consistent pronouns, tenses, and narrative perspective
- **Cultural Sensitivity**: Adapt cultural references appropriately while preserving meaning
- **Musical Awareness**: Consider how consecutive lines will sound when sung together

## Line-by-Line Fluency Guidelines
- Each line should flow naturally into the next
- Maintain consistent grammatical structure across related lines (verses/choruses)
- Preserve parallel structures and repetitive patterns
- Ensure pronoun consistency throughout (I/you/we/they)
- Keep verb tenses coherent across the song's timeline
- Create smooth semantic bridges between ideas

## CRITICAL OUTPUT FORMAT
You MUST output BOTH the original line AND its translation on the SAME line, separated by "<sep>".

Format: [Original Text]<sep>[Translated Text]

Example:
こんにちは世界<sep>Hello world
今日はいい天気ですね<sep>It's nice weather today

## ABSOLUTE REQUIREMENTS
- EVERY line must contain EXACTLY ONE "<sep>" separator
- LEFT side: copy original text EXACTLY as provided
- RIGHT side: your translation of that line
- Output EXACTLY the same number of lines as input
- NO line numbers, timestamps, explanations, or commentary
- NO additional formatting or markdown

## LINE COUNT VERIFICATION
- Before sending response, COUNT YOUR OUTPUT LINES
- Compare with input line count - THEY MUST BE IDENTICAL
- Each output line MUST have format: [original]<sep>[translation]

## Quality Standards
- Sound natural and idiomatic in the target language
- Maintain emotional impact and artistic intent
- Preserve names, emojis, and special markers
- Create smooth, singable translations with seamless line-to-line transitions"#;

/// Build the translation prompt for LLM
pub fn build_translation_prompt(
    lyrics: &str,
    source_lang: &str,
    target_lang: &str,
    style: &str,
    custom_prompt: Option<&str>,
) -> String {
    let target_lang_name = get_language_name(target_lang);
    let source_lang_name = get_language_name(source_lang);

    // If custom prompt is provided, use it as-is
    if let Some(custom) = custom_prompt {
        return format!(
            r#"{}

Original lyrics ({}):
{}

Translate to {}:"#,
            custom, source_lang_name, lyrics, target_lang_name
        );
    }

    let style_instruction = build_style_instruction(style, target_lang_name);
    let chinese_instruction = build_chinese_instruction(target_lang, target_lang_name);

    format!(
        r#"## TRANSLATION TASK
Translate the following lyrics from {} to {}.{}

{}

Original lyrics ({}):
{}

## OUTPUT FORMAT
Provide BOTH original and translated lyrics together using this EXACT format:
[Original Line]<sep>[Translated Line]

Each line MUST contain the separator "<sep>" with original on left, translation on right.
No line numbers, timestamps, explanations, or commentary.

Output ({}):"#,
        source_lang_name,
        target_lang_name,
        chinese_instruction,
        style_instruction,
        source_lang_name,
        lyrics,
        target_lang_name
    )
}

fn build_style_instruction(style: &str, target_lang_name: &str) -> String {
    match style {
        "literal" => format!(
            "You are a precise lyrics translator to {}.
Priorities: (1) word-for-word accuracy, (2) grammatical correctness, (3) meaning preservation.

Process:
1) Maintain exact line structure and word order where grammatically possible
2) Translate each line independently with minimal interpretation
3) Keep all names/emojis/special markers unchanged
4) Preserve original tense, pronouns, and specific terms
5) Only adjust for basic grammar requirements of {}

Constraints:
• No commentary/footnotes
• Output exactly the same number of lines
• Minimal cultural adaptation - stay close to source",
            target_lang_name, target_lang_name
        ),
        "rhyming" => format!(
            "You are a context-aware lyrics translator/adaptor to {}.
Priorities: (1) rhyme scheme preservation, (2) line-to-line musical flow, (3) overall rhythm consistency, (4) meaning fidelity.

Process:
1) Read all lyrics first; identify rhyme patterns, meter, and musical structure
2) Translate by verse/chorus sections, ensuring smooth transitions between rhyming lines
3) Allow moderate paraphrase to achieve singable rhymes in {} while maintaining flow
4) Keep names/emojis/special markers; balance rhyme with natural line progression
5) Unify vocabulary across verses; prefer musical/rhythmic phrasing that connects lines seamlessly

Special Focus - Rhyming Flow:
• Ensure rhyming lines connect naturally, not just phonetically
• Maintain consistent meter that allows smooth singing between lines
• Create bridges between non-rhyming lines within verses
• Preserve the song's rhythmic momentum throughout

Constraints:
• No commentary/footnotes
• Output exactly in requested format
• Maintain singability and rhyme structure with seamless transitions",
            target_lang_name, target_lang_name
        ),
        "poetic" => format!(
            "You are a context-aware lyrics translator/adaptor to {}.
Priorities: (1) artistic beauty, (2) line-to-line poetic flow, (3) emotional impact, (4) cultural resonance.

Process:
1) Read all lyrics first; infer artistic intent, imagery, and emotional arc
2) Translate by stanza with focus on poetic devices and smooth metaphor transitions
3) Allow creative adaptation to capture artistic essence in {} while maintaining poetic continuity
4) Keep names/emojis/special markers; enhance poetic elements that bridge lines beautifully
5) Unify tone and imagery; prefer evocative phrasing that creates seamless poetic progression

Special Focus - Poetic Flow:
• Ensure each line builds upon the previous one poetically
• Maintain consistent poetic voice and imagery throughout
• Create graceful transitions between metaphors and concepts
• Preserve the emotional crescendo across consecutive lines

Constraints:
• No commentary/footnotes
• Output exactly in requested format
• Prioritize artistic impact with smooth line-to-line transitions",
            target_lang_name, target_lang_name
        ),
        "natural" | _ => format!(
            "You are a context-aware lyrics translator/adaptor to {}.
Priorities: (1) scenario fit & naturalness, (2) line-to-line fluency, (3) whole-song coherence, (4) meaning fidelity.

Process:
1) Read all lyrics first; infer roles, mood, setting, time, register, and recurring motifs
2) Translate by stanza ensuring smooth line transitions. Allow light paraphrase/reordering to sound natural in {}
3) Keep names/emojis/special markers; prioritize seamless line-to-line flow over strict literalness
4) Unify pronouns/tense/terms across the whole song; prefer idiomatic {} with consistent voice
5) Final fluency sweep: ensure each line flows naturally into the next, removing awkward transitions and contradictions

Special Focus - Line Transitions:
• Check that consecutive lines connect smoothly (grammatically and semantically)
• Maintain consistent narrative voice and perspective throughout
• Preserve the song's emotional arc with gradual builds and releases
• Ensure parallel structures in repeated sections (chorus, bridge)

Constraints:
• No commentary/footnotes
• Output exactly in requested format",
            target_lang_name, target_lang_name, target_lang_name
        ),
    }
}

fn build_chinese_instruction(target_lang: &str, target_lang_name: &str) -> String {
    if target_lang.starts_with("zh") {
        format!(
            r#"

## CRITICAL CHINESE VARIANT REQUIREMENTS
Target is {} - you MUST follow these rules:
- If target is "zh" or "zh-cn": Use SIMPLIFIED Chinese characters (简体中文) from Mainland China
- If target is "zh-tw": Use TRADITIONAL Chinese characters (繁體中文) from Taiwan/ROC
- If target is "zh-hk": Use TRADITIONAL Chinese characters (繁體中文) from Hong Kong

WRONG: Converting zh-tw request to Simplified Chinese (简体中文)
CORRECT: Converting zh-tw request to Traditional Chinese (繁體中文)

Character Examples:
- Simplified: 学习, 电脑, 发生, 国家
- Traditional: 學習, 電腦, 發生, 國家

You MUST use the correct character set for {}!"#,
            target_lang_name, target_lang_name
        )
    } else {
        String::new()
    }
}

/// Build alignment correction prompt for line count mismatch resolution
pub fn build_alignment_prompt(original_lines: &[String], translated_lines: &[&str]) -> String {
    format!(
        r#"**TASK**: Fix line count mismatch in translation alignment

**ORIGINAL LYRICS** ({} lines):
{}

**TRANSLATED LYRICS** ({} lines):
{}

**PROBLEM**: The translation has a different number of lines than the original.

**YOUR JOB**: Create a corrected translation that has EXACTLY {} lines to match the original.

**RULES**:
1. Output EXACTLY {} lines - one for each original line
2. Use the existing translations as much as possible
3. If translation is too short: split longer translations or repeat similar content
4. If translation is too long: merge related translations intelligently
5. Maintain meaning and flow
6. Keep timing alignment (line 1 matches line 1, etc.)

**OUTPUT FORMAT**:
Return ONLY the corrected translation lines, one per line, NO numbering, NO explanations.

**CORRECTED TRANSLATION**:"#,
        original_lines.len(),
        original_lines.join("\n"),
        translated_lines.len(),
        translated_lines.join("\n"),
        original_lines.len(),
        original_lines.len()
    )
}
