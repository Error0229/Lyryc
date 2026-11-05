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

/// The log file, moved to `lyryc.old.log` whenever it passes `max` bytes:
/// at startup and while running, since a tray app may run for weeks.
struct LogFile {
    path: PathBuf,
    file: Option<File>,
    written: u64,
    max: u64,
}

impl LogFile {
    fn open(path: PathBuf, max: u64) -> Self {
        let mut log = LogFile { path, file: None, written: 0, max };
        log.reopen();
        log
    }

    fn reopen(&mut self) {
        self.file = None;
        if fs::metadata(&self.path).is_ok_and(|m| m.len() > self.max) {
            let _ = fs::rename(&self.path, self.path.with_file_name("lyryc.old.log"));
        }
        self.file = OpenOptions::new().create(true).append(true).open(&self.path).ok();
        self.written = fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0);
    }

    fn write(&mut self, buf: &[u8]) {
        if let Some(file) = &mut self.file {
            let _ = file.write_all(buf);
            self.written += buf.len() as u64;
        }
        if self.written > self.max {
            self.reopen();
        }
    }
}

fn open_log() -> Option<LogFile> {
    let dir = log_dir();
    fs::create_dir_all(&dir).ok()?;
    Some(LogFile::open(dir.join("lyryc.log"), MAX_BYTES))
}

/// stderr plus the file. A failing file never stops logging to stderr.
struct Tee(Option<LogFile>);

impl Write for Tee {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let _ = io::stderr().write_all(buf);
        if let Some(log) = &mut self.0 {
            log.write(buf);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        if let Some(file) = self.0.as_mut().and_then(|l| l.file.as_mut()) {
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
    log::log!(target: "frontend", level, "{}", one_line(&message));
}

/// Longest webview message kept: the clock's diagnostics run to a few KB.
const MAX_FRONTEND_CHARS: usize = 16 * 1024;

/// A webview message as one bounded log line: its newlines (stack traces)
/// escaped, so it can't pass for lines of its own.
fn one_line(message: &str) -> String {
    let mut out: String = message.chars().take(MAX_FRONTEND_CHARS).collect();
    if out.len() < message.len() {
        out.push_str(" …(cut)");
    }
    out.replace('\r', "").replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_webview_message_is_one_bounded_line() {
        assert_eq!(one_line("Error: boom\n    at f (x.ts:1)\r\n"), "Error: boom\\n    at f (x.ts:1)\\n");
        let long = "x".repeat(MAX_FRONTEND_CHARS + 10);
        let line = one_line(&long);
        assert!(line.ends_with("…(cut)") && line.chars().count() < long.len());
    }

    #[test]
    fn the_file_rotates_while_running() {
        let dir = std::env::temp_dir().join(format!("lyryc-log-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("lyryc.log");
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(dir.join("lyryc.old.log"));
        let mut log = LogFile::open(path.clone(), 100);
        log.write(&[b'a'; 80]);
        assert!(!dir.join("lyryc.old.log").exists(), "under the limit");
        log.write(&[b'b'; 40]);
        assert!(dir.join("lyryc.old.log").exists(), "passing it moves the file aside");
        assert_eq!(fs::metadata(&path).unwrap().len(), 0, "and starts a new one");
        log.write(b"c");
        assert_eq!(fs::read(&path).unwrap(), b"c");
        drop(log);
        let _ = fs::remove_dir_all(&dir);
    }
}
