//! Example: like or unlike the Spotify song that's playing.
//!
//! Spotify's Mac app has no AppleScript command for "like", so this uses the
//! Spotify Web API with OAuth PKCE sign-in (no client secret in the app).
//! Sign-in opens your browser; Spotify sends you back to a one-time listener
//! on http://127.0.0.1:4848/callback, which this module runs only while a
//! sign-in is in progress. The refresh token is saved in the app's data
//! folder, readable only by you.
//!
//! Setup:
//! 1. Create an app at https://developer.spotify.com/dashboard (Web API).
//! 2. Add the redirect URI `http://127.0.0.1:4848/callback`.
//! 3. Build with its Client ID: `SPOTIFY_CLIENT_ID=… npm run build`
//!    (a Client ID isn't a secret; PKCE apps have no secret).

use base64::Engine;
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const PORT: u16 = 4848;
const REDIRECT_URI: &str = "http://127.0.0.1:4848/callback";
const SCOPES: &str = "user-library-read user-library-modify";

/// Set at build time: `SPOTIFY_CLIENT_ID=… npm run build`.
fn client_id() -> Option<&'static str> {
    option_env!("SPOTIFY_CLIENT_ID").filter(|s| !s.trim().is_empty())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// Built with a Client ID.
    pub configured: bool,
    /// Signed in (a refresh token is saved).
    pub connected: bool,
}

/// PKCE code challenge for a verifier: base64url(sha256(verifier)).
pub fn challenge(verifier: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// "spotify:track:abc123" -> "abc123". None for anything that isn't a track.
pub fn track_id(uri: &str) -> Option<String> {
    let id = uri.strip_prefix("spotify:track:")?;
    (!id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric())).then(|| id.to_string())
}

fn random_token() -> String {
    format!("{}{}", uuid::Uuid::new_v4().simple(), uuid::Uuid::new_v4().simple())
}

fn token_file() -> PathBuf {
    dirs::data_dir().unwrap_or_default().join("edge-starter").join("spotify-token")
}

fn refresh_token() -> Option<String> {
    std::fs::read_to_string(token_file()).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn save_refresh_token(t: &str) {
    let f = token_file();
    let _ = std::fs::create_dir_all(f.parent().unwrap());
    let _ = std::fs::write(&f, t);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o600));
    }
}

static ACCESS: Mutex<Option<(String, Instant)>> = Mutex::new(None);

pub fn status() -> Status {
    Status { configured: client_id().is_some(), connected: client_id().is_some() && refresh_token().is_some() }
}

fn token_request(form: &[(&str, &str)]) -> Result<Value, String> {
    let mut res = ureq::post("https://accounts.spotify.com/api/token")
        .config()
        .http_status_as_error(false)
        .build()
        .send_form(form.iter().copied())
        .map_err(|e| e.to_string())?;
    let ok = res.status().is_success();
    let v: Value = res.body_mut().read_json().map_err(|e| e.to_string())?;
    if !ok {
        return Err(v["error_description"].as_str().or(v["error"].as_str()).unwrap_or("Spotify sign-in failed").to_string());
    }
    if let Some(t) = v["access_token"].as_str() {
        let secs = v["expires_in"].as_u64().unwrap_or(3600).saturating_sub(60);
        *ACCESS.lock().unwrap() = Some((t.to_string(), Instant::now() + Duration::from_secs(secs)));
    }
    // Spotify may rotate the refresh token; keep the newest one.
    if let Some(r) = v["refresh_token"].as_str() {
        save_refresh_token(r);
    }
    Ok(v)
}

fn access_token() -> Result<String, String> {
    if let Some((t, exp)) = ACCESS.lock().unwrap().as_ref() {
        if Instant::now() < *exp {
            return Ok(t.clone());
        }
    }
    let id = client_id().ok_or("Built without SPOTIFY_CLIENT_ID")?;
    let refresh = refresh_token().ok_or("Not signed in to Spotify")?;
    token_request(&[("grant_type", "refresh_token"), ("refresh_token", &refresh), ("client_id", id)])?;
    ACCESS.lock().unwrap().as_ref().map(|(t, _)| t.clone()).ok_or("Spotify didn't return a token".into())
}

/// Pull `name=value` out of a query string.
fn query_param(query: &str, name: &str) -> Option<String> {
    url::form_urlencoded::parse(query.as_bytes()).find(|(k, _)| k == name).map(|(_, v)| v.into_owned())
}

/// Open Spotify sign-in in the browser and wait (up to 5 minutes) for the
/// redirect back to the one-time listener. Blocks: call from a background thread.
pub fn login() -> Result<(), String> {
    let id = client_id().ok_or("Built without SPOTIFY_CLIENT_ID")?;
    let listener = TcpListener::bind(("127.0.0.1", PORT)).map_err(|e| format!("Port {PORT} is busy ({e})"))?;
    let (state, verifier) = (random_token(), random_token());
    let mut u = url::Url::parse("https://accounts.spotify.com/authorize").unwrap();
    u.query_pairs_mut()
        .append_pair("client_id", id)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", REDIRECT_URI)
        .append_pair("scope", SCOPES)
        .append_pair("state", &state)
        .append_pair("code_challenge_method", "S256")
        .append_pair("code_challenge", &challenge(&verifier));
    std::process::Command::new("open").arg(u.as_str()).spawn().map_err(|e| e.to_string())?;

    // Accept connections until the real callback arrives (browsers may also ask for /favicon.ico).
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_secs(300);
    while Instant::now() < deadline {
        let Ok((mut stream, _)) = listener.accept() else {
            std::thread::sleep(Duration::from_millis(200));
            continue;
        };
        let _ = stream.set_nonblocking(false);
        let mut line = String::new();
        let _ = BufReader::new(&stream).read_line(&mut line);
        let path = line.split_whitespace().nth(1).unwrap_or("");
        let Some(query) = path.strip_prefix("/callback?") else {
            let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            continue;
        };
        let result = match (query_param(query, "code"), query_param(query, "state")) {
            (Some(code), Some(s)) if s == state => token_request(&[
                ("grant_type", "authorization_code"),
                ("code", &code),
                ("redirect_uri", REDIRECT_URI),
                ("client_id", id),
                ("code_verifier", &verifier),
            ])
            .map(|_| ()),
            (_, Some(s)) if s != state => Err("This sign-in didn't come from this app".into()),
            _ => Err(query_param(query, "error").unwrap_or_else(|| "Spotify sign-in was cancelled".into())),
        };
        let msg = match &result {
            Ok(()) => "Spotify is connected. You can close this tab.",
            Err(_) => "Spotify sign-in failed. Close this tab and try again.",
        };
        let body = format!("<!doctype html><meta charset=utf-8><body style=\"font:20px system-ui;background:#000;color:#eee;padding:48px\">{msg}</body>");
        let _ = stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes());
        return result;
    }
    Err("Sign-in timed out".into())
}

/// Is this track in your Liked Songs? Uses `/me/library/contains`: Spotify
/// removed `/me/tracks` for development-mode apps in February 2026.
pub fn is_liked(id: &str) -> Result<bool, String> {
    let token = access_token()?;
    let mut res = ureq::get(&format!("https://api.spotify.com/v1/me/library/contains?uris=spotify:track:{id}"))
        .header("Authorization", &format!("Bearer {token}"))
        .call()
        .map_err(|e| e.to_string())?;
    let v: Value = res.body_mut().read_json().map_err(|e| e.to_string())?;
    Ok(v[0].as_bool().unwrap_or(false))
}

/// Like (save to Liked Songs) or unlike a track.
pub fn set_liked(id: &str, liked: bool) -> Result<(), String> {
    let token = access_token()?;
    let url = format!("https://api.spotify.com/v1/me/library?uris=spotify:track:{id}");
    let auth = format!("Bearer {token}");
    let res = if liked { ureq::put(&url).header("Authorization", &auth).send_empty() } else { ureq::delete(&url).header("Authorization", &auth).call() };
    res.map(|_| ()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_challenge_is_base64url_sha256() {
        assert_eq!(challenge("abc123-test-verifier"), "WyHqlGJe1VdXe6jlbGis0Bhv3KK9bgTczenQ05HIBWc");
    }

    #[test]
    fn track_ids_only_for_tracks() {
        assert_eq!(track_id("spotify:track:4uLU6hMCjMI75M1A2tKUQC").as_deref(), Some("4uLU6hMCjMI75M1A2tKUQC"));
        assert_eq!(track_id("spotify:episode:abc"), None);
        assert_eq!(track_id("spotify:track:../me"), None);
        assert_eq!(query_param("code=a%20b&state=x", "code").as_deref(), Some("a b"));
    }
}
