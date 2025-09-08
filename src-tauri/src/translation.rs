use log::{debug, info, error, warn};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

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
                debug!("Model {} is blacklisted until {}", model_id, blacklist_until);
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
        .as_secs() + wait_seconds;
    
    if let Ok(mut blacklist) = MODEL_BLACKLIST.lock() {
        blacklist.insert(model_id.to_string(), blacklist_until);
        warn!("Blacklisted model {} for {} seconds (until {})", model_id, wait_seconds, blacklist_until);
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

/// Main translation function - uses LLM with fallback support
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
        });
    }

    info!("Starting translation for {} lines from {} to {}", 
          lyrics.len(), config.source_language, config.target_language);

    let result = match config.provider.as_str() {
        "gemini" => translate_lyrics_gemini(&lyrics, &config).await,
        "github" | _ => translate_lyrics_github(&lyrics, &config).await,
    };

    match result {
        Ok(mut translation_result) => {
            translation_result.processing_time_ms = start_time.elapsed().as_millis() as u64;
            info!("Translation completed successfully in {}ms", translation_result.processing_time_ms);
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
            })
        }
    }
}

/// Translate lyrics using GitHub AI models (similar to track_cleaning.rs pattern)
async fn translate_lyrics_github(
    lyrics: &[LyricLine],
    config: &TranslationConfig,
) -> Result<TranslationResult, String> {
    // Get API key from config or environment
    let github_token = config.github_api_key.clone()
        .or_else(|| std::env::var("GITHUB_TOKEN").ok())
        .ok_or_else(|| "GitHub API key required for translation".to_string())?;

    // High-performance models from test results - only use models that work
    let all_models = [
        "meta/Llama-4-Maverick-17B-128E-Instruct-FP8",  // 98.33% accuracy - best performer
        "openai/gpt-4.1-nano",                           // 98.33% accuracy - excellent
        "openai/gpt-4o-mini",                            // 96.67% accuracy - reliable
        "mistral ai/Mistral-Large-2411",                 // 96.67% accuracy - proven
        "mistral ai/mistral-medium-2505",                // 96.67% accuracy - consistent
        "mistral ai/mistral-small-2503",                 // 96.67% accuracy - fast
        "meta/Meta-Llama-3.1-8B-Instruct",              // 95% accuracy - very fast
        "mistral ai/Codestral-2501",                     // 95% accuracy - good
        "deepseek/DeepSeek-V3-0324",                     // 90% accuracy - backup
        "meta/Llama-3.3-70B-Instruct",                   // 90% accuracy - free
        "xai/grok-3",                                     // 90% accuracy - alternative
        "meta/Llama-4-Scout-17B-16E-Instruct",          // 90% accuracy - reliable
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

    info!("Available GitHub models: {} out of {} total", available_models.len(), all_models.len());

    // Pass language detection to translation function - no separate API call
    let source_language = config.source_language.clone();

    for (i, model_id) in available_models.iter().enumerate() {
        match try_llm_translation(lyrics, config, &source_language, model_id, &github_token).await {
            Ok(result) => {
                info!(
                    "Translation successful with {}: {} lines translated",
                    model_id, result.translated_lines.len()
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
                    model_id, e, i + 1, available_models.len()
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
    let client = Client::builder()
        .timeout(Duration::from_secs(45))
        .build()
        .map_err(|e| e.to_string())?;

    // Create the translation prompt
    let lyrics_text: Vec<String> = lyrics.iter().map(|line| line.text.clone()).collect();
    let lyrics_input = lyrics_text.join("\n");

    let translation_prompt = build_translation_prompt(
        &lyrics_input,
        source_language,
        &config.target_language,
        &config.style,
        config.custom_prompt.as_deref(),
    );

    let system_content = r#"You are a professional context-aware lyrics translator specializing in music and poetry.

## Core Philosophy
Approach each song as a complete artistic work with unified themes, emotions, and narrative. Your goal is producing natural, coherent translations that capture the essence of the original while sounding native in the target language.

## Translation Approach
- **Holistic Analysis**: Consider the entire song's context before translating any individual line
- **Natural Flow**: Prioritize idiomatic expressions over literal word-for-word translation  
- **Coherence**: Ensure consistent tone, pronouns, and terminology throughout
- **Cultural Sensitivity**: Adapt cultural references appropriately while preserving meaning
- **Musical Awareness**: Consider how the translation will sound when sung

## Output Requirements
- Return EXACTLY one translated line per input line
- Maintain the same total number of lines as input
- No line numbers, timestamps, or formatting
- No explanations, commentary, or footnotes
- Raw translated text only

## Quality Standards
- Sound natural and idiomatic in the target language
- Maintain emotional impact and artistic intent
- Preserve names, emojis, and special markers
- Create smooth, singable translations when appropriate"#;

    let payload = json!({
        "messages": [
            {"role": "system", "content": system_content},
            {"role": "user", "content": translation_prompt}
        ],
        "temperature": 0.3,
        "max_tokens": 2048,
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

    // For auto-detection, use fallback detection or "unknown"
    let final_detected_language = if source_language == "auto" {
        detect_language_fallback(lyrics)
    } else {
        source_language.to_string()
    };

    // Parse the translation response
    let translated_lines = parse_translation_response(response_text, lyrics, &final_detected_language)?;

    Ok(TranslationResult {
        success: true,
        translated_lines,
        detected_language: Some(final_detected_language),
        processing_time_ms: 0, // Will be set by caller
        error: None,
    })
}

fn build_translation_prompt(
    lyrics: &str,
    _source_lang: &str,
    target_lang: &str,
    style: &str,
    custom_prompt: Option<&str>,
) -> String {
    let target_lang_name = get_language_name(target_lang);
    
    // If custom prompt is provided, use it as-is
    if let Some(custom) = custom_prompt {
        return format!(
            r#"{}

Original lyrics:
{}

Translated lyrics:"#,
            custom, lyrics
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
Priorities: (1) rhyme scheme preservation, (2) musical flow, (3) meaning fidelity.

Process:
1) Read all lyrics first; identify rhyme patterns, meter, and musical structure
2) Translate by verse/chorus sections, maintaining rhyme schemes and rhythm
3) Allow moderate paraphrase to achieve singable rhymes in {}
4) Keep names/emojis/special markers; prioritize rhyme over literal meaning
5) Unify vocabulary across verses; prefer musical/rhythmic phrasing

Constraints:
• No commentary/footnotes  
• Output exactly in requested format
• Maintain singability and rhyme structure",
            target_lang_name, target_lang_name
        ),
        "poetic" => format!(
            "You are a context-aware lyrics translator/adaptor to {}.
Priorities: (1) artistic beauty, (2) emotional impact, (3) cultural resonance.

Process:
1) Read all lyrics first; infer artistic intent, imagery, and emotional arc
2) Translate by stanza with focus on poetic devices and metaphor
3) Allow creative adaptation to capture artistic essence in {}
4) Keep names/emojis/special markers; enhance poetic elements
5) Unify tone and imagery; prefer beautiful, evocative phrasing

Constraints:
• No commentary/footnotes
• Output exactly in requested format  
• Prioritize artistic impact over literal accuracy",
            target_lang_name, target_lang_name
        ),
        "natural" | _ => format!(
            "You are a context-aware lyrics translator/adaptor to {}.
Priorities: (1) scenario fit & naturalness, (2) whole-song coherence, (3) meaning fidelity.

Process:
1) Read all lyrics first; infer roles, mood, setting, time, register, and recurring motifs
2) Translate by stanza (not strict line-by-line literal). Allow light paraphrase/reordering to sound natural in {}
3) Keep names/emojis/special markers; maintain natural flow over strict rhyme
4) Unify pronouns/tense/terms across the whole song; prefer idiomatic {}
5) Final coherence sweep: remove awkward phrasing and contradictions

Constraints:
• No commentary/footnotes
• Output exactly in requested format",
            target_lang_name, target_lang_name, target_lang_name
        ),
    };

    format!(
        r#"{}

Original lyrics:
{}

Translated lyrics:"#,
        style_specific_instruction, lyrics
    )
}

fn parse_translation_response(
    response_text: &str,
    original_lyrics: &[LyricLine],
    detected_language: &str,
) -> Result<Vec<TranslatedLyricLine>, String> {
    // Clean the response text
    let cleaned_response = response_text
        .trim()
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();

    if cleaned_response.len() != original_lyrics.len() {
        debug!(
            "Line count mismatch: original {} vs translated {}",
            original_lyrics.len(),
            cleaned_response.len()
        );
    }

    let mut translated_lines = Vec::new();

    for (i, original_line) in original_lyrics.iter().enumerate() {
        let translated_text = cleaned_response
            .get(i)
            .unwrap_or(&original_line.text.as_str())
            .to_string();

        translated_lines.push(TranslatedLyricLine {
            time: original_line.time,
            text: translated_text.clone(),
            duration: original_line.duration,
            original_text: original_line.text.clone(),
            translated_text,
            translation_confidence: Some(0.85), // Placeholder confidence
            detected_language: Some(detected_language.to_string()),
        });
    }

    Ok(translated_lines)
}


/// Translate lyrics using Google Gemini API
async fn translate_lyrics_gemini(
    lyrics: &[LyricLine],
    config: &TranslationConfig,
) -> Result<TranslationResult, String> {
    // Get API key from config or environment
    let gemini_api_key = config.gemini_api_key.clone()
        .or_else(|| std::env::var("GEMINI_API_KEY").ok())
        .ok_or_else(|| "Gemini API key required for translation".to_string())?;

    // Available Gemini models (based on the pricing table)
    let models = [
        "gemini-2.0-flash",      // Latest with 1M TPM, 15 RPM
        "gemini-2.5-flash",      // Fast with 250K TPM, 10 RPM
        "gemini-2.5-pro",        // Most capable with 250K TPM, 5 RPM
    ];

    // Pass language detection to translation function - no separate API call
    let source_language = config.source_language.clone();

    for (i, model_id) in models.iter().enumerate() {
        match try_gemini_translation(lyrics, config, &source_language, model_id, &gemini_api_key).await {
            Ok(result) => {
                info!(
                    "Translation successful with {}: {} lines translated",
                    model_id, result.translated_lines.len()
                );
                return Ok(result);
            }
            Err(e) => {
                debug!(
                    "Translation failed with {}: {} (attempt {}/{})",
                    model_id, e, i + 1, models.len()
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
    let client = Client::builder()
        .timeout(Duration::from_secs(45))
        .build()
        .map_err(|e| e.to_string())?;

    // Create the translation prompt
    let lyrics_text: Vec<String> = lyrics.iter().map(|line| line.text.clone()).collect();
    let lyrics_input = lyrics_text.join("\n");

    let translation_prompt = build_translation_prompt(
        &lyrics_input,
        source_language,
        &config.target_language,
        &config.style,
        config.custom_prompt.as_deref(),
    );

    let system_instruction = r#"You are a professional context-aware lyrics translator specializing in music and poetry.

## Core Philosophy
Approach each song as a complete artistic work with unified themes, emotions, and narrative. Your goal is producing natural, coherent translations that capture the essence of the original while sounding native in the target language.

## Translation Approach
- **Holistic Analysis**: Consider the entire song's context before translating any individual line
- **Natural Flow**: Prioritize idiomatic expressions over literal word-for-word translation  
- **Coherence**: Ensure consistent tone, pronouns, and terminology throughout
- **Cultural Sensitivity**: Adapt cultural references appropriately while preserving meaning
- **Musical Awareness**: Consider how the translation will sound when sung

## Output Requirements
- Return EXACTLY one translated line per input line
- Maintain the same total number of lines as input
- No line numbers, timestamps, or formatting
- No explanations, commentary, or footnotes
- Raw translated text only

## Quality Standards
- Sound natural and idiomatic in the target language
- Maintain emotional impact and artistic intent
- Preserve names, emojis, and special markers
- Create smooth, singable translations when appropriate"#;

    let payload = json!({
        "contents": [{
            "parts": [{
                "text": format!("{}\n\n{}", system_instruction, translation_prompt)
            }]
        }],
        "generationConfig": {
            "temperature": 0.3,
            "maxOutputTokens": 2048,
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

    let url = format!("https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}", model_id, gemini_api_key);
    
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
    
    // Parse Gemini response format
    let response_text = result["candidates"][0]["content"]["parts"][0]["text"]
        .as_str()
        .ok_or("No content in Gemini response".to_string())?;

    // For auto-detection, use fallback detection or "unknown"
    let final_detected_language = if source_language == "auto" {
        detect_language_fallback(lyrics)
    } else {
        source_language.to_string()
    };

    // Parse the translation response using the shared function
    let translated_lines = parse_translation_response(response_text, lyrics, &final_detected_language)?;

    Ok(TranslationResult {
        success: true,
        translated_lines,
        detected_language: Some(final_detected_language),
        processing_time_ms: 0, // Will be set by caller
        error: None,
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

    // Basic Unicode range detection for non-Latin scripts
    if sample_text.chars().any(|c| matches!(c, '\u{3040}'..='\u{309f}' | '\u{30a0}'..='\u{30ff}' | '\u{4e00}'..='\u{9faf}')) {
        return "ja".to_string(); // Japanese
    }
    if sample_text.chars().any(|c| matches!(c, '\u{ac00}'..='\u{d7af}')) {
        return "ko".to_string(); // Korean
    }
    if sample_text.chars().any(|c| matches!(c, '\u{4e00}'..='\u{9fff}')) {
        return "zh".to_string(); // Chinese (default to simplified)
    }
    if sample_text.chars().any(|c| matches!(c, '\u{0400}'..='\u{04ff}')) {
        return "ru".to_string(); // Russian
    }
    if sample_text.chars().any(|c| matches!(c, '\u{0600}'..='\u{06ff}')) {
        return "ar".to_string(); // Arabic
    }
    
    // Default to English for Latin scripts
    debug!("Language detection fallback to English");
    "en".to_string()
}

fn get_language_name(code: &str) -> &str {
    match code {
        "auto" => "Auto-detect",
        "en" => "English",
        "es" => "Spanish",
        "fr" => "French", 
        "de" => "German",
        "it" => "Italian",
        "pt" => "Portuguese",
        "ru" => "Russian",
        "ja" => "Japanese",
        "ko" => "Korean",
        "zh" => "Chinese (Simplified)",
        "zh-tw" => "Chinese (Traditional)",
        "ar" => "Arabic",
        "hi" => "Hindi",
        "th" => "Thai",
        _ => "Unknown Language",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_detection() {
        let japanese_lyrics = vec![
            LyricLine {
                time: 0.0,
                text: "これは日本語の歌詞です".to_string(),
                duration: Some(3.0),
            }
        ];
        
        assert_eq!(detect_language_fallback(&japanese_lyrics), "ja");
        
        let english_lyrics = vec![
            LyricLine {
                time: 0.0,
                text: "This is an English song".to_string(), 
                duration: Some(3.0),
            }
        ];
        
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
                    println!("  Line {}: '{}' -> '{}'", i+1, line.original_text, line.translated_text);
                }
                
                assert!(result.success, "Translation should succeed");
                assert!(!result.translated_lines.is_empty(), "Should have translated lines");
            },
            Err(e) => {
                println!("❌ Translation failed: {}", e);
                // Don't panic in test - just report the failure
                assert!(false, "Translation test failed: {}", e);
            }
        }
    }
}