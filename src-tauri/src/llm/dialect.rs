//! Wire formats for the three dialects. Pure functions: build an `HttpCall`,
//! parse a response body. No I/O here, so everything is unit-testable.

use serde_json::{json, Value};

use super::presets::Dialect;
use super::{ChatRequest, Target};

pub const ANTHROPIC_VERSION: &str = "2023-06-01";

/// A fully described HTTP request. `body: None` means GET.
pub struct HttpCall {
    pub url: String,
    pub headers: Vec<(&'static str, String)>,
    pub body: Option<Value>,
}

/// OpenAI-compatible servers disagree on a few knobs (reasoning models reject
/// `temperature` and `max_tokens`, some local servers reject `response_format`).
/// We start with the widely accepted form and relax on a 400 that names the knob.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OpenAiOpts {
    pub drop_temperature: bool,
    pub completion_tokens: bool,
    pub drop_json: bool,
}

impl OpenAiOpts {
    pub fn relax(self, error_message: &str) -> Option<Self> {
        let msg = error_message.to_lowercase();
        let mut next = self;
        if msg.contains("max_tokens") && !self.completion_tokens {
            next.completion_tokens = true;
        } else if msg.contains("temperature") && !self.drop_temperature {
            next.drop_temperature = true;
        } else if msg.contains("response_format") && !self.drop_json {
            next.drop_json = true;
        } else {
            return None;
        }
        Some(next)
    }
}

fn auth_headers(t: &Target) -> Vec<(&'static str, String)> {
    let key = t.api_key.clone();
    let mut headers: Vec<(&'static str, String)> = match t.preset.dialect {
        Dialect::Openai => key
            .map(|k| ("Authorization", format!("Bearer {k}")))
            .into_iter()
            .collect(),
        Dialect::Anthropic => key.map(|k| ("x-api-key", k)).into_iter().collect(),
        Dialect::Gemini => key.map(|k| ("x-goog-api-key", k)).into_iter().collect(),
    };
    if t.preset.dialect == Dialect::Anthropic {
        headers.push(("anthropic-version", ANTHROPIC_VERSION.to_string()));
    }
    headers
}

fn gemini_model(model: &str) -> &str {
    model.strip_prefix("models/").unwrap_or(model)
}

pub fn chat_call(t: &Target, req: &ChatRequest, opts: OpenAiOpts) -> HttpCall {
    let base = &t.base_url;
    let (url, body) = match t.preset.dialect {
        Dialect::Openai => (
            format!("{base}/chat/completions"),
            openai_body(t, req, opts),
        ),
        Dialect::Anthropic => (format!("{base}/messages"), anthropic_body(t, req)),
        Dialect::Gemini => (
            format!("{base}/models/{}:generateContent", gemini_model(&t.model)),
            gemini_body(req),
        ),
    };
    HttpCall {
        url,
        headers: auth_headers(t),
        body: Some(body),
    }
}

fn openai_body(t: &Target, req: &ChatRequest, opts: OpenAiOpts) -> Value {
    let mut messages = Vec::new();
    if let Some(system) = req.system {
        messages.push(json!({"role": "system", "content": system}));
    }
    messages.push(json!({"role": "user", "content": req.user}));

    let mut body = json!({"model": t.model, "messages": messages});
    let tokens_field = if opts.completion_tokens {
        "max_completion_tokens"
    } else {
        "max_tokens"
    };
    body[tokens_field] = json!(req.max_tokens);
    if let (Some(temp), false) = (req.temperature, opts.drop_temperature) {
        body["temperature"] = json!(temp);
    }
    if req.json && !opts.drop_json {
        body["response_format"] = json!({"type": "json_object"});
    }
    body
}

fn anthropic_body(t: &Target, req: &ChatRequest) -> Value {
    let mut body = json!({
        "model": t.model,
        "max_tokens": req.max_tokens,
        "messages": [{"role": "user", "content": req.user}],
    });
    if let Some(system) = req.system {
        body["system"] = json!(system);
    }
    if let Some(temp) = req.temperature {
        body["temperature"] = json!(temp);
    }
    body
}

fn gemini_body(req: &ChatRequest) -> Value {
    let mut config = json!({"maxOutputTokens": req.max_tokens});
    if let Some(temp) = req.temperature {
        config["temperature"] = json!(temp);
    }
    if req.json {
        config["responseMimeType"] = json!("application/json");
    }
    let mut body = json!({
        "contents": [{"role": "user", "parts": [{"text": req.user}]}],
        "generationConfig": config,
    });
    if let Some(system) = req.system {
        body["systemInstruction"] = json!({"parts": [{"text": system}]});
    }
    body
}

pub fn models_call(t: &Target) -> HttpCall {
    let url = match (t.preset.models_url, t.preset.dialect) {
        (Some(url), _) => url.to_string(),
        (None, Dialect::Openai) => format!("{}/models", t.base_url),
        (None, Dialect::Anthropic) => format!("{}/models?limit=1000", t.base_url),
        (None, Dialect::Gemini) => format!("{}/models?pageSize=1000", t.base_url),
    };
    HttpCall {
        url,
        headers: auth_headers(t),
        body: None,
    }
}

/// Extract the reply text, or a human-readable reason why there is none.
pub fn parse_chat(dialect: Dialect, body: &Value) -> Result<String, String> {
    let text = match dialect {
        Dialect::Openai => parse_openai(body)?,
        Dialect::Anthropic => parse_anthropic(body)?,
        Dialect::Gemini => parse_gemini(body)?,
    };
    if text.trim().is_empty() {
        return Err("empty reply".to_string());
    }
    Ok(text)
}

fn parse_openai(body: &Value) -> Result<String, String> {
    let choice = &body["choices"][0];
    if choice.is_null() {
        return Err("response has no choices".to_string());
    }
    if choice["finish_reason"] == "length" {
        return Err("reply truncated by the token limit".to_string());
    }
    let content = &choice["message"]["content"];
    // Some compatible servers return content as an array of parts.
    Ok(match content {
        Value::String(s) => s.clone(),
        Value::Array(parts) => join_text(parts.iter()),
        _ => String::new(),
    })
}

fn parse_anthropic(body: &Value) -> Result<String, String> {
    if body["stop_reason"] == "max_tokens" {
        return Err("reply truncated by the token limit".to_string());
    }
    let blocks = body["content"]
        .as_array()
        .ok_or("response has no content")?;
    Ok(join_text(blocks.iter().filter(|b| b["type"] == "text")))
}

fn parse_gemini(body: &Value) -> Result<String, String> {
    let candidate = &body["candidates"][0];
    if candidate.is_null() {
        let reason = body["promptFeedback"]["blockReason"]
            .as_str()
            .unwrap_or("no candidates returned");
        return Err(format!("prompt blocked: {reason}"));
    }
    match candidate["finishReason"].as_str() {
        None | Some("STOP") => {}
        Some("MAX_TOKENS") => return Err("reply truncated by the token limit".to_string()),
        Some(other) => return Err(format!("reply incomplete: {other}")),
    }
    let parts = candidate["content"]["parts"]
        .as_array()
        .ok_or("response has no content parts")?;
    Ok(join_text(parts.iter().filter(|p| p["thought"] != true)))
}

fn join_text<'a>(parts: impl Iterator<Item = &'a Value>) -> String {
    parts.filter_map(|p| p["text"].as_str()).collect()
}

/// All three dialects put the message at `error.message`; fall back to raw text.
pub fn parse_error(raw: &str) -> String {
    let parsed: Option<Value> = serde_json::from_str(raw).ok();
    let message = parsed.as_ref().and_then(|v| {
        v["error"]["message"]
            .as_str()
            .or_else(|| v["error"].as_str())
            .or_else(|| v["message"].as_str())
            .or_else(|| v["detail"].as_str())
    });
    let message = message.unwrap_or(raw).trim();
    let mut out: String = message.chars().take(300).collect();
    if out.len() < message.len() {
        out.push('…');
    }
    out
}

/// Accepts `{data:[{id}]}` (OpenAI, Anthropic, OpenRouter), `[{id}]` (GitHub
/// catalog) and `{models:[{name:"models/x", supportedGenerationMethods}]}` (Gemini).
pub fn parse_models(body: &Value) -> Vec<String> {
    let items = body["data"]
        .as_array()
        .or_else(|| body["models"].as_array())
        .or_else(|| body.as_array());
    let mut out: Vec<String> = Vec::new();
    for item in items.into_iter().flatten() {
        if let Some(methods) = item["supportedGenerationMethods"].as_array() {
            if !methods.iter().any(|m| m == "generateContent") {
                continue;
            }
        }
        let id = item["id"].as_str().or_else(|| item["name"].as_str());
        if let Some(id) = id.map(gemini_model) {
            if !out.iter().any(|m| m == id) {
                out.push(id.to_string());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::presets::find;

    fn target(preset: &str, key: Option<&str>) -> Target {
        let p = find(preset).unwrap();
        Target {
            label: p.name.to_string(),
            preset: p,
            base_url: p.base_url.to_string(),
            api_key: key.map(str::to_string),
            model: "m-1".to_string(),
        }
    }

    fn req(json: bool) -> ChatRequest<'static> {
        ChatRequest {
            system: Some("sys"),
            user: "hello",
            temperature: Some(0.3),
            max_tokens: 100,
            json,
        }
    }

    fn header<'a>(call: &'a HttpCall, name: &str) -> Option<&'a str> {
        call.headers
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v.as_str())
    }

    #[test]
    fn openai_request_shape() {
        let call = chat_call(
            &target("openai", Some("sk-x")),
            &req(true),
            OpenAiOpts::default(),
        );
        assert_eq!(call.url, "https://api.openai.com/v1/chat/completions");
        assert_eq!(header(&call, "Authorization"), Some("Bearer sk-x"));
        let body = call.body.unwrap();
        assert_eq!(body["model"], "m-1");
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["content"], "hello");
        assert_eq!(body["max_tokens"], 100);
        assert!((body["temperature"].as_f64().unwrap() - 0.3).abs() < 1e-6);
        assert_eq!(body["response_format"]["type"], "json_object");
    }

    #[test]
    fn openai_without_key_or_system() {
        let mut r = req(false);
        r.system = None;
        let call = chat_call(&target("ollama", None), &r, OpenAiOpts::default());
        assert_eq!(call.url, "http://localhost:11434/v1/chat/completions");
        assert!(call.headers.is_empty());
        let body = call.body.unwrap();
        assert_eq!(body["messages"].as_array().unwrap().len(), 1);
        assert!(body.get("response_format").is_none());
    }

    #[test]
    fn openai_relaxes_rejected_knobs() {
        let o = OpenAiOpts::default();
        let o = o
            .relax("Unsupported parameter: 'max_tokens' is not supported with this model. Use 'max_completion_tokens' instead.")
            .unwrap();
        assert!(o.completion_tokens);
        let o = o
            .relax("Unsupported value: 'temperature' does not support 0.3 with this model.")
            .unwrap();
        assert!(o.drop_temperature);
        let o = o
            .relax("'response_format.type' must be 'json_schema'")
            .unwrap();
        assert!(o.drop_json);
        assert!(o.relax("temperature again").is_none());
        assert!(OpenAiOpts::default().relax("Incorrect API key").is_none());

        let body = chat_call(&target("openai", Some("k")), &req(true), o)
            .body
            .unwrap();
        assert_eq!(body["max_completion_tokens"], 100);
        assert!(body.get("max_tokens").is_none());
        assert!(body.get("temperature").is_none());
        assert!(body.get("response_format").is_none());
    }

    #[test]
    fn anthropic_request_shape() {
        let call = chat_call(
            &target("anthropic", Some("ak")),
            &req(true),
            OpenAiOpts::default(),
        );
        assert_eq!(call.url, "https://api.anthropic.com/v1/messages");
        assert_eq!(header(&call, "x-api-key"), Some("ak"));
        assert_eq!(header(&call, "anthropic-version"), Some(ANTHROPIC_VERSION));
        let body = call.body.unwrap();
        assert_eq!(body["system"], "sys");
        assert_eq!(body["max_tokens"], 100);
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn gemini_request_shape_keeps_key_out_of_url() {
        let mut t = target("gemini", Some("gk"));
        t.model = "models/gemini-x".to_string();
        let call = chat_call(&t, &req(true), OpenAiOpts::default());
        assert_eq!(
            call.url,
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-x:generateContent"
        );
        assert!(!call.url.contains("gk"));
        assert_eq!(header(&call, "x-goog-api-key"), Some("gk"));
        let body = call.body.unwrap();
        assert_eq!(body["systemInstruction"]["parts"][0]["text"], "sys");
        assert_eq!(body["contents"][0]["parts"][0]["text"], "hello");
        assert_eq!(body["generationConfig"]["maxOutputTokens"], 100);
        assert_eq!(
            body["generationConfig"]["responseMimeType"],
            "application/json"
        );
    }

    #[test]
    fn parses_openai_replies() {
        let ok = json!({"choices":[{"message":{"content":"hi"},"finish_reason":"stop"}]});
        assert_eq!(parse_chat(Dialect::Openai, &ok).unwrap(), "hi");
        let parts = json!({"choices":[{"message":{"content":[{"type":"text","text":"a"},{"type":"text","text":"b"}]}}]});
        assert_eq!(parse_chat(Dialect::Openai, &parts).unwrap(), "ab");
        let cut = json!({"choices":[{"message":{"content":"h"},"finish_reason":"length"}]});
        assert!(parse_chat(Dialect::Openai, &cut)
            .unwrap_err()
            .contains("truncated"));
        assert!(parse_chat(Dialect::Openai, &json!({"choices":[]})).is_err());
        let empty = json!({"choices":[{"message":{"content":null}}]});
        assert_eq!(
            parse_chat(Dialect::Openai, &empty).unwrap_err(),
            "empty reply"
        );
    }

    #[test]
    fn parses_anthropic_replies() {
        let ok = json!({"content":[{"type":"thinking","thinking":"x"},{"type":"text","text":"hi"}],"stop_reason":"end_turn"});
        assert_eq!(parse_chat(Dialect::Anthropic, &ok).unwrap(), "hi");
        let cut = json!({"content":[{"type":"text","text":"h"}],"stop_reason":"max_tokens"});
        assert!(parse_chat(Dialect::Anthropic, &cut).is_err());
    }

    #[test]
    fn parses_gemini_replies() {
        let ok = json!({"candidates":[{"content":{"parts":[{"text":"plan","thought":true},{"text":"hi"}]},"finishReason":"STOP"}]});
        assert_eq!(parse_chat(Dialect::Gemini, &ok).unwrap(), "hi");
        let cut = json!({"candidates":[{"content":{"parts":[{"text":"h"}]},"finishReason":"MAX_TOKENS"}]});
        assert!(parse_chat(Dialect::Gemini, &cut)
            .unwrap_err()
            .contains("truncated"));
        let safety = json!({"candidates":[{"finishReason":"SAFETY"}]});
        assert!(parse_chat(Dialect::Gemini, &safety)
            .unwrap_err()
            .contains("SAFETY"));
        let blocked = json!({"promptFeedback":{"blockReason":"PROHIBITED_CONTENT"}});
        assert!(parse_chat(Dialect::Gemini, &blocked)
            .unwrap_err()
            .contains("PROHIBITED_CONTENT"));
    }

    #[test]
    fn parses_error_bodies() {
        assert_eq!(
            parse_error(
                r#"{"error":{"message":"Incorrect API key provided","type":"invalid_request_error"}}"#
            ),
            "Incorrect API key provided"
        );
        assert_eq!(
            parse_error(
                r#"{"type":"error","error":{"type":"authentication_error","message":"invalid x-api-key"}}"#
            ),
            "invalid x-api-key"
        );
        assert_eq!(
            parse_error(
                r#"{"error":{"code":400,"message":"API key not valid.","status":"INVALID_ARGUMENT"}}"#
            ),
            "API key not valid."
        );
        assert_eq!(
            parse_error(r#"{"error":"model not found"}"#),
            "model not found"
        );
        assert_eq!(parse_error("Bad Gateway"), "Bad Gateway");
        assert!(parse_error(&"x".repeat(1000)).chars().count() <= 301);
    }

    #[test]
    fn models_endpoints() {
        assert_eq!(
            models_call(&target("groq", Some("k"))).url,
            "https://api.groq.com/openai/v1/models"
        );
        assert_eq!(
            models_call(&target("github", Some("k"))).url,
            "https://models.github.ai/catalog/models"
        );
        let a = models_call(&target("anthropic", Some("k")));
        assert!(a.url.starts_with("https://api.anthropic.com/v1/models"));
        assert!(a.body.is_none());
        let g = models_call(&target("gemini", Some("k")));
        assert!(g
            .url
            .starts_with("https://generativelanguage.googleapis.com/v1beta/models?"));
        assert!(!g.url.contains("key="));
    }

    #[test]
    fn parses_model_lists() {
        let openai = json!({"object":"list","data":[{"id":"a"},{"id":"b"},{"id":"a"}]});
        assert_eq!(parse_models(&openai), vec!["a", "b"]);
        let github = json!([{"id":"openai/gpt-4.1"},{"id":"meta/llama"}]);
        assert_eq!(parse_models(&github), vec!["openai/gpt-4.1", "meta/llama"]);
        let gemini = json!({"models":[
            {"name":"models/gemini-x","supportedGenerationMethods":["generateContent","countTokens"]},
            {"name":"models/embedding-1","supportedGenerationMethods":["embedContent"]}
        ]});
        assert_eq!(parse_models(&gemini), vec!["gemini-x"]);
        assert!(parse_models(&json!({"error":{}})).is_empty());
    }
}
