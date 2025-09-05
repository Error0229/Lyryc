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
    pub cost_estimate_usd: f64,
    pub performance_score: f64,
    pub response_quality: String,
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
    pub best_performance: f64,
    pub fastest_model: Option<String>,
    pub cheapest_model: Option<String>,
    pub highest_quality: Option<String>,
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
            // Try to find pricing data using various model ID formats
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

            for key in potential_keys {
                if !key.is_empty() {
                    if let Some(pricing) = self.pricing_data.get(&key) {
                        pricing_info = Some(pricing.clone());
                        break;
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

        // Randomly sample up to 65 test cases
        let mut rng = thread_rng();
        all_test_data.shuffle(&mut rng);
        self.test_data = all_test_data.into_iter().take(65).collect();

        println!("Loaded {} test cases (randomly sampled)", self.test_data.len());

        // Load prompt template
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

**Output**: For each line, return one JSON object on its own line, with the schema specified in the System Prompt (`cleaned_title`, `kept_markers`, `removed_context`, `confidence`, `notes`). Do not include explanations beyond `notes`. Do not translate titles."#,
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
                    cost_estimate_usd: 0.0,
                    performance_score: 0.0,
                    response_quality: "error".to_string(),
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
            "max_tokens": std::cmp::min(1000, max_tokens / 4),
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

                        // Log response to file
                        self.log_response_to_file(model_id, response_text, test_index).await.ok();

                        // Evaluate response quality
                        let performance_score = self.evaluate_response_quality(response_text);
                        let quality = if performance_score > 0.7 {
                            "good"
                        } else if performance_score > 0.4 {
                            "fair"
                        } else {
                            "poor"
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
                                cost_estimate_usd: cost,
                                performance_score,
                                response_quality: quality.to_string(),
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
                cost_estimate_usd: 0.0,
                performance_score: 0.0,
                response_quality: "error".to_string(),
            },
            error: Some(error),
            test_metadata: TestMetadata {
                test_date: chrono::Utc::now().to_rfc3339(),
                test_data_size: self.test_data.len(),
                test_type: "track_name_cleaning".to_string(),
            },
        }
    }

    async fn log_response_to_file(&self, model_id: &str, response_text: &str, test_index: usize) -> Result<(), Box<dyn std::error::Error>> {
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
        let log_content = format!(
            "Model: {}\nTest Index: {}\nTimestamp: {}\nTest Data Size: {}\n{}
\nResponse:\n{}\n{}",
            model_id,
            test_index,
            chrono::Utc::now().to_rfc3339(),
            self.test_data.len(),
            "=".repeat(80),
            response_text,
            "=".repeat(80)
        );

        fs::write(&filename, log_content)?;
        println!("  📝 Response logged to: {}", filename);
        Ok(())
    }

    fn evaluate_response_quality(&self, response_text: &str) -> f64 {
        let lines: Vec<&str> = response_text.trim().split('\n').collect();
        let mut valid_json_count = 0;
        let total_expected = self.test_data.len();

        for line in lines {
            let trimmed = line.trim();
            if trimmed.starts_with('{') && trimmed.ends_with('}') {
                if let Ok(parsed) = serde_json::from_str::<Value>(trimmed) {
                    if parsed.get("cleaned_title").is_some() && parsed.get("confidence").is_some() {
                        valid_json_count += 1;
                    }
                }
            }
        }

        if total_expected == 0 {
            return 0.0;
        }

        (valid_json_count as f64 / total_expected as f64).min(1.0)
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
        // Calculate summary statistics
        let successful_results: Vec<_> = results.iter().filter(|r| r.error.is_none()).collect();

        let summary = TestSummary {
            total_models_tested: results.len(),
            successful_tests: successful_results.len(),
            failed_tests: results.len() - successful_results.len(),
            average_response_time: if successful_results.is_empty() {
                0.0
            } else {
                successful_results
                    .iter()
                    .map(|r| r.performance.response_time_seconds)
                    .sum::<f64>()
                    / successful_results.len() as f64
            },
            average_cost: if successful_results.is_empty() {
                0.0
            } else {
                successful_results
                    .iter()
                    .map(|r| r.performance.cost_estimate_usd)
                    .sum::<f64>()
                    / successful_results.len() as f64
            },
            best_performance: successful_results
                .iter()
                .map(|r| r.performance.performance_score)
                .fold(0.0, f64::max),
            fastest_model: successful_results
                .iter()
                .min_by(|a, b| {
                    a.performance
                        .response_time_seconds
                        .partial_cmp(&b.performance.response_time_seconds)
                        .unwrap()
                })
                .map(|r| r.model_id.clone()),
            cheapest_model: successful_results
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
