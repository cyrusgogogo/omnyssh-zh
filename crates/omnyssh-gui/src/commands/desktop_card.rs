//! Native window lifecycle for the compact, always-on-top dashboard card.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};

use crate::error::CommandError;
use crate::state::GuiState;

pub const DESKTOP_CARD_WINDOW_LABEL: &str = "desktop-card";
// Tauri joins this against `devUrl` in development and its app protocol in release.
// Including `index.html` works only for packaged assets; SvelteKit dev has no such route.
const DESKTOP_CARD_APP_PATH: &str = "?view=desktop-card";
const OPEN_HOST_EVENT: &str = "desktop-card-open-host";

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
    .inner_size(380.0, 310.0)
    .min_inner_size(320.0, 270.0)
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
}
