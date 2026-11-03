//! The downloads the feature needs, fetched on first enable into the app's
//! local data dir and checked by SHA-256: the int8 MMS aligner, ONNX Runtime
//! (Microsoft's release build, loaded at run time so the app itself carries
//! none of it), and yt-dlp. Windows only for now: that is where the runtime
//! download is wired up.
//!
//! Model source: no official int8 export runs at the study's speed (the
//! onnx-community `model_int8.onnx` quantizes the conv layers to ConvInteger
//! and is 3.6× slower on CPU), so the default is a public export with the
//! study's recipe (MatMul-only dynamic int8, float Conv), pinned by hash.
//! `LYRYC_PREALIGN_MODEL_URL` + `LYRYC_PREALIGN_MODEL_SHA256` point it elsewhere,
//! e.g. at a file made with `research/prealign/bench/export_onnx.py`
//! (see docs/prealign.md).

use log::{debug, info, warn};
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

pub const MODEL_URL: &str =
    "https://huggingface.co/romara-labs/mms-300m-1130-forced-aligner-ONNX/resolve/f515b917cf1f4f14d12a4768940cd8b4eb3e2119/model.q8.onnx";
pub const MODEL_SHA256: &str = "bef3271b8f4496fd173da1d2392c4bc7d496e0245cf1e8700481f812795b4abe";
pub const MODEL_BYTES: u64 = 356_289_622;

/// ONNX Runtime 1.22 (what `ort` 2.0.0-rc.10 speaks), CPU build, from Microsoft's GitHub release.
const ORT_ZIP_URL: &str = "https://github.com/microsoft/onnxruntime/releases/download/v1.22.0/onnxruntime-win-x64-1.22.0.zip";
const ORT_ZIP_SHA256: &str = "174c616efc0271194488642a72f1a514e01487da4dfe84c49296d66e40ebe0da";
const ORT_DLL_IN_ZIP: &str = "onnxruntime-win-x64-1.22.0/lib/onnxruntime.dll";

const YTDLP_ASSET: &str = "yt-dlp.exe";
const YTDLP_RELEASE: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/";
/// `yt-dlp -U` at most this often.
const YTDLP_UPDATE_EVERY: Duration = Duration::from_secs(24 * 3600);

/// Where everything lives: `<app local data>/prealign/`.
#[derive(Clone, Debug)]
pub struct Paths {
    pub dir: PathBuf,
}

impl Paths {
    pub fn model(&self) -> PathBuf {
        self.dir.join("mms_fa.int8.onnx")
    }
    pub fn runtime(&self) -> PathBuf {
        self.dir.join("onnxruntime.dll")
    }
    pub fn ytdlp(&self) -> PathBuf {
        self.dir.join(YTDLP_ASSET)
    }
    /// Audio lives here only between download and alignment.
    pub fn tmp(&self) -> PathBuf {
        self.dir.join("tmp")
    }
    fn ytdlp_stamp(&self) -> PathBuf {
        self.dir.join("yt-dlp.updated")
    }
    /// Both files are in place (the model's hash is checked before first use).
    pub fn ready(&self) -> bool {
        let model_ok = match (file_len(&self.model()), model_source().bytes) {
            (Some(n), Some(want)) => n == want,
            (Some(n), None) => n > 0,
            (None, _) => false,
        };
        model_ok && [self.runtime(), self.ytdlp()].iter().all(|p| file_len(p).is_some_and(|n| n > 0))
    }
    pub fn disk_bytes(&self) -> u64 {
        [self.model(), self.runtime(), self.ytdlp()].iter().filter_map(|p| file_len(p)).sum()
    }
}

fn file_len(p: &Path) -> Option<u64> {
    std::fs::metadata(p).ok().map(|m| m.len())
}

pub struct ModelSource {
    pub url: String,
    pub sha256: String,
    pub bytes: Option<u64>,
}

/// The pinned default, or the user's override (both variables, or neither).
pub fn model_source() -> ModelSource {
    match (std::env::var("LYRYC_PREALIGN_MODEL_URL"), std::env::var("LYRYC_PREALIGN_MODEL_SHA256")) {
        (Ok(url), Ok(sha)) if !url.trim().is_empty() && sha.trim().len() == 64 => {
            ModelSource { url: url.trim().into(), sha256: sha.trim().to_lowercase(), bytes: None }
        }
        _ => ModelSource { url: MODEL_URL.into(), sha256: MODEL_SHA256.into(), bytes: Some(MODEL_BYTES) },
    }
}

/// Download progress for the UI.
#[derive(Clone, Debug)]
pub struct Progress {
    pub file: &'static str,
    pub done: u64,
    pub total: Option<u64>,
}

fn client() -> Client {
    Client::builder()
        .user_agent(concat!("Lyryc v", env!("CARGO_PKG_VERSION"), " (https://github.com/Error0229/Lyryc)"))
        .connect_timeout(Duration::from_secs(20))
        .build()
        .expect("prealign HTTP client")
}

/// Fetch whatever is missing. Sequential: the model, the runtime, then yt-dlp.
pub async fn ensure(paths: &Paths, progress: impl Fn(Progress) + Send + Sync) -> Result<(), String> {
    if !cfg!(windows) {
        return Err("word-level timing is only available on Windows for now".into());
    }
    std::fs::create_dir_all(&paths.dir).map_err(|e| e.to_string())?;
    let client = client();
    let src = model_source();
    if !paths.model().exists() {
        info!("Downloading the alignment model from {}", src.url);
        download(&client, &src.url, &paths.model(), &src.sha256, "model", &progress).await?;
    }
    if !paths.runtime().exists() {
        info!("Downloading ONNX Runtime");
        let zip = paths.dir.join("onnxruntime.zip");
        download(&client, ORT_ZIP_URL, &zip, ORT_ZIP_SHA256, "runtime", &progress).await?;
        let (zip_path, dll) = (zip.clone(), paths.runtime());
        let unpacked = tokio::task::spawn_blocking(move || extract(&zip_path, ORT_DLL_IN_ZIP, &dll)).await.map_err(|e| e.to_string());
        let _ = std::fs::remove_file(&zip);
        unpacked??;
    }
    if !paths.ytdlp().exists() {
        let sums = client
            .get(format!("{YTDLP_RELEASE}SHA2-256SUMS"))
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| format!("yt-dlp checksums: {e}"))?
            .text()
            .await
            .map_err(|e| e.to_string())?;
        let sha = sums
            .lines()
            .find_map(|l| l.split_once("  ").filter(|(_, name)| name.trim() == YTDLP_ASSET).map(|(h, _)| h.to_string()))
            .ok_or("yt-dlp checksum not published")?;
        info!("Downloading {}", YTDLP_ASSET);
        download(&client, &format!("{YTDLP_RELEASE}{YTDLP_ASSET}"), &paths.ytdlp(), &sha, "yt-dlp", &progress).await?;
        touch(&paths.ytdlp_stamp());
    }
    Ok(())
}

/// Stream `url` to `<dest>.part`, check its SHA-256, then move it into place.
async fn download(
    client: &Client,
    url: &str,
    dest: &Path,
    sha256: &str,
    file: &'static str,
    progress: &(impl Fn(Progress) + Send + Sync),
) -> Result<(), String> {
    let part = dest.with_extension("part");
    let mut resp = client.get(url).send().await.and_then(|r| r.error_for_status()).map_err(|e| format!("{file}: {e}"))?;
    let total = resp.content_length();
    let mut out = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut done = 0u64;
    let mut last = 0u64;
    let result: Result<(), String> = async {
        while let Some(chunk) = resp.chunk().await.map_err(|e| format!("{file}: {e}"))? {
            out.write_all(&chunk).map_err(|e| e.to_string())?;
            hasher.update(&chunk);
            done += chunk.len() as u64;
            if done - last > 1 << 20 || Some(done) == total {
                last = done;
                progress(Progress { file, done, total });
            }
        }
        Ok(())
    }
    .await;
    drop(out);
    let digest = hex(&hasher.finalize());
    let verdict = result.and_then(|_| {
        if digest == sha256 {
            Ok(())
        } else {
            Err(format!("{file}: checksum mismatch ({digest})"))
        }
    });
    match verdict {
        Ok(()) => std::fs::rename(&part, dest).map_err(|e| e.to_string()),
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            Err(e)
        }
    }
}

/// Copy one file out of a zip archive.
fn extract(zip: &Path, name: &str, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(zip).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut entry = archive.by_name(name).map_err(|e| format!("{name}: {e}"))?;
    let part = dest.with_extension("part");
    let mut out = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
    drop(out);
    std::fs::rename(&part, dest).map_err(|e| e.to_string())
}

pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// Full-file check of the model, once per session before first use.
pub fn verify_model(paths: &Paths) -> bool {
    let src = model_source();
    let Ok(mut f) = std::fs::File::open(paths.model()) else {
        return false;
    };
    let mut hasher = Sha256::new();
    if std::io::copy(&mut f, &mut hasher).is_err() {
        return false;
    }
    let ok = hex(&hasher.finalize()) == src.sha256;
    if !ok {
        warn!("Alignment model failed its checksum; removing it");
        let _ = std::fs::remove_file(paths.model());
    }
    ok
}

/// `yt-dlp -U` if the last update is a day old. YouTube changes break it often.
pub async fn update_ytdlp_if_due(paths: &Paths) {
    let due = std::fs::metadata(paths.ytdlp_stamp())
        .and_then(|m| m.modified())
        .map(|t| SystemTime::now().duration_since(t).unwrap_or_default() > YTDLP_UPDATE_EVERY)
        .unwrap_or(true);
    if !due {
        return;
    }
    touch(&paths.ytdlp_stamp());
    let mut cmd = super::ytdlp::command(&paths.ytdlp());
    cmd.arg("-U");
    match tokio::time::timeout(Duration::from_secs(120), cmd.output()).await {
        Ok(Ok(out)) => debug!("yt-dlp -U: {}", String::from_utf8_lossy(&out.stdout).trim()),
        _ => warn!("yt-dlp -U failed or timed out"),
    }
}

fn touch(p: &Path) {
    let _ = std::fs::write(p, b"");
}

/// Delete the model, the runtime, yt-dlp and any leftover audio.
pub fn remove_all(paths: &Paths) -> Result<(), String> {
    match std::fs::remove_dir_all(&paths.dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}
