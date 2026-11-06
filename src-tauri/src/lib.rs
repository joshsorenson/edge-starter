//! Edge Starter: a minimal app for the Corsair XENEON Edge on macOS.
//!
//! What it does for you:
//! 1. Finds the Edge (the ~32:9 display) and pins a borderless window over it,
//!    even with a Retina main display at 2x and the Edge at 1x.
//! 2. Takes over the Edge's touchscreen so touches land in this window instead
//!    of clicking your main display (see touch.rs), and hands them to the page
//!    as `window.__edgeTouch("down" | "move" | "up", x, y)`.
//! 3. Puts the window back on the Edge if it's moved off it, and
//!    follows the Edge when it's unplugged and plugged back in.
//! 4. Lives in the menu bar (no Dock icon): waits for the Edge, shows itself on
//!    it, hides when it's unplugged, and can open at login (see tray.rs).
//!
//! Your UI lives in `ui/`. Start there.

mod examples;
mod touch;
mod tray;

use std::{path::PathBuf, sync::OnceLock, time::Duration};
use tauri::{LogicalPosition, LogicalSize, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

/// The Edge is 2560x720 (32:9). Any display at least this wide-to-tall counts.
pub const EDGE_MIN_ASPECT: f64 = 3.0;

static APP: OnceLock<tauri::AppHandle> = OnceLock::new();

fn log_path() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join("Library/Logs/edge-starter.log")
}

/// Append a line to ~/Library/Logs/edge-starter.log (and stderr). Handy for
/// checking touch permissions on a built app, where stderr isn't visible.
pub(crate) fn log(msg: &str) {
    use std::io::Write;
    eprintln!("edge-starter: {msg}");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(log_path()) {
        let _ = writeln!(f, "{} {msg}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
    }
}

/// Put the window over the Edge, edge to edge. Without the Edge, hide it: the
/// app waits in the menu bar until the Edge is plugged in.
///
/// This doesn't use macOS native fullscreen (its own Space): a menu bar app
/// can't hold it reliably, which leaves the window half-placed and touches
/// landing in the wrong spot. Instead the borderless window is pinned over
/// the Edge's exact bounds, above that display's menu bar, on every Space.
///
/// Positions use macOS point coordinates (`touch::edge_rect`). Physical
/// pixels go wrong with a 2x main display and the 1x Edge.
fn place(win: &WebviewWindow, edge: Option<touch::Rect>) {
    let _ = win.set_fullscreen(false);
    match edge {
        Some(r) => {
            pin_above_menu_bar(win);
            let _ = win.set_position(LogicalPosition::new(r.x, r.y));
            let _ = win.set_size(LogicalSize::new(r.w, r.h));
            let _ = win.show();
            log(&format!("window: pinned over the Edge at {},{} {}x{}", r.x, r.y, r.w, r.h));
        }
        None => {
            let _ = win.hide();
        }
    }
}

/// Is the window visible and covering exactly the Edge's bounds?
fn on_edge(win: &WebviewWindow, r: touch::Rect) -> bool {
    let sf = win.scale_factor().unwrap_or(1.0);
    let (Ok(p), Ok(sz)) = (win.outer_position(), win.outer_size()) else { return false };
    let near = |a: f64, b: f64| (a - b).abs() < 2.0;
    win.is_visible().unwrap_or(false)
        && near(p.x as f64 / sf, r.x)
        && near(p.y as f64 / sf, r.y)
        && near(sz.width as f64 / sf, r.w)
        && near(sz.height as f64 / sf, r.h)
}

/// Window level above the menu bar (so it covers the Edge's own menu bar),
/// shown on every Space, left alone by Mission Control.
fn pin_above_menu_bar(win: &WebviewWindow) {
    #[cfg(target_os = "macos")]
    if let Ok(ptr) = win.ns_window() {
        use objc2::msg_send;
        use objc2::runtime::AnyObject;
        // NSMainMenuWindowLevel is 24; one above it covers the menu bar.
        const LEVEL: isize = 25;
        // canJoinAllSpaces | stationary | fullScreenAuxiliary
        const BEHAVIOR: usize = (1 << 0) | (1 << 4) | (1 << 8);
        unsafe {
            let ns: &AnyObject = &*(ptr as *const AnyObject);
            let _: () = msg_send![ns, setLevel: LEVEL];
            let _: () = msg_send![ns, setCollectionBehavior: BEHAVIOR];
            let _: () = msg_send![ns, setHasShadow: false];
        }
    }
}

/// "Show on Edge" in the menu bar: put the window back on the Edge.
pub(crate) fn show_on_edge(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        place(&win, touch::edge_rect());
    }
}

/// Bring this app forward with its window focused, so the keyboard types into
/// the text field the user tapped. Called from ui/touch.js.
#[tauri::command]
fn focus_window() {
    if let Some(win) = APP.get().and_then(|a| a.get_webview_window("main")) {
        let _ = win.set_focus();
    }
}

/// "on", "needsPermission", "failed", … for showing touch status in the UI.
#[tauri::command]
fn touch_status() -> touch::TouchStatus {
    touch::status()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .invoke_handler(tauri::generate_handler![
            focus_window,
            touch_status,
            // Examples (src/examples): delete the ones you don't want.
            examples::now_playing,
            examples::media_control,
            examples::spotify_status,
            examples::spotify_login,
            examples::spotify_liked,
            examples::spotify_set_liked,
            examples::reminders_list,
            examples::reminders_request_access,
            examples::reminder_set_completed,
            examples::reminder_add,
            examples::photos_random,
            examples::photos_request_access,
            examples::photo_set_favorite,
            examples::weather,
            examples::system_snapshot,
        ])
        .setup(|app| {
            let _ = APP.set(app.handle().clone());

            // Example: start sampling CPU / memory / network for system_snapshot.
            examples::system::start();

            // Menu bar app: no Dock icon, waits for the Edge. Remove these
            // lines if you'd rather have a regular Dock app.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            if tray::OPEN_AT_LOGIN_BY_DEFAULT {
                tray::default_open_at_login(app.handle());
            }
            tray::build(app.handle())?;

            // Touches go straight into the page. The mouse never moves.
            let handle = app.handle().clone();
            touch::start(move |ev| {
                let Some(win) = handle.get_webview_window("main") else { return };
                let (kind, x, y) = match ev {
                    touch::TouchEvent::Down(x, y) => ("down", x, y),
                    touch::TouchEvent::Move(x, y) => ("move", x, y),
                    touch::TouchEvent::Up(x, y) => ("up", x, y),
                };
                let _ = win.eval(format!("window.__edgeTouch&&window.__edgeTouch('{kind}',{x:.1},{y:.1})"));
            });

            let win = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title("Edge Starter")
                .decorations(false)
                .background_color(tauri::window::Color(0, 0, 0, 255))
                .visible(false)
                .build()?;
            let edge = touch::edge_rect();
            if edge.is_none() {
                log("window: Edge not connected; waiting in the menu bar");
            }
            place(&win, edge);
            tray::set_status(if edge.is_some() { "On the Edge" } else { "Waiting for the Edge" });

            // Follow the Edge when it's plugged in, unplugged or rearranged, and
            // put the window back fullscreen if something knocked it out.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let mut last = edge;
                loop {
                    std::thread::sleep(Duration::from_secs(3));
                    let Some(win) = handle.get_webview_window("main") else { break };
                    let now = touch::edge_rect();
                    let knocked_out = now.is_some_and(|r| !on_edge(&win, r));
                    if now != last || knocked_out {
                        if now != last {
                            log(if now.is_some() { "window: Edge connected; showing" } else { "window: Edge unplugged; hiding" });
                            tray::set_status(if now.is_some() { "On the Edge" } else { "Waiting for the Edge" });
                        }
                        let _ = handle.run_on_main_thread(move || place(&win, now));
                        last = now;
                    }
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Edge Starter");
}
