//! Self-update from GitHub Releases (`latest.json`, see tauri.conf.json).
//!
//! The backend checks on its own, shortly after start and then every few hours,
//! and tells every window through `update-status`. Installing restarts the
//! app, so it only ever happens on the user's click.

use std::sync::Mutex;
use std::time::Duration;

use log::{info, warn};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};

const FIRST_CHECK: Duration = Duration::from_secs(15);
const RECHECK: Duration = Duration::from_secs(6 * 60 * 60);
/// Progress events while downloading: one per this many bytes.
const PROGRESS_STEP: u64 = 512 * 1024;

#[derive(Clone, Default, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum UpdateStatus {
    #[default]
    Idle,
    Checking,
    UpToDate,
    Available { version: String, notes: Option<String> },
    Downloading { version: String, done: u64, total: Option<u64> },
    Error { message: String },
}

impl UpdateStatus {
    fn busy(&self) -> bool {
        matches!(self, Self::Checking | Self::Downloading { .. })
    }
}

#[derive(Default)]
struct Inner {
    status: UpdateStatus,
    /// The update `status` announces as available, ready to install.
    pending: Option<Update>,
}

#[derive(Default)]
pub struct UpdaterState(Mutex<Inner>);

fn status(app: &AppHandle) -> UpdateStatus {
    app.state::<UpdaterState>().0.lock().unwrap().status.clone()
}

fn publish(app: &AppHandle, status: UpdateStatus) {
    app.state::<UpdaterState>().0.lock().unwrap().status = status.clone();
    let _ = app.emit("update-status", status);
}

/// Move to `next` unless a check or download is already running.
fn begin(app: &AppHandle, next: UpdateStatus) -> Result<(), UpdateStatus> {
    let state = app.state::<UpdaterState>();
    let mut inner = state.0.lock().unwrap();
    if inner.status.busy() {
        return Err(inner.status.clone());
    }
    inner.status = next.clone();
    drop(inner);
    let _ = app.emit("update-status", next);
    Ok(())
}

async fn check(app: &AppHandle) -> UpdateStatus {
    if let Err(busy) = begin(app, UpdateStatus::Checking) {
        return busy;
    }
    let found = match app.updater() {
        Ok(updater) => updater.check().await,
        Err(e) => Err(e),
    };
    let next = match &found {
        Ok(Some(update)) => {
            info!("Update available: {} -> {}", update.current_version, update.version);
            UpdateStatus::Available { version: update.version.clone(), notes: update.body.clone() }
        }
        Ok(None) => UpdateStatus::UpToDate,
        Err(e) => {
            warn!("Update check failed: {e}");
            UpdateStatus::Error { message: format!("Couldn't check for updates: {e}") }
        }
    };
    app.state::<UpdaterState>().0.lock().unwrap().pending = found.ok().flatten();
    publish(app, next.clone());
    next
}

/// Background checks. Dev builds skip them: their version is always "old".
pub fn start(app: &AppHandle) {
    if cfg!(debug_assertions) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK).await;
        loop {
            // Once one is waiting, rechecking would only flicker the button.
            if !matches!(status(&app), UpdateStatus::Available { .. }) {
                check(&app).await;
            }
            tokio::time::sleep(RECHECK).await;
        }
    });
}

#[tauri::command]
pub fn update_status(app: AppHandle) -> UpdateStatus {
    status(&app)
}

#[tauri::command]
pub async fn update_check(app: AppHandle) -> UpdateStatus {
    check(&app).await
}

/// Download, verify, install and restart. Returns only if that fails.
#[tauri::command]
pub async fn update_install(app: AppHandle) -> Result<(), String> {
    let update = app.state::<UpdaterState>().0.lock().unwrap().pending.take();
    let Some(update) = update else {
        return Err("No update to install. Check for updates first.".into());
    };
    let version = update.version.clone();
    if begin(&app, UpdateStatus::Downloading { version: version.clone(), done: 0, total: None }).is_err() {
        app.state::<UpdaterState>().0.lock().unwrap().pending = Some(update);
        return Err("An update check or install is already running.".into());
    }

    let (mut done, mut shown) = (0u64, 0u64);
    let progress = |chunk: usize, total: Option<u64>| {
        done += chunk as u64;
        if done - shown >= PROGRESS_STEP || Some(done) == total {
            shown = done;
            publish(&app, UpdateStatus::Downloading { version: version.clone(), done, total });
        }
    };
    // On Windows the installer takes over and this process exits inside the call.
    match update.download_and_install(progress, || info!("Update {version} downloaded, installing")).await {
        Ok(()) => app.restart(),
        Err(e) => {
            warn!("Update {version} failed: {e}");
            let message = format!("Couldn't install {version}: {e}");
            publish(&app, UpdateStatus::Error { message: message.clone() });
            Err(message)
        }
    }
}
