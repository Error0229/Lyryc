# Model Performance Tester

A standalone Rust application to comprehensively test AI models from GitHub AI on cost, performance, and speed metrics.

## Features

- **All Chat Models**: Tests every chat-completion model from available-models.json
- **Real Pricing**: Matches models with actual pricing data  
- **30 Test Cases**: Uses 30 track names for robust evaluation
- **Single Request**: One API call per model to respect free tier limits
- **Comprehensive Results**: Performance scores, costs, and response times

## Setup

1. **Ensure your .env file is in the parent directory with:**
   ```env
   GITHUB_TOKEN=your_actual_github_token_here
   ```

2. **Run the test:**
   ```bash
   cd model_tester
   cargo run
   ```

## Output

Results are saved to `../model_performance_results.json` with:
- Summary statistics (fastest, cheapest, highest quality models)
- Individual model results with detailed metrics
- Test configuration metadata

## Why Standalone?

This is a separate Rust project (not in the main Tauri app) because:
- Clean separation of concerns
- No suspicious "bin" folders in main project
- Proper Rust project structure
- No compiler warnings about visibility