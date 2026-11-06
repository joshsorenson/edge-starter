//! Menu bar icon. Edge Starter lives here instead of the Dock: it waits for the
//! Edge, shows itself on it, and hides when the Edge is unplugged.
//!
//! The icon is `icons/tray.png`, a black-on-transparent "template" image that
//! macOS tints for light and dark menu bars. Add your own menu items below.

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, Wry};
use tauri_plugin_autostart::ManagerExt;

/// The status line at the top of the menu ("Waiting for the Edge", …).
static STATUS: std::sync::OnceLock<MenuItem<Wry>> = std::sync::OnceLock::new();

/// Update the status line, e.g. when the Edge is plugged in or out.
pub fn set_status(text: &str) {
    if let Some(item) = STATUS.get() {
        let _ = item.set_text(text);
    }
}

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let status = MenuItem::with_id(app, "status", "Waiting for the Edge", false, None::<&str>)?;
    let show = MenuItem::with_id(app, "show", "Show on Edge", true, None::<&str>)?;
    let at_login = app.autolaunch().is_enabled().unwrap_or(false);
    let login = CheckMenuItem::with_id(app, "login", "Open at login", true, at_login, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Edge Starter", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[&status, &PredefinedMenuItem::separator(app)?, &show, &login, &PredefinedMenuItem::separator(app)?, &quit],
    )?;
    let _ = STATUS.set(status);

    let login_item = login.clone();
    TrayIconBuilder::with_id("edge-starter")
        .icon(Image::from_bytes(include_bytes!("../icons/tray.png"))?)
        .icon_as_template(true)
        .tooltip("Edge Starter")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "show" => crate::show_on_edge(app),
            "login" => {
                let auto = app.autolaunch();
                let on = !auto.is_enabled().unwrap_or(false);
                let _ = if on { auto.enable() } else { auto.disable() };
                let _ = login_item.set_checked(auto.is_enabled().unwrap_or(on));
                crate::log(&format!("tray: open at login {}", if on { "on" } else { "off" }));
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}

/// Turn "Open at login" on the first time the app runs. Set to false if you'd
/// rather leave it off until the user ticks it in the menu.
pub const OPEN_AT_LOGIN_BY_DEFAULT: bool = true;

/// Turn on "Open at login" the first time Edge Starter runs, so it's always
/// waiting for the Edge. After that, the menu checkbox is the user's call.
pub fn default_open_at_login(app: &AppHandle) {
    let Ok(dir) = app.path().app_data_dir() else { return };
    let flag = dir.join("open-at-login-set");
    if flag.exists() {
        return;
    }
    let _ = std::fs::create_dir_all(&dir);
    if app.autolaunch().enable().is_ok() {
        crate::log("tray: turned on open at login (first run)");
    }
    let _ = std::fs::write(flag, b"1");
}
