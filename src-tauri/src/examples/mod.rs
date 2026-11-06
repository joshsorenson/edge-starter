//! Examples you can keep, copy or delete: now playing (Spotify / Apple Music)
//! with a Spotify like button, Apple Reminders, Apple Photos, weather, and
//! this Mac's CPU / memory / network. Each module stands on its own; the
//! Tauri commands the page calls are at the bottom of this file, and the
//! matching UI is `ui/examples.html` + `ui/examples/*.js`.
//!
//! To drop one, delete its module, its commands here and its entries in
//! `lib.rs`'s `generate_handler!`, plus any permission keys it added to
//! Info.plist / Entitlements.plist.

pub mod now_playing;
pub mod photos;
pub mod reminders;
pub mod spotify;
pub mod system;
pub mod weather;

use serde_json::{json, Value};

/// Run blocking work (AppleScript, EventKit, HTTP) off the main thread.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())
}

// ── Now playing + Spotify ──────────────────────────────────────────────────

#[tauri::command]
pub async fn now_playing() -> Result<Option<now_playing::NowPlaying>, String> {
    blocking(now_playing::now_playing).await
}

/// `app`: "Spotify" | "Music"; `action`: "playpause" | "next" | "previous".
#[tauri::command]
pub async fn media_control(app: String, action: String) -> Result<(), String> {
    blocking(move || now_playing::control(&app, &action)).await?
}

#[tauri::command]
pub fn spotify_status() -> spotify::Status {
    spotify::status()
}

/// Opens Spotify sign-in in the browser; resolves once you've signed in.
#[tauri::command]
pub async fn spotify_login() -> Result<(), String> {
    blocking(spotify::login).await?
}

#[tauri::command]
pub async fn spotify_liked(id: String) -> Result<bool, String> {
    if spotify::track_id(&format!("spotify:track:{id}")).is_none() {
        return Err("not a Spotify track id".into());
    }
    blocking(move || spotify::is_liked(&id)).await?
}

#[tauri::command]
pub async fn spotify_set_liked(id: String, liked: bool) -> Result<(), String> {
    if spotify::track_id(&format!("spotify:track:{id}")).is_none() {
        return Err("not a Spotify track id".into());
    }
    blocking(move || spotify::set_liked(&id, liked)).await?
}

// ── Reminders ──────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn reminders_list() -> Result<Value, String> {
    blocking(|| json!({ "access": reminders::access(), "reminders": reminders::list() })).await
}

#[tauri::command]
pub async fn reminders_request_access() -> Result<reminders::Access, String> {
    blocking(reminders::request_access).await
}

#[tauri::command]
pub async fn reminder_set_completed(id: String, completed: bool) -> Result<(), String> {
    blocking(move || reminders::set_completed(&id, completed)).await?
}

/// Adds to your default Reminders list.
#[tauri::command]
pub async fn reminder_add(title: String) -> Result<reminders::Reminder, String> {
    let title: String = title.trim().chars().take(300).collect();
    if title.is_empty() {
        return Err("title is required".into());
    }
    blocking(move || reminders::add(&title)).await?
}

// ── Photos ─────────────────────────────────────────────────────────────────

/// A random Favorite (or a recent photo) as a data URL, plus access state.
#[tauri::command]
pub async fn photos_random() -> Result<Value, String> {
    blocking(|| {
        let access = photos::access();
        let cache = dirs::cache_dir().unwrap_or_default().join("edge-starter");
        let photo = if access == photos::Access::Granted { photos::random(&cache)? } else { None };
        Ok::<_, String>(json!({ "access": access, "photo": photo }))
    })
    .await?
}

#[tauri::command]
pub async fn photos_request_access() -> Result<photos::Access, String> {
    blocking(photos::request_access).await
}

/// Mark or unmark a Favorite in Photos. `asset` is the photo's `asset` field.
#[tauri::command]
pub async fn photo_set_favorite(asset: String, favorite: bool) -> Result<(), String> {
    if !photos::valid_asset_id(&asset) {
        return Err("not a Photos id".into());
    }
    blocking(move || photos::set_favorite(&asset, favorite)).await?
}

// ── Weather + system ───────────────────────────────────────────────────────

#[tauri::command]
pub async fn weather() -> Result<weather::Weather, String> {
    blocking(weather::fetch).await?
}

#[tauri::command]
pub fn system_snapshot() -> system::Snapshot {
    system::snapshot()
}
