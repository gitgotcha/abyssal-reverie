//! System tray: icon + context menu + left-click-to-show.
//!
//! The tray is the app's only persistent surface when the main window is
//! hidden. It is built once in `lib::run` setup and updated in place by the
//! `set_tray_indicator` command, which the frontend calls every second while
//! running (and on every state change) so the tooltip and menu labels reflect
//! the live remaining time.

use tauri::{
    menu::{Menu, MenuId, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, Wry,
};

use crate::repository;

/// Stable menu-item ids. Frontend/Rust communicate tray actions via the
/// `tray-action` event; ids stay on the Rust side.
const ID_STATUS: &str = "tray_status";
const ID_TOGGLE: &str = "tray_toggle";
const ID_RESET: &str = "tray_reset";
const ID_SHOW: &str = "tray_show";
const ID_TOGGLE_MINI: &str = "tray_toggle_mini";
const ID_QUIT: &str = "tray_quit";

/// Tray action broadcast to the frontend so it can reuse its existing
/// pause/resume/reset handlers (which own the revision + optimistic-concurrency
/// flow). `Show` and `Quit` are handled entirely in Rust.
#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TrayAction {
    Toggle,
    Reset,
}

/// Handles held in managed state so `set_tray_indicator` can update labels
/// without rebuilding the menu (rebuilding flickers on Windows).
pub struct TrayHandles {
    status: MenuItem<Wry>,
    toggle: MenuItem<Wry>,
    tray: TrayIcon,
}

impl TrayHandles {
    /// Borrows the managed tray handles, if already built. Returns `None`
    /// during the brief window before `build_tray` completes.
    pub fn get(app: &AppHandle) -> Option<&Self> {
        app.try_state::<Self>().map(|s| s.inner())
    }
}

/// Builds the tray icon and its menu, wires menu + icon events, and manages
/// `TrayHandles` so later indicator updates are O(1).
pub fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let status = MenuItem::with_id(app, ID_STATUS, "Abyssal Reverie · 空闲", false, None::<&str>)?;
    let toggle = MenuItem::with_id(app, ID_TOGGLE, "开始专注", true, None::<&str>)?;
    let reset = MenuItem::with_id(app, ID_RESET, "结束本次", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let show = MenuItem::with_id(app, ID_SHOW, "显示主窗口", true, None::<&str>)?;
    let toggle_mini = MenuItem::with_id(app, ID_TOGGLE_MINI, "显示/隐藏小窗", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, ID_QUIT, "退出 Abyssal Reverie", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[&status, &toggle, &reset, &sep1, &show, &toggle_mini, &quit],
    )?;

    let icon = tauri::include_image!("icons/32x32.png");
    // Capture the app handle so the icon-click handler can show the window;
    // `TrayIcon` exposes no public app-handle accessor.
    let app_for_icon = app.clone();
    let tray = TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .tooltip("Abyssal Reverie · 空闲")
        // Left click shows the window; the menu opens on right click.
        .show_menu_on_left_click(false)
        .menu(&menu)
        .on_menu_event(on_menu_event)
        .on_tray_icon_event(move |_tray, event| {
            let show = match event {
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } => true,
                TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                } => true,
                _ => false,
            };
            if show {
                show_main_window(&app_for_icon);
            }
        })
        .build(app)?;

    app.manage(TrayHandles {
        status,
        toggle,
        tray,
    });
    Ok(())
}

/// Payload pushed from the frontend via `set_tray_indicator`.
#[derive(Debug, serde::Deserialize)]
pub struct TrayIndicator {
    pub tooltip: String,
    pub status_label: String,
    pub toggle_label: String,
}

/// Updates tooltip + status/toggle menu texts in place. No-op if the tray has
/// not been built yet (e.g. during early bootstrap races).
pub fn apply_indicator(app: &AppHandle, ind: &TrayIndicator) {
    if let Some(h) = TrayHandles::get(app) {
        let _ = h.tray.set_tooltip(Some(&ind.tooltip));
        let _ = h.status.set_text(&ind.status_label);
        let _ = h.toggle.set_text(&ind.toggle_label);
    }
}

fn on_menu_event(app: &AppHandle, event: tauri::menu::MenuEvent) {
    let id: &MenuId = event.id();
    match id.as_ref() {
        ID_TOGGLE => {
            let _ = app.emit("tray-action", TrayAction::Toggle);
        }
        ID_RESET => {
            let _ = app.emit("tray-action", TrayAction::Reset);
        }
        ID_SHOW => {
            show_main_window(app);
        }
        ID_TOGGLE_MINI => {
            // v1.3 B5: show/hide the mini window directly in Rust.
            if let Some(mini) = app.get_webview_window("mini") {
                if mini.is_visible().unwrap_or(false) {
                    let _ = mini.hide();
                } else {
                    let _ = mini.show();
                }
            }
        }
        ID_QUIT => {
            // User rule: if a focus session is running, freeze it as paused
            // before exit so focus time is not silently consumed on the next
            // launch (the user resumes manually). A non-running timer is left
            // untouched. The WAL is checkpointed inside the call so the change
            // survives the imminent process exit.
            if let Some(state) = app.try_state::<crate::AppState>() {
                if let Ok(conn) = state.db.lock() {
                    let _ = repository::persist_running_as_paused(&conn);
                }
            }
            app.exit(0);
        }
        _ => {}
    }
}

/// Shows + focuses the main window, creating it if it was closed (it is only
/// ever hidden, never destroyed, but be defensive).
fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}
