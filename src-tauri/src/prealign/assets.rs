//! The downloads the feature needs, fetched on first enable into the app's
//! local data dir and checked by SHA-256: the three int8 ONNX files of
//! echora-kara-300m (encoder, text head, frame head), ONNX Runtime (Microsoft's
//! release build, loaded at run time so the app itself carries none of it), and
//! yt-dlp. Windows only for now: that is where the runtime download is wired up.
//!
//! The model files are our own exports (`research/prealign/bench/export_onnx.py
//! echora` and `echo_head.py`), pinned by URL, SHA-256 and size. Each can be
//! pointed elsewhere with `LYRYC_PREALIGN_<ENCODER|TEXT_HEAD|FRAME_HEAD>_URL` +
//! `_SHA256`, or put in the prealign dir by hand (see docs/prealign.md).

use log::{debug, info, warn};
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

// TODO(owner): hosting is not decided yet. Replace these three placeholders
// (`.invalid` never resolves, so a download fails loudly) with the pinned URLs
// of the uploaded files. The hashes and sizes below are of the files as built.
pub const ENCODER_URL: &str = "https://example.invalid/lyryc-prealign/echora_enc.int8.onnx";
pub const TEXT_HEAD_URL: &str = "https://example.invalid/lyryc-prealign/echo_text.int8.onnx";
pub const FRAME_HEAD_URL: &str = "https://example.invalid/lyryc-prealign/echo_frame.int8.onnx";

/// One pinned file of the model.
pub struct ModelFile {
    /// Its name in the prealign dir.
    pub name: &'static str,
    /// `LYRYC_PREALIGN_<env>_URL` / `_SHA256` override it.
    env: &'static str,
    url: &'static str,
    sha256: &'static str,
    bytes: u64,
}

/// Encoder, text head, frame head: the order `Paths::models` gives them in.
pub const MODELS: [ModelFile; 3] = [
    ModelFile {
        name: "echora_enc.int8.onnx",
        env: "ENCODER",
        url: ENCODER_URL,
        sha256: "a8b9a9591bc4ffaab7e981a132a87c697dc14d090fdcc73b7a4f1ad9bfca2fbe",
        bytes: 356_168_399,
    },
    ModelFile {
        name: "echo_text.int8.onnx",
        env: "TEXT_HEAD",
        url: TEXT_HEAD_URL,
        sha256: "908e7a0beaa15281023bbdd55f8a10676b504e6e5d0c2be507c2bc038203dee1",
        bytes: 23_471_631,
    },
    ModelFile {
        name: "echo_frame.int8.onnx",
        env: "FRAME_HEAD",
        url: FRAME_HEAD_URL,
        sha256: "b33c7c9062edcd86ffb31aec2c72e1ae2de10789a31a4b49a5fff13e6d412897",
        bytes: 18_104_239,
    },
];

/// Model files of earlier versions, deleted on start.
const STALE: [&str; 1] = ["mms_fa.int8.onnx"];

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
    /// Encoder, text head, frame head.
    pub fn models(&self) -> [PathBuf; 3] {
        MODELS.each_ref().map(|m| self.dir.join(m.name))
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
    /// Every file is in place (the models' hashes are checked before first use).
    pub fn ready(&self) -> bool {
        let model_ok = |m: &ModelFile| match (file_len(&self.dir.join(m.name)), m.source().bytes) {
            (Some(n), Some(want)) => n == want,
            (Some(n), None) => n > 0,
            (None, _) => false,
        };
        MODELS.iter().all(model_ok) && [self.runtime(), self.ytdlp()].iter().all(|p| file_len(p).is_some_and(|n| n > 0))
    }
    pub fn disk_bytes(&self) -> u64 {
        self.models().iter().chain([&self.runtime(), &self.ytdlp()]).filter_map(|p| file_len(p)).sum()
    }
    /// Drop what an earlier version downloaded.
    pub fn remove_stale(&self) {
        for name in STALE {
            if std::fs::remove_file(self.dir.join(name)).is_ok() {
                info!("Pre-align: removed the old model {}", name);
            }
        }
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

impl ModelFile {
    /// The pinned default, or the user's override (both variables, or neither).
    pub fn source(&self) -> ModelSource {
        let var = |suffix: &str| std::env::var(format!("LYRYC_PREALIGN_{}_{suffix}", self.env)).map(|v| v.trim().to_string());
        match (var("URL"), var("SHA256")) {
            (Ok(url), Ok(sha)) if !url.is_empty() && sha.len() == 64 => ModelSource { url, sha256: sha.to_lowercase(), bytes: None },
            _ => ModelSource { url: self.url.into(), sha256: self.sha256.into(), bytes: Some(self.bytes) },
        }
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
    // One progress bar over the three files, when their sizes are known.
    let total: Option<u64> = MODELS.iter().map(|m| m.source().bytes).sum();
    let mut before = 0;
    for (m, path) in MODELS.iter().zip(paths.models()) {
        let src = m.source();
        if !path.exists() {
            info!("Downloading {} from {}", m.name, src.url);
            let report = |p: Progress| progress(Progress { file: "model", done: before + p.done, total: total.or(p.total) });
            download(&client, &src.url, &path, &src.sha256, m.name, &report).await?;
        }
        before += src.bytes.unwrap_or(0);
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

/// Full-file check of the model files, once per session before first use.
/// A file that fails is removed, so the next enable downloads it again.
pub fn verify_models(paths: &Paths) -> bool {
    MODELS.iter().zip(paths.models()).all(|(m, path)| {
        let ok = sha256_of(&path).is_some_and(|h| h == m.source().sha256);
        if !ok {
            warn!("Alignment model file {} failed its checksum; removing it", m.name);
            let _ = std::fs::remove_file(&path);
        }
        ok
    })
}

fn sha256_of(path: &Path) -> Option<String> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    std::io::copy(&mut f, &mut hasher).ok()?;
    Some(hex(&hasher.finalize()))
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
