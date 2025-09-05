use dotenv::dotenv;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};
use tokio;
use rand::seq::SliceRandom;
use rand::thread_rng;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ModelInfo {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub original_name: Option<String>,
    #[serde(default)]
    pub friendly_name: Option<String>,
    #[serde(default)]
    pub task: Option<String>,
    #[serde(default)]
    pub publisher: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PricingInfo {
    pub input_cost_per_token: Option<f64>,
    pub output_cost_per_token: Option<f64>,
    pub max_tokens: Option<u32>,
    pub max_input_tokens: Option<u32>,
    pub max_output_tokens: Option<u32>,
}

#[derive(Debug, Serialize, Clone)]
pub struct ModelTestResult {
    pub model_id: String,
    pub friendly_name: String,
    pub pricing: PricingData,
    pub performance: PerformanceData,
    pub error: Option<String>,
    pub test_metadata: TestMetadata,
}

#[derive(Debug, Serialize, Clone)]
pub struct PricingData {
    pub input_cost_per_token: f64,
    pub output_cost_per_token: f64,
    pub max_tokens: u32,
}

#[derive(Debug, Serialize, Clone)]
pub struct PerformanceData {
    pub response_time_seconds: f64,
    pub tokens_used: u32,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cost_estimate_usd: f64,
    pub tokens_per_second: f64,
    pub cost_per_valid_output: f64,
    pub performance_score: f64,
    pub response_quality: String,
    pub correct_cleanings: u32,
    pub accuracy_rate: f64,
}

#[derive(Debug, Serialize, Clone)]
pub struct TestMetadata {
    pub test_date: String,
    pub test_data_size: usize,
    pub test_type: String,
}

#[derive(Debug, Serialize)]
pub struct TestSummary {
    pub total_models_tested: usize,
    pub successful_tests: usize,
    pub failed_tests: usize,
    pub average_response_time: f64,
    pub average_cost: f64,
    pub average_tokens_per_second: f64,
    pub average_accuracy_rate: f64,
    pub best_performance: f64,
    pub fastest_model: Option<String>,
    pub cheapest_model: Option<String>,
    pub highest_quality: Option<String>,
    pub best_accuracy_model: Option<String>,
    pub most_efficient_model: Option<String>, // best tokens per second
}

#[derive(Debug, Serialize)]
pub struct FinalResults {
    pub summary: TestSummary,
    pub test_configuration: TestConfiguration,
    pub individual_results: Vec<ModelTestResult>,
}

#[derive(Debug, Serialize)]
pub struct TestConfiguration {
    pub models_tested: Vec<String>,
    pub test_data_source: String,
    pub prompt_template: String,
    pub pricing_data_source: String,
    pub model_data_source: String,
}

pub struct ModelPerformanceTester {
    client: Client,
    github_token: String,
    available_models: Vec<ModelInfo>,
    pricing_data: HashMap<String, PricingInfo>,
    test_data: Vec<(String, String)>, // (original, cleaned) pairs
    prompt_system: String,
    test_models: Vec<(String, ModelInfo, Option<PricingInfo>)>, // (model_id, model_info, pricing)
}

impl ModelPerformanceTester {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // Load environment variables from .env file
        dotenv().ok();

        let github_token = std::env::var("GITHUB_TOKEN").map_err(|_| {
            "GITHUB_TOKEN environment variable is required. Please check your .env file."
        })?;

        let client = Client::builder().timeout(Duration::from_secs(60)).build()?;

        let mut tester = Self {
            client,
            github_token,
            available_models: Vec::new(),
            pricing_data: HashMap::new(),
            test_data: Vec::new(),
            prompt_system: String::new(),
            test_models: Vec::new(), // Will be populated in load_data()
        };

        tester.load_data()?;
        Ok(tester)
    }

    fn load_data(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Loading available models...");

        // Load ALL available models
        let models_content = fs::read_to_string("../tests/available-models.json")?;
        let all_models: Vec<Value> = serde_json::from_str(&models_content)?;

        for model_value in all_models.iter() {
            if let Ok(model) = serde_json::from_value::<ModelInfo>(model_value.clone()) {
                // Filter for chat-completion models
                if let Some(task) = &model.task {
                    if task == "chat-completion" {
                        self.available_models.push(model);
                    }
                }
            }
        }

        println!(
            "Found {} chat-completion models",
            self.available_models.len()
        );

        // Load ALL pricing data
        println!("Loading pricing data...");
        let pricing_content =
            fs::read_to_string("../tests/model_prices_and_context_window.json")?;
        let all_pricing: Value = serde_json::from_str(&pricing_content)?;

        if let Value::Object(pricing_map) = all_pricing {
            for (key, value) in pricing_map.iter() {
                if let Ok(pricing) = serde_json::from_value::<PricingInfo>(value.clone()) {
                    self.pricing_data.insert(key.clone(), pricing);
                }
            }
        }

        println!("Loaded pricing data for {} models", self.pricing_data.len());

        // Match models with pricing data and create test list
        println!("Matching models with pricing data...");
        for model in &self.available_models {
            // Try to find pricing data using various model ID formats with fuzzy matching
            let mut pricing_info = None;
            let potential_keys = vec![
                model.id.clone(),
                model.original_name.clone().unwrap_or_default(),
                model.name.clone().unwrap_or_default(),
                // Try common GitHub AI format
                format!(
                    "{}/{}",
                    model.publisher.clone().unwrap_or_default().to_lowercase(),
                    model.original_name.clone().unwrap_or_default()
                ),
            ];

            // First try exact matches
            for key in &potential_keys {
                if !key.is_empty() {
                    if let Some(pricing) = self.pricing_data.get(key) {
                        pricing_info = Some(pricing.clone());
                        break;
                    }
                }
            }

            // If no exact match found, try fuzzy matching with space-dash replacement
            if pricing_info.is_none() {
                for key in &potential_keys {
                    if !key.is_empty() {
                        // Try with spaces replaced by dashes
                        let key_with_dashes = key.replace(" ", "-");
                        if let Some(pricing) = self.pricing_data.get(&key_with_dashes) {
                            pricing_info = Some(pricing.clone());
                            break;
                        }
                        
                        // Try with dashes replaced by spaces
                        let key_with_spaces = key.replace("-", " ");
                        if let Some(pricing) = self.pricing_data.get(&key_with_spaces) {
                            pricing_info = Some(pricing.clone());
                            break;
                        }

                        // Try case-insensitive fuzzy matching for partial matches
                        let key_lower = key.to_lowercase();
                        for (pricing_key, pricing_value) in &self.pricing_data {
                            let pricing_key_lower = pricing_key.to_lowercase();
                            // Check if model name appears in pricing key or vice versa
                            if (key_lower.len() > 3 && pricing_key_lower.contains(&key_lower)) ||
                               (pricing_key_lower.len() > 3 && key_lower.contains(&pricing_key_lower)) {
                                pricing_info = Some(pricing_value.clone());
                                break;
                            }
                        }
                        
                        if pricing_info.is_some() {
                            break;
                        }
                    }
                }
            }

            // Extract model ID for GitHub AI format
            let model_id =
                if let (Some(publisher), Some(name)) = (&model.publisher, &model.original_name) {
                    format!("{}/{}", publisher.to_lowercase(), name)
                } else if let Some(original_name) = &model.original_name {
                    original_name.clone()
                } else {
                    model.id.clone()
                };

            self.test_models
                .push((model_id, model.clone(), pricing_info));
        }

        println!("Prepared {} models for testing", self.test_models.len());

        // Load all test data first, then randomly sample up to 65 cases
        let csv_content = fs::read_to_string("../tests/LyricCleanData.csv")?;
        let mut reader = csv::Reader::from_reader(csv_content.as_bytes());
        let mut all_test_data = Vec::new();

        for result in reader.records() {
            let record = result?;
            if record.len() >= 2 {
                all_test_data.push((record[0].to_string(), record[1].to_string()));
            }
        }

        // Randomly sample up to 60 test cases
        let mut rng = thread_rng();
        all_test_data.shuffle(&mut rng);
        self.test_data = all_test_data.into_iter().take(60).collect();

        println!("Loaded {} test cases (randomly sampled)", self.test_data.len());

        // Load prompt template from the original file
        let prompt_content = fs::read_to_string("../TrackNameCleaner.prompt.yml")?;
        let prompt_yaml: Value = serde_yaml::from_str(&prompt_content)?;

        if let Some(messages) = prompt_yaml.get("messages") {
            if let Some(system_msg) = messages.get(0) {
                if let Some(content) = system_msg.get("content") {
                    self.prompt_system = content.as_str().unwrap_or("").to_string();
                }
            }
        }

        Ok(())
    }

    fn get_model_info(&self, model_id: &str) -> (Option<ModelInfo>, Option<PricingInfo>) {
        // Find the model in our test_models list
        for (id, model_info, pricing_info) in &self.test_models {
            if id == model_id {
                return (Some(model_info.clone()), pricing_info.clone());
            }
        }
        (None, None)
    }

    fn create_test_prompt(&self) -> String {
        let test_tracks: Vec<String> = self
            .test_data
            .iter()
            .map(|(original, _)| original.clone())
            .collect();

        let tracks_text = test_tracks.join("\n");

        format!(
            r#"**Task**: Clean the following track titles into canonical song titles using human judgment.

**Configuration** (YAML):

```yaml
keep_feat: false
keep_version_markers: true
prefer_primary_language: as_is
allow_album_context: false
preserve_parenthetical_if_ambiguous: true
```

**Items (one per line):**

```
{}
```

**Output Format - CRITICAL**:
- Return EXACTLY one JSON object per input line
- Each JSON object must be on its own line
- NO markdown formatting (no ```json blocks)  
- NO wrapping in code blocks or backticks
- NO extra text, explanations, or commentary
- ONLY raw JSON objects, one per line
- Schema: `cleaned_title`, `kept_markers`, `removed_context`, `confidence`, `notes`

IMPORTANT: Output must be parseable as raw JSON. Do not use markdown, code blocks, or any formatting."#,
            tracks_text
        )
    }

    async fn test_model(&self, model_id: &str, test_index: usize) -> ModelTestResult {
        println!("Testing model: {}", model_id);

        let (model_info, pricing_info) = self.get_model_info(model_id);

        let friendly_name = model_info
            .as_ref()
            .and_then(|m| m.friendly_name.as_ref())
            .unwrap_or(&model_id.to_string())
            .clone();

        let input_cost = pricing_info
            .as_ref()
            .and_then(|p| p.input_cost_per_token)
            .unwrap_or(0.0);

        let output_cost = pricing_info
            .as_ref()
            .and_then(|p| p.output_cost_per_token)
            .unwrap_or(0.0);

        let max_tokens = pricing_info
            .as_ref()
            .and_then(|p| p.max_tokens.or(p.max_input_tokens))
            .unwrap_or(4096);

        // Create test prompt
        let user_content = self.create_test_prompt();

        // Estimate input tokens (rough approximation: ~4 chars per token)
        let estimated_input_tokens = (self.prompt_system.len() + user_content.len()) / 4;

        // Check token limits
        if estimated_input_tokens > (max_tokens as usize * 7 / 10) {
            return ModelTestResult {
                model_id: model_id.to_string(),
                friendly_name,
                pricing: PricingData {
                    input_cost_per_token: input_cost,
                    output_cost_per_token: output_cost,
                    max_tokens,
                },
                performance: PerformanceData {
                    response_time_seconds: 0.0,
                    tokens_used: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                    cost_estimate_usd: 0.0,
                    tokens_per_second: 0.0,
                    cost_per_valid_output: 0.0,
                    performance_score: 0.0,
                    response_quality: "error".to_string(),
                    correct_cleanings: 0,
                    accuracy_rate: 0.0,
                },
                error: Some(format!(
                    "Input too long: ~{} tokens (max: {})",
                    estimated_input_tokens, max_tokens
                )),
                test_metadata: TestMetadata {
                    test_date: chrono::Utc::now().to_rfc3339(),
                    test_data_size: self.test_data.len(),
                    test_type: "track_name_cleaning".to_string(),
                },
            };
        }

        // Prepare request payload
        let payload = json!({
            "messages": [
                {"role": "system", "content": self.prompt_system},
                {"role": "user", "content": user_content}
            ],
            "temperature": 0.1,
            "max_tokens": std::cmp::min(3000, max_tokens / 2),
            "model": model_id
        });

        // Execute request
        let start_time = Instant::now();

        let response = self
            .client
            .post("https://models.github.ai/inference/chat/completions")
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", self.github_token))
            .json(&payload)
            .send()
            .await;

        let response_time = start_time.elapsed().as_secs_f64();

        match response {
            Ok(resp) if resp.status().is_success() => {
                match resp.json::<Value>().await {
                    Ok(result) => {
                        // Extract usage information
                        let empty_usage = json!({});
                        let usage = result.get("usage").unwrap_or(&empty_usage);
                        let total_tokens = usage
                            .get("total_tokens")
                            .and_then(|v| v.as_u64())
                            .unwrap_or((estimated_input_tokens + 500) as u64)
                            as u32;

                        let completion_tokens = usage
                            .get("completion_tokens")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(500) as u32;

                        // Calculate costs
                        let input_tokens = total_tokens - completion_tokens;
                        let cost = (input_tokens as f64 * input_cost)
                            + (completion_tokens as f64 * output_cost);

                        // Extract response text
                        let response_text = result["choices"][0]["message"]["content"]
                            .as_str()
                            .unwrap_or("");

                        // Check if response appears truncated
                        let appears_truncated = !response_text.trim().ends_with('}') && 
                                               response_text.lines().count() < self.test_data.len();
                        
                        // Log response to file
                        self.log_response_to_file(model_id, response_text, test_index, appears_truncated).await.ok();

                        // Evaluate response quality
                        let (performance_score, correct_cleanings) = self.evaluate_response_quality_detailed(response_text);
                        let accuracy_rate = correct_cleanings as f64 / self.test_data.len() as f64;
                        let quality = if performance_score >= 0.95 {
                            "excellent"
                        } else if performance_score >= 0.8 {
                            "good"  
                        } else if performance_score >= 0.6 {
                            "fair"
                        } else if performance_score >= 0.3 {
                            "poor"
                        } else {
                            "very_poor"
                        };

                        // Calculate tokens per second
                        let tokens_per_second = if response_time > 0.0 {
                            total_tokens as f64 / response_time
                        } else {
                            0.0
                        };

                        // Calculate cost per correct output (only count actually correct cleanings)
                        let cost_per_valid_output = if correct_cleanings == self.test_data.len() as u32 {
                            cost / correct_cleanings as f64
                        } else if correct_cleanings > 0 {
                            // Heavy penalty for incorrect/incomplete responses
                            (cost / correct_cleanings as f64) * 3.0
                        } else {
                            f64::INFINITY
                        };

                        ModelTestResult {
                            model_id: model_id.to_string(),
                            friendly_name,
                            pricing: PricingData {
                                input_cost_per_token: input_cost,
                                output_cost_per_token: output_cost,
                                max_tokens,
                            },
                            performance: PerformanceData {
                                response_time_seconds: response_time,
                                tokens_used: total_tokens,
                                input_tokens,
                                output_tokens: completion_tokens,
                                cost_estimate_usd: cost,
                                tokens_per_second,
                                cost_per_valid_output,
                                performance_score,
                                response_quality: quality.to_string(),
                                correct_cleanings: correct_cleanings,
                                accuracy_rate,
                            },
                            error: None,
                            test_metadata: TestMetadata {
                                test_date: chrono::Utc::now().to_rfc3339(),
                                test_data_size: self.test_data.len(),
                                test_type: "track_name_cleaning".to_string(),
                            },
                        }
                    }
                    Err(e) => self.create_error_result(
                        model_id,
                        &friendly_name,
                        input_cost,
                        output_cost,
                        max_tokens,
                        format!("JSON parse error: {}", e),
                    ),
                }
            }
            Ok(resp) => {
                let status = resp.status();
                let error_text = resp
                    .text()
                    .await
                    .unwrap_or_else(|_| "Unknown error".to_string());
                self.create_error_result(
                    model_id,
                    &friendly_name,
                    input_cost,
                    output_cost,
                    max_tokens,
                    format!("HTTP {}: {}", status, error_text),
                )
            }
            Err(e) => self.create_error_result(
                model_id,
                &friendly_name,
                input_cost,
                output_cost,
                max_tokens,
                format!("Request error: {}", e),
            ),
        }
    }

    fn create_error_result(
        &self,
        model_id: &str,
        friendly_name: &str,
        input_cost: f64,
        output_cost: f64,
        max_tokens: u32,
        error: String,
    ) -> ModelTestResult {
        ModelTestResult {
            model_id: model_id.to_string(),
            friendly_name: friendly_name.to_string(),
            pricing: PricingData {
                input_cost_per_token: input_cost,
                output_cost_per_token: output_cost,
                max_tokens,
            },
            performance: PerformanceData {
                response_time_seconds: 0.0,
                tokens_used: 0,
                input_tokens: 0,
                output_tokens: 0,
                cost_estimate_usd: 0.0,
                tokens_per_second: 0.0,
                cost_per_valid_output: 0.0,
                performance_score: 0.0,
                response_quality: "error".to_string(),
                correct_cleanings: 0,
                accuracy_rate: 0.0,
            },
            error: Some(error),
            test_metadata: TestMetadata {
                test_date: chrono::Utc::now().to_rfc3339(),
                test_data_size: self.test_data.len(),
                test_type: "track_name_cleaning".to_string(),
            },
        }
    }

    async fn log_response_to_file(&self, model_id: &str, response_text: &str, test_index: usize, appears_truncated: bool) -> Result<(), Box<dyn std::error::Error>> {
        // Create logs directory if it doesn't exist
        let logs_dir = "../test_logs";
        if !Path::new(logs_dir).exists() {
            fs::create_dir_all(logs_dir)?;
        }

        // Create a safe filename from model_id
        let safe_model_id = model_id.replace("/", "_").replace(":", "_");
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        let filename = format!("{}/{}_{}_test_{}.log", logs_dir, safe_model_id, timestamp, test_index);

        // Log both request details and response
        let truncation_warning = if appears_truncated {
            "\n⚠️  WARNING: Response appears to be truncated!\n"
        } else {
            ""
        };
        
        let log_content = format!(
            "Model: {}\nTest Index: {}\nTimestamp: {}\nTest Data Size: {}\nResponse Length: {} chars{}\n{}\n\nResponse:\n{}\n{}",
            model_id,
            test_index,
            chrono::Utc::now().to_rfc3339(),
            self.test_data.len(),
            response_text.len(),
            truncation_warning,
            "=".repeat(80),
            response_text,
            "=".repeat(80)
        );

        fs::write(&filename, log_content)?;
        println!("  📝 Response logged to: {}", filename);
        Ok(())
    }


    fn evaluate_response_quality_detailed(&self, response_text: &str) -> (f64, u32) {
        // Clean up markdown formatting that models might add despite instructions
        let cleaned_response = self.clean_markdown_formatting(response_text);
        let lines: Vec<&str> = cleaned_response.trim().split('\n').collect();
        let mut valid_json_count = 0u32;
        let mut correct_cleanings = 0u32;
        let total_expected = self.test_data.len();

        for (line_idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            // Must be a complete JSON object
            if trimmed.starts_with('{') && trimmed.ends_with('}') {
                if let Ok(parsed) = serde_json::from_str::<Value>(trimmed) {
                    // Strict validation for required fields
                    if let Some(cleaned_title) = parsed.get("cleaned_title") {
                        if let Some(title_str) = cleaned_title.as_str() {
                            let title_trimmed = title_str.trim();
                            // Strict criteria:
                            // 1. Must not be empty
                            // 2. Must be at least 1 character after trimming
                            // 3. Must not be obviously broken/invalid
                            // 4. Must have confidence field with valid value
                            if !title_trimmed.is_empty() && 
                               title_trimmed.len() >= 1 &&
                               !title_trimmed.contains("�") && // No replacement characters
                               !title_trimmed.starts_with("ERROR") &&
                               !title_trimmed.starts_with("FAIL") {
                                
                                // Check that all required fields exist for the full schema
                                if let Some(confidence) = parsed.get("confidence") {
                                    if let Some(conf_val) = confidence.as_f64() {
                                        // Confidence must be between 0.0 and 1.0
                                        if conf_val >= 0.0 && conf_val <= 1.0 {
                                            // Check for other required fields in original schema
                                            let has_kept_markers = parsed.get("kept_markers").is_some();
                                            let has_removed_context = parsed.get("removed_context").is_some();
                                            
                                            if has_kept_markers && has_removed_context {
                                                valid_json_count += 1;
                                                
                                                // CRITICAL: Verify against expected cleaned title from test data
                                                if line_idx < self.test_data.len() {
                                                    let expected_cleaned = &self.test_data[line_idx].1;
                                                    if self.titles_match(title_trimmed, expected_cleaned) {
                                                        correct_cleanings += 1;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Much stricter scoring based on actual correctness
        let accuracy_ratio = correct_cleanings as f64 / total_expected as f64;
        let completion_ratio = valid_json_count as f64 / total_expected as f64;
        
        let score = if total_expected == 0 {
            0.0
        } else if correct_cleanings == 0 {
            // Zero credit if no correct cleanings
            0.0
        } else if valid_json_count as usize == total_expected && correct_cleanings >= (total_expected as u32 * 9 / 10) {
            // Full credit only if complete AND 90%+ correct
            accuracy_ratio
        } else {
            // Heavy penalties for incomplete or incorrect responses
            (accuracy_ratio * completion_ratio * 0.6).min(1.0)
        };

        (score, correct_cleanings) // Return correct cleanings, not just valid JSON
    }

    fn titles_match(&self, cleaned: &str, expected: &str) -> bool {
        let cleaned_norm = self.normalize_title(cleaned);
        let expected_norm = self.normalize_title(expected);
        
        // Exact match after normalization
        if cleaned_norm == expected_norm {
            return true;
        }
        
        // Allow minor variations (case, punctuation, spacing)
        let cleaned_simple = cleaned_norm.to_lowercase().chars()
            .filter(|c| c.is_alphanumeric() || c.is_whitespace())
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
            
        let expected_simple = expected_norm.to_lowercase().chars()
            .filter(|c| c.is_alphanumeric() || c.is_whitespace())
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        
        cleaned_simple == expected_simple
    }
    
    fn normalize_title(&self, title: &str) -> String {
        title.trim()
            .replace("\"", "")
            .replace("'", "'")
            .replace("…", "...")
            .replace("–", "-")
            .replace("—", "-")
    }

    fn clean_markdown_formatting(&self, text: &str) -> String {
        let mut result = text.to_string();
        
        // Remove markdown code blocks
        result = result.replace("```json", "");
        result = result.replace("```JSON", "");
        result = result.replace("```", "");
        
        // Remove single backticks around JSON objects
        result = result.replace("`{", "{");
        result = result.replace("}`", "}");
        
        // Remove common markdown artifacts
        result = result.replace("**Output:**", "");
        result = result.replace("**Response:**", "");
        result = result.replace("Here is the output:", "");
        result = result.replace("Here are the results:", "");
        result = result.replace("Output:", "");
        result = result.replace("Response:", "");
        
        // Clean up extra whitespace and empty lines
        result = result.lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        
        result
    }

    pub async fn run_tests(&self) -> Vec<ModelTestResult> {
        let mut results = Vec::new();

        println!("Starting tests on {} models...", self.test_models.len());
        println!("Test data: {} track names", self.test_data.len());
        println!("{}", "-".repeat(60));

        for (i, (model_id, model_info, _pricing)) in self.test_models.iter().enumerate() {
            let display_name = model_info.friendly_name.as_ref().unwrap_or(model_id);
            println!(
                "\n[{}/{}] Testing: {} ({})",
                i + 1,
                self.test_models.len(),
                model_id,
                display_name
            );

            let result = self.test_model(model_id, i).await;

            if let Some(error) = &result.error {
                println!("  ❌ Error: {}", error);
            } else {
                println!(
                    "  ✅ Success: {:.2}s, {} tokens, ${:.6}",
                    result.performance.response_time_seconds,
                    result.performance.tokens_used,
                    result.performance.cost_estimate_usd
                );
                println!(
                    "     Performance: {:.2}, Quality: {}",
                    result.performance.performance_score, result.performance.response_quality
                );
            }

            results.push(result);

            // Increased delay between requests to 10 seconds due to rate limit
            tokio::time::sleep(Duration::from_secs(10)).await;
        }

        results
    }

    pub fn save_results(
        &self,
        results: Vec<ModelTestResult>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Calculate summary statistics - be strict about what counts as "successful"
        let successful_results: Vec<_> = results.iter().filter(|r| 
            r.error.is_none() && 
            r.performance.performance_score >= 0.8 && 
            r.performance.correct_cleanings >= (self.test_data.len() as u32 * 8 / 10) // At least 80% correct cleanings
        ).collect();
        
        let all_completed_results: Vec<_> = results.iter().filter(|r| r.error.is_none()).collect();

        let summary = TestSummary {
            total_models_tested: results.len(),
            successful_tests: successful_results.len(),
            failed_tests: results.len() - successful_results.len(),
            average_response_time: if all_completed_results.is_empty() {
                0.0
            } else {
                all_completed_results
                    .iter()
                    .map(|r| r.performance.response_time_seconds)
                    .sum::<f64>()
                    / all_completed_results.len() as f64
            },
            average_cost: if all_completed_results.is_empty() {
                0.0
            } else {
                all_completed_results
                    .iter()
                    .map(|r| r.performance.cost_estimate_usd)
                    .sum::<f64>()
                    / all_completed_results.len() as f64
            },
            average_tokens_per_second: if all_completed_results.is_empty() {
                0.0
            } else {
                all_completed_results
                    .iter()
                    .map(|r| r.performance.tokens_per_second)
                    .sum::<f64>()
                    / all_completed_results.len() as f64
            },
            average_accuracy_rate: if all_completed_results.is_empty() {
                0.0
            } else {
                all_completed_results
                    .iter()
                    .map(|r| r.performance.accuracy_rate)
                    .sum::<f64>()
                    / all_completed_results.len() as f64
            },
            best_performance: successful_results
                .iter()
                .map(|r| r.performance.performance_score)
                .fold(0.0, f64::max),
            fastest_model: all_completed_results
                .iter()
                .min_by(|a, b| {
                    a.performance
                        .response_time_seconds
                        .partial_cmp(&b.performance.response_time_seconds)
                        .unwrap()
                })
                .map(|r| r.model_id.clone()),
            cheapest_model: all_completed_results
                .iter()
                .min_by(|a, b| {
                    a.performance
                        .cost_estimate_usd
                        .partial_cmp(&b.performance.cost_estimate_usd)
                        .unwrap()
                })
                .map(|r| r.model_id.clone()),
            highest_quality: successful_results
                .iter()
                .max_by(|a, b| {
                    a.performance
                        .performance_score
                        .partial_cmp(&b.performance.performance_score)
                        .unwrap()
                })
                .map(|r| r.model_id.clone()),
            best_accuracy_model: successful_results
                .iter()
                .max_by(|a, b| {
                    a.performance
                        .accuracy_rate
                        .partial_cmp(&b.performance.accuracy_rate)
                        .unwrap()
                })
                .map(|r| r.model_id.clone()),
            most_efficient_model: all_completed_results
                .iter()
                .max_by(|a, b| {
                    a.performance
                        .tokens_per_second
                        .partial_cmp(&b.performance.tokens_per_second)
                        .unwrap()
                })
                .map(|r| r.model_id.clone()),
        };

        let final_results = FinalResults {
            summary,
            test_configuration: TestConfiguration {
                models_tested: self
                    .test_models
                    .iter()
                    .map(|(id, _, _)| id.clone())
                    .collect(),
                test_data_source: "LyricCleanData.csv".to_string(),
                prompt_template: "TrackNameCleaner.prompt.yml".to_string(),
                pricing_data_source: "model_prices_and_context_window.json".to_string(),
                model_data_source: "available-models.json".to_string(),
            },
            individual_results: results.clone(),
        };

        // Save to file
        let output_file = "../model_performance_results.json";
        let json_output = serde_json::to_string_pretty(&final_results)?;
        fs::write(output_file, json_output)?;

        // Print summary
        println!("\n{}", "=".repeat(60));
        println!("Results saved to: model_performance_results.json");
        println!("{}", "=".repeat(60));
        println!("Summary:");
        println!(
            "  Total models tested: {}",
            final_results.summary.total_models_tested
        );
        println!(
            "  Successful tests: {}",
            final_results.summary.successful_tests
        );
        println!("  Failed tests: {}", final_results.summary.failed_tests);

        if !successful_results.is_empty() {
            println!(
                "  Average response time: {:.2}s",
                final_results.summary.average_response_time
            );
            println!("  Average cost: ${:.6}", final_results.summary.average_cost);
            println!(
                "  Best performance score: {:.2}",
                final_results.summary.best_performance
            );
            if let Some(fastest) = &final_results.summary.fastest_model {
                println!("  Fastest model: {}", fastest);
            }
            if let Some(cheapest) = &final_results.summary.cheapest_model {
                println!("  Cheapest model: {}", cheapest);
            }
            if let Some(highest_quality) = &final_results.summary.highest_quality {
                println!("  Highest quality: {}", highest_quality);
            }
        }

        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tester = ModelPerformanceTester::new()?;
    let results = tester.run_tests().await;
    tester.save_results(results)?;
    Ok(())
}
