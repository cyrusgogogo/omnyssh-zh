//! Native window lifecycle for the compact, always-on-top dashboard card.

use std::future::Future;

use serde::Serialize;
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, State, WebviewUrl,
    WebviewWindowBuilder,
};

use super::settings::{load_terminal_open_mode, open_system_terminal};
use crate::dto::HostRuntimeSnapshotDto;
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

#[derive(Debug, PartialEq, Eq)]
enum DesktopCardHostTarget {
    MainWindow,
    SystemTerminal,
}

fn desktop_card_host_target(kind: &str, terminal_open_mode: &str) -> DesktopCardHostTarget {
    if kind == "terminal" && terminal_open_mode == "system" {
        DesktopCardHostTarget::SystemTerminal
    } else {
        DesktopCardHostTarget::MainWindow
    }
}

async fn dispatch_desktop_card_host_target<SystemAction, MainWindowAction>(
    target: DesktopCardHostTarget,
    system_action: SystemAction,
    main_window_action: MainWindowAction,
) -> Result<(), CommandError>
where
    SystemAction: Future<Output = Result<(), CommandError>>,
    MainWindowAction: Future<Output = Result<(), CommandError>>,
{
    match target {
        DesktopCardHostTarget::SystemTerminal => system_action.await,
        DesktopCardHostTarget::MainWindow => main_window_action.await,
    }
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
    expanded_above: bool,
) -> Result<bool, CommandError> {
    let window = app
        .get_webview_window(DESKTOP_CARD_WINDOW_LABEL)
        .ok_or_else(|| CommandError::new("desktop-card", "desktop card window is not open"))?;
    let (width, height) = if compact {
        (compact_width(host_count), DOTS_HEIGHT)
    } else {
        (EXPANDED_WIDTH, EXPANDED_HEIGHT)
    };

    let scale = window.scale_factor().map_err(command_error)?;
    let compact_height = logical_to_physical(DOTS_HEIGHT, scale);
    let expanded_height = logical_to_physical(EXPANDED_HEIGHT, scale);
    let was_compact = window.inner_size().map_err(command_error)?.height <= compact_height + 2;
    let mut opens_above = false;

    if compact && expanded_above {
        let position = window.outer_position().map_err(command_error)?;
        let offset = expanded_height.saturating_sub(compact_height) as i32;
        window
            .set_position(PhysicalPosition::new(position.x, position.y + offset))
            .map_err(command_error)?;
    } else if !compact && was_compact {
        let position = window.outer_position().map_err(command_error)?;
        if let Some(monitor) = window.current_monitor().map_err(command_error)? {
            opens_above = should_expand_above(
                position.y,
                compact_height,
                expanded_height,
                monitor.position().y,
                monitor.size().height,
            );
        }
        if opens_above {
            let offset = expanded_height.saturating_sub(compact_height) as i32;
            window
                .set_position(PhysicalPosition::new(position.x, position.y - offset))
                .map_err(command_error)?;
        }
    }
    window
        .set_size(LogicalSize::new(width, height))
        .map_err(command_error)?;
    Ok(opens_above)
}

#[tauri::command]
#[specta::specta]
pub async fn get_desktop_card_snapshot(
    state: State<'_, GuiState>,
) -> Result<Vec<HostRuntimeSnapshotDto>, CommandError> {
    Ok(state.host_runtime_snapshots())
}

fn compact_width(host_count: u32) -> f64 {
    // 32 px per accessible dot target plus 24 px horizontal breathing room. The
    // switcher scrolls without a visible bar when more hosts exceed the cap.
    (f64::from(host_count.max(1)) * 32.0 + 24.0).clamp(DOTS_MIN_WIDTH, DOTS_MAX_WIDTH)
}

fn logical_to_physical(value: f64, scale: f64) -> u32 {
    (value * scale).round().max(1.0) as u32
}

fn should_expand_above(
    window_top: i32,
    compact_height: u32,
    expanded_height: u32,
    monitor_top: i32,
    monitor_height: u32,
) -> bool {
    let required = i64::from(expanded_height.saturating_sub(compact_height));
    let top_space = i64::from(window_top) - i64::from(monitor_top);
    let monitor_bottom = i64::from(monitor_top) + i64::from(monitor_height);
    let compact_bottom = i64::from(window_top) + i64::from(compact_height);
    let bottom_space = monitor_bottom - compact_bottom;
    bottom_space < required && top_space >= required
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

/// Open the selected host without creating sessions in the compact-card webview.
/// System-terminal actions can launch directly; embedded terminals and SFTP must
/// live in the main webview so closing the card cannot leave an invisible tab.
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

    let target = if kind == "terminal" {
        let mode = load_terminal_open_mode().await?;
        desktop_card_host_target(&kind, &mode)
    } else {
        DesktopCardHostTarget::MainWindow
    };
    let system_host_name = host_name.clone();
    dispatch_desktop_card_host_target(
        target,
        open_system_terminal(state, system_host_name),
        async move {
            let main = app
                .get_webview_window("main")
                .ok_or_else(|| CommandError::new("desktop-card", "main window is not available"))?;
            main.show().map_err(command_error)?;
            main.unminimize().map_err(command_error)?;
            main.set_focus().map_err(command_error)?;
            main.emit(OPEN_HOST_EVENT, OpenHostPayload { host_name, kind })
                .map_err(command_error)
        },
    )
    .await
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
    use std::cell::Cell;

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
    fn desktop_card_routes_system_terminal_only_for_terminal_actions() {
        assert_eq!(
            desktop_card_host_target("terminal", "system"),
            DesktopCardHostTarget::SystemTerminal
        );
        assert_eq!(
            desktop_card_host_target("terminal", "default"),
            DesktopCardHostTarget::MainWindow
        );
        assert_eq!(
            desktop_card_host_target("sftp", "system"),
            DesktopCardHostTarget::MainWindow
        );
    }

    #[tokio::test]
    async fn desktop_card_system_terminal_bypasses_the_main_window() {
        let system_calls = Cell::new(0);
        let main_window_calls = Cell::new(0);

        dispatch_desktop_card_host_target(
            DesktopCardHostTarget::SystemTerminal,
            async {
                system_calls.set(system_calls.get() + 1);
                Ok(())
            },
            async {
                main_window_calls.set(main_window_calls.get() + 1);
                Ok(())
            },
        )
        .await
        .expect("system terminal action should succeed");

        assert_eq!(system_calls.get(), 1);
        assert_eq!(main_window_calls.get(), 0);
    }

    #[test]
    fn compact_desktop_card_width_tracks_hosts_with_safe_bounds() {
        assert_eq!(compact_width(0), DOTS_MIN_WIDTH);
        assert_eq!(compact_width(1), DOTS_MIN_WIDTH);
        assert_eq!(compact_width(2), 88.0);
        assert_eq!(compact_width(100), DOTS_MAX_WIDTH);
    }

    #[test]
    fn compact_card_expands_below_by_default() {
        assert!(!should_expand_above(120, 48, 310, 0, 1080));
    }

    #[test]
    fn compact_card_expands_above_when_near_the_monitor_bottom() {
        assert!(should_expand_above(1000, 48, 310, 0, 1080));
        // Monitor coordinates can be negative in a multi-monitor layout.
        assert!(should_expand_above(-100, 48, 310, -1080, 1080));
    }

    #[test]
    fn compact_card_keeps_the_default_when_neither_side_has_room() {
        assert!(!should_expand_above(100, 48, 310, 0, 240));
    }
}
