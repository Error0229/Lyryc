//! Wire formats for the three dialects. Pure functions: build an `HttpCall`,
//! parse a response body. No I/O here, so everything is unit-testable.

use serde_json::{json, Value};
use std::borrow::Cow;

use super::presets::{Dialect, WebSearch};
use super::{ChatRequest, Completion, Target, Tools};

pub const ANTHROPIC_VERSION: &str = "2023-06-01";
/// Cap on searches per Anthropic request; each one is billed.
const ANTHROPIC_MAX_SEARCHES: u32 = 3;

/// A fully described HTTP request. `body: None` means GET.
pub struct HttpCall {
    pub url: String,
    pub headers: Vec<(&'static str, String)>,
    pub body: Option<Value>,
}

/// OpenAI-compatible servers disagree on a few knobs (reasoning models reject
/// `temperature` and `max_tokens`, some local servers reject `response_format`).
/// We start with the widely accepted form and relax on a 400 that names the knob.
/// `drop_json` also applies to every dialect when a schema meets web search.
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
        } else if (msg.contains("response_format") || msg.contains("text.format"))
            && !self.drop_json
        {
            next.drop_json = true;
        } else {
            return None;
        }
        Some(next)
    }

    /// Gemini 2.x rejects a JSON mime type next to a tool, and Anthropic's
    /// citations reject `output_config.format`. Either way keep the search
    /// and spell the schema out in the system prompt instead.
    pub fn relax_schema(self, error_message: &str) -> Option<Self> {
        let msg = error_message.to_lowercase();
        let names_it = ["json", "mime", "schema", "output_config", "citation"]
            .iter()
            .any(|k| msg.contains(k));
        (names_it && !self.drop_json).then_some(Self {
            drop_json: true,
            ..self
        })
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

/// The search tool this call uses. The transport has already refused
/// `web_search` on presets without one.
fn search(t: &Target, tools: Tools) -> Option<WebSearch> {
    t.preset.web_search.filter(|_| tools.web_search)
}

fn uses_responses_api(t: &Target, tools: Tools) -> bool {
    search(t, tools) == Some(WebSearch::Responses)
}

/// Can the schema go in the request itself? Not after a relax, and not on
/// Anthropic with search: web search always cites, and citations reject
/// `output_config.format`.
fn native_schema(t: &Target, tools: Tools, opts: OpenAiOpts) -> bool {
    tools.schema.is_some()
        && !opts.drop_json
        && !(tools.web_search && t.preset.dialect == Dialect::Anthropic)
}

/// The system prompt, plus the schema spelled out when the request can't carry it.
fn system_text<'a>(
    t: &Target,
    req: &ChatRequest<'a>,
    tools: Tools,
    opts: OpenAiOpts,
) -> Option<Cow<'a, str>> {
    let Some(schema) = tools.schema.filter(|_| !native_schema(t, tools, opts)) else {
        return req.system.map(Cow::Borrowed);
    };
    let rule = format!(
        "Reply with only a JSON object, no prose and no code fence, matching this JSON Schema:\n{}",
        schema.schema
    );
    Some(Cow::Owned(match req.system {
        Some(system) => format!("{system}\n\n{rule}"),
        None => rule,
    }))
}

pub fn call(t: &Target, req: &ChatRequest, tools: Tools, opts: OpenAiOpts) -> HttpCall {
    let base = &t.base_url;
    let (url, body) = match t.preset.dialect {
        Dialect::Openai if uses_responses_api(t, tools) => (
            format!("{base}/responses"),
            responses_body(t, req, tools, opts),
        ),
        Dialect::Openai => (
            format!("{base}/chat/completions"),
            openai_body(t, req, tools, opts),
        ),
        Dialect::Anthropic => (
            format!("{base}/messages"),
            anthropic_body(t, req, tools, opts),
        ),
        Dialect::Gemini => (
            format!("{base}/models/{}:generateContent", gemini_model(&t.model)),
            gemini_body(t, req, tools, opts),
        ),
    };
    HttpCall {
        url,
        headers: auth_headers(t),
        body: Some(body),
    }
}

fn openai_body(t: &Target, req: &ChatRequest, tools: Tools, opts: OpenAiOpts) -> Value {
    let mut messages = Vec::new();
    if let Some(system) = system_text(t, req, tools, opts) {
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
    if let Some(s) = tools.schema.filter(|_| native_schema(t, tools, opts)) {
        body["response_format"] = json!({
            "type": "json_schema",
            "json_schema": {"name": s.name, "schema": s.schema, "strict": true},
        });
    } else if req.json && !opts.drop_json && tools.schema.is_none() {
        body["response_format"] = json!({"type": "json_object"});
    }
    if search(t, tools) == Some(WebSearch::OpenRouter) {
        body["tools"] = json!([{"type": "openrouter:web_search"}]);
    }
    body
}

/// OpenAI / xAI Responses API, used only for web search: Chat Completions
/// has no search tool on either.
fn responses_body(t: &Target, req: &ChatRequest, tools: Tools, opts: OpenAiOpts) -> Value {
    let mut body = json!({
        "model": t.model,
        "input": req.user,
        "max_output_tokens": req.max_tokens,
        "tools": [{"type": "web_search"}],
    });
    if let Some(system) = system_text(t, req, tools, opts) {
        body["instructions"] = json!(system);
    }
    if let (Some(temp), false) = (req.temperature, opts.drop_temperature) {
        body["temperature"] = json!(temp);
    }
    if let Some(s) = tools.schema.filter(|_| native_schema(t, tools, opts)) {
        body["text"] = json!({"format": {
            "type": "json_schema", "name": s.name, "schema": s.schema, "strict": true,
        }});
    } else if req.json && !opts.drop_json && tools.schema.is_none() {
        body["text"] = json!({"format": {"type": "json_object"}});
    }
    body
}

fn anthropic_body(t: &Target, req: &ChatRequest, tools: Tools, opts: OpenAiOpts) -> Value {
    let mut body = json!({
        "model": t.model,
        "max_tokens": req.max_tokens,
        "messages": [{"role": "user", "content": req.user}],
    });
    if let Some(system) = system_text(t, req, tools, opts) {
        body["system"] = json!(system);
    }
    if let Some(temp) = req.temperature {
        body["temperature"] = json!(temp);
    }
    if search(t, tools).is_some() {
        body["tools"] = json!([{
            "type": "web_search_20250305",
            "name": "web_search",
            "max_uses": ANTHROPIC_MAX_SEARCHES,
        }]);
    }
    if let Some(s) = tools.schema.filter(|_| native_schema(t, tools, opts)) {
        body["output_config"] = json!({"format": {"type": "json_schema", "schema": s.schema}});
    }
    body
}

fn gemini_body(t: &Target, req: &ChatRequest, tools: Tools, opts: OpenAiOpts) -> Value {
    let mut config = json!({"maxOutputTokens": req.max_tokens});
    if let Some(temp) = req.temperature {
        config["temperature"] = json!(temp);
    }
    if let Some(s) = tools.schema.filter(|_| native_schema(t, tools, opts)) {
        config["responseMimeType"] = json!("application/json");
        config["responseJsonSchema"] = s.schema.clone();
    } else if req.json && !opts.drop_json {
        config["responseMimeType"] = json!("application/json");
    }
    let mut body = json!({
        "contents": [{"role": "user", "parts": [{"text": req.user}]}],
        "generationConfig": config,
    });
    if let Some(system) = system_text(t, req, tools, opts) {
        body["systemInstruction"] = json!({"parts": [{"text": system}]});
    }
    if search(t, tools).is_some() {
        body["tools"] = json!([{"google_search": {}}]);
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

/// Reply text plus grounding, or a human-readable reason why there is none.
pub fn parse_completion(t: &Target, tools: Tools, body: &Value) -> Result<Completion, String> {
    if t.preset.dialect == Dialect::Openai && uses_responses_api(t, tools) {
        let text = parse_responses(body)?;
        if text.trim().is_empty() {
            return Err("empty reply".to_string());
        }
        let output = || each(&body["output"]);
        let urls = output()
            .flat_map(|o| each(&o["content"]))
            .flat_map(|c| each(&c["annotations"]))
            .filter(|a| a["type"] == "url_citation")
            .map(|a| &a["url"]);
        let searches = output().filter(|o| o["type"] == "web_search_call").count();
        return Ok(completion(text, urls, searches as u64));
    }
    let text = parse_chat(t.preset.dialect, body)?;
    Ok(match t.preset.dialect {
        Dialect::Openai => {
            let message = &body["choices"][0]["message"];
            let urls = each(&message["annotations"]).map(|a| &a["url_citation"]["url"]);
            let searches = &body["usage"]["server_tool_use_details"]["web_search_requests"];
            completion(text, urls, searches.as_u64().unwrap_or(0))
        }
        Dialect::Anthropic => {
            let urls = each(&body["content"])
                .flat_map(|b| each(&b["citations"]))
                .map(|c| &c["url"]);
            let searches = &body["usage"]["server_tool_use"]["web_search_requests"];
            completion(text, urls, searches.as_u64().unwrap_or(0))
        }
        Dialect::Gemini => {
            let meta = &body["candidates"][0]["groundingMetadata"];
            let urls = each(&meta["groundingChunks"]).map(|c| &c["web"]["uri"]);
            let searches = each(&meta["webSearchQueries"]).count();
            completion(text, urls, searches as u64)
        }
    })
}

fn each(v: &Value) -> impl Iterator<Item = &Value> {
    v.as_array().into_iter().flatten()
}

fn completion<'a>(
    text: String,
    urls: impl Iterator<Item = &'a Value>,
    searches: u64,
) -> Completion {
    let mut sources: Vec<String> = Vec::new();
    for url in urls.filter_map(Value::as_str) {
        if !sources.iter().any(|s| s == url) {
            sources.push(url.to_string());
        }
    }
    Completion {
        text,
        sources,
        searches: u32::try_from(searches).unwrap_or(u32::MAX),
    }
}

/// Responses API: text from `message` items; a refusal part is an error.
fn parse_responses(body: &Value) -> Result<String, String> {
    if body["status"] == "incomplete" {
        return Err(match body["incomplete_details"]["reason"].as_str() {
            Some("max_output_tokens") => "reply truncated by the token limit".to_string(),
            other => format!("reply incomplete: {}", other.unwrap_or("unknown reason")),
        });
    }
    let output = body["output"].as_array().ok_or("response has no output")?;
    let mut text = String::new();
    for part in output
        .iter()
        .filter(|o| o["type"] == "message")
        .flat_map(|o| each(&o["content"]))
    {
        match part["type"].as_str() {
            Some("output_text") => text.push_str(part["text"].as_str().unwrap_or_default()),
            Some("refusal") => {
                let why = part["refusal"].as_str().unwrap_or_default();
                return Err(format!("model refused: {why}"));
            }
            _ => {}
        }
    }
    Ok(text)
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
    match body["stop_reason"].as_str() {
        Some("max_tokens") => return Err("reply truncated by the token limit".to_string()),
        Some("pause_turn") => return Err("web search paused before the reply".to_string()),
        _ => {}
    }
    let blocks = body["content"]
        .as_array()
        .ok_or("response has no content")?;
    // With web search the model narrates between searches; the answer is
    // whatever follows the last search result.
    let answer = blocks
        .iter()
        .rposition(|b| b["type"] == "web_search_tool_result")
        .map_or(0, |i| i + 1);
    Ok(join_text(
        blocks[answer..].iter().filter(|b| b["type"] == "text"),
    ))
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

    fn chat_call(t: &Target, req: &ChatRequest, opts: OpenAiOpts) -> HttpCall {
        call(t, req, Tools::default(), opts)
    }

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

    // --- web search + JSON schema --------------------------------------
    // Request shapes follow each provider's docs (2026-11); see
    // docs/research/llm-search/REPORT.md for the links.

    fn schema_value() -> Value {
        json!({"type":"object","additionalProperties":false,
               "properties":{"a":{"type":"string"}},"required":["a"]})
    }

    fn tools(web_search: bool, schema: &Value) -> Tools<'_> {
        Tools {
            web_search,
            schema: Some(super::super::JsonSchema {
                name: "thing",
                schema,
            }),
        }
    }

    fn system_of(body: &Value) -> String {
        body["system"]
            .as_str()
            .or_else(|| body["instructions"].as_str())
            .or_else(|| body["systemInstruction"]["parts"][0]["text"].as_str())
            .or_else(|| body["messages"][0]["content"].as_str())
            .unwrap_or_default()
            .to_string()
    }

    #[test]
    fn openai_search_uses_responses_api_with_native_schema() {
        let s = schema_value();
        let call = call(
            &target("openai", Some("sk")),
            &req(true),
            tools(true, &s),
            OpenAiOpts::default(),
        );
        assert_eq!(call.url, "https://api.openai.com/v1/responses");
        assert_eq!(header(&call, "Authorization"), Some("Bearer sk"));
        let body = call.body.unwrap();
        assert_eq!(body["tools"], json!([{"type": "web_search"}]));
        assert_eq!(body["input"], "hello");
        assert_eq!(body["instructions"], "sys");
        assert_eq!(body["max_output_tokens"], 100);
        assert!(body.get("messages").is_none());
        let format = &body["text"]["format"];
        assert_eq!(format["type"], "json_schema");
        assert_eq!(format["name"], "thing");
        assert_eq!(format["strict"], true);
        assert_eq!(format["schema"], s);
    }

    #[test]
    fn xai_search_uses_responses_api_too() {
        let s = schema_value();
        let call = call(
            &target("xai", Some("k")),
            &req(false),
            tools(true, &s),
            OpenAiOpts::default(),
        );
        assert_eq!(call.url, "https://api.x.ai/v1/responses");
        assert!(call.body.unwrap().get("search_parameters").is_none());
    }

    #[test]
    fn openai_schema_without_search_stays_on_chat_completions() {
        let s = schema_value();
        let body = call(
            &target("openai", Some("k")),
            &req(true),
            tools(false, &s),
            OpenAiOpts::default(),
        )
        .body
        .unwrap();
        assert!(body.get("tools").is_none());
        assert_eq!(body["response_format"]["type"], "json_schema");
        assert_eq!(body["response_format"]["json_schema"]["schema"], s);
        assert_eq!(body["messages"][0]["content"], "sys");
    }

    #[test]
    fn openrouter_search_is_a_server_tool_on_chat_completions() {
        let s = schema_value();
        let call = call(
            &target("openrouter", Some("k")),
            &req(true),
            tools(true, &s),
            OpenAiOpts::default(),
        );
        assert_eq!(call.url, "https://openrouter.ai/api/v1/chat/completions");
        let body = call.body.unwrap();
        assert_eq!(body["tools"], json!([{"type": "openrouter:web_search"}]));
        assert!(body.get("plugins").is_none());
        assert_eq!(body["response_format"]["json_schema"]["name"], "thing");
    }

    #[test]
    fn anthropic_search_puts_the_schema_in_the_prompt() {
        let s = schema_value();
        let body = call(
            &target("anthropic", Some("k")),
            &req(true),
            tools(true, &s),
            OpenAiOpts::default(),
        )
        .body
        .unwrap();
        assert_eq!(body["tools"][0]["type"], "web_search_20250305");
        assert_eq!(body["tools"][0]["name"], "web_search");
        assert_eq!(body["tools"][0]["max_uses"], ANTHROPIC_MAX_SEARCHES);
        assert!(body.get("output_config").is_none());
        let system = system_of(&body);
        assert!(system.starts_with("sys\n\n"));
        assert!(system.contains(r#""required":["a"]"#), "{system}");
    }

    #[test]
    fn anthropic_schema_without_search_is_native() {
        let s = schema_value();
        let body = call(
            &target("anthropic", Some("k")),
            &req(true),
            tools(false, &s),
            OpenAiOpts::default(),
        )
        .body
        .unwrap();
        assert!(body.get("tools").is_none());
        assert_eq!(body["output_config"]["format"]["type"], "json_schema");
        assert_eq!(body["output_config"]["format"]["schema"], s);
        assert_eq!(body["system"], "sys");
    }

    #[test]
    fn gemini_search_with_native_schema_then_relaxed() {
        let s = schema_value();
        let t = target("gemini", Some("gk"));
        let body = call(&t, &req(true), tools(true, &s), OpenAiOpts::default())
            .body
            .unwrap();
        assert_eq!(body["tools"], json!([{"google_search": {}}]));
        let config = &body["generationConfig"];
        assert_eq!(config["responseMimeType"], "application/json");
        assert_eq!(config["responseJsonSchema"], s);
        assert_eq!(system_of(&body), "sys");

        // Gemini 2.x: "Tool use with a response mime type: 'application/json' is unsupported"
        let relaxed = OpenAiOpts::default()
            .relax_schema("Tool use with a response mime type: 'application/json' is unsupported")
            .unwrap();
        let body = call(&t, &req(true), tools(true, &s), relaxed).body.unwrap();
        assert_eq!(body["tools"], json!([{"google_search": {}}]));
        assert!(body["generationConfig"].get("responseMimeType").is_none());
        assert!(body["generationConfig"].get("responseJsonSchema").is_none());
        assert!(system_of(&body).contains(r#""additionalProperties":false"#));
    }

    #[test]
    fn relax_schema_only_on_errors_that_name_it() {
        let o = OpenAiOpts::default();
        assert!(o.relax_schema("API key not valid.").is_none());
        let o = o
            .relax_schema("citations are not supported with output_config.format")
            .unwrap();
        assert!(o.drop_json);
        assert!(o.relax_schema("json again").is_none());
        let o = OpenAiOpts::default()
            .relax("Invalid parameter: 'text.format' is not supported")
            .unwrap();
        assert!(o.drop_json);
    }

    #[test]
    fn plain_calls_are_unchanged_by_the_new_fields() {
        for preset in ["openai", "anthropic", "gemini", "openrouter", "xai"] {
            let t = target(preset, Some("k"));
            let a = chat_call(&t, &req(true), OpenAiOpts::default());
            let b = call(&t, &req(true), Tools::default(), OpenAiOpts::default());
            assert_eq!(a.url, b.url);
            assert_eq!(a.body, b.body);
            let body = b.body.unwrap();
            assert!(body.get("tools").is_none(), "{preset}");
            assert!(!a.url.ends_with("/responses"));
        }
    }

    // --- parsing grounded replies --------------------------------------

    #[test]
    fn parses_responses_api_output() {
        let s = schema_value();
        let t = target("openai", Some("k"));
        let body = json!({"id":"resp_1","object":"response","status":"completed","output":[
            {"type":"web_search_call","id":"ws_1","status":"completed",
             "action":{"type":"search","query":"snow country yorushika"}},
            {"type":"web_search_call","id":"ws_2","status":"completed",
             "action":{"type":"open_page","url":"https://a.example"}},
            {"type":"message","id":"msg_1","status":"completed","role":"assistant","content":[
                {"type":"output_text","text":"{\"a\":\"雪国\"}","annotations":[
                    {"type":"url_citation","start_index":0,"end_index":5,"url":"https://a.example","title":"A"},
                    {"type":"url_citation","start_index":5,"end_index":9,"url":"https://a.example","title":"A"},
                    {"type":"url_citation","start_index":5,"end_index":9,"url":"https://b.example","title":"B"}]}]}]});
        let c = parse_completion(&t, tools(true, &s), &body).unwrap();
        assert_eq!(c.text, r#"{"a":"雪国"}"#);
        assert_eq!(c.sources, ["https://a.example", "https://b.example"]);
        assert_eq!(c.searches, 2);

        let refusal = json!({"status":"completed","output":[{"type":"message","content":[
            {"type":"refusal","refusal":"I can't help with that."}]}]});
        assert!(parse_completion(&t, tools(true, &s), &refusal)
            .unwrap_err()
            .contains("refused"));
        let cut = json!({"status":"incomplete","incomplete_details":{"reason":"max_output_tokens"},"output":[]});
        assert!(parse_completion(&t, tools(true, &s), &cut)
            .unwrap_err()
            .contains("truncated"));
    }

    #[test]
    fn parses_openrouter_annotations() {
        let s = schema_value();
        let t = target("openrouter", Some("k"));
        let body = json!({"choices":[{"finish_reason":"stop","message":{"role":"assistant",
            "content":"{\"a\":\"x\"}","annotations":[{"type":"url_citation","url_citation":{
                "url":"https://r.example","title":"R","content":"…","start_index":0,"end_index":3}}]}}],
            "usage":{"server_tool_use_details":{"web_search_requests":1}}});
        let c = parse_completion(&t, tools(true, &s), &body).unwrap();
        assert_eq!(c.sources, ["https://r.example"]);
        assert_eq!(c.searches, 1);
    }

    #[test]
    fn parses_anthropic_search_turns() {
        let s = schema_value();
        let t = target("anthropic", Some("k"));
        let body = json!({"role":"assistant","stop_reason":"end_turn","content":[
            {"type":"text","text":"I'll search for this song."},
            {"type":"server_tool_use","id":"srvtoolu_1","name":"web_search","input":{"query":"雪国 ヨルシカ"}},
            {"type":"web_search_tool_result","tool_use_id":"srvtoolu_1","content":[
                {"type":"web_search_result","url":"https://a.example","title":"A",
                 "encrypted_content":"xx","page_age":"2024"}]},
            {"type":"text","text":"{\"a\":","citations":[{"type":"web_search_result_location",
                "url":"https://a.example","title":"A","encrypted_index":"yy","cited_text":"雪国"}]},
            {"type":"text","text":"\"雪国\"}"}],
            "usage":{"input_tokens":10,"output_tokens":5,"server_tool_use":{"web_search_requests":1}}});
        let c = parse_completion(&t, tools(true, &s), &body).unwrap();
        assert_eq!(c.text, r#"{"a":"雪国"}"#);
        assert_eq!(c.sources, ["https://a.example"]);
        assert_eq!(c.searches, 1);

        let paused = json!({"stop_reason":"pause_turn","content":[]});
        assert!(parse_completion(&t, tools(true, &s), &paused)
            .unwrap_err()
            .contains("paused"));
    }

    #[test]
    fn parses_gemini_grounding_metadata() {
        let s = schema_value();
        let t = target("gemini", Some("k"));
        let body = json!({"candidates":[{"finishReason":"STOP",
            "content":{"role":"model","parts":[{"text":"{\"a\":\"雪国\"}"}]},
            "groundingMetadata":{
                "webSearchQueries":["雪国 ヨルシカ","Snow Country Yorushika"],
                "groundingChunks":[
                    {"web":{"uri":"https://vertexaisearch.cloud.google.com/grounding-api-redirect/1","title":"a.example"}},
                    {"web":{"uri":"https://vertexaisearch.cloud.google.com/grounding-api-redirect/2","title":"b.example"}}],
                "groundingSupports":[]}}]});
        let c = parse_completion(&t, tools(true, &s), &body).unwrap();
        assert_eq!(c.text, r#"{"a":"雪国"}"#);
        assert_eq!(c.sources.len(), 2);
        assert_eq!(c.searches, 2);

        let plain =
            json!({"candidates":[{"content":{"parts":[{"text":"hi"}]},"finishReason":"STOP"}]});
        let c = parse_completion(&t, Tools::default(), &plain).unwrap();
        assert_eq!(
            c,
            Completion {
                text: "hi".into(),
                ..Completion::default()
            }
        );
    }
}
