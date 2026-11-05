//! Logs go to stderr (the `tauri dev` terminal) and to a file, because a
//! release build has no console: without the file, nothing it logs survives.
//! `%LOCALAPPDATA%\com.lyryc.app\logs\lyryc.log`, appended across runs, moved
//! to `lyryc.old.log` once it passes `MAX_BYTES`.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;

const MAX_BYTES: u64 = 5 * 1024 * 1024;

pub fn log_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("com.lyryc.app")
        .join("logs")
}

fn open_log() -> Option<File> {
    let dir = log_dir();
    fs::create_dir_all(&dir).ok()?;
    let path = dir.join("lyryc.log");
    if fs::metadata(&path).is_ok_and(|m| m.len() > MAX_BYTES) {
        let _ = fs::rename(&path, dir.join("lyryc.old.log"));
    }
    OpenOptions::new().create(true).append(true).open(path).ok()
}

/// stderr plus the file. A failing file never stops logging to stderr.
struct Tee(Option<File>);

impl Write for Tee {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let _ = io::stderr().write_all(buf);
        if let Some(file) = &mut self.0 {
            let _ = file.write_all(buf);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Some(file) = &mut self.0 {
            let _ = file.flush();
        }
        io::stderr().flush()
    }
}

/// RUST_LOG overrides. tao's event-loop warnings are upstream noise.
pub fn init() {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,lyryc_lib=debug,tao=error"),
    )
    .target(env_logger::Target::Pipe(Box::new(Tee(open_log()))))
    .init();
}

/// The webview's errors and warnings, which otherwise only its devtools see.
#[tauri::command]
pub fn log_frontend(level: String, message: String) {
    let level = match level.as_str() {
        "error" => log::Level::Error,
        "warn" => log::Level::Warn,
        _ => log::Level::Info,
    };
    log::log!(target: "frontend", level, "{}", message);
}
