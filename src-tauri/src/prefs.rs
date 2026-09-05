//! Device-local window preferences (v1.3 B3).
//!
//! Stored in `device-settings.json` next to the SQLite database. These are
//! PER-DEVICE settings (window position, always-on-top, collapsed state) —
//! they never enter SQLite and never participate in future sync (roadmap §5.3).

use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, PhysicalPosition};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MiniWindowPrefs {
    pub x: Option<i32>,
    pub y: Option<i32>,
    pub always_on_top: bool,
    pub collapsed: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DeviceSettings {
    pub mini: MiniWindowPrefs,
}

fn device_settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("device-settings.json"))
        .map_err(|e| format!("app data dir unavailable: {e}"))
}

pub fn load_device_settings(app: &AppHandle) -> DeviceSettings {
    let Ok(path) = device_settings_path(app) else {
        return DeviceSettings::default();
    };
    fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save_device_settings(app: &AppHandle, settings: &DeviceSettings) -> Result<(), String> {
    let path = device_settings_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

/// Clamps a saved position back into the visible area: if the point is not
/// inside any connected monitor (e.g. the external screen was unplugged), the
/// window falls back to the primary monitor's top-left work area.
pub fn clamp_position(
    app: &AppHandle,
    x: i32,
    y: i32,
    window_w: i32,
    window_h: i32,
) -> (i32, i32) {
    let monitors = match app.available_monitors() {
        Ok(list) => list,
        Err(_) => return (x, y),
    };
    let inside = monitors.iter().any(|m| {
        let pos = m.position();
        let size = m.size();
        // Require the window's title area (top-left corner + some extent) to
        // be on a monitor so it is actually reachable.
        x >= pos.x - window_w / 2
            && y >= pos.y - window_h / 2
            && x < pos.x + size.width as i32
            && y < pos.y + size.height as i32
    });
    if inside {
        return (x, y);
    }
    // Fallback: top-left of the primary monitor (or first available).
    let primary = app.primary_monitor().ok().flatten();
    let fallback = primary
        .as_ref()
        .map(|m| monitors.iter().find(|m2| m2.name() == m.name()))
        .unwrap_or(None)
        .or_else(|| monitors.first());
    match fallback {
        Some(m) => (m.position().x + 40, m.position().y + 40),
        None => (x, y),
    }
}

/// Applies saved mini-window prefs (position + always-on-top). Does not show
/// the window — visibility stays under the user's tray/timer control.
pub fn apply_mini_prefs(app: &AppHandle, prefs: &MiniWindowPrefs) {
    if let Some(mini) = app.get_webview_window("mini") {
        if let (Some(x), Some(y)) = (prefs.x, prefs.y) {
            let (cx, cy) = clamp_position(app, x, y, 300, 140);
            let _ = mini.set_position(PhysicalPosition::new(cx, cy));
        }
        let _ = mini.set_always_on_top(prefs.always_on_top);
    }
}
