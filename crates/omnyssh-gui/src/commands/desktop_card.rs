//! Native window lifecycle for the compact, always-on-top dashboard card.

use serde::Serialize;
use tauri::{AppHandle, Emitter, LogicalSize, Manager, State, WebviewUrl, WebviewWindowBuilder};

use crate::error::CommandError;
use crate::state::GuiState;

pub const DESKTOP_CARD_WINDOW_LABEL: &str = "desktop-card";
// Tauri joins this against `devUrl` in development and its app protocol in release.
// Including `index.html` works only for packaged assets; SvelteKit dev has no such route.
const DESKTOP_CARD_APP_PATH: &str = "?view=desktop-card";
const OPEN_HOST_EVENT: &str = "desktop-card-open-host";
const EXPANDED_WIDTH: f64 = 380.0;
const EXPANDED_HEIGHT: f64 = 310.0;
const DOTS_HEIGHT: f64 = 48.0;
const DOTS_MIN_WIDTH: f64 = 72.0;
const DOTS_MAX_WIDTH: f64 = EXPANDED_WIDTH;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct OpenHostPayload {
    host_name: String,
    kind: String,
}

#[tauri::command]
#[specta::specta]
pub async fn show_desktop_card(app: AppHandle) -> Result<(), CommandError> {
    if let Some(window) = app.get_webview_window(DESKTOP_CARD_WINDOW_LABEL) {
        window.show().map_err(command_error)?;
        window.unminimize().map_err(command_error)?;
        window.set_focus().map_err(command_error)?;
        return Ok(());
    }

    WebviewWindowBuilder::new(
        &app,
        DESKTOP_CARD_WINDOW_LABEL,
        WebviewUrl::App(DESKTOP_CARD_APP_PATH.into()),
    )
    .title("OmnySSH Desktop Card")
    .inner_size(EXPANDED_WIDTH, EXPANDED_HEIGHT)
    .min_inner_size(DOTS_MIN_WIDTH, DOTS_HEIGHT)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    // The operating-system shadow is rectangular for a frameless transparent
    // window on Windows. The card draws its own soft, rounded shadow instead.
    .shadow(false)
    .visible(false)
    .center()
    .build()
    .map_err(command_error)?;
    Ok(())
}

/// Resize the native window as well as changing the webview layout. A transparent
/// Tauri webview clips content at its native bounds, so the dot-only mode cannot be
/// implemented by hiding the expanded card with CSS alone.
#[tauri::command]
#[specta::specta]
pub async fn set_desktop_card_compact(
    app: AppHandle,
    compact: bool,
    host_count: u32,
) -> Result<(), CommandError> {
    let window = app
        .get_webview_window(DESKTOP_CARD_WINDOW_LABEL)
        .ok_or_else(|| CommandError::new("desktop-card", "desktop card window is not open"))?;
    let (width, height) = if compact {
        (compact_width(host_count), DOTS_HEIGHT)
    } else {
        (EXPANDED_WIDTH, EXPANDED_HEIGHT)
    };
    window
        .set_size(LogicalSize::new(width, height))
        .map_err(command_error)
}

fn compact_width(host_count: u32) -> f64 {
    // 32 px per accessible dot target plus 24 px horizontal breathing room. The
    // switcher scrolls without a visible bar when more hosts exceed the cap.
    (f64::from(host_count.max(1)) * 32.0 + 24.0).clamp(DOTS_MIN_WIDTH, DOTS_MAX_WIDTH)
}

#[tauri::command]
#[specta::specta]
pub async fn set_desktop_card_always_on_top(
    app: AppHandle,
    always_on_top: bool,
) -> Result<bool, CommandError> {
    let window = app
        .get_webview_window(DESKTOP_CARD_WINDOW_LABEL)
        .ok_or_else(|| CommandError::new("desktop-card", "desktop card window is not open"))?;
    window
        .set_always_on_top(always_on_top)
        .map_err(command_error)?;
    window.is_always_on_top().map_err(command_error)
}

/// Bring the main window forward and ask it to open the selected host. Sessions
/// must live in the main webview: creating one in the compact card would leave an
/// invisible terminal/SFTP tab behind when the card closes.
#[tauri::command]
#[specta::specta]
pub async fn open_desktop_card_host(
    app: AppHandle,
    state: State<'_, GuiState>,
    host_name: String,
    kind: String,
) -> Result<(), CommandError> {
    if !matches!(kind.as_str(), "terminal" | "sftp") {
        return Err(CommandError::new(
            "desktop-card",
            format!("unsupported desktop card action '{kind}'"),
        ));
    }
    if state.host_by_name(&host_name).is_none() {
        return Err(CommandError::new(
            "desktop-card",
            format!("unknown host '{host_name}'"),
        ));
    }

    let main = app
        .get_webview_window("main")
        .ok_or_else(|| CommandError::new("desktop-card", "main window is not available"))?;
    main.show().map_err(command_error)?;
    main.unminimize().map_err(command_error)?;
    main.set_focus().map_err(command_error)?;
    main.emit(OPEN_HOST_EVENT, OpenHostPayload { host_name, kind })
        .map_err(command_error)
}

#[tauri::command]
#[specta::specta]
pub async fn close_desktop_card(app: AppHandle) -> Result<(), CommandError> {
    if let Some(window) = app.get_webview_window(DESKTOP_CARD_WINDOW_LABEL) {
        window.close().map_err(command_error)?;
    }
    Ok(())
}

fn command_error(error: impl ToString) -> CommandError {
    CommandError::new("desktop-card", error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_card_has_a_stable_secondary_window_label() {
        assert_eq!(DESKTOP_CARD_WINDOW_LABEL, "desktop-card");
    }

    #[test]
    fn desktop_card_targets_the_app_root_in_dev_and_release() {
        assert_eq!(DESKTOP_CARD_APP_PATH, "?view=desktop-card");
    }

    #[test]
    fn desktop_card_host_actions_use_a_stable_main_window_event() {
        assert_eq!(OPEN_HOST_EVENT, "desktop-card-open-host");
    }

    #[test]
    fn compact_desktop_card_width_tracks_hosts_with_safe_bounds() {
        assert_eq!(compact_width(0), DOTS_MIN_WIDTH);
        assert_eq!(compact_width(1), DOTS_MIN_WIDTH);
        assert_eq!(compact_width(2), 88.0);
        assert_eq!(compact_width(100), DOTS_MAX_WIDTH);
    }
}
