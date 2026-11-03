//! Backend-owned LLM settings: provider instances (with keys) and per-task
//! routes, persisted as JSON in the app config dir. Keys never leave this
//! module in full; the frontend sees `hasKey` and a masked `keyHint`.

use log::warn;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use super::presets::{self, Preset, GEMINI, GITHUB};
use super::Target;

pub const FILE_NAME: &str = "llm-settings.json";
const VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Task {
    Translation,
    Cleaning,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Route {
    pub provider_id: String,
    pub model: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Routes {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub translation: Option<Route>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cleaning: Option<Route>,
}

impl Routes {
    fn slot(&mut self, task: Task) -> &mut Option<Route> {
        match task {
            Task::Translation => &mut self.translation,
            Task::Cleaning => &mut self.cleaning,
        }
    }
}

/// Persisted provider instance. No `Debug`: it holds the key.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProviderRecord {
    id: String,
    preset_id: String,
    label: String,
    base_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    api_key: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    providers: Vec<ProviderRecord>,
    #[serde(default)]
    routes: Routes,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmProvider {
    pub id: String,
    pub preset_id: String,
    pub label: String,
    pub base_url: String,
    pub has_key: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmSettings {
    pub providers: Vec<LlmProvider>,
    pub routes: Routes,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInput {
    pub id: Option<String>,
    pub preset_id: String,
    pub label: Option<String>,
    pub base_url: Option<String>,
    /// `None` keeps the existing key, `""` clears it.
    pub api_key: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyKeys {
    pub github_api_key: Option<String>,
    pub gemini_api_key: Option<String>,
}

/// An un-stored key for a preset: legacy keys in a call, or env vars.
pub type AdhocKey = (&'static str, String);

impl LegacyKeys {
    /// Non-empty keys as (presetId, key), `first_preset` ordered first.
    pub fn candidates(&self, first_preset: &str) -> Vec<AdhocKey> {
        let mut out: Vec<AdhocKey> = [
            (GITHUB, &self.github_api_key),
            (GEMINI, &self.gemini_api_key),
        ]
        .into_iter()
        .filter_map(|(preset, key)| Some((preset, clean_key(key.as_deref()?)?)))
        .collect();
        out.sort_by_key(|(preset, _)| *preset != first_preset);
        out
    }
}

/// Development fallback: keys from `GITHUB_TOKEN` / `GEMINI_API_KEY`, in dev
/// builds only. A user's `GITHUB_TOKEN` is often a broad credential, and a
/// release must never spend it without being asked.
pub fn env_keys() -> Vec<AdhocKey> {
    if !cfg!(debug_assertions) {
        return Vec::new();
    }
    LegacyKeys {
        github_api_key: std::env::var("GITHUB_TOKEN").ok(),
        gemini_api_key: std::env::var("GEMINI_API_KEY").ok(),
    }
    .candidates(GEMINI)
}

fn clean_key(key: &str) -> Option<String> {
    let key = key.trim();
    (!key.is_empty()).then(|| key.to_string())
}

pub fn mask_key(key: &str) -> String {
    let chars: Vec<char> = key.chars().collect();
    if chars.len() <= 8 {
        return "••••".to_string();
    }
    let head: String = chars[..3].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}…{tail}")
}

fn adhoc_target(keys: &[AdhocKey]) -> Option<Target> {
    keys.iter().find_map(|(preset_id, key)| {
        let preset = presets::find(preset_id)?;
        Some(Target {
            label: preset.name.to_string(),
            preset,
            base_url: preset.base_url.to_string(),
            api_key: Some(key.clone()),
            model: preset.default_model()?.to_string(),
        })
    })
}

pub struct Store {
    path: Option<PathBuf>,
    settings: Settings,
}

pub type LlmState = Arc<Mutex<Store>>;

static GLOBAL: OnceLock<LlmState> = OnceLock::new();

/// Load the store from `dir` and register it globally, so code paths without
/// Tauri state (cleaning inside `fetch_lyrics`) see the same settings.
pub fn init(dir: Option<PathBuf>) -> LlmState {
    let store = match dir {
        Some(dir) => Store::load(dir.join(FILE_NAME)),
        None => {
            warn!("No app config dir; LLM settings will not persist");
            Store::in_memory()
        }
    };
    GLOBAL.get_or_init(|| Arc::new(Mutex::new(store))).clone()
}

/// Resolve the route for `task` against the global store.
/// `call_keys` are legacy keys that arrived with the current call.
pub fn resolve(task: Task, call_keys: &[AdhocKey]) -> Option<Target> {
    match GLOBAL.get() {
        Some(state) => lock(state).resolve(task, call_keys, &env_keys()),
        None => Store::in_memory().resolve(task, call_keys, &env_keys()),
    }
}

/// Poisoning only means another thread panicked mid-update; the data is
/// still a valid `Store`, so keep going rather than take the app down.
pub fn lock(state: &LlmState) -> std::sync::MutexGuard<'_, Store> {
    state.lock().unwrap_or_else(|e| e.into_inner())
}

impl Store {
    pub fn in_memory() -> Self {
        Store {
            path: None,
            settings: Settings::default(),
        }
    }

    pub fn load(path: PathBuf) -> Self {
        let settings = match fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|e| {
                let backup = path.with_extension("json.corrupt");
                warn!(
                    "LLM settings unreadable ({e}); moved aside to {}",
                    backup.display()
                );
                let _ = fs::rename(&path, &backup);
                Settings::default()
            }),
            Err(_) => Settings::default(),
        };
        Store {
            path: Some(path),
            settings,
        }
    }

    fn save(&mut self) -> Result<(), String> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        self.settings.version = VERSION;
        let json = serde_json::to_vec_pretty(&self.settings).map_err(|e| e.to_string())?;
        write_private_atomic(path, &json)
            .map_err(|e| format!("Could not save LLM settings to {}: {e}", path.display()))
    }

    pub fn view(&self) -> LlmSettings {
        LlmSettings {
            providers: self.settings.providers.iter().map(view_provider).collect(),
            routes: self.settings.routes.clone(),
        }
    }

    fn find(&self, id: &str) -> Option<&ProviderRecord> {
        self.settings.providers.iter().find(|p| p.id == id)
    }

    pub fn upsert(&mut self, input: ProviderInput) -> Result<LlmProvider, String> {
        let preset = presets::find(&input.preset_id)
            .ok_or_else(|| format!("Unknown provider preset '{}'", input.preset_id))?;
        let existing = match &input.id {
            Some(id) => Some(
                self.find(id)
                    .ok_or_else(|| format!("Unknown provider '{id}'"))?
                    .clone(),
            ),
            None => None,
        };
        let preset_changed = existing.as_ref().is_some_and(|p| p.preset_id != preset.id);
        let mut rec = existing.unwrap_or_else(|| new_record(preset));
        rec.preset_id = preset.id.to_string();

        if let Some(label) = input.label {
            rec.label = non_empty(&label).unwrap_or(preset.name).to_string();
        }
        match input.base_url {
            Some(url) => rec.base_url = normalize_url(&url, preset),
            None if preset_changed => rec.base_url = preset.base_url.to_string(),
            None => {}
        }
        if let Some(key) = input.api_key {
            rec.api_key = clean_key(&key);
        }
        if !(rec.base_url.starts_with("http://") || rec.base_url.starts_with("https://")) {
            return Err(format!(
                "{} needs a base URL starting with http:// or https://",
                rec.label
            ));
        }

        let view = view_provider(&rec);
        match self.settings.providers.iter_mut().find(|p| p.id == rec.id) {
            Some(slot) => *slot = rec,
            None => self.settings.providers.push(rec),
        }
        self.save()?;
        Ok(view)
    }

    pub fn remove(&mut self, id: &str) -> Result<(), String> {
        let before = self.settings.providers.len();
        self.settings.providers.retain(|p| p.id != id);
        let routes = &mut self.settings.routes;
        for task in [Task::Translation, Task::Cleaning] {
            let slot = routes.slot(task);
            if slot.as_ref().is_some_and(|r| r.provider_id == id) {
                *slot = None;
            }
        }
        if self.settings.providers.len() == before {
            return Ok(());
        }
        self.save()
    }

    pub fn set_route(&mut self, task: Task, route: Option<Route>) -> Result<(), String> {
        let route = match route {
            Some(r) => {
                if self.find(&r.provider_id).is_none() {
                    return Err(format!("Unknown provider '{}'", r.provider_id));
                }
                let model = non_empty(&r.model).ok_or("Choose a model for this route")?;
                Some(Route {
                    provider_id: r.provider_id,
                    model: model.to_string(),
                })
            }
            None => None,
        };
        *self.settings.routes.slot(task) = route;
        self.save()
    }

    /// Target for an explicit provider + model (testing, listing models).
    pub fn target_for(&self, provider_id: &str, model: &str) -> Result<Target, String> {
        let rec = self
            .find(provider_id)
            .ok_or_else(|| format!("Unknown provider '{provider_id}'"))?;
        let preset = presets::find(&rec.preset_id)
            .ok_or_else(|| format!("{}: unknown preset '{}'", rec.label, rec.preset_id))?;
        Ok(Target {
            label: rec.label.clone(),
            preset,
            base_url: rec.base_url.clone(),
            api_key: rec.api_key.clone(),
            model: model.trim().to_string(),
        })
    }

    fn routed(&self, task: Task) -> Option<Target> {
        let routes = &self.settings.routes;
        let to_target = |r: &Route| self.target_for(&r.provider_id, &r.model).ok();
        let own = match task {
            Task::Translation => &routes.translation,
            Task::Cleaning => &routes.cleaning,
        };
        own.as_ref().and_then(to_target).or_else(|| match task {
            Task::Cleaning => routes.translation.as_ref().and_then(to_target),
            Task::Translation => None,
        })
    }

    /// Route for `task`, else a legacy key from this call, else (only when the
    /// store is empty) the development env keys.
    pub fn resolve(&self, task: Task, call_keys: &[AdhocKey], env: &[AdhocKey]) -> Option<Target> {
        self.routed(task)
            .or_else(|| adhoc_target(call_keys))
            .or_else(|| {
                if self.settings.providers.is_empty() {
                    adhoc_target(env)
                } else {
                    None
                }
            })
    }

    /// Create GitHub Models / Gemini instances from old frontend keys and
    /// route unset tasks to the first one. Idempotent.
    pub fn import_legacy(&mut self, keys: &LegacyKeys) -> Result<LlmSettings, String> {
        let mut changed = false;
        let mut first: Option<Route> = None;
        for (preset_id, key) in keys.candidates(GEMINI) {
            let Some(preset) = presets::find(preset_id) else {
                continue;
            };
            let found =
                self.settings.providers.iter().find(|p| {
                    p.preset_id == preset_id && p.api_key.as_deref() == Some(key.as_str())
                });
            let id = match found {
                Some(p) => p.id.clone(),
                None => {
                    let mut rec = new_record(preset);
                    rec.api_key = Some(key);
                    let id = rec.id.clone();
                    self.settings.providers.push(rec);
                    changed = true;
                    id
                }
            };
            if first.is_none() && !preset.retired() {
                first = preset.default_model().map(|model| Route {
                    provider_id: id,
                    model: model.to_string(),
                });
            }
        }
        if let Some(route) = first {
            for task in [Task::Translation, Task::Cleaning] {
                let slot = self.settings.routes.slot(task);
                if slot.is_none() {
                    *slot = Some(route.clone());
                    changed = true;
                }
            }
        }
        if changed {
            self.save()?;
        }
        Ok(self.view())
    }
}

fn new_record(preset: &Preset) -> ProviderRecord {
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    ProviderRecord {
        id: format!("{}-{}", preset.id, &suffix[..8]),
        preset_id: preset.id.to_string(),
        label: preset.name.to_string(),
        base_url: preset.base_url.to_string(),
        api_key: None,
    }
}

fn view_provider(p: &ProviderRecord) -> LlmProvider {
    LlmProvider {
        id: p.id.clone(),
        preset_id: p.preset_id.clone(),
        label: p.label.clone(),
        base_url: p.base_url.clone(),
        has_key: p.api_key.is_some(),
        key_hint: p.api_key.as_deref().map(mask_key),
    }
}

fn non_empty(s: &str) -> Option<&str> {
    let s = s.trim();
    (!s.is_empty()).then_some(s)
}

fn normalize_url(url: &str, preset: &Preset) -> String {
    non_empty(url)
        .unwrap_or(preset.base_url)
        .trim_end_matches('/')
        .to_string()
}

/// Write to a sibling temp file with owner-only permissions, then rename over
/// the target so a crash never leaves a half-written settings file.
fn write_private_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut file = opts.open(&tmp)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(data)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path() -> PathBuf {
        std::env::temp_dir()
            .join(format!("lyryc-llm-test-{}", uuid::Uuid::new_v4().simple()))
            .join(FILE_NAME)
    }

    fn input(preset: &str, key: Option<&str>) -> ProviderInput {
        ProviderInput {
            preset_id: preset.to_string(),
            api_key: key.map(str::to_string),
            ..Default::default()
        }
    }

    fn route(id: &str, model: &str) -> Option<Route> {
        Some(Route {
            provider_id: id.to_string(),
            model: model.to_string(),
        })
    }

    #[test]
    fn masks_keys() {
        assert_eq!(mask_key("sk-abcdefghija1b2"), "sk-…a1b2");
        assert_eq!(mask_key("short"), "••••");
        assert_eq!(mask_key("12345678"), "••••");
        assert_eq!(mask_key("ключключключ"), "клю…ключ");
    }

    #[test]
    fn round_trips_through_disk_without_leaking_keys_in_view() {
        let path = temp_path();
        let mut store = Store::load(path.clone());
        let p = store
            .upsert(input("openai", Some(" sk-test-1234567890 ")))
            .unwrap();
        assert!(p.has_key);
        assert_eq!(p.key_hint.as_deref(), Some("sk-…7890"));
        store
            .set_route(Task::Translation, route(&p.id, "gpt-x"))
            .unwrap();

        let reloaded = Store::load(path.clone());
        assert_eq!(reloaded.view(), store.view());
        let t = reloaded.target_for(&p.id, "gpt-x").unwrap();
        assert_eq!(t.api_key.as_deref(), Some("sk-test-1234567890"));

        let view_json = serde_json::to_string(&reloaded.view()).unwrap();
        assert!(!view_json.contains("sk-test-1234567890"));
        assert!(view_json.contains("\"hasKey\":true"));
        assert!(view_json.contains("\"providerId\""));

        let on_disk = fs::read_to_string(&path).unwrap();
        assert!(on_disk.contains("\"apiKey\""));
        assert!(!path.with_extension("json.tmp").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn corrupt_file_is_moved_aside() {
        let path = temp_path();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{not json").unwrap();
        let store = Store::load(path.clone());
        assert!(store.view().providers.is_empty());
        assert!(path.with_extension("json.corrupt").exists());
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn upsert_key_semantics() {
        let mut s = Store::in_memory();
        let p = s.upsert(input("groq", Some("gsk_0123456789"))).unwrap();
        assert_eq!(p.base_url, "https://api.groq.com/openai/v1");
        assert_eq!(p.label, "Groq");

        let mut keep = input("groq", None);
        keep.id = Some(p.id.clone());
        keep.label = Some("Work Groq".into());
        let p2 = s.upsert(keep).unwrap();
        assert!(p2.has_key);
        assert_eq!(p2.label, "Work Groq");
        assert_eq!(p2.id, p.id);

        let mut clear = input("groq", Some(""));
        clear.id = Some(p.id.clone());
        assert!(!s.upsert(clear).unwrap().has_key);
        assert_eq!(s.view().providers.len(), 1);
    }

    #[test]
    fn upsert_validates() {
        let mut s = Store::in_memory();
        assert!(s.upsert(input("nope", None)).is_err());
        assert!(s
            .upsert(input("custom", None))
            .unwrap_err()
            .contains("base URL"));
        let mut custom = input("custom", None);
        custom.base_url = Some("http://localhost:8080/v1/".into());
        assert_eq!(
            s.upsert(custom).unwrap().base_url,
            "http://localhost:8080/v1"
        );
        let mut unknown = input("openai", None);
        unknown.id = Some("ghost".into());
        assert!(s.upsert(unknown).is_err());
        assert_eq!(s.view().providers.len(), 1);
    }

    #[test]
    fn removing_provider_clears_its_routes() {
        let mut s = Store::in_memory();
        let a = s.upsert(input("openai", Some("k1-aaaaaaaa"))).unwrap();
        let b = s.upsert(input("anthropic", Some("k2-bbbbbbbb"))).unwrap();
        s.set_route(Task::Translation, route(&a.id, "m")).unwrap();
        s.set_route(Task::Cleaning, route(&b.id, "n")).unwrap();
        s.remove(&a.id).unwrap();
        let v = s.view();
        assert_eq!(v.providers.len(), 1);
        assert!(v.routes.translation.is_none());
        assert_eq!(v.routes.cleaning, route(&b.id, "n"));
        assert!(s.set_route(Task::Translation, route(&a.id, "m")).is_err());
        assert!(s.set_route(Task::Translation, route(&b.id, "  ")).is_err());
    }

    #[test]
    fn route_fallback_order() {
        let call = vec![(GEMINI, "call-key".to_string())];
        let env = vec![(GITHUB, "env-key".to_string())];
        let mut s = Store::in_memory();

        // Empty store: call keys, then env keys, else nothing.
        assert_eq!(
            s.resolve(Task::Translation, &call, &env)
                .unwrap()
                .api_key
                .as_deref(),
            Some("call-key")
        );
        assert_eq!(
            s.resolve(Task::Cleaning, &[], &env).unwrap().preset.id,
            GITHUB
        );
        assert!(s.resolve(Task::Translation, &[], &[]).is_none());

        // A configured store disables the env fallback, but call keys still work.
        let a = s.upsert(input("openai", Some("stored-key-1"))).unwrap();
        assert!(s.resolve(Task::Translation, &[], &env).is_none());
        assert_eq!(
            s.resolve(Task::Translation, &call, &env).unwrap().preset.id,
            GEMINI
        );

        // Cleaning falls back to the translation route.
        s.set_route(Task::Translation, route(&a.id, "gpt-t"))
            .unwrap();
        let t = s.resolve(Task::Cleaning, &call, &env).unwrap();
        assert_eq!((t.preset.id, t.model.as_str()), ("openai", "gpt-t"));

        let b = s.upsert(input("mistral", Some("stored-key-2"))).unwrap();
        s.set_route(Task::Cleaning, route(&b.id, "mistral-c"))
            .unwrap();
        assert_eq!(
            s.resolve(Task::Cleaning, &[], &[]).unwrap().model,
            "mistral-c"
        );
        assert_eq!(
            s.resolve(Task::Translation, &[], &[]).unwrap().model,
            "gpt-t"
        );
    }

    #[test]
    fn legacy_candidates_respect_preferred_provider() {
        let keys = LegacyKeys {
            github_api_key: Some("gh".into()),
            gemini_api_key: Some(" gm ".into()),
        };
        let order: Vec<_> = keys
            .candidates(GEMINI)
            .into_iter()
            .map(|(p, _)| p)
            .collect();
        assert_eq!(order, vec![GEMINI, GITHUB]);
        assert_eq!(keys.candidates(GITHUB)[1].1, "gm");
        let empty = LegacyKeys {
            github_api_key: Some("  ".into()),
            gemini_api_key: None,
        };
        assert!(empty.candidates(GITHUB).is_empty());
    }

    #[test]
    fn import_legacy_is_idempotent() {
        let path = temp_path();
        let mut s = Store::load(path.clone());
        let keys = LegacyKeys {
            github_api_key: Some("ghp_legacy_token".into()),
            gemini_api_key: Some("AIza_legacy_key".into()),
        };
        let first = s.import_legacy(&keys).unwrap();
        assert_eq!(first.providers.len(), 2);
        let gm = first
            .providers
            .iter()
            .find(|p| p.preset_id == GEMINI)
            .unwrap();
        let expected = route(
            &gm.id,
            presets::find(GEMINI).unwrap().default_model().unwrap(),
        );
        assert_eq!(first.routes.translation, expected);
        assert_eq!(first.routes.cleaning, expected);

        let second = s.import_legacy(&keys).unwrap();
        assert_eq!(second, first);
        assert_eq!(Store::load(path.clone()).view(), first);

        // A different key creates a new instance but leaves routes alone.
        let third = s
            .import_legacy(&LegacyKeys {
                github_api_key: Some("ghp_other_token".into()),
                gemini_api_key: None,
            })
            .unwrap();
        assert_eq!(third.providers.len(), 3);
        assert_eq!(third.routes, first.routes);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn import_never_routes_to_retired_preset() {
        let mut s = Store::in_memory();
        let v = s
            .import_legacy(&LegacyKeys {
                github_api_key: Some("ghp_legacy_token".into()),
                gemini_api_key: None,
            })
            .unwrap();
        assert_eq!(v.providers.len(), 1);
        assert_eq!(v.routes.translation, None);
        assert_eq!(v.routes.cleaning, None);
    }

    #[test]
    fn import_routes_only_unset_tasks() {
        let mut s = Store::in_memory();
        let a = s
            .upsert(input("anthropic", Some("sk-ant-xxxxxxxx")))
            .unwrap();
        s.set_route(Task::Translation, route(&a.id, "claude-haiku-5-5"))
            .unwrap();
        let v = s
            .import_legacy(&LegacyKeys {
                github_api_key: None,
                gemini_api_key: Some("AIza_legacy_key".into()),
            })
            .unwrap();
        assert_eq!(v.routes.translation, route(&a.id, "claude-haiku-5-5"));
        let cleaning = v.routes.cleaning.unwrap();
        assert_eq!(
            cleaning.model,
            presets::find(GEMINI).unwrap().default_model().unwrap()
        );
        assert!(
            s.import_legacy(&LegacyKeys::default())
                .unwrap()
                .providers
                .len()
                == 2
        );
    }
}
