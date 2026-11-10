//! Transient SFTP windows. Native destruction owns connection cleanup, including
//! connects that finish after the webview has already gone away.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use omnyssh_core::ssh::sftp::SftpManager;
use tauri::{AppHandle, Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::error::CommandError;
use crate::state::GuiState;

const WINDOW_PREFIX: &str = "sftp-card-";

#[derive(Default)]
pub struct SftpCardState {
    operation: tokio::sync::Mutex<()>,
    next_id: AtomicU64,
    windows: Mutex<HashMap<String, CardWindow>>,
}

struct CardWindow {
    host_name: String,
    sessions: HashSet<u64>,
    focused: bool,
    ready: bool,
}

pub fn is_sftp_card(label: &str) -> bool {
    label.starts_with(WINDOW_PREFIX)
}

impl SftpCardState {
    pub fn register_session(
        &self,
        label: &str,
        state: &GuiState,
        manager: SftpManager,
    ) -> Result<u64, CommandError> {
        let mut windows = self.windows.lock().unwrap();
        let card = windows
            .get_mut(label)
            .ok_or_else(|| error("SFTP card was closed"))?;
        let id = state.register_sftp(manager);
        card.sessions.insert(id);
        Ok(id)
    }

    pub fn focus_changed(&self, label: &str, focused: bool) -> bool {
        let mut windows = self.windows.lock().unwrap();
        let Some(card) = windows.get_mut(label) else {
            return false;
        };
        // WebView2 can transfer focus during creation, before its content has
        // mounted. Such a transition must never destroy the opening window.
        let should_close = card.ready && card.focused && !focused;
        card.focused = focused;
        should_close
    }

    pub fn destroyed(&self, label: &str, state: &GuiState) {
        if let Some(card) = self.windows.lock().unwrap().remove(label) {
            for id in card.sessions {
                state.close_sftp(id);
            }
        }
    }
}

fn app_path(host_name: &str) -> Result<String, CommandError> {
    let url = tauri::Url::parse_with_params(
        "https://localhost/",
        [("view", "sftp-card"), ("host", host_name)],
    )
    .map_err(error)?;
    Ok(format!("?{}", url.query().unwrap_or_default()))
}

#[tauri::command]
#[specta::specta]
pub async fn open_sftp_card(
    app: AppHandle,
    state: State<'_, GuiState>,
    host_name: String,
) -> Result<(), CommandError> {
    if state.host_by_name(&host_name).is_none() {
        return Err(error(format!("unknown host '{host_name}'")));
    }
    let cards = app.state::<SftpCardState>();
    let _operation = cards.operation.lock().await;
    let existing = cards
        .windows
        .lock()
        .unwrap()
        .iter()
        .find(|(_, card)| card.host_name == host_name)
        .map(|(label, _)| label.clone());
    if let Some(window) = existing.and_then(|label| app.get_webview_window(&label)) {
        window.show().map_err(error)?;
        window.unminimize().map_err(error)?;
        return window.set_focus().map_err(error);
    }

    let label = format!(
        "{WINDOW_PREFIX}{}",
        cards.next_id.fetch_add(1, Ordering::Relaxed)
    );
    let path = app_path(&host_name)?;
    cards.windows.lock().unwrap().insert(
        label.clone(),
        CardWindow {
            host_name: host_name.clone(),
            sessions: HashSet::new(),
            focused: false,
            ready: false,
        },
    );
    let result = WebviewWindowBuilder::new(&app, &label, WebviewUrl::App(path.into()))
        .title(format!("{host_name} · SFTP — OmnySSH"))
        .inner_size(1080.0, 640.0)
        .min_inner_size(720.0, 420.0)
        .resizable(true)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .center()
        .build();
    let window = result.map_err(|build_error| {
        cards.destroyed(&label, &state);
        error(build_error)
    })?;
    // Do not rely on a page-load callback to reveal a newly created WebView2
    // window. The callback may run before the window is registered with Tauri.
    if let Err(show_error) = window.show().and_then(|_| window.set_focus()) {
        let _ = window.close();
        cards.destroyed(&label, &state);
        return Err(error(show_error));
    }
    Ok(())
}

/// Called once the card shell and its event listeners have mounted. Serialize
/// with creation because a fast page can invoke this before `build` returns.
#[tauri::command]
#[specta::specta]
pub async fn ready_sftp_card(app: AppHandle, window: WebviewWindow) -> Result<(), CommandError> {
    let cards = app.state::<SftpCardState>();
    let _operation = cards.operation.lock().await;
    if cards
        .windows
        .lock()
        .unwrap()
        .get(window.label())
        .ok_or_else(|| error("SFTP card was closed"))?
        .ready
    {
        return Ok(());
    }
    // The initial show/focus can precede the OS mapping the window. Reassert it
    // once, after the shell is usable; subsequent page loads never steal focus.
    window
        .show()
        .and_then(|_| window.set_focus())
        .map_err(error)?;
    let focused = window.is_focused().map_err(error)?;
    let mut windows = cards.windows.lock().unwrap();
    let card = windows
        .get_mut(window.label())
        .ok_or_else(|| error("SFTP card was closed"))?;
    card.ready = true;
    card.focused = focused;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn close_sftp_card(window: WebviewWindow) -> Result<(), CommandError> {
    if !is_sftp_card(window.label()) {
        return Err(error("not an SFTP card window"));
    }
    window.close().map_err(error)
}

fn error(value: impl ToString) -> CommandError {
    CommandError::new("sftp-card", value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_paths_encode_host_names_without_changing_the_view() {
        let path = app_path("生产 & backup/#1?view=desktop-card").unwrap();
        let url = tauri::Url::parse(&format!("https://localhost/{path}")).unwrap();
        let params: HashMap<_, _> = url.query_pairs().collect();
        assert_eq!(params["view"], "sftp-card");
        assert_eq!(params["host"], "生产 & backup/#1?view=desktop-card");
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn blur_only_dismisses_a_card_after_it_has_received_focus() {
        let state = SftpCardState::default();
        state.windows.lock().unwrap().insert(
            "sftp-card-0".into(),
            CardWindow {
                host_name: "web-1".into(),
                sessions: HashSet::new(),
                focused: false,
                ready: false,
            },
        );
        assert!(!state.focus_changed("sftp-card-0", false));
        assert!(!state.focus_changed("sftp-card-0", true));
        assert!(!state.focus_changed("sftp-card-0", false));
        state
            .windows
            .lock()
            .unwrap()
            .get_mut("sftp-card-0")
            .unwrap()
            .ready = true;
        assert!(!state.focus_changed("sftp-card-0", false));
        assert!(!state.focus_changed("sftp-card-0", true));
        assert!(state.focus_changed("sftp-card-0", false));
        assert!(!state.focus_changed("main", false));
    }
}
