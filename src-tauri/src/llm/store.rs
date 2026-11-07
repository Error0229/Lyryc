//! Backend-owned LLM settings: provider instances and per-task routes,
//! persisted as JSON in the app config dir; their keys rest in the OS keyring
//! (`vault.rs`). Keys never leave this module in full; the frontend sees
//! `hasKey` and a masked `keyHint`.

use log::warn;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use super::presets::{self, Preset, GEMINI, GITHUB};
use super::vault::{NoKeyring, OsKeyring, Vault};
use super::Target;

pub const FILE_NAME: &str = "llm-settings.json";
const VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Task {
    Translation,
    Cleaning,
    /// Translates a song the translation route declined (a refusal) or failed
    /// on. Only ever the route the user set: lyrics go to no provider they
    /// didn't choose for them.
    Backup,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup: Option<Route>,
}

impl Routes {
    fn slot(&mut self, task: Task) -> &mut Option<Route> {
        match task {
            Task::Translation => &mut self.translation,
            Task::Cleaning => &mut self.cleaning,
            Task::Backup => &mut self.backup,
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
    /// In memory, the key wherever it rests. On disk only when no keyring
    /// would take it, and in files written before keys moved to the keyring.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    api_key: Option<String>,
    /// The OS keyring holds this provider's key, under its id.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    in_keyring: bool,
}

impl ProviderRecord {
    /// A key the keyring doesn't hold yet: it would go to disk as plain text.
    fn key_outside_keyring(&self) -> bool {
        self.api_key.is_some() && !self.in_keyring
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
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
    /// Some key sits in the settings file as plain text: this machine has no
    /// working keyring.
    pub keys_in_file: bool,
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
    vault: Box<dyn Vault>,
}

pub type LlmState = Arc<Mutex<Store>>;

static GLOBAL: OnceLock<LlmState> = OnceLock::new();

/// Load the store from `dir` and register it globally, so code paths without
/// Tauri state (cleaning inside `fetch_lyrics`) see the same settings.
pub fn init(dir: Option<PathBuf>) -> LlmState {
    let store = match dir {
        Some(dir) => {
            let mut store = Store::load(dir.join(FILE_NAME));
            store.secure_keys();
            store
        }
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

/// `Store::translation_backup` on the global store.
pub fn translation_backup() -> Option<Target> {
    GLOBAL.get().and_then(|state| lock(state).translation_backup())
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
            vault: Box::new(NoKeyring),
        }
    }

    /// Read the file and fetch its keys from the OS keyring. Writes nothing:
    /// a plaintext key stays put until `secure_keys` or the next save.
    pub fn load(path: PathBuf) -> Self {
        Self::load_with(path, Box::new(OsKeyring))
    }

    pub fn load_with(path: PathBuf, vault: Box<dyn Vault>) -> Self {
        let mut settings: Settings = match fs::read_to_string(&path) {
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
        for p in settings.providers.iter_mut().filter(|p| p.in_keyring) {
            match vault.get(&p.id) {
                Ok(Some(key)) => p.api_key = Some(key),
                Ok(None) => warn!("{}: no key in the system keyring; enter it again", p.label),
                // Keep the marker: the keyring may answer next launch, and a
                // save must not mistake "unreadable" for "no key".
                Err(e) => warn!("{}: system keyring unreadable ({e}); its key is unavailable", p.label),
            }
        }
        Store {
            path: Some(path),
            settings,
            vault,
        }
    }

    /// Move plaintext keys from the file into the keyring. One-way; a no-op
    /// once done, and when there is no keyring to move them to.
    pub fn secure_keys(&mut self) {
        if !self.settings.providers.iter().any(ProviderRecord::key_outside_keyring) {
            return;
        }
        if let Err(e) = self.save() {
            warn!("Could not move LLM keys into the system keyring: {e}");
        }
    }

    /// Hand new keys to the keyring, then write the file without them.
    fn save(&mut self) -> Result<(), String> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        for p in self.settings.providers.iter_mut().filter(|p| p.key_outside_keyring()) {
            match self.vault.set(&p.id, p.api_key.as_deref().unwrap_or_default()) {
                Ok(()) => p.in_keyring = true,
                Err(e) => warn!("System keyring refused {}'s key ({e}); keeping it in {FILE_NAME}", p.label),
            }
        }
        let mut disk = self.settings.clone();
        disk.version = VERSION;
        for p in disk.providers.iter_mut().filter(|p| p.in_keyring) {
            p.api_key = None;
        }
        let json = serde_json::to_vec_pretty(&disk).map_err(|e| e.to_string())?;
        write_private_atomic(path, &json)
            .map_err(|e| format!("Could not save LLM settings to {}: {e}", path.display()))
    }

    /// Delete `rec`'s key from the keyring. A failure leaves an orphan entry
    /// that nothing points to any more, not a key in use.
    fn forget_key(&self, rec: &mut ProviderRecord) {
        if !rec.in_keyring {
            return;
        }
        if let Err(e) = self.vault.delete(&rec.id) {
            warn!("Could not delete {}'s key from the system keyring: {e}", rec.label);
        }
        rec.in_keyring = false;
    }

    pub fn view(&self) -> LlmSettings {
        let providers = &self.settings.providers;
        LlmSettings {
            providers: providers.iter().map(view_provider).collect(),
            routes: self.settings.routes.clone(),
            keys_in_file: self.path.is_some()
                && providers.iter().any(ProviderRecord::key_outside_keyring),
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
        if !(rec.base_url.starts_with("http://") || rec.base_url.starts_with("https://")) {
            return Err(format!(
                "{} needs a base URL starting with http:// or https://",
                rec.label
            ));
        }
        // Only once nothing but the save can fail: a rejected edit must not
        // have deleted the stored key already.
        if let Some(key) = input.api_key.map(|k| clean_key(&k)) {
            if key != rec.api_key {
                self.forget_key(&mut rec);
                rec.api_key = key;
            }
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
        if let Some(mut rec) = self.find(id).cloned() {
            self.forget_key(&mut rec);
        }
        let before = self.settings.providers.len();
        self.settings.providers.retain(|p| p.id != id);
        let routes = &mut self.settings.routes;
        for task in [Task::Translation, Task::Cleaning, Task::Backup] {
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
            Task::Backup => &routes.backup,
        };
        own.as_ref().and_then(to_target).or_else(|| match task {
            Task::Cleaning => routes.translation.as_ref().and_then(to_target),
            Task::Translation | Task::Backup => None,
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

    /// The backup route for translation, unless it is the translation route
    /// itself (asking the same model again would only repeat the answer).
    pub fn translation_backup(&self) -> Option<Target> {
        let routes = &self.settings.routes;
        let backup = routes.backup.as_ref().filter(|b| routes.translation.as_ref() != Some(*b))?;
        self.target_for(&backup.provider_id, &backup.model).ok()
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
        in_keyring: false,
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
    use crate::llm::vault::MemVault;

    fn temp_path() -> PathBuf {
        std::env::temp_dir()
            .join(format!("lyryc-llm-test-{}", uuid::Uuid::new_v4().simple()))
            .join(FILE_NAME)
    }

    /// Tests never touch the real OS keyring.
    fn open(path: &Path, vault: &MemVault) -> Store {
        Store::load_with(path.to_path_buf(), Box::new(vault.clone()))
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
    fn round_trips_with_the_key_in_the_keyring_not_the_file_or_view() {
        let path = temp_path();
        let vault = MemVault::default();
        let mut store = open(&path, &vault);
        let p = store
            .upsert(input("openai", Some(" sk-test-1234567890 ")))
            .unwrap();
        assert!(p.has_key);
        assert_eq!(p.key_hint.as_deref(), Some("sk-…7890"));
        store
            .set_route(Task::Translation, route(&p.id, "gpt-x"))
            .unwrap();

        let reloaded = open(&path, &vault);
        assert_eq!(reloaded.view(), store.view());
        assert!(!reloaded.view().keys_in_file);
        let t = reloaded.target_for(&p.id, "gpt-x").unwrap();
        assert_eq!(t.api_key.as_deref(), Some("sk-test-1234567890"));

        let view_json = serde_json::to_string(&reloaded.view()).unwrap();
        assert!(!view_json.contains("sk-test-1234567890"));
        assert!(view_json.contains("\"hasKey\":true"));
        assert!(view_json.contains("\"providerId\""));

        let on_disk = fs::read_to_string(&path).unwrap();
        assert!(!on_disk.contains("sk-test-1234567890"));
        assert!(!on_disk.contains("\"apiKey\""));
        assert!(on_disk.contains("\"inKeyring\": true"));
        assert!(vault.has(&p.id));
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
    fn plaintext_keys_from_older_files_move_into_the_keyring() {
        let path = temp_path();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let legacy = r#"{"version":1,"providers":[{"id":"openai-old","presetId":"openai","label":"OpenAI","baseUrl":"https://api.openai.com/v1","apiKey":"sk-legacy-1234567890"}],"routes":{"translation":{"providerId":"openai-old","model":"gpt-x"}}}"#;
        fs::write(&path, legacy).unwrap();
        let vault = MemVault::default();

        // Loading alone writes nothing.
        let mut s = open(&path, &vault);
        assert!(s.view().keys_in_file);
        assert!(!vault.has("openai-old"));

        s.secure_keys();
        assert!(!s.view().keys_in_file);
        assert!(vault.has("openai-old"));
        let on_disk = fs::read_to_string(&path).unwrap();
        assert!(!on_disk.contains("sk-legacy"));
        assert!(on_disk.contains("\"translation\""));

        let t = open(&path, &vault).resolve(Task::Translation, &[], &[]).unwrap();
        assert_eq!(t.api_key.as_deref(), Some("sk-legacy-1234567890"));
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn without_a_keyring_keys_stay_in_the_file_and_say_so() {
        let path = temp_path();
        let mut s = Store::load_with(path.clone(), Box::new(NoKeyring));
        let p = s.upsert(input("openai", Some("sk-nokeyring-12345"))).unwrap();
        assert!(s.view().keys_in_file);
        let reloaded = Store::load_with(path.clone(), Box::new(NoKeyring));
        let t = reloaded.target_for(&p.id, "m").unwrap();
        assert_eq!(t.api_key.as_deref(), Some("sk-nokeyring-12345"));
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn clearing_or_removing_deletes_the_keyring_entry() {
        let path = temp_path();
        let vault = MemVault::default();
        let mut s = open(&path, &vault);
        let a = s.upsert(input("openai", Some("sk-aaaaaaaaaa"))).unwrap();
        let b = s.upsert(input("mistral", Some("sk-bbbbbbbbbb"))).unwrap();
        assert!(vault.has(&a.id) && vault.has(&b.id));

        let clear = ProviderInput { id: Some(a.id.clone()), ..input("openai", Some("")) };
        assert!(!s.upsert(clear).unwrap().has_key);
        assert!(!vault.has(&a.id));

        s.remove(&b.id).unwrap();
        assert!(!vault.has(&b.id));
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_rejected_edit_keeps_the_stored_key() {
        let path = temp_path();
        let vault = MemVault::default();
        let mut s = open(&path, &vault);
        let local = || Some("http://localhost:1234/v1".to_string());
        let p = s
            .upsert(ProviderInput { base_url: local(), ..input("custom", Some("sk-keepme-123")) })
            .unwrap();
        let bad = ProviderInput {
            id: Some(p.id.clone()),
            base_url: Some("ftp://nope".into()),
            ..input("custom", Some("sk-other-456"))
        };
        assert!(s.upsert(bad).is_err());
        assert!(vault.has(&p.id));
        let t = open(&path, &vault).target_for(&p.id, "m").unwrap();
        assert_eq!(t.api_key.as_deref(), Some("sk-keepme-123"));
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_key_missing_from_the_keyring_reads_as_no_key() {
        let path = temp_path();
        let vault = MemVault::default();
        let p = open(&path, &vault).upsert(input("openai", Some("sk-gone-1234567"))).unwrap();
        vault.0.lock().unwrap().clear();
        let s = open(&path, &vault);
        assert!(!s.view().providers[0].has_key);
        assert!(!s.view().keys_in_file);
        assert_eq!(s.target_for(&p.id, "m").unwrap().api_key, None);
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn corrupt_file_is_moved_aside() {
        let path = temp_path();
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{not json").unwrap();
        let store = Store::load_with(path.clone(), Box::new(NoKeyring));
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
    fn the_translation_backup_is_only_the_route_the_user_set() {
        let mut s = Store::in_memory();
        let a = s.upsert(input("anthropic", Some("k-anthropic"))).unwrap();
        let g = s.upsert(input("gemini", Some("k-gemini"))).unwrap();
        s.set_route(Task::Translation, route(&a.id, "claude-sonnet-5-5")).unwrap();
        s.set_route(Task::Cleaning, route(&g.id, "gemini-3.5-flash-lite")).unwrap();
        // Gemini is set up and routed for cleaning, but not as a backup: no backup.
        assert!(s.translation_backup().is_none());
        // Backup routes don't stand in for anything else.
        assert!(s.resolve(Task::Backup, &[], &[]).is_none());

        s.set_route(Task::Backup, route(&g.id, "gemini-3.5-pro")).unwrap();
        let b = s.translation_backup().unwrap();
        assert_eq!((b.preset.id, b.model.as_str()), (GEMINI, "gemini-3.5-pro"));

        // The translation route itself as backup would only repeat it.
        s.set_route(Task::Backup, route(&a.id, "claude-sonnet-5-5")).unwrap();
        assert!(s.translation_backup().is_none());

        // Removing its provider clears the backup route too.
        s.set_route(Task::Backup, route(&g.id, "gemini-3.5-pro")).unwrap();
        s.remove(&g.id).unwrap();
        assert!(s.view().routes.backup.is_none());
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
        let vault = MemVault::default();
        let mut s = open(&path, &vault);
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
        assert_eq!(open(&path, &vault).view(), first);

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
