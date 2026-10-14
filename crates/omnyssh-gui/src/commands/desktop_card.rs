//! Native window lifecycle for the compact, always-on-top dashboard card.

use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, State, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};
use tauri_plugin_store::StoreExt;
use tokio::sync::Mutex;

use super::settings::{load_terminal_open_mode, open_system_terminal};
use crate::desktop_card_motion::{offscreen_right, position_at, DURATION_MS};
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

#[derive(Default)]
pub struct DesktopCardState {
    // Serialize visibility, resizing and closing so they cannot overwrite a flight.
    operation: Mutex<()>,
    hidden: AtomicBool,
    moving: AtomicBool,
    // Keep the last on-screen origin while the hidden window stays offscreen.
    origin: std::sync::Mutex<Option<PhysicalPosition<i32>>>,
}

pub fn save_desktop_card_position(app: &AppHandle) {
    let state = app.state::<DesktopCardState>();
    let position = if state.hidden.load(Ordering::Acquire) || state.moving.load(Ordering::Acquire) {
        *state.origin.lock().unwrap()
    } else {
        app.get_webview_window(DESKTOP_CARD_WINDOW_LABEL)
            .and_then(|w| w.outer_position().ok())
    };
    if let (Some(position), Some(window), Ok(store)) = (
        position,
        app.get_webview_window(DESKTOP_CARD_WINDOW_LABEL),
        app.store("desktop-window.json"),
    ) {
        if let Ok(size) = window.inner_size() {
            // The next window starts expanded, even when the saved preference is
            // dots. Persist its centre so that the initial compact resize cannot
            // add half the width difference to its position on every restart.
            store.set(
                "geometry",
                serde_json::json!([position.x, position.y, size.width]),
            );
            let _ = store.save();
        }
    }
}

pub fn desktop_card_is_hidden(app: &AppHandle) -> bool {
    app.state::<DesktopCardState>()
        .hidden
        .load(Ordering::Acquire)
}

/// Clear visibility state after an OS-level close (for example Alt+F4).
pub async fn desktop_card_destroyed(app: AppHandle) -> Result<(), CommandError> {
    let state = app.state::<DesktopCardState>();
    let _operation = state.operation.lock().await;
    if app.get_webview_window(DESKTOP_CARD_WINDOW_LABEL).is_none() {
        reset_visibility(&state);
    }
    Ok(())
}

fn reset_visibility(state: &DesktopCardState) {
    state.hidden.store(false, Ordering::Release);
    state.moving.store(false, Ordering::Release);
    *state.origin.lock().unwrap() = None;
}

async fn fly(window: &WebviewWindow, from: i32, to: i32, y: i32) -> Result<(), CommandError> {
    let start = Instant::now();
    loop {
        let progress = start.elapsed().as_secs_f64() / (DURATION_MS as f64 / 1000.0);
        window
            .set_position(PhysicalPosition::new(position_at(from, to, progress), y))
            .map_err(command_error)?;
        if progress >= 1.0 {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(16)).await;
    }
}

async fn animate_visibility(
    window: &WebviewWindow,
    state: &DesktopCardState,
    visible: bool,
) -> Result<(), CommandError> {
    let origin = if visible {
        let remembered = *state.origin.lock().unwrap();
        match remembered {
            Some(position) => position,
            None => window.outer_position().map_err(command_error)?,
        }
    } else {
        let position = window.outer_position().map_err(command_error)?;
        *state.origin.lock().unwrap() = Some(position);
        save_desktop_card_position(window.app_handle());
        position
    };
    let monitors = window.available_monitors().map_err(command_error)?;
    let outside = offscreen_right(
        monitors
            .iter()
            .map(|monitor| (monitor.position().x, monitor.size().width)),
        origin.x,
    );
    state.moving.store(true, Ordering::Release);
    state.hidden.store(!visible, Ordering::Release);
    let result = async {
        if visible {
            window
                .set_position(PhysicalPosition::new(outside, origin.y))
                .map_err(command_error)?;
            window.unminimize().map_err(command_error)?;
            window.show().map_err(command_error)?;
            fly(window, outside, origin.x, origin.y).await?;
        } else {
            fly(window, origin.x, outside, origin.y).await?;
            window.hide().map_err(command_error)?;
            // Do not move a just-hidden native window back: hide is asynchronous
            // on some compositors and that move caused a flash at the origin.
        }
        Ok(())
    }
    .await;
    if result.is_err() {
        // A failed window-manager operation must leave a recoverable card.
        let _ = window.set_position(origin);
        let _ = window.show();
        state.hidden.store(false, Ordering::Release);
    }
    state.moving.store(false, Ordering::Release);
    result
}

#[tauri::command]
#[specta::specta]
pub async fn toggle_desktop_card_visibility(app: AppHandle) -> Result<(), CommandError> {
    let state = app.state::<DesktopCardState>();
    let _operation = state.operation.lock().await;
    if let Some(window) = app.get_webview_window(DESKTOP_CARD_WINDOW_LABEL) {
        animate_visibility(&window, &state, state.hidden.load(Ordering::Acquire)).await?;
    }
    Ok(())
}

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
    let state = app.state::<DesktopCardState>();
    let _operation = state.operation.lock().await;
    if let Some(window) = app.get_webview_window(DESKTOP_CARD_WINDOW_LABEL) {
        if state.hidden.load(Ordering::Acquire) {
            animate_visibility(&window, &state, true).await?;
        }
        window.show().map_err(command_error)?;
        window.unminimize().map_err(command_error)?;
        window.set_focus().map_err(command_error)?;
        return Ok(());
    }

    state.hidden.store(false, Ordering::Release);
    let result = WebviewWindowBuilder::new(
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
    .map_err(command_error);
    let window = result?;
    if let Ok(store) = app.store("desktop-window.json") {
        if let Some((left, top, width)) = store
            .get("geometry")
            .and_then(|v| serde_json::from_value::<(i32, i32, u32)>(v).ok())
        {
            let position = [
                anchored_window_left(
                    left,
                    width,
                    window.inner_size().map_err(command_error)?.width,
                ),
                top,
            ];
            // A disconnected monitor must not strand the card outside the desktop.
            if window
                .available_monitors()
                .unwrap_or_default()
                .iter()
                .any(|m| {
                    // A dot strip beside the left edge can temporarily place the
                    // expanded window beyond that edge; its centre is still valid.
                    let center = i64::from(left) + i64::from(width) / 2;
                    center >= i64::from(m.position().x)
                        && center < i64::from(m.position().x) + i64::from(m.size().width)
                        && i64::from(top) >= i64::from(m.position().y)
                        && i64::from(top) + 48
                            <= i64::from(m.position().y) + i64::from(m.size().height)
                })
            {
                window
                    .set_position(PhysicalPosition::new(position[0], position[1]))
                    .map_err(command_error)?;
            }
        }
    }
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
    let state = app.state::<DesktopCardState>();
    let _operation = state.operation.lock().await;
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
    let target_width = logical_to_physical(width, scale);
    let current_size = window.inner_size().map_err(command_error)?;
    let was_compact = current_size.height <= compact_height + 2;
    let mut opens_above = expanded_above && !compact && !was_compact;

    if compact || was_compact {
        let position = if state.hidden.load(Ordering::Acquire) {
            let remembered = *state.origin.lock().unwrap();
            match remembered {
                Some(position) => position,
                None => window.outer_position().map_err(command_error)?,
            }
        } else {
            window.outer_position().map_err(command_error)?
        };
        if !compact {
            if let Some(monitor) = window
                .available_monitors()
                .map_err(command_error)?
                .into_iter()
                .find(|m| {
                    i64::from(position.x) >= i64::from(m.position().x)
                        && i64::from(position.x)
                            < i64::from(m.position().x) + i64::from(m.size().width)
                        && i64::from(position.y) >= i64::from(m.position().y)
                        && i64::from(position.y)
                            < i64::from(m.position().y) + i64::from(m.size().height)
                })
            {
                opens_above = should_expand_above(
                    position.y,
                    compact_height,
                    expanded_height,
                    monitor.position().y,
                    monitor.size().height,
                );
            }
        }
        let target_y = if compact && expanded_above {
            position.y + expanded_height.saturating_sub(compact_height) as i32
        } else if opens_above {
            position.y - expanded_height.saturating_sub(compact_height) as i32
        } else {
            position.y
        };
        let target = PhysicalPosition::new(
            anchored_window_left(position.x, current_size.width, target_width),
            target_y,
        );
        if state.hidden.load(Ordering::Acquire) {
            *state.origin.lock().unwrap() = Some(target);
        } else {
            window.set_position(target).map_err(command_error)?;
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

fn anchored_window_left(window_left: i32, current_width: u32, target_width: u32) -> i32 {
    window_left + (current_width as i32 - target_width as i32) / 2
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
    let state = app.state::<DesktopCardState>();
    let _operation = state.operation.lock().await;
    save_desktop_card_position(&app);
    if let Some(window) = app.get_webview_window(DESKTOP_CARD_WINDOW_LABEL) {
        window.close().map_err(command_error)?;
    }
    // Destroyed clears visibility after CloseRequested has saved the visible origin.
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
    fn compact_position_survives_reopening_through_an_expanded_window() {
        for (left, compact_width, expanded_width) in
            [(-1700, 88, 380), (500, 110, 475), (-1234, 97, 418)]
        {
            let restored = anchored_window_left(left, compact_width, expanded_width);
            assert_eq!(
                anchored_window_left(restored, expanded_width, compact_width),
                left
            );
        }
    }

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

    #[test]
    fn resizing_the_card_keeps_the_host_dots_horizontally_anchored() {
        assert_eq!(anchored_window_left(300, 152, 380), 186);
        assert_eq!(anchored_window_left(186, 380, 152), 300);
    }
}
