//! Signed desktop updates. The GUI checks and installs the same platform-specific
//! manifest; it never offers or installs the core's terminal-only archive.

use std::sync::atomic::Ordering;
use std::time::Duration;

use serde::Serialize;
use tauri::{ipc::Channel, AppHandle, Manager};
use tauri_plugin_updater::{Update, UpdaterExt};
use tauri_specta::Event;

use omnyssh_core::config::app_config::{
    load_app_config, save_update_config as core_save_update_config,
};

use crate::dto::{UpdateConfigDto, UpdateInfoDto};
use crate::error::CommandError;

const RELEASE_BASE: &str = "https://github.com/cyrusgogogo/omnyssh-zh/releases";

#[derive(Default)]
pub struct UpdateState(tokio::sync::Mutex<()>);

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(tag = "phase", rename_all = "camelCase")]
pub enum UpdateProgress {
    Checking,
    Downloading { received: u64, total: Option<u64> },
    Verifying,
    Installing,
    Restarting,
}

fn error(code: &str, value: impl ToString) -> CommandError {
    CommandError::new(code, value.to_string())
}

fn release_endpoint(version: &str) -> Result<tauri::Url, CommandError> {
    // Do not accept paths or arbitrary URLs from the webview. Pin installation to
    // the version the user selected, even if another release appears meanwhile.
    let version = semver::Version::parse(version).map_err(|e| error("update-install", e))?;
    format!("{RELEASE_BASE}/download/v{version}/latest.json")
        .parse()
        .map_err(|e| error("update-install", e))
}

async fn find_update(
    app: &AppHandle,
    version: Option<&str>,
) -> Result<Option<Update>, CommandError> {
    let mut builder = app.updater_builder().timeout(Duration::from_secs(20));
    if let Some(version) = version {
        builder = builder
            .endpoints(vec![release_endpoint(version)?])
            .map_err(|e| error("update-check", e))?;
    }
    builder
        .build()
        .map_err(|e| error("update-check", e))?
        .check()
        .await
        .map_err(|e| error("update-check", e))
}

/// Check the signed desktop manifest for this OS, architecture and package format.
#[tauri::command]
#[specta::specta]
pub async fn check_update(app: AppHandle) -> Result<Option<UpdateInfoDto>, CommandError> {
    Ok(find_update(&app, None).await?.map(|update| UpdateInfoDto {
        tag: format!("v{}", update.version),
        url: format!("{RELEASE_BASE}/tag/v{}", update.version),
        version: update.version,
        can_self_update: true,
    }))
}

/// Download, verify the publisher's signature, install, then restart. Failures
/// leave the current app running and release the lock so the user can retry.
#[tauri::command]
#[specta::specta]
pub async fn install_update(
    app: AppHandle,
    version: String,
    on_progress: Channel<UpdateProgress>,
) -> Result<(), CommandError> {
    let state = app.state::<UpdateState>();
    let _operation = state
        .0
        .try_lock()
        .map_err(|_| error("update-install", "An update is already in progress."))?;
    let _ = on_progress.send(UpdateProgress::Checking);
    let mut update = find_update(&app, Some(&version)).await?.ok_or_else(|| {
        error(
            "update-install",
            "This update is no longer available. Check again.",
        )
    })?;
    if update.version != version {
        return Err(error(
            "update-install",
            "The update manifest has an unexpected version.",
        ));
    }
    // A large bundle on a slow connection needs longer than the manifest request.
    update.timeout = Some(Duration::from_secs(30 * 60));
    let mut received = 0;
    let _ = on_progress.send(UpdateProgress::Downloading {
        received,
        total: None,
    });
    let bytes = update
        .download(
            |chunk, total| {
                received += chunk as u64;
                let _ = on_progress.send(UpdateProgress::Downloading { received, total });
            },
            || {
                let _ = on_progress.send(UpdateProgress::Verifying);
            },
        )
        .await
        .map_err(|e| error("update-install", e))?;
    // `download` verifies the signature before returning any bytes for installation.
    let _ = on_progress.send(UpdateProgress::Installing);
    tauri::async_runtime::spawn_blocking(move || update.install(bytes))
        .await
        .map_err(|e| error("update-install", e))?
        .map_err(|e| error("update-install", e))?;
    let _ = on_progress.send(UpdateProgress::Restarting);
    crate::EXIT_REQUESTED.store(true, Ordering::Release);
    app.restart();
}

#[tauri::command]
#[specta::specta]
pub async fn load_update_config() -> Result<UpdateConfigDto, CommandError> {
    let config = tauri::async_runtime::spawn_blocking(|| load_app_config(None))
        .await
        .map_err(|e| error("update-config", e))?
        .map_err(|e| error("update-config", e))?;
    Ok((&config.update).into())
}

#[tauri::command]
#[specta::specta]
pub async fn save_update_config(config: UpdateConfigDto) -> Result<(), CommandError> {
    let update = config.into();
    tauri::async_runtime::spawn_blocking(move || core_save_update_config(&update))
        .await
        .map_err(|e| error("update-config", e))?
        .map_err(|e| error("update-config", e))
}

/// Startup checks remain quiet offline and honour the saved skipped version.
pub async fn startup_update_check(app: AppHandle) {
    let config = load_app_config(None).unwrap_or_default().update;
    if !config.check_on_startup {
        return;
    }
    if let Ok(Some(info)) = check_update(app.clone()).await {
        if info.version != config.skip_version {
            let _ = crate::events::UpdateAvailable { info }.emit(&app);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_endpoint_pins_the_selected_release() {
        assert_eq!(
            release_endpoint("1.2.3").unwrap().as_str(),
            "https://github.com/cyrusgogogo/omnyssh-zh/releases/download/v1.2.3/latest.json"
        );
        for invalid in [
            "../latest",
            "1.2.3/../../other",
            "https://example.com",
            "v1.2.3",
            "",
        ] {
            assert!(release_endpoint(invalid).is_err());
        }
    }
}
