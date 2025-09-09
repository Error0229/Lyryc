use anyhow::Result;
use autoruby::{annotate, format, select};
use serde::{Deserialize, Serialize};
use tauri::command;

#[derive(Debug, Serialize, Deserialize)]
pub struct FuriganaSegment {
    pub text: String,
    pub reading: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FuriganaResult {
    pub segments: Vec<FuriganaSegment>,
    pub html: String,
}

/// Generate furigana for Japanese text using autoruby
pub async fn generate_furigana_for_text(text: &str) -> Result<FuriganaResult> {
    // Check if text contains Japanese characters
    if !contains_japanese(text) {
        return Ok(FuriganaResult {
            segments: vec![FuriganaSegment {
                text: text.to_string(),
                reading: None,
            }],
            html: text.to_string(),
        });
    }

    // Create annotator with integrated dictionary
    let annotator = annotate::Annotator::new_with_integrated_dictionary();
    
    // Annotate the text with furigana
    let annotated = annotator.annotate(text);
    
    // Generate HTML output using the correct formatter
    let html_result = annotated.render(&select::heuristic::All, &format::Html);
    
    // For now, parse the HTML to create segments
    // In a real implementation, we'd access the annotation data directly
    let segments = parse_html_to_segments(&html_result, text);
    
    Ok(FuriganaResult {
        segments,
        html: html_result,
    })
}

/// Parse HTML ruby tags to create segments
fn parse_html_to_segments(html: &str, original_text: &str) -> Vec<FuriganaSegment> {
    let mut segments = Vec::new();
    
    // Simple regex to match ruby tags: <ruby>text<rp>(</rp><rt>reading</rt><rp>)</rp></ruby>
    let ruby_regex = regex::Regex::new(r"<ruby>([^<]+)<rp>\(</rp><rt>([^<]+)</rt><rp>\)</rp></ruby>").unwrap();
    
    let mut last_end = 0;
    
    for mat in ruby_regex.find_iter(html) {
        let match_str = mat.as_str();
        let start = mat.start();
        let end = mat.end();
        
        // Add any text before this ruby tag
        if start > last_end {
            let before_text = &html[last_end..start];
            if !before_text.is_empty() {
                segments.push(FuriganaSegment {
                    text: before_text.to_string(),
                    reading: None,
                });
            }
        }
        
        // Extract text and reading from ruby tag
        if let Some(caps) = ruby_regex.captures(match_str) {
            let text = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let reading = caps.get(2).map(|m| m.as_str()).unwrap_or("");
            
            segments.push(FuriganaSegment {
                text: text.to_string(),
                reading: Some(reading.to_string()),
            });
        }
        
        last_end = end;
    }
    
    // Add any remaining text
    if last_end < html.len() {
        let remaining = &html[last_end..];
        if !remaining.is_empty() {
            segments.push(FuriganaSegment {
                text: remaining.to_string(),
                reading: None,
            });
        }
    }
    
    // If no ruby tags found, return original text as single segment
    if segments.is_empty() {
        segments.push(FuriganaSegment {
            text: original_text.to_string(),
            reading: None,
        });
    }
    
    segments
}

/// Check if text contains Japanese characters (hiragana, katakana, or kanji)
fn contains_japanese(text: &str) -> bool {
    text.chars().any(|c| {
        let code = c as u32;
        // Hiragana: U+3040-U+309F
        (code >= 0x3040 && code <= 0x309F) ||
        // Katakana: U+30A0-U+30FF
        (code >= 0x30A0 && code <= 0x30FF) ||
        // Kanji (CJK Unified Ideographs): U+4E00-U+9FAF
        (code >= 0x4E00 && code <= 0x9FAF)
    })
}

/// Tauri command to generate furigana for Japanese text
#[command]
pub async fn generate_furigana_command(text: String) -> Result<FuriganaResult, String> {
    generate_furigana_for_text(&text)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_furigana_generation() {
        let test_cases = vec![
            ("今日", "today"),
            ("学生", "student"), 
            ("こんにちは", "hello - hiragana only"),
            ("Hello", "english text"),
        ];

        for (text, description) in test_cases {
            println!("Testing: {} ({})", text, description);
            match generate_furigana_for_text(text).await {
                Ok(result) => {
                    println!("  Segments: {:?}", result.segments);
                    println!("  HTML: {}", result.html);
                }
                Err(e) => println!("  Error: {}", e),
            }
            println!();
        }
    }

    #[test]
    fn test_japanese_detection() {
        assert!(contains_japanese("今日"));
        assert!(contains_japanese("こんにちは"));
        assert!(contains_japanese("カタカナ"));
        assert!(!contains_japanese("Hello"));
        assert!(!contains_japanese("123"));
    }
}