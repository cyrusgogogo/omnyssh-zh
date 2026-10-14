//! Live desktop preferences and recoverable global shortcut registration.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tauri_plugin_store::StoreExt;

use crate::commands::desktop_card::{
    show_desktop_card, toggle_desktop_card_visibility, DESKTOP_CARD_WINDOW_LABEL,
};
use crate::error::CommandError;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CloseBehavior {
    Quit,
    #[default]
    Tray,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum TrayClickBehavior {
    ToggleCard,
    #[default]
    OpenMain,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(default, rename_all = "camelCase")]
pub struct DesktopSettingsDto {
    pub close_behavior: CloseBehavior,
    pub tray_click_behavior: TrayClickBehavior,
    // Empty disables the global shortcut.
    pub shortcut: String,
    pub shortcut_registered: bool,
    pub shortcut_error: Option<String>,
}

impl Default for DesktopSettingsDto {
    fn default() -> Self {
        Self {
            close_behavior: CloseBehavior::Tray,
            tray_click_behavior: TrayClickBehavior::OpenMain,
            shortcut: "Control+Shift+H".into(),
            shortcut_registered: false,
            shortcut_error: None,
        }
    }
}

#[derive(Default)]
pub struct DesktopPreferences {
    value: Mutex<DesktopSettingsDto>,
    operation: tokio::sync::Mutex<()>,
}

impl DesktopPreferences {
    pub fn snapshot(&self) -> DesktopSettingsDto {
        self.value.lock().unwrap().clone()
    }

    fn publish(&self, app: &AppHandle, value: DesktopSettingsDto) {
        *self.value.lock().unwrap() = value.clone();
        let _ = app.emit("desktop-settings-changed", value);
    }
}

fn error(value: impl ToString) -> CommandError {
    CommandError::new("desktop-settings", value.to_string())
}

fn parse_shortcut(value: &str) -> Result<Option<Shortcut>, CommandError> {
    if value.is_empty() {
        return Ok(None);
    }
    let shortcut: Shortcut = value.parse().map_err(error)?;
    if !shortcut
        .mods
        .intersects(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER)
    {
        return Err(error("Use Control, Alt or Meta together with a key"));
    }
    Ok(Some(shortcut))
}

fn register(app: &AppHandle, shortcut: Shortcut) -> Result<(), CommandError> {
    app.global_shortcut()
        .on_shortcut(shortcut, |app, _, event| {
            // The platform already suppresses key repeat. An additional pressed latch
            // can become permanent when an OS shortcut steals the release event.
            if event.state == ShortcutState::Pressed {
                toggle_card_from_tray(app);
            }
        })
        .map_err(|e| error(format!("Shortcut unavailable (it may be in use): {e}")))
}

pub fn toggle_card_from_tray(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = if app.get_webview_window(DESKTOP_CARD_WINDOW_LABEL).is_some() {
            toggle_desktop_card_visibility(app.clone()).await
        } else {
            show_desktop_card(app.clone()).await
        };
        if let Err(error) = result {
            use tauri_specta::Event;
            let _ = crate::events::Error {
                message: error.raw_detail.unwrap_or(error.code),
            }
            .emit(&app);
        }
    });
}

pub fn initialize(app: &AppHandle) -> Result<(), CommandError> {
    let store = app.store("desktop-settings.json").map_err(error)?;
    let mut value = store
        .get("preferences")
        .map(serde_json::from_value::<DesktopSettingsDto>)
        .transpose()
        .map_err(error)?
        .unwrap_or_default();
    value.shortcut_registered = false;
    value.shortcut_error = None;
    match parse_shortcut(&value.shortcut).and_then(|s| s.map(|s| register(app, s)).transpose()) {
        Ok(Some(())) => value.shortcut_registered = true,
        Ok(None) => {}
        Err(e) => value.shortcut_error = Some(e.raw_detail.unwrap_or(e.code)),
    }
    app.state::<DesktopPreferences>().publish(app, value);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn load_desktop_settings(app: AppHandle) -> DesktopSettingsDto {
    app.state::<DesktopPreferences>().snapshot()
}

#[tauri::command]
#[specta::specta]
pub async fn save_desktop_settings(
    app: AppHandle,
    mut settings: DesktopSettingsDto,
    retry_shortcut: bool,
) -> Result<DesktopSettingsDto, CommandError> {
    let state = app.state::<DesktopPreferences>();
    let _operation = state.operation.lock().await;
    let previous = state.snapshot();
    let store = app.store("desktop-settings.json").map_err(error)?;
    settings.shortcut = settings.shortcut.trim().to_string();
    let next_key = parse_shortcut(&settings.shortcut)?;
    let old_key = parse_shortcut(&previous.shortcut).ok().flatten();
    let changed = next_key != old_key;
    settings.shortcut_registered = previous.shortcut_registered;
    settings.shortcut_error = previous.shortcut_error.clone();

    if changed || retry_shortcut || !previous.shortcut_registered {
        // A retry clears stale OS state, including a missing release on X11.
        if !changed {
            if let Some(key) = old_key.filter(|key| app.global_shortcut().is_registered(*key)) {
                app.global_shortcut().unregister(key).map_err(error)?;
            }
        }
        settings.shortcut_registered = false;
        settings.shortcut_error = None;
        if let Some(key) = next_key {
            if let Err(e) = register(&app, key) {
                if changed {
                    return Err(e);
                } // Keep the existing shortcut on a failed replacement.
                settings.shortcut_error = Some(e.raw_detail.unwrap_or(e.code));
            } else {
                settings.shortcut_registered = true;
            }
        }
    }

    store.set(
        "preferences",
        serde_json::to_value(&settings).map_err(error)?,
    );
    if let Err(e) = store.save() {
        store.set(
            "preferences",
            serde_json::to_value(&previous).map_err(error)?,
        );
        if changed {
            if let Some(key) = next_key.filter(|_| settings.shortcut_registered) {
                let _ = app.global_shortcut().unregister(key);
            }
        } else {
            let mut actual = previous;
            actual.shortcut_registered = settings.shortcut_registered;
            actual.shortcut_error = settings.shortcut_error;
            state.publish(&app, actual);
        }
        return Err(error(e));
    }
    if changed {
        if let Some(key) = old_key.filter(|key| app.global_shortcut().is_registered(*key)) {
            if let Err(e) = app.global_shortcut().unregister(key) {
                if let Some(new_key) = next_key {
                    let _ = app.global_shortcut().unregister(new_key);
                }
                store.set(
                    "preferences",
                    serde_json::to_value(&previous).map_err(error)?,
                );
                let _ = store.save();
                return Err(error(e));
            }
        }
    }
    state.publish(&app, settings.clone());
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_preserve_tray_behavior_and_shortcut() {
        let settings: DesktopSettingsDto = serde_json::from_str("{}").unwrap();
        assert_eq!(settings.close_behavior, CloseBehavior::Tray);
        assert_eq!(settings.tray_click_behavior, TrayClickBehavior::OpenMain);
        assert_eq!(settings.shortcut, "Control+Shift+H");
    }

    #[test]
    fn shortcut_requires_a_modifier_and_supports_disabling() {
        assert!(parse_shortcut("").unwrap().is_none());
        assert!(parse_shortcut("H").is_err());
        assert!(parse_shortcut("Shift+H").is_err());
        assert!(parse_shortcut("Control+Shift+H").unwrap().is_some());
        assert!(parse_shortcut("Alt+KeyJ").unwrap().is_some());
        assert!(parse_shortcut("Control+Invalid").is_err());
    }
}
