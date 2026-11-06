//! Example: what's playing in Spotify or Apple Music, plus play/pause, next
//! and previous, using AppleScript.
//!
//! macOS doesn't let apps read the system "Now Playing" info, so this asks the
//! two players directly. Only players that are already running are asked, so
//! it never launches one. macOS asks once per player to allow control
//! (Automation). Needs NSAppleEventsUsageDescription in Info.plist and the
//! apple-events entitlement (hardened runtime).

use serde::Serialize;
use std::process::Command;

#[derive(Debug, Serialize, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NowPlaying {
    /// "Spotify" or "Music".
    pub app: String,
    pub playing: bool,
    pub title: String,
    pub artist: String,
    pub album: String,
    /// Cover art URL (Spotify only).
    pub artwork_url: Option<String>,
    /// Spotify track id, for liking the song (Spotify tracks only).
    pub track_id: Option<String>,
}

pub const PLAYERS: &[&str] = &["Spotify", "Music"];
const SEP: &str = "\u{1f}";

fn osascript(script: &str) -> Option<String> {
    let out = Command::new("osascript").arg("-e").arg(script).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn script_for(app: &str) -> String {
    let (art, id) = if app == "Spotify" { ("artwork url of current track", "id of current track") } else { ("\"\"", "\"\"") };
    format!(
        r#"if application "{app}" is running then
  tell application "{app}"
    if player state is stopped then return ""
    set s to (player state as text)
    return s & "{SEP}" & (name of current track) & "{SEP}" & (artist of current track) & "{SEP}" & (album of current track) & "{SEP}" & {art} & "{SEP}" & {id}
  end tell
end if
return """#
    )
}

/// Parse one player's AppleScript output. Empty means nothing loaded.
pub fn parse(app: &str, out: &str) -> Option<NowPlaying> {
    let parts: Vec<&str> = out.split(SEP).collect();
    if parts.len() < 5 {
        return None;
    }
    Some(NowPlaying {
        app: app.into(),
        playing: parts[0] == "playing",
        title: parts[1].into(),
        artist: parts[2].into(),
        album: parts[3].into(),
        artwork_url: Some(parts[4].to_string()).filter(|s| s.starts_with("http")),
        track_id: parts.get(5).and_then(|u| super::spotify::track_id(u)),
    })
}

/// What's playing. Prefers a player that is actually playing over a paused one.
pub fn now_playing() -> Option<NowPlaying> {
    let found: Vec<NowPlaying> = PLAYERS.iter().filter_map(|app| osascript(&script_for(app)).and_then(|o| parse(app, &o))).collect();
    found.iter().find(|n| n.playing).cloned().or_else(|| found.into_iter().next())
}

/// Play/pause, next or previous on `app`.
pub fn control(app: &str, action: &str) -> Result<(), String> {
    if !PLAYERS.contains(&app) {
        return Err("unknown player".into());
    }
    let cmd = match action {
        "playpause" => "playpause",
        "next" => "next track",
        "previous" => "previous track",
        _ => return Err("unknown action".into()),
    };
    osascript(&format!(r#"if application "{app}" is running then tell application "{app}" to {cmd}"#))
        .map(|_| ())
        .ok_or_else(|| format!("{app} didn't respond"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_spotify_output() {
        let out = format!("playing{SEP}Song{SEP}Artist{SEP}Album{SEP}https://i.scdn.co/image/x{SEP}spotify:track:4uLU6hMCjMI75M1A2tKUQC");
        let n = parse("Spotify", &out).unwrap();
        assert!(n.playing);
        assert_eq!((n.title.as_str(), n.track_id.as_deref()), ("Song", Some("4uLU6hMCjMI75M1A2tKUQC")));
        assert_eq!(parse("Music", ""), None);
    }
}
