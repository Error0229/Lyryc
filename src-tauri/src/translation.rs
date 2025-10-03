use log::{debug, error, info, warn};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

// Line count mismatch resolution structures
#[derive(Debug, Clone)]
pub enum MismatchType {
    LinesShort,   // LLM returned fewer lines than expected
    LinesExtra,   // LLM returned more lines than expected
    LinesSame,    // Count matches, no issue
}

#[derive(Debug, Clone)]
pub struct MismatchResolution {
    pub strategy_used: String,
    pub confidence: f64,
    pub original_count: usize,
    pub translated_count: usize,
    pub alignment_map: Vec<(usize, usize)>, // (original_index, translated_index)
    pub success: bool,
}

#[derive(Debug, Clone)]
pub enum TranslationStrategy {
    Combined,       // Translate all lines together (default)
    Chunked(usize), // Translate in chunks of N lines
    LineByLine,     // Translate each line individually
    AlignmentFix,   // Use LLM to fix alignment mismatch
}

// Global model blacklist to track rate-limited models
lazy_static::lazy_static! {
    static ref MODEL_BLACKLIST: Arc<Mutex<HashMap<String, u64>>> = Arc::new(Mutex::new(HashMap::new()));
}

/// Check if a model is currently blacklisted (rate-limited)
pub fn is_model_blacklisted(model_id: &str) -> bool {
    if let Ok(blacklist) = MODEL_BLACKLIST.lock() {
        if let Some(&blacklist_until) = blacklist.get(model_id) {
            let current_time = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();

            if current_time < blacklist_until {
                debug!(
                    "Model {} is blacklisted until {}",
                    model_id, blacklist_until
                );
                return true;
            } else {
                // Blacklist has expired, remove it
                debug!("Model {} blacklist expired, removing", model_id);
                drop(blacklist);
                if let Ok(mut blacklist) = MODEL_BLACKLIST.lock() {
                    blacklist.remove(model_id);
                }
            }
        }
    }
    false
}

/// Blacklist a model for a specified duration (in seconds)
pub fn blacklist_model(model_id: &str, wait_seconds: u64) {
    let blacklist_until = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + wait_seconds;

    if let Ok(mut blacklist) = MODEL_BLACKLIST.lock() {
        blacklist.insert(model_id.to_string(), blacklist_until);
        warn!(
            "Blacklisted model {} for {} seconds (until {})",
            model_id, wait_seconds, blacklist_until
        );
    }
}

// Removed parse_rate_limit_wait_time since we use fixed 24-hour blacklisting

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TranslationConfig {
    pub source_language: String,
    pub target_language: String,
    pub style: String, // "natural", "literal", "poetic", "rhyming", "custom"
    pub custom_prompt: Option<String>,
    pub preserve_rhyme: bool,
    pub preserve_meaning: bool,
    pub maintain_structure: bool,
    pub provider: String, // "github" or "gemini"
    pub github_api_key: Option<String>,
    pub gemini_api_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LyricLine {
    pub time: f64,
    pub text: String,
    pub duration: Option<f64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TranslatedLyricLine {
    pub time: f64,
    pub text: String,
    pub duration: Option<f64>,
    pub original_text: String,
    pub translated_text: String,
    pub translation_confidence: Option<f64>,
    pub detected_language: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TranslationResult {
    pub success: bool,
    pub translated_lines: Vec<TranslatedLyricLine>,
    pub detected_language: Option<String>,
    pub processing_time_ms: u64,
    pub error: Option<String>,
    pub is_same_language: bool, // New field: LLM-determined same language flag
}

impl Default for TranslationConfig {
    fn default() -> Self {
        Self {
            source_language: "auto".to_string(),
            target_language: "en".to_string(),
            style: "natural".to_string(),
            custom_prompt: None,
            preserve_rhyme: true,
            preserve_meaning: true,
            maintain_structure: true,
            provider: "gemini".to_string(), // Default to Gemini for better rate limits
            github_api_key: None,
            gemini_api_key: None,
        }
    }
}

/// Main translation function - uses LLM with enhanced mismatch resolution and fallback support
pub async fn translate_lyrics(
    lyrics: Vec<LyricLine>,
    config: TranslationConfig,
) -> Result<TranslationResult, String> {
    let start_time = std::time::Instant::now();

    if lyrics.is_empty() {
        return Ok(TranslationResult {
            success: false,
            translated_lines: vec![],
            detected_language: None,
            processing_time_ms: 0,
            error: Some("No lyrics provided".to_string()),
            is_same_language: false,
        });
    }

    info!(
        "Starting enhanced translation for {} lines from {} to {}",
        lyrics.len(),
        config.source_language,
        config.target_language
    );

    // Define progressive translation strategies
    let strategies = vec![
        TranslationStrategy::Combined,       // Try normal translation first
        TranslationStrategy::Chunked(5),     // If that fails, try smaller chunks
        TranslationStrategy::LineByLine,     // Last resort: line-by-line
    ];

    let result = translate_with_strategies(&lyrics, &config, &strategies).await;

    match result {
        Ok(mut translation_result) => {
            translation_result.processing_time_ms = start_time.elapsed().as_millis() as u64;
            info!(
                "Translation completed successfully in {}ms",
                translation_result.processing_time_ms
            );
            Ok(translation_result)
        }
        Err(e) => {
            let processing_time_ms = start_time.elapsed().as_millis() as u64;
            error!("Translation failed after {}ms: {}", processing_time_ms, e);
            Ok(TranslationResult {
                success: false,
                translated_lines: vec![],
                detected_language: None,
                processing_time_ms,
                error: Some(e),
                is_same_language: false,
            })
        }
    }
}

/// Enhanced translation function with explicit retry strategies
pub async fn translate_lyrics_with_retry_strategies(
    lyrics: Vec<LyricLine>,
    config: TranslationConfig,
    strategies: Vec<TranslationStrategy>,
) -> Result<TranslationResult, String> {
    let start_time = std::time::Instant::now();

    if lyrics.is_empty() {
        return Ok(TranslationResult {
            success: false,
            translated_lines: vec![],
            detected_language: None,
            processing_time_ms: 0,
            error: Some("No lyrics provided".to_string()),
            is_same_language: false,
        });
    }

    info!(
        "Starting translation with {} strategies for {} lines from {} to {}",
        strategies.len(),
        lyrics.len(),
        config.source_language,
        config.target_language
    );

    let result = translate_with_strategies(&lyrics, &config, &strategies).await;

    match result {
        Ok(mut translation_result) => {
            translation_result.processing_time_ms = start_time.elapsed().as_millis() as u64;
            info!(
                "Translation with strategies completed successfully in {}ms",
                translation_result.processing_time_ms
            );
            Ok(translation_result)
        }
        Err(e) => {
            let processing_time_ms = start_time.elapsed().as_millis() as u64;
            error!(
                "Translation with strategies failed after {}ms: {}",
                processing_time_ms, e
            );
            Ok(TranslationResult {
                success: false,
                translated_lines: vec![],
                detected_language: None,
                processing_time_ms,
                error: Some(e),
                is_same_language: false,
            })
        }
    }
}

/// Translate lyrics using GitHub AI models (similar to track_cleaning.rs pattern)
async fn translate_lyrics_github(
    lyrics: &[LyricLine],
    config: &TranslationConfig,
) -> Result<TranslationResult, String> {
    // Get GitHub token (caller has already verified availability)
    let github_token = config.github_api_key.clone()
        .filter(|key| !key.trim().is_empty())
        .or_else(|| std::env::var("GITHUB_TOKEN").ok())
        .ok_or_else(|| "GitHub API key required for translation. Please enter your API key in the Translation settings.".to_string())?;

    // High-performance models from test results - only use models that work
    let all_models = [
        "meta/Llama-4-Maverick-17B-128E-Instruct-FP8", // 98.33% accuracy - best performer
        "openai/gpt-4.1-nano",                         // 98.33% accuracy - excellent
        "openai/gpt-4o-mini",                          // 96.67% accuracy - reliable
        "mistral ai/Mistral-Large-2411",               // 96.67% accuracy - proven
        "mistral ai/mistral-medium-2505",              // 96.67% accuracy - consistent
        "mistral ai/mistral-small-2503",               // 96.67% accuracy - fast
        "meta/Meta-Llama-3.1-8B-Instruct",             // 95% accuracy - very fast
        "mistral ai/Codestral-2501",                   // 95% accuracy - good
        "deepseek/DeepSeek-V3-0324",                   // 90% accuracy - backup
        "meta/Llama-3.3-70B-Instruct",                 // 90% accuracy - free
        "xai/grok-3",                                  // 90% accuracy - alternative
        "meta/Llama-4-Scout-17B-16E-Instruct",         // 90% accuracy - reliable
    ];

    // Filter out blacklisted models
    let available_models: Vec<&str> = all_models
        .iter()
        .filter(|model| !is_model_blacklisted(model))
        .cloned()
        .collect();

    if available_models.is_empty() {
        return Err("All GitHub AI models are currently rate-limited".to_string());
    }

    info!(
        "Available GitHub models: {} out of {} total",
        available_models.len(),
        all_models.len()
    );

    // Pass language detection to translation function - no separate API call
    let source_language = config.source_language.clone();

    for (i, model_id) in available_models.iter().enumerate() {
        match try_llm_translation(lyrics, config, &source_language, model_id, &github_token).await {
            Ok(result) => {
                info!(
                    "Translation successful with {}: {} lines translated",
                    model_id,
                    result.translated_lines.len()
                );
                return Ok(result);
            }
            Err(e) => {
                // Check if this is a rate limit error and blacklist the model for 24 hours
                if e.contains("429") && e.contains("Rate limit") {
                    // GitHub AI rate limits are daily per model, so blacklist for exactly 24 hours
                    blacklist_model(model_id, 86400); // 24 hours = 86400 seconds
                    warn!("Model {} rate limited, blacklisted for 24 hours", model_id);
                }

                debug!(
                    "Translation failed with {}: {} (attempt {}/{})",
                    model_id,
                    e,
                    i + 1,
                    available_models.len()
                );
                if i < available_models.len() - 1 {
                    // Add delay between attempts
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
        }
    }

    Err("All LLM translation attempts failed".to_string())
}

async fn try_llm_translation(
    lyrics: &[LyricLine],
    config: &TranslationConfig,
    source_language: &str,
    model_id: &str,
    github_token: &str,
) -> Result<TranslationResult, String> {
    // Detect language locally first if auto-detect mode
    let detected_language = if source_language == "auto" {
        detect_language_fallback(lyrics)
    } else {
        source_language.to_string()
    };

    // Check if detected language matches target - skip translation if same
    if detected_language == config.target_language {
        debug!(
            "Detected language {} matches target {}, skipping translation",
            detected_language, config.target_language
        );
        return Ok(TranslationResult {
            success: true,
            translated_lines: vec![],
            detected_language: Some(detected_language),
            processing_time_ms: 0,
            error: None,
            is_same_language: true,
        });
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(45))
        .build()
        .map_err(|e| e.to_string())?;

    // Create the translation prompt - simplified since detection already happened
    let lyrics_text: Vec<String> = lyrics.iter().map(|line| line.text.clone()).collect();
    let lyrics_input = lyrics_text.join("\n");

    let translation_prompt = build_translation_prompt(
        &lyrics_input,
        &detected_language,
        &config.target_language,
        &config.style,
        config.custom_prompt.as_deref(),
    );

    let system_content = r#"You are a professional context-aware lyrics translator specializing in music and poetry.

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
You MUST output BOTH the original line AND its translation on the SAME line, separated by " ||| ".

Format: [Original Text] ||| [Translated Text]

Example:
こんにちは世界 ||| Hello world
今日はいい天気ですね ||| It's nice weather today

## ABSOLUTE REQUIREMENTS
- EVERY line must contain EXACTLY ONE " ||| " separator
- LEFT side: copy original text EXACTLY as provided
- RIGHT side: your translation of that line
- Output EXACTLY the same number of lines as input
- NO line numbers, timestamps, explanations, or commentary
- NO additional formatting or markdown

## LINE COUNT VERIFICATION
- Before sending response, COUNT YOUR OUTPUT LINES
- Compare with input line count - THEY MUST BE IDENTICAL
- Each output line MUST have format: [original] ||| [translation]

## Quality Standards
- Sound natural and idiomatic in the target language
- Maintain emotional impact and artistic intent
- Preserve names, emojis, and special markers
- Create smooth, singable translations with seamless line-to-line transitions"#;

    let payload = json!({
        "messages": [
            {"role": "system", "content": system_content},
            {"role": "user", "content": translation_prompt}
        ],
        "temperature": 0.3,
        "max_tokens": 4096,
        "model": model_id
    });

    let response = client
        .post("https://models.github.ai/inference/chat/completions")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", github_token))
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!("HTTP {}: {}", status, error_text));
    }

    let result: Value = response.json().await.map_err(|e| e.to_string())?;
    let response_text = result["choices"][0]["message"]["content"]
        .as_str()
        .ok_or("No content in response".to_string())?;

    // Parse the translation response with enhanced mismatch resolution
    // Note: is_same_language will always be false here since we already checked that above
    let (translated_lines, _is_same_language) =
        parse_translation_response(response_text, lyrics, &detected_language, Some(config)).await?;

    Ok(TranslationResult {
        success: true,
        translated_lines,
        detected_language: Some(detected_language),
        processing_time_ms: 0, // Will be set by caller
        error: None,
        is_same_language: false, // Always false here - same language is handled earlier
    })
}

fn build_translation_prompt(
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

    // Enhanced context-aware translation system
    let style_specific_instruction = match style {
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
    };

    // Special handling for Chinese variants
    let chinese_instruction = if target_lang.starts_with("zh") {
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
    };

    format!(
        r#"## TRANSLATION TASK
Translate the following lyrics from {} to {}.{}

{}

Original lyrics ({}):
{}

## OUTPUT FORMAT
Provide BOTH original and translated lyrics together using this EXACT format:
[Original Line] ||| [Translated Line]

Each line MUST contain the separator " ||| " with original on left, translation on right.
No line numbers, timestamps, explanations, or commentary.

Output ({}):"#,
        source_lang_name,
        target_lang_name,
        chinese_instruction,
        style_specific_instruction,
        source_lang_name,
        lyrics,
        target_lang_name
    )
}

async fn parse_translation_response(
    response_text: &str,
    original_lyrics: &[LyricLine],
    detected_language: &str,
    config: Option<&TranslationConfig>,
) -> Result<(Vec<TranslatedLyricLine>, bool), String> {
    let response_text = response_text.trim();

    // Clean the response text - split by lines and filter empty
    let cleaned_response = response_text
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();

    // Enhanced parsing: handle new format with " ||| " separator
    let mut translated_lines = Vec::new();

    // Check if the response uses the new format (original ||| translated)
    let uses_new_format = cleaned_response.iter().any(|line| line.contains(" ||| "));

    if uses_new_format {
        info!("Parsing new format with ' ||| ' separator");

        for (i, original_line) in original_lyrics.iter().enumerate() {
            if i >= cleaned_response.len() {
                warn!("Missing translation for line {}, using fallback", i);
                // Fallback to original text if translation is missing
                translated_lines.push(TranslatedLyricLine {
                    time: original_line.time,
                    text: original_line.text.clone(),
                    duration: original_line.duration,
                    original_text: original_line.text.clone(),
                    translated_text: original_line.text.clone(),
                    translation_confidence: Some(0.0),
                    detected_language: Some(detected_language.to_string()),
                });
                continue;
            }

            let response_line = cleaned_response[i];

            // Split by " ||| " separator
            if let Some(separator_pos) = response_line.find(" ||| ") {
                let original_part = response_line[..separator_pos].trim();
                let translated_part = response_line[separator_pos + 5..].trim(); // 5 = length of " ||| "

                // Verify original text matches (with some tolerance for whitespace)
                let original_matches = original_part.trim() == original_line.text.trim();
                let confidence = if original_matches { 0.98 } else { 0.85 };

                if !original_matches {
                    debug!(
                        "Original text mismatch at line {}: expected '{}', got '{}'",
                        i, original_line.text, original_part
                    );
                }

                translated_lines.push(TranslatedLyricLine {
                    time: original_line.time,
                    text: translated_part.to_string(),
                    duration: original_line.duration,
                    original_text: original_line.text.clone(),
                    translated_text: translated_part.to_string(),
                    translation_confidence: Some(confidence),
                    detected_language: Some(detected_language.to_string()),
                });
            } else {
                warn!("Line {} missing ' ||| ' separator: '{}'", i, response_line);
                // Fallback: treat entire line as translation
                translated_lines.push(TranslatedLyricLine {
                    time: original_line.time,
                    text: response_line.to_string(),
                    duration: original_line.duration,
                    original_text: original_line.text.clone(),
                    translated_text: response_line.to_string(),
                    translation_confidence: Some(0.7), // Lower confidence due to format issue
                    detected_language: Some(detected_language.to_string()),
                });
            }
        }

        // Perfect line count match with new format
        if translated_lines.len() == original_lyrics.len() {
            info!("New format parsing successful: {} lines matched", translated_lines.len());
            return Ok((translated_lines, false));
        } else {
            warn!(
                "Line count mismatch after new format parsing: {} vs {}",
                translated_lines.len(),
                original_lyrics.len()
            );
        }
    }

    // OLD FORMAT FALLBACK: Handle legacy format without separator
    // Enhanced line count mismatch detection and resolution
    if cleaned_response.len() != original_lyrics.len() {
        warn!(
            "Line count mismatch detected: original {} vs translated {}",
            original_lyrics.len(),
            cleaned_response.len()
        );

        // Attempt to resolve the mismatch using multiple strategies
        return resolve_line_count_mismatch(
            &cleaned_response,
            original_lyrics,
            detected_language,
            config,
        ).await;
    }

    // Perfect match - proceed normally with old format
    let mut legacy_translated_lines = Vec::new();

    for (i, original_line) in original_lyrics.iter().enumerate() {
        let translated_text = cleaned_response[i].to_string();

        legacy_translated_lines.push(TranslatedLyricLine {
            time: original_line.time,
            text: translated_text.clone(),
            duration: original_line.duration,
            original_text: original_line.text.clone(),
            translated_text,
            translation_confidence: Some(0.95), // High confidence for perfect match
            detected_language: Some(detected_language.to_string()),
        });
    }

    Ok((legacy_translated_lines, false)) // Translation provided, same_language = false
}

/// Comprehensive line count mismatch resolution system
async fn resolve_line_count_mismatch(
    translated_lines: &[&str],
    original_lyrics: &[LyricLine],
    detected_language: &str,
    config: Option<&TranslationConfig>,
) -> Result<(Vec<TranslatedLyricLine>, bool), String> {
    let mismatch_type = classify_mismatch(translated_lines.len(), original_lyrics.len());

    info!(
        "Resolving {:?} mismatch: {} translated vs {} original lines",
        mismatch_type, translated_lines.len(), original_lyrics.len()
    );

    // Strategy 1: LLM-based alignment resolution (if config available)
    if let Some(config) = config {
        if let Ok((resolved_lines, is_same)) = resolve_alignment_with_llm(
            translated_lines,
            original_lyrics,
            detected_language,
            config,
        ).await {
            info!("LLM-based alignment resolution successful");
            return Ok((resolved_lines, is_same));
        } else {
            debug!("LLM-based alignment failed, trying other strategies");
        }
    }

    // Strategy 2: Smart alignment mapping for reasonable mismatches
    if let Ok(alignment) = create_smart_alignment(translated_lines, original_lyrics) {
        if alignment.confidence > 0.7 {
            info!(
                "Smart alignment successful with confidence {:.2}",
                alignment.confidence
            );
            return apply_alignment_mapping(
                translated_lines,
                original_lyrics,
                detected_language,
                &alignment,
            );
        }
    }

    // Strategy 3: Intelligent merging/splitting for close mismatches
    match mismatch_type {
        MismatchType::LinesShort => {
            // LLM merged some lines - try intelligent splitting
            if let Ok(resolved) = resolve_by_intelligent_splitting(
                translated_lines,
                original_lyrics,
                detected_language,
            ) {
                return Ok(resolved);
            }
        }
        MismatchType::LinesExtra => {
            // LLM split some lines - try intelligent merging
            if let Ok(resolved) = resolve_by_intelligent_merging(
                translated_lines,
                original_lyrics,
                detected_language,
            ) {
                return Ok(resolved);
            }
        }
        MismatchType::LinesSame => unreachable!(),
    }

    // Strategy 4: Conservative fallback - preserve original structure
    warn!("Using conservative fallback due to alignment failure");
    Ok(create_conservative_fallback(
        translated_lines,
        original_lyrics,
        detected_language,
    ))
}

fn classify_mismatch(translated_count: usize, original_count: usize) -> MismatchType {
    match translated_count.cmp(&original_count) {
        std::cmp::Ordering::Less => MismatchType::LinesShort,
        std::cmp::Ordering::Greater => MismatchType::LinesExtra,
        std::cmp::Ordering::Equal => MismatchType::LinesSame,
    }
}

/// Create smart alignment mapping between original and translated lines
fn create_smart_alignment(
    translated_lines: &[&str],
    original_lyrics: &[LyricLine],
) -> Result<MismatchResolution, String> {
    let original_count = original_lyrics.len();
    let translated_count = translated_lines.len();

    // Calculate alignment confidence based on line count ratio
    let ratio = if original_count > translated_count {
        translated_count as f64 / original_count as f64
    } else {
        original_count as f64 / translated_count as f64
    };

    // High confidence for small mismatches, lower for large ones
    let base_confidence = if ratio > 0.8 { 0.8 } else if ratio > 0.5 { 0.6 } else { 0.3 };

    let mut alignment_map = Vec::new();

    // Simple proportional alignment for now
    for i in 0..original_count {
        let translated_index = (i as f64 * translated_count as f64 / original_count as f64) as usize;
        let mapped_index = translated_index.min(translated_count.saturating_sub(1));
        alignment_map.push((i, mapped_index));
    }

    Ok(MismatchResolution {
        strategy_used: "smart_alignment".to_string(),
        confidence: base_confidence,
        original_count,
        translated_count,
        alignment_map,
        success: true,
    })
}

fn apply_alignment_mapping(
    translated_lines: &[&str],
    original_lyrics: &[LyricLine],
    detected_language: &str,
    alignment: &MismatchResolution,
) -> Result<(Vec<TranslatedLyricLine>, bool), String> {
    let mut result_lines = Vec::new();

    for (original_idx, translated_idx) in &alignment.alignment_map {
        let original_line = &original_lyrics[*original_idx];
        let translated_text = translated_lines
            .get(*translated_idx)
            .unwrap_or(&original_line.text.as_str())
            .to_string();

        result_lines.push(TranslatedLyricLine {
            time: original_line.time,
            text: translated_text.clone(),
            duration: original_line.duration,
            original_text: original_line.text.clone(),
            translated_text,
            translation_confidence: Some(alignment.confidence),
            detected_language: Some(detected_language.to_string()),
        });
    }

    Ok((result_lines, false))
}

fn resolve_by_intelligent_splitting(
    translated_lines: &[&str],
    original_lyrics: &[LyricLine],
    detected_language: &str,
) -> Result<(Vec<TranslatedLyricLine>, bool), String> {
    debug!("Attempting intelligent splitting to resolve line shortage");

    // Simple strategy: duplicate last translation for missing lines
    let mut result_lines = Vec::new();

    for (i, original_line) in original_lyrics.iter().enumerate() {
        let translated_text = if i < translated_lines.len() {
            translated_lines[i].to_string()
        } else {
            // For missing lines, use the last available translation or original text
            translated_lines
                .last()
                .unwrap_or(&original_line.text.as_str())
                .to_string()
        };

        result_lines.push(TranslatedLyricLine {
            time: original_line.time,
            text: translated_text.clone(),
            duration: original_line.duration,
            original_text: original_line.text.clone(),
            translated_text,
            translation_confidence: Some(0.6), // Lower confidence due to approximation
            detected_language: Some(detected_language.to_string()),
        });
    }

    Ok((result_lines, false))
}

fn resolve_by_intelligent_merging(
    translated_lines: &[&str],
    original_lyrics: &[LyricLine],
    detected_language: &str,
) -> Result<(Vec<TranslatedLyricLine>, bool), String> {
    debug!("Attempting intelligent merging to resolve line excess");

    // Simple strategy: merge consecutive translations for original lines
    let mut result_lines = Vec::new();
    let excess_ratio = translated_lines.len() as f64 / original_lyrics.len() as f64;

    for (i, original_line) in original_lyrics.iter().enumerate() {
        let start_idx = (i as f64 * excess_ratio) as usize;
        let end_idx = ((i + 1) as f64 * excess_ratio) as usize;

        // Merge translations in this range
        let merged_translation = translated_lines[start_idx..end_idx.min(translated_lines.len())]
            .join(" ")
            .trim()
            .to_string();

        let final_translation = if merged_translation.is_empty() {
            original_line.text.clone()
        } else {
            merged_translation
        };

        result_lines.push(TranslatedLyricLine {
            time: original_line.time,
            text: final_translation.clone(),
            duration: original_line.duration,
            original_text: original_line.text.clone(),
            translated_text: final_translation,
            translation_confidence: Some(0.6), // Lower confidence due to approximation
            detected_language: Some(detected_language.to_string()),
        });
    }

    Ok((result_lines, false))
}

fn create_conservative_fallback(
    translated_lines: &[&str],
    original_lyrics: &[LyricLine],
    detected_language: &str,
) -> (Vec<TranslatedLyricLine>, bool) {
    let mut result_lines = Vec::new();

    for (i, original_line) in original_lyrics.iter().enumerate() {
        let translated_text = translated_lines
            .get(i)
            .unwrap_or(&original_line.text.as_str())
            .to_string();

        result_lines.push(TranslatedLyricLine {
            time: original_line.time,
            text: translated_text.clone(),
            duration: original_line.duration,
            original_text: original_line.text.clone(),
            translated_text,
            translation_confidence: Some(0.4), // Low confidence for fallback
            detected_language: Some(detected_language.to_string()),
        });
    }

    (result_lines, false)
}

/// LLM-based alignment resolution - uses AI to create proper line mappings
async fn resolve_alignment_with_llm(
    translated_lines: &[&str],
    original_lyrics: &[LyricLine],
    detected_language: &str,
    config: &TranslationConfig,
) -> Result<(Vec<TranslatedLyricLine>, bool), String> {
    info!(
        "Attempting LLM-based alignment resolution for {} -> {} line mismatch",
        original_lyrics.len(),
        translated_lines.len()
    );

    // Build alignment prompt
    let original_text: Vec<String> = original_lyrics.iter().map(|l| l.text.clone()).collect();
    let translated_text = translated_lines.join("\n");

    let alignment_prompt = format!(
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
        original_lyrics.len(),
        original_text.join("\n"),
        translated_lines.len(),
        translated_text,
        original_lyrics.len(),
        original_lyrics.len()
    );

    // Try to get alignment using available LLM provider
    let alignment_result = match config.provider.as_str() {
        "gemini" => {
            if let Some(api_key) = &config.gemini_api_key {
                try_gemini_alignment(&alignment_prompt, api_key).await
            } else if let Ok(env_key) = std::env::var("GEMINI_API_KEY") {
                try_gemini_alignment(&alignment_prompt, &env_key).await
            } else {
                Err("No Gemini API key available for alignment".to_string())
            }
        }
        "github" | _ => {
            if let Some(api_key) = &config.github_api_key {
                try_github_alignment(&alignment_prompt, api_key).await
            } else if let Ok(env_key) = std::env::var("GITHUB_TOKEN") {
                try_github_alignment(&alignment_prompt, &env_key).await
            } else {
                Err("No GitHub token available for alignment".to_string())
            }
        }
    };

    match alignment_result {
        Ok(corrected_lines) => {
            info!(
                "LLM alignment successful: {} corrected lines",
                corrected_lines.len()
            );

            // Verify the correction worked
            if corrected_lines.len() == original_lyrics.len() {
                let mut result_lines = Vec::new();

                for (i, original_line) in original_lyrics.iter().enumerate() {
                    let translated_text = corrected_lines
                        .get(i)
                        .unwrap_or(&original_line.text)
                        .clone();

                    result_lines.push(TranslatedLyricLine {
                        time: original_line.time,
                        text: translated_text.clone(),
                        duration: original_line.duration,
                        original_text: original_line.text.clone(),
                        translated_text,
                        translation_confidence: Some(0.8), // Good confidence for LLM-corrected
                        detected_language: Some(detected_language.to_string()),
                    });
                }

                Ok((result_lines, false))
            } else {
                warn!(
                    "LLM alignment failed: expected {} lines, got {}",
                    original_lyrics.len(),
                    corrected_lines.len()
                );
                Err("LLM alignment returned incorrect line count".to_string())
            }
        }
        Err(e) => {
            warn!("LLM alignment failed: {}", e);
            Err(e)
        }
    }
}

async fn try_gemini_alignment(prompt: &str, api_key: &str) -> Result<Vec<String>, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    let payload = json!({
        "contents": [{
            "parts": [{
                "text": prompt
            }]
        }],
        "generationConfig": {
            "temperature": 0.1,
            "maxOutputTokens": 4096,
            "topP": 0.8,
            "topK": 10
        }
    });

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash-lite:generateContent?key={}",
        api_key
    );

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        return Err(format!("Gemini alignment request failed: {}", response.status()));
    }

    let result: Value = response.json().await.map_err(|e| e.to_string())?;
    let response_text = result["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .ok_or("No text in Gemini alignment response")?;

    Ok(response_text
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect())
}

async fn try_github_alignment(prompt: &str, github_token: &str) -> Result<Vec<String>, String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;

    let payload = json!({
        "messages": [
            {"role": "user", "content": prompt}
        ],
        "temperature": 0.1,
        "max_tokens": 4096,
        "model": "openai/gpt-4o-mini"
    });

    let response = client
        .post("https://models.github.ai/inference/chat/completions")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", github_token))
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        return Err(format!("GitHub alignment request failed: {}", response.status()));
    }

    let result: Value = response.json().await.map_err(|e| e.to_string())?;
    let response_text = result["choices"][0]["message"]["content"]
        .as_str()
        .ok_or("No content in GitHub alignment response")?;

    Ok(response_text
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect())
}

/// Enhanced translation with retry strategies
async fn translate_with_strategies(
    lyrics: &[LyricLine],
    config: &TranslationConfig,
    strategies: &[TranslationStrategy],
) -> Result<TranslationResult, String> {
    for (i, strategy) in strategies.iter().enumerate() {
        debug!("Trying translation strategy {}: {:?}", i + 1, strategy);

        let result = match strategy {
            TranslationStrategy::Combined => {
                // Use existing combined translation
                translate_lyrics_with_config(lyrics, config).await
            }
            TranslationStrategy::Chunked(chunk_size) => {
                translate_lyrics_chunked(lyrics, config, *chunk_size).await
            }
            TranslationStrategy::LineByLine => {
                translate_lyrics_line_by_line(lyrics, config).await
            }
            TranslationStrategy::AlignmentFix => {
                // This strategy is used within other strategies, not standalone
                Err("AlignmentFix strategy must be used with another strategy".to_string())
            }
        };

        match result {
            Ok(translation_result) => {
                if translation_result.success {
                    info!("Translation successful with strategy: {:?}", strategy);
                    return Ok(translation_result);
                } else {
                    debug!("Strategy {:?} succeeded but with errors", strategy);
                }
            }
            Err(e) => {
                debug!("Strategy {:?} failed: {}", strategy, e);
            }
        }
    }

    Err("All translation strategies failed".to_string())
}

async fn translate_lyrics_with_config(
    lyrics: &[LyricLine],
    config: &TranslationConfig,
) -> Result<TranslationResult, String> {
    // Use existing translation logic
    match config.provider.as_str() {
        "gemini" => translate_lyrics_gemini(lyrics, config).await,
        "github" | _ => translate_lyrics_github(lyrics, config).await,
    }
}

async fn translate_lyrics_chunked(
    lyrics: &[LyricLine],
    config: &TranslationConfig,
    chunk_size: usize,
) -> Result<TranslationResult, String> {
    let start_time = std::time::Instant::now();
    let mut all_translated_lines = Vec::new();
    let mut detected_language = None;

    for chunk in lyrics.chunks(chunk_size) {
        let chunk_result = translate_lyrics_with_config(chunk, config).await?;

        if !chunk_result.success {
            return Err("Chunk translation failed".to_string());
        }

        all_translated_lines.extend(chunk_result.translated_lines);
        if detected_language.is_none() {
            detected_language = chunk_result.detected_language;
        }
    }

    Ok(TranslationResult {
        success: true,
        translated_lines: all_translated_lines,
        detected_language,
        processing_time_ms: start_time.elapsed().as_millis() as u64,
        error: None,
        is_same_language: false,
    })
}

async fn translate_lyrics_line_by_line(
    lyrics: &[LyricLine],
    config: &TranslationConfig,
) -> Result<TranslationResult, String> {
    let start_time = std::time::Instant::now();
    let mut all_translated_lines = Vec::new();
    let mut detected_language = None;

    for line in lyrics {
        let single_line = vec![line.clone()];
        let line_result = translate_lyrics_with_config(&single_line, config).await?;

        if !line_result.success || line_result.translated_lines.is_empty() {
            // Fallback to original text for failed lines
            all_translated_lines.push(TranslatedLyricLine {
                time: line.time,
                text: line.text.clone(),
                duration: line.duration,
                original_text: line.text.clone(),
                translated_text: line.text.clone(),
                translation_confidence: Some(0.0), // No translation confidence
                detected_language: detected_language.clone(),
            });
        } else {
            all_translated_lines.extend(line_result.translated_lines);
            if detected_language.is_none() {
                detected_language = line_result.detected_language;
            }
        }
    }

    Ok(TranslationResult {
        success: true,
        translated_lines: all_translated_lines,
        detected_language,
        processing_time_ms: start_time.elapsed().as_millis() as u64,
        error: None,
        is_same_language: false,
    })
}

/// Translate lyrics using Google Gemini API
async fn translate_lyrics_gemini(
    lyrics: &[LyricLine],
    config: &TranslationConfig,
) -> Result<TranslationResult, String> {
    // Get Gemini API key (caller has already verified availability)
    let gemini_api_key = config.gemini_api_key.clone()
        .filter(|key| !key.trim().is_empty())
        .or_else(|| std::env::var("GEMINI_API_KEY").ok())
        .ok_or_else(|| "Gemini API key required for translation. Please enter your API key in the Translation settings.".to_string())?;

    // Available Gemini models (based on the pricing table)
    let models = [
        "gemini-2.5-flash-lite", // Fast with 250K TPM, 10 RPM
        "gemini-2.0-flash",      // Latest with 1M TPM, 15 RPM
        "gemini-2.5-flash",      // Fast with 250K TPM, 10 RPM
        "gemini-2.0-flash-lite", // Fast with 250K TPM, 10 RPM
        "gemini-2.5-pro",        // Most capable with 250K TPM, 5 RPM
    ];

    // Pass language detection to translation function - no separate API call
    let source_language = config.source_language.clone();

    for (i, model_id) in models.iter().enumerate() {
        match try_gemini_translation(lyrics, config, &source_language, model_id, &gemini_api_key)
            .await
        {
            Ok(result) => {
                info!(
                    "Translation successful with {}: {} lines translated",
                    model_id,
                    result.translated_lines.len()
                );
                return Ok(result);
            }
            Err(e) => {
                debug!(
                    "Translation failed with {}: {} (attempt {}/{})",
                    model_id,
                    e,
                    i + 1,
                    models.len()
                );
                if i < models.len() - 1 {
                    // Add delay between attempts
                    tokio::time::sleep(Duration::from_millis(500)).await;
                }
            }
        }
    }

    Err("All Gemini translation attempts failed".to_string())
}

async fn try_gemini_translation(
    lyrics: &[LyricLine],
    config: &TranslationConfig,
    source_language: &str,
    model_id: &str,
    gemini_api_key: &str,
) -> Result<TranslationResult, String> {
    // Detect language locally first if auto-detect mode
    let detected_language = if source_language == "auto" {
        detect_language_fallback(lyrics)
    } else {
        source_language.to_string()
    };

    // Check if detected language matches target - skip translation if same
    if detected_language == config.target_language {
        debug!(
            "Detected language {} matches target {}, skipping translation",
            detected_language, config.target_language
        );
        return Ok(TranslationResult {
            success: true,
            translated_lines: vec![],
            detected_language: Some(detected_language),
            processing_time_ms: 0,
            error: None,
            is_same_language: true,
        });
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(45))
        .build()
        .map_err(|e| e.to_string())?;

    // Create the translation prompt - simplified since detection already happened
    let lyrics_text: Vec<String> = lyrics.iter().map(|line| line.text.clone()).collect();
    let lyrics_input = lyrics_text.join("\n");

    let translation_prompt = build_translation_prompt(
        &lyrics_input,
        &detected_language,
        &config.target_language,
        &config.style,
        config.custom_prompt.as_deref(),
    );

    let system_instruction = r#"You are a professional context-aware lyrics translator specializing in music and poetry.

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
You MUST output BOTH the original line AND its translation on the SAME line, separated by " ||| ".

Format: [Original Text] ||| [Translated Text]

Example:
こんにちは世界 ||| Hello world
今日はいい天気ですね ||| It's nice weather today

## ABSOLUTE REQUIREMENTS
- EVERY line must contain EXACTLY ONE " ||| " separator
- LEFT side: copy original text EXACTLY as provided
- RIGHT side: your translation of that line
- Output EXACTLY the same number of lines as input
- NO line numbers, timestamps, explanations, or commentary
- NO additional formatting or markdown

## LINE COUNT VERIFICATION
- Before sending response, COUNT YOUR OUTPUT LINES
- Compare with input line count - THEY MUST BE IDENTICAL
- Each output line MUST have format: [original] ||| [translation]

## Quality Standards
- Sound natural and idiomatic in the target language
- Maintain emotional impact and artistic intent
- Preserve names, emojis, and special markers
- Create smooth, singable translations with seamless line-to-line transitions"#;

    let payload = json!({
        "contents": [{
            "parts": [{
                "text": format!("{}\n\n{}", system_instruction, translation_prompt)
            }]
        }],
        "generationConfig": {
            "temperature": 0.3,
            "maxOutputTokens": 8192,
            "topP": 0.95,
            "topK": 40
        },
        "safetySettings": [
            {
                "category": "HARM_CATEGORY_HARASSMENT",
                "threshold": "BLOCK_MEDIUM_AND_ABOVE"
            },
            {
                "category": "HARM_CATEGORY_HATE_SPEECH",
                "threshold": "BLOCK_MEDIUM_AND_ABOVE"
            },
            {
                "category": "HARM_CATEGORY_SEXUALLY_EXPLICIT",
                "threshold": "BLOCK_MEDIUM_AND_ABOVE"
            },
            {
                "category": "HARM_CATEGORY_DANGEROUS_CONTENT",
                "threshold": "BLOCK_MEDIUM_AND_ABOVE"
            }
        ]
    });

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model_id, gemini_api_key
    );

    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(format!("HTTP {}: {}", status, error_text));
    }

    let result: Value = response.json().await.map_err(|e| e.to_string())?;

    // Check for finish reason issues first
    if let Some(finish_reason) = result["candidates"][0]["finishReason"].as_str() {
        if finish_reason == "MAX_TOKENS" {
            return Err("Gemini response truncated due to token limit. Try shorter input or increase maxOutputTokens.".to_string());
        }
        if finish_reason != "STOP" {
            return Err(format!(
                "Gemini response incomplete. Finish reason: {}",
                finish_reason
            ));
        }
    }

    // Parse Gemini response format - handle both old and new response structures
    let response_text = result["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .or_else(|| {
            // Fallback for newer API versions that might have different structure
            result["candidates"][0]["content"]["text"].as_str()
        })
        .or_else(|| {
            // Another fallback - check if parts array exists but is structured differently
            if let Some(parts) = result["candidates"][0]["content"]["parts"].as_array() {
                if let Some(first_part) = parts.get(0) {
                    return first_part["text"].as_str();
                }
            }
            None
        })
        .ok_or_else(|| {
            format!(
                "No text content found in Gemini response. Response structure: {:?}",
                result["candidates"][0]["content"]
            )
        })?;

    // Parse the translation response using the shared function with enhanced mismatch resolution
    // Note: is_same_language will always be false here since we already checked that above
    let (translated_lines, _is_same_language) =
        parse_translation_response(response_text, lyrics, &detected_language, Some(config)).await?;

    Ok(TranslationResult {
        success: true,
        translated_lines,
        detected_language: Some(detected_language),
        processing_time_ms: 0, // Will be set by caller
        error: None,
        is_same_language: false, // Always false here - same language is handled earlier
    })
}

fn detect_language_fallback(lyrics: &[LyricLine]) -> String {
    let sample_text = lyrics
        .iter()
        .take(3)
        .map(|line| line.text.clone())
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();

    // More accurate language detection that doesn't confuse Japanese and Chinese
    let hiragana_count = sample_text
        .chars()
        .filter(|c| matches!(c, '\u{3040}'..='\u{309f}'))
        .count();
    let katakana_count = sample_text
        .chars()
        .filter(|c| matches!(c, '\u{30a0}'..='\u{30ff}'))
        .count();
    let kanji_count = sample_text
        .chars()
        .filter(|c| matches!(c, '\u{4e00}'..='\u{9faf}'))
        .count();
    let total_chars = sample_text.chars().count();

    // Japanese detection: must have kana (hiragana or katakana)
    if hiragana_count > 0 || katakana_count > 0 {
        let kana_ratio = (hiragana_count + katakana_count) as f32 / total_chars as f32;
        if kana_ratio > 0.1 {
            // At least 10% kana indicates Japanese
            return "ja".to_string();
        }
    }

    // Korean detection
    if sample_text
        .chars()
        .any(|c| matches!(c, '\u{ac00}'..='\u{d7af}'))
    {
        return "ko".to_string();
    }

    // Chinese detection: has CJK characters but very little or no kana
    if kanji_count > 0 {
        let kana_ratio = (hiragana_count + katakana_count) as f32 / total_chars as f32;
        let kanji_ratio = kanji_count as f32 / total_chars as f32;
        if kanji_ratio > 0.3 && kana_ratio < 0.1 {
            // Mostly CJK with little kana
            // Check for Traditional Chinese specific characters
            let has_traditional = sample_text.chars().any(|c| {
                matches!(
                    c,
                    '這' | '樣'
                        | '時'
                        | '會'
                        | '過'
                        | '應'
                        | '來'
                        | '對'
                        | '於'
                        | '個'
                        | '電'
                        | '學'
                        | '國'
                        | '說'
                        | '話'
                        | '語'
                        | '見'
                        | '間'
                        | '開'
                        | '關'
                )
            });

            if has_traditional {
                return "zh-tw".to_string(); // Default to Taiwan Traditional
            } else {
                return "zh".to_string(); // Default to Simplified
            }
        }
    }

    // Russian detection
    if sample_text
        .chars()
        .any(|c| matches!(c, '\u{0400}'..='\u{04ff}'))
    {
        return "ru".to_string();
    }

    // Arabic detection
    if sample_text
        .chars()
        .any(|c| matches!(c, '\u{0600}'..='\u{06ff}'))
    {
        return "ar".to_string();
    }

    // European language detection with common word patterns
    let english_patterns = [
        " the ", " and ", " is ", " are ", " to ", " of ", " in ", " that ",
    ];
    let spanish_patterns = [
        " el ", " la ", " de ", " que ", " en ", " un ", " se ", " no ",
    ];
    let french_patterns = [
        " le ", " de ", " et ", " à ", " un ", " ce ", " que ", " du ",
    ];
    let german_patterns = [
        " der ", " die ", " das ", " und ", " ich ", " ist ", " du ", " sie ",
    ];
    let portuguese_patterns = [
        " de ", " que ", " não ", " um ", " se ", " da ", " do ", " em ",
    ];
    let italian_patterns = [
        " il ", " di ", " che ", " non ", " un ", " la ", " del ", " da ",
    ];

    if english_patterns
        .iter()
        .any(|&pattern| sample_text.contains(pattern))
    {
        return "en".to_string();
    }

    if spanish_patterns
        .iter()
        .any(|&pattern| sample_text.contains(pattern))
    {
        return "es".to_string();
    }

    if french_patterns
        .iter()
        .any(|&pattern| sample_text.contains(pattern))
    {
        return "fr".to_string();
    }

    if german_patterns
        .iter()
        .any(|&pattern| sample_text.contains(pattern))
    {
        return "de".to_string();
    }

    if portuguese_patterns
        .iter()
        .any(|&pattern| sample_text.contains(pattern))
    {
        return "pt".to_string();
    }

    if italian_patterns
        .iter()
        .any(|&pattern| sample_text.contains(pattern))
    {
        return "it".to_string();
    }

    // Default to English for undetected Latin scripts
    debug!("Language detection fallback to English");
    "en".to_string()
}

fn get_language_name(code: &str) -> &str {
    match code {
        "auto" => "Auto-detect",
        "en" => "English",
        "es" => "Español (Spanish)",
        "fr" => "Français (French)",
        "de" => "Deutsch (German)",
        "it" => "Italiano (Italian)",
        "pt" => "Português (Portuguese)",
        "ru" => "Русский (Russian)",
        "ja" => "日本語 (Japanese)",
        "ko" => "한국어 (Korean)",
        "zh" => "简体中文 (Chinese Simplified)",
        "zh-cn" => "简体中文 (Chinese Simplified)",
        "zh-tw" => "繁體中文 (Chinese Traditional - Taiwan)",
        "zh-hk" => "繁體中文 (Chinese Traditional - Hong Kong)",
        "ar" => "العربية (Arabic)",
        "hi" => "हिन्दी (Hindi)",
        "th" => "ไทย (Thai)",
        _ => "Unknown Language",
    }
}

fn get_native_language_name(code: &str) -> &str {
    match code {
        "auto" => "Auto-detect",
        "en" => "English",
        "es" => "Español",
        "fr" => "Français",
        "de" => "Deutsch",
        "it" => "Italiano",
        "pt" => "Português",
        "ru" => "Русский",
        "ja" => "日本語",
        "ko" => "한국어",
        "zh" => "简体中文",
        "zh-cn" => "简体中文",
        "zh-tw" => "繁體中文",
        "zh-hk" => "繁體中文",
        "ar" => "العربية",
        "hi" => "हिन्दी",
        "th" => "ไทย",
        _ => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_detection() {
        let japanese_lyrics = vec![LyricLine {
            time: 0.0,
            text: "これは日本語の歌詞です".to_string(),
            duration: Some(3.0),
        }];

        assert_eq!(detect_language_fallback(&japanese_lyrics), "ja");

        let english_lyrics = vec![LyricLine {
            time: 0.0,
            text: "This is an English song".to_string(),
            duration: Some(3.0),
        }];

        assert_eq!(detect_language_fallback(&english_lyrics), "en");
    }

    #[test]
    fn test_translation_config_default() {
        let config = TranslationConfig::default();
        assert_eq!(config.source_language, "auto");
        assert_eq!(config.target_language, "en");
        assert_eq!(config.style, "natural");
        assert!(config.preserve_meaning);
    }

    #[tokio::test]
    async fn test_translation_backend() {
        println!("🧪 Starting translation backend test...");

        // Test data - simple Spanish lyrics
        let test_lyrics = vec![
            LyricLine {
                time: 0.0,
                text: "Hola mundo, ¿cómo estás?".to_string(),
                duration: Some(3.0),
            },
            LyricLine {
                time: 3.0,
                text: "Es un día muy bonito".to_string(),
                duration: Some(3.0),
            },
        ];

        let test_config = TranslationConfig {
            source_language: "auto".to_string(),
            target_language: "en".to_string(),
            style: "natural".to_string(),
            custom_prompt: None,
            preserve_rhyme: true,
            preserve_meaning: true,
            maintain_structure: true,
            provider: "github".to_string(), // Use GitHub for testing
            github_api_key: None,
            gemini_api_key: None,
        };

        println!("📝 Testing with {} lines:", test_lyrics.len());
        for line in &test_lyrics {
            println!("  {}s: '{}'", line.time, line.text);
        }

        match translate_lyrics(test_lyrics, test_config).await {
            Ok(result) => {
                println!("✅ Translation successful!");
                println!("  Success: {}", result.success);
                println!("  Lines translated: {}", result.translated_lines.len());
                println!("  Detected language: {:?}", result.detected_language);
                println!("  Processing time: {}ms", result.processing_time_ms);

                for (i, line) in result.translated_lines.iter().enumerate() {
                    println!(
                        "  Line {}: '{}' -> '{}'",
                        i + 1,
                        line.original_text,
                        line.translated_text
                    );
                }

                assert!(result.success, "Translation should succeed");
                assert!(
                    !result.translated_lines.is_empty(),
                    "Should have translated lines"
                );
            }
            Err(e) => {
                println!("❌ Translation failed: {}", e);
                // Don't panic in test - just report the failure
                assert!(false, "Translation test failed: {}", e);
            }
        }
    }
}
