//! HTTP transport: one shared client, one retry on 429/5xx honouring
//! `Retry-After`, and readable errors that name the provider and model.

use log::{debug, warn};
use reqwest::Client;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex, OnceLock};
use std::time::Duration;
use tokio::sync::Semaphore;

use super::dialect::{self, HttpCall, OpenAiOpts};
use super::presets::Dialect;
use super::{ChatRequest, Completion, LlmError, Target, Tools};

/// Requests in flight per provider. Translation, song identification and the
/// lookahead can otherwise all fire at once, and providers (free tiers above
/// all) answer bursts with 429/503. The rest wait their turn.
const MAX_IN_FLIGHT: usize = 2;

/// The turn-taking for one provider (its name and base URL).
fn slots(target: &Target) -> Arc<Semaphore> {
    static SLOTS: LazyLock<Mutex<HashMap<String, Arc<Semaphore>>>> = LazyLock::new(Default::default);
    let key = format!("{}|{}", target.label, target.base_url);
    SLOTS.lock().unwrap().entry(key).or_insert_with(|| Arc::new(Semaphore::new(MAX_IN_FLIGHT))).clone()
}

/// Longest server-requested wait we sit through before giving up.
const MAX_RETRY_WAIT: Duration = Duration::from_secs(20);

fn client() -> &'static Client {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        Client::builder()
            // reqwest drops `Authorization` on a cross-host redirect but not
            // `x-api-key` / `x-goog-api-key`. LLM APIs don't redirect: a 3xx is an error.
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(90))
            .build()
            .expect("failed to build HTTP client")
    })
}

struct Reply {
    status: u16,
    retry_after: Option<Duration>,
    body: String,
}

async fn send(call: &HttpCall) -> Result<Reply, String> {
    let mut req = match &call.body {
        Some(body) => client().post(&call.url).json(body),
        None => client().get(&call.url),
    };
    for (name, value) in &call.headers {
        req = req.header(*name, value);
    }
    let resp = req.send().await.map_err(describe_network_error)?;
    let status = resp.status().as_u16();
    let retry_after = resp
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(parse_retry_after);
    let body = resp.text().await.map_err(describe_network_error)?;
    Ok(Reply {
        status,
        retry_after,
        body,
    })
}

fn describe_network_error(e: reqwest::Error) -> String {
    // reqwest errors embed the URL; none of ours carry a key, but strip it anyway.
    let e = e.without_url();
    if e.is_timeout() {
        "request timed out".to_string()
    } else if e.is_connect() {
        format!("could not connect ({e})")
    } else {
        e.to_string()
    }
}

fn parse_retry_after(value: &str) -> Option<Duration> {
    value
        .trim()
        .parse::<f64>()
        .ok()
        .map(Duration::from_secs_f64)
}

fn is_retryable(status: u16) -> bool {
    status == 429 || (500..600).contains(&status)
}

/// How long to wait before the single retry, or `None` if the server asked
/// for longer than we are willing to block a UI request.
fn backoff(status: u16, retry_after: Option<Duration>) -> Option<Duration> {
    let default = if status == 429 { 2 } else { 1 };
    let wait = retry_after.unwrap_or(Duration::from_secs(default));
    (wait <= MAX_RETRY_WAIT).then_some(wait)
}

fn status_text(status: u16) -> String {
    reqwest::StatusCode::from_u16(status)
        .map(|s| s.to_string())
        .unwrap_or_else(|_| status.to_string())
}

struct Ctx<'a>(&'a Target);

impl Ctx<'_> {
    fn err(&self, status: Option<u16>, message: impl Into<String>) -> LlmError {
        let mut message = message.into();
        if let Some(key) = self.0.api_key.as_deref().filter(|k| k.len() >= 4) {
            message = message.replace(key, "***");
        }
        LlmError {
            provider: self.0.label.clone(),
            model: self.0.model.clone(),
            status,
            message,
        }
    }

    fn http_err(&self, reply: &Reply) -> LlmError {
        let detail = dialect::parse_error(&reply.body);
        self.err(
            Some(reply.status),
            format!("HTTP {}: {}", status_text(reply.status), detail),
        )
    }

    fn check_key(&self) -> Result<(), LlmError> {
        if self.0.preset.key_required && self.0.api_key.is_none() {
            return Err(self.err(None, "no API key set for this provider"));
        }
        Ok(())
    }
}

/// Send `call` (rebuilt by `build` on each attempt), retrying once on 429/5xx.
async fn send_with_retry(ctx: &Ctx<'_>, build: impl Fn() -> HttpCall) -> Result<Reply, LlmError> {
    let mut retried = false;
    loop {
        let reply = send(&build()).await.map_err(|e| ctx.err(None, e))?;
        if !is_retryable(reply.status) || retried {
            return Ok(reply);
        }
        let Some(wait) = backoff(reply.status, reply.retry_after) else {
            return Err(ctx.err(
                Some(reply.status),
                format!(
                    "rate limited, retry after {}s",
                    reply.retry_after.unwrap_or_default().as_secs()
                ),
            ));
        };
        warn!(
            "{} ({}) returned HTTP {}, retrying in {:?}",
            ctx.0.label, ctx.0.model, reply.status, wait
        );
        tokio::time::sleep(wait).await;
        retried = true;
    }
}

/// Run one chat completion against `target`.
pub async fn complete(target: &Target, req: &ChatRequest<'_>) -> Result<String, LlmError> {
    complete_with(target, req, Tools::default())
        .await
        .map(|c| c.text)
}

/// [`complete`] with the provider's web search and/or a JSON schema.
pub async fn complete_with(
    target: &Target,
    req: &ChatRequest<'_>,
    tools: Tools<'_>,
) -> Result<Completion, LlmError> {
    let ctx = Ctx(target);
    ctx.check_key()?;
    if tools.web_search && target.preset.web_search.is_none() {
        return Err(ctx.err(
            None,
            format!(
                "{} has no built-in web search (supported: OpenAI, xAI, OpenRouter, Anthropic, Gemini)",
                target.preset.name
            ),
        ));
    }
    let slots = slots(target);
    let _turn = slots.acquire().await.expect("never closed");
    let mut opts = OpenAiOpts::default();
    loop {
        debug!("LLM request to {} ({})", target.label, target.model);
        let reply = send_with_retry(&ctx, || dialect::call(target, req, tools, opts)).await?;
        if (200..300).contains(&reply.status) {
            let body: Value = serde_json::from_str(&reply.body)
                .map_err(|e| ctx.err(Some(reply.status), format!("invalid JSON reply: {e}")))?;
            return dialect::parse_completion(target, tools, &body)
                .map_err(|m| ctx.err(Some(reply.status), m));
        }
        let error = dialect::parse_error(&reply.body);
        let relaxed = match (target.preset.dialect, reply.status) {
            (Dialect::Openai, 400 | 422) => opts.relax(&error),
            _ => None,
        }
        .or_else(|| match reply.status {
            400 | 422 if tools.web_search || tools.schema.is_some() => opts.relax_schema(&error),
            _ => None,
        });
        match relaxed {
            Some(next) => {
                debug!(
                    "{} rejected a parameter, retrying with {:?}",
                    target.label, next
                );
                opts = next;
            }
            None => return Err(ctx.http_err(&reply)),
        }
    }
}

/// Live model list from the provider's models endpoint. Falls back to the
/// preset's suggestions when the endpoint is missing or broken, but not when
/// the key is rejected: the user needs to see that.
pub async fn list_models(target: &Target) -> Result<Vec<String>, LlmError> {
    let ctx = Ctx(target);
    ctx.check_key()?;
    let fetched = async {
        let reply = send_with_retry(&ctx, || dialect::models_call(target)).await?;
        if !(200..300).contains(&reply.status) {
            return Err(ctx.http_err(&reply));
        }
        let body: Value = serde_json::from_str(&reply.body)
            .map_err(|e| ctx.err(Some(reply.status), format!("invalid JSON reply: {e}")))?;
        Ok(dialect::parse_models(&body))
    }
    .await;

    let suggested: Vec<String> = target
        .preset
        .suggested_models
        .iter()
        .map(|m| m.to_string())
        .collect();
    match fetched {
        Ok(models) if !models.is_empty() => Ok(models),
        Err(e) if e.is_auth() || suggested.is_empty() => Err(e),
        Err(e) => {
            warn!("Listing models failed, using suggestions: {}", e);
            Ok(suggested)
        }
        Ok(_) => Ok(suggested),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(label: &str, base_url: &str) -> Target {
        let preset = crate::llm::presets::find("custom").unwrap();
        Target { label: label.into(), preset, base_url: base_url.into(), api_key: None, model: "m".into() }
    }

    #[test]
    fn a_provider_takes_two_requests_at_a_time() {
        let a = slots(&target("Burst", "https://burst.example/v1"));
        let _one = a.clone().try_acquire_owned().unwrap();
        let _two = a.clone().try_acquire_owned().unwrap();
        assert!(a.clone().try_acquire_owned().is_err(), "the third waits");
        let same = slots(&target("Burst", "https://burst.example/v1"));
        assert!(same.try_acquire().is_err(), "one queue per provider");
        let other = slots(&target("Burst", "https://other.example/v1"));
        assert!(other.try_acquire().is_ok(), "another provider is not held up");
    }

    #[test]
    fn retry_policy() {
        assert!(is_retryable(429));
        assert!(is_retryable(503));
        assert!(!is_retryable(400));
        assert!(!is_retryable(401));
        assert_eq!(backoff(429, None), Some(Duration::from_secs(2)));
        assert_eq!(backoff(500, None), Some(Duration::from_secs(1)));
        assert_eq!(
            backoff(429, Some(Duration::from_secs(7))),
            Some(Duration::from_secs(7))
        );
        assert_eq!(backoff(429, Some(Duration::from_secs(3600))), None);
    }

    #[test]
    fn retry_after_parsing() {
        assert_eq!(parse_retry_after("5"), Some(Duration::from_secs(5)));
        assert_eq!(
            parse_retry_after(" 1.5 "),
            Some(Duration::from_millis(1500))
        );
        assert_eq!(parse_retry_after("Wed, 21 Oct 2015 07:28:00 GMT"), None);
    }

    #[test]
    fn errors_name_provider_and_model_and_redact_key() {
        let p = super::super::presets::find("openai").unwrap();
        let t = Target {
            label: "My OpenAI".into(),
            preset: p,
            base_url: p.base_url.into(),
            api_key: Some("sk-secret-123".into()),
            model: "gpt-x".into(),
        };
        let ctx = Ctx(&t);
        let reply = Reply {
            status: 401,
            retry_after: None,
            body: r#"{"error":{"message":"Incorrect API key provided: sk-secret-123"}}"#.into(),
        };
        let e = ctx.http_err(&reply);
        assert!(e.is_auth());
        let s = e.to_string();
        assert_eq!(
            s,
            "My OpenAI (gpt-x): HTTP 401 Unauthorized: Incorrect API key provided: ***"
        );
        assert!(!s.contains("sk-secret"));
    }

    #[tokio::test]
    async fn missing_key_fails_before_network() {
        let p = super::super::presets::find("anthropic").unwrap();
        let t = Target {
            label: "Anthropic".into(),
            preset: p,
            base_url: p.base_url.into(),
            api_key: None,
            model: "claude-haiku-5-5".into(),
        };
        let req = ChatRequest {
            system: None,
            user: "hi",
            temperature: None,
            max_tokens: 8,
            json: false,
        };
        let e = complete(&t, &req).await.unwrap_err();
        assert!(e.to_string().contains("no API key"));
    }

    #[tokio::test]
    async fn search_on_a_provider_without_one_fails_before_network() {
        let p = super::super::presets::find("groq").unwrap();
        let t = Target {
            label: "Groq".into(),
            preset: p,
            base_url: p.base_url.into(),
            api_key: Some("k".into()),
            model: "llama".into(),
        };
        let req = ChatRequest {
            system: None,
            user: "hi",
            temperature: None,
            max_tokens: 8,
            json: false,
        };
        let tools = Tools {
            web_search: true,
            schema: None,
        };
        let e = complete_with(&t, &req, tools).await.unwrap_err();
        assert!(e.to_string().contains("no built-in web search"), "{e}");
        assert!(e.status.is_none());
    }

    #[tokio::test]
    #[ignore = "live network; needs GEMINI_API_KEY or GITHUB_TOKEN"]
    async fn live_smoke() {
        use crate::llm::store::{env_keys, Store};
        dotenv::dotenv().ok();
        let target = Store::in_memory()
            .resolve(crate::llm::Task::Translation, &[], &env_keys())
            .expect("no env key set");
        let req = ChatRequest {
            system: Some("You are terse."),
            user: "Reply with the single word: pong",
            temperature: Some(0.0),
            max_tokens: 512,
            json: false,
        };
        let reply = complete(&target, &req).await.unwrap();
        assert!(!reply.trim().is_empty());
    }
}
