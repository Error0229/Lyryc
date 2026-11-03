//! Fetch the audio of one exact YouTube Music videoId with the standalone yt-dlp.
//! Format 140 (AAC-LC in MP4) so it decodes in Rust, with no ffmpeg. Fallbacks
//! are other AAC-in-MP4 streams; a video with none simply isn't aligned.

use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;

const FORMATS: &str = "140/bestaudio[ext=m4a]/bestaudio[acodec^=mp4a]";
const TIMEOUT: Duration = Duration::from_secs(180);

pub fn command(exe: &Path) -> Command {
    let mut cmd = Command::new(exe);
    cmd.kill_on_drop(true).stdin(std::process::Stdio::null());
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    cmd
}

/// The downloaded file. Deleted when dropped, whatever happens in between.
pub struct Audio {
    pub path: PathBuf,
    /// yt-dlp's length for the video, in seconds.
    pub duration: Option<f64>,
}

impl Drop for Audio {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub async fn fetch(exe: &Path, tmp: &Path, video_id: &str) -> Result<Audio, String> {
    if !crate::ytmusic::is_video_id(video_id) {
        return Err(format!("not a video id: {video_id:?}"));
    }
    std::fs::create_dir_all(tmp).map_err(|e| e.to_string())?;
    let template = tmp.join(format!("{video_id}.%(ext)s"));
    let mut cmd = command(exe);
    cmd.args(["--no-playlist", "--no-warnings", "--quiet", "--no-progress", "--no-part", "--no-mtime", "--no-cache-dir"])
        .args(["-f", FORMATS, "--print", "after_move:%(duration)s\t%(filepath)s", "-o"])
        .arg(&template)
        .arg(format!("https://music.youtube.com/watch?v={video_id}"));
    let out = match tokio::time::timeout(TIMEOUT, cmd.output()).await {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => return Err(format!("yt-dlp: {e}")),
        Err(_) => {
            cleanup(tmp, video_id);
            return Err("yt-dlp timed out".into());
        }
    };
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout.lines().rev().find(|l| l.contains('\t'));
    let Some((duration, path)) = line.and_then(|l| l.split_once('\t')).filter(|_| out.status.success()) else {
        cleanup(tmp, video_id);
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("yt-dlp failed: {}", err.trim().lines().last().unwrap_or("no output")));
    };
    // Trust only the file name, and only inside our own directory.
    let name = Path::new(path.trim()).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let path = tmp.join(&name);
    if !name.starts_with(video_id) || !path.is_file() {
        cleanup(tmp, video_id);
        return Err(format!("yt-dlp output not found: {name:?}"));
    }
    Ok(Audio { path, duration: duration.trim().parse().ok() })
}

/// Remove anything a failed run left for this id.
fn cleanup(tmp: &Path, video_id: &str) {
    if let Ok(dir) = std::fs::read_dir(tmp) {
        for e in dir.flatten() {
            if e.file_name().to_string_lossy().starts_with(video_id) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_is_deleted_when_dropped() {
        let dir = std::env::temp_dir().join(format!("lyryc-ytdlp-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("x.m4a");
        std::fs::write(&path, b"audio").unwrap();
        drop(Audio { path: path.clone(), duration: None });
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
