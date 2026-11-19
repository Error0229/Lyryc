//! The browser's YouTube sign-in, lent by the extension for one yt-dlp run.
//! The user's own uploads are private videos: they stream only for the
//! account that owns them, so yt-dlp needs that account's cookies.
//!
//! The cookies are asked for only when a video needs them, kept in memory,
//! written to a file only for the run, and never logged.

use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use tokio::sync::oneshot;

/// The extension answers at once; longer means it is an old one that can't.
const ANSWER: Duration = Duration::from_secs(5);
/// One of these means the browser is signed in to YouTube.
const SIGNED_IN: [&str; 2] = ["SAPISID", "__Secure-3PAPISID"];

/// One cookie, as `chrome.cookies` gives it.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Cookie {
    domain: String,
    #[serde(default)]
    host_only: bool,
    path: String,
    #[serde(default)]
    secure: bool,
    /// Seconds since the epoch; none for a session cookie.
    expiration_date: Option<f64>,
    name: String,
    value: String,
}

/// The extension's `YOUTUBE_COOKIES` message.
#[derive(Debug, Deserialize)]
struct Reply {
    #[serde(default)]
    cookies: Vec<Cookie>,
    /// Why it has none to give: no permission, most likely.
    error: Option<String>,
}

/// The connection that sends YouTube Music's queue: the browser playing it,
/// signed in to the account that owns the uploads. Another browser or profile
/// may be connected too, signed out or as someone else.
static PLAYER: Mutex<Option<String>> = Mutex::new(None);

/// Who waits for an answer, and from which connection. One at a time:
/// pre-align fetches one video at a time.
static WAITING: Mutex<Option<(String, oneshot::Sender<Reply>)>> = Mutex::new(None);

/// `client` sent YouTube Music's queue.
pub fn on_queue(client: &str) {
    *PLAYER.lock().unwrap() = Some(client.to_string());
}

/// `client`'s answer to `COOKIES_REQUEST`. Only the connection asked is heard.
pub fn on_reply(client: &str, data: &serde_json::Value) {
    let mut waiting = WAITING.lock().unwrap();
    if waiting.as_ref().is_some_and(|(asked, _)| asked == client) {
        let reply = Reply::deserialize(data).unwrap_or(Reply { cookies: vec![], error: Some("its answer had the wrong shape".into()) });
        let (_, tx) = waiting.take().unwrap();
        let _ = tx.send(reply);
    }
}

/// A cookie file for yt-dlp's `--cookies`. Deleted when dropped.
pub struct Jar {
    pub path: PathBuf,
}

impl Drop for Jar {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// YouTube's cookies from the browser, in a file under `dir` for `video_id`'s
/// run, or why not.
pub async fn youtube(dir: &Path, video_id: &str) -> Result<Jar, String> {
    let client = PLAYER.lock().unwrap().clone().ok_or("the browser extension isn't connected")?;
    let (tx, rx) = oneshot::channel();
    *WAITING.lock().unwrap() = Some((client.clone(), tx));
    if !crate::websocket::send_to_client(&client, "COOKIES_REQUEST", serde_json::json!({})).await {
        WAITING.lock().unwrap().take();
        return Err("the browser extension isn't connected".into());
    }
    let reply = match tokio::time::timeout(ANSWER, rx).await {
        Ok(Ok(reply)) => reply,
        _ => {
            WAITING.lock().unwrap().take();
            return Err("the browser extension didn't answer. Update it and reload it in the browser's extensions page".into());
        }
    };
    let text = netscape(&reply)?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    // The guard first: a write that fails partway leaves nothing behind.
    let jar = Jar { path: dir.join(format!("cookies-{video_id}.txt")) };
    write_private(&jar.path, &text).map_err(|e| e.to_string())?;
    Ok(jar)
}

/// Readable by the user alone. On Windows the user's profile already is. Made
/// anew: an existing file would keep its own permissions.
fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    let _ = std::fs::remove_file(path);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    options.open(path)?.write_all(text.as_bytes())
}

/// The cookies in the Netscape format yt-dlp reads, or why they won't do.
fn netscape(reply: &Reply) -> Result<String, String> {
    if let Some(e) = &reply.error {
        return Err(format!("the browser extension couldn't read YouTube's cookies: {e}"));
    }
    if !reply.cookies.iter().any(|c| SIGNED_IN.contains(&c.name.as_str())) {
        // Names only: values are the sign-in.
        let names: Vec<String> = reply.cookies.iter().map(|c| format!("{}@{}", c.name, c.domain)).collect();
        log::debug!("YouTube cookies without a sign-in: [{}]", names.join(", "));
        return Err("the browser isn't signed in to YouTube".into());
    }
    let mut out = String::from("# Netscape HTTP Cookie File\n");
    // A tab or line break would split a line; real cookies have neither.
    let fits = |c: &&Cookie| ![&c.name, &c.value, &c.path].iter().any(|s| s.contains(['\t', '\n', '\r']));
    // HttpOnly cookies go in as plain lines: yt-dlp sends them either way, and
    // the `#HttpOnly_` prefix only keeps them from scripts, which it has none of.
    for c in reply.cookies.iter().filter(fits) {
        // A cookie for subdomains is written with a leading dot, a host's without.
        let domain = c.domain.trim_start_matches('.');
        let (domain, subdomains) = if c.host_only { (domain.to_string(), "FALSE") } else { (format!(".{domain}"), "TRUE") };
        let secure = if c.secure { "TRUE" } else { "FALSE" };
        let expires = c.expiration_date.map_or(0, |t| t as i64);
        out += &format!("{domain}\t{subdomains}\t{}\t{secure}\t{expires}\t{}\t{}\n", c.path, c.name, c.value);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn reply(v: serde_json::Value) -> Reply {
        Reply::deserialize(&v).unwrap()
    }

    #[test]
    fn writes_what_chrome_gives_as_a_netscape_file() {
        let r = reply(json!({"cookies": [
            {"domain": ".youtube.com", "hostOnly": false, "path": "/", "secure": true, "expirationDate": 1800000000.5, "name": "SAPISID", "value": "abc"},
            {"domain": "music.youtube.com", "hostOnly": true, "path": "/", "secure": false, "name": "YSC", "value": "x"},
        ]}));
        assert_eq!(
            netscape(&r).unwrap(),
            "# Netscape HTTP Cookie File\n.youtube.com\tTRUE\t/\tTRUE\t1800000000\tSAPISID\tabc\nmusic.youtube.com\tFALSE\t/\tFALSE\t0\tYSC\tx\n"
        );
    }

    #[test]
    fn says_why_there_are_none() {
        let signed_out = reply(json!({"cookies": [{"domain": ".youtube.com", "path": "/", "name": "YSC", "value": "x"}]}));
        assert_eq!(netscape(&signed_out).unwrap_err(), "the browser isn't signed in to YouTube");
        let refused = reply(json!({"error": "chrome.cookies is undefined"}));
        assert!(netscape(&refused).unwrap_err().contains("chrome.cookies is undefined"));
    }

    #[test]
    fn only_the_browser_asked_is_heard() {
        let (tx, mut rx) = oneshot::channel();
        *WAITING.lock().unwrap() = Some(("brave".into(), tx));
        on_reply("chrome", &json!({"cookies": []}));
        assert!(rx.try_recv().is_err(), "another browser's answer is not the one asked for");
        on_reply("brave", &json!({"cookies": "not a list"}));
        assert!(rx.try_recv().unwrap().error.is_some(), "a malformed answer is an answer, not a wait to the timeout");
        on_reply("brave", &json!({"cookies": []})); // nobody waits: dropped
    }

    #[test]
    fn a_cookie_that_would_split_a_line_is_left_out() {
        let r = reply(json!({"cookies": [
            {"domain": ".youtube.com", "path": "/", "name": "SAPISID", "value": "abc"},
            {"domain": ".youtube.com", "path": "/", "name": "odd", "value": "a\tb\nc"},
        ]}));
        assert_eq!(netscape(&r).unwrap().lines().count(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn the_file_is_the_users_alone() {
        use std::os::unix::fs::PermissionsExt;
        let path = std::env::temp_dir().join(format!("lyryc-cookies-test-{}", std::process::id()));
        write_private(&path, "x").unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        let _ = std::fs::remove_file(path);
    }
}
