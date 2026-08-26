use omnyssh_core::config::ssh_config_manager::{self, ManagedHost, ManagerPaths};

use crate::dto::{ManagedSshHostDto, SshApplyReportDto, SshConfigPreviewDto, SshConfigSnapshotDto};
use crate::error::CommandError;

fn command_error(error: impl ToString) -> CommandError {
    CommandError::new("ssh-config", error.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn get_ssh_config_snapshot() -> Result<SshConfigSnapshotDto, CommandError> {
    tokio::task::spawn_blocking(|| {
        let snapshot = ssh_config_manager::inspect_default().map_err(command_error)?;
        Ok(SshConfigSnapshotDto::from_core(snapshot))
    })
    .await
    .map_err(|error| CommandError::new("ssh-config", error.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn preview_ssh_config(
    hosts: Vec<ManagedSshHostDto>,
) -> Result<SshConfigPreviewDto, CommandError> {
    tokio::task::spawn_blocking(move || {
        let paths = ManagerPaths::discover().map_err(command_error)?;
        let desired = hosts
            .into_iter()
            .map(Into::into)
            .collect::<Vec<ManagedHost>>();
        ssh_config_manager::preview(&paths, &desired)
            .map(Into::into)
            .map_err(command_error)
    })
    .await
    .map_err(|error| CommandError::new("ssh-config", error.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn apply_ssh_config(
    hosts: Vec<ManagedSshHostDto>,
    expected_main_hash: String,
    expected_managed_hash: String,
    allow_missing_ssh: bool,
) -> Result<SshApplyReportDto, CommandError> {
    tokio::task::spawn_blocking(move || {
        let paths = ManagerPaths::discover().map_err(command_error)?;
        let desired = hosts
            .into_iter()
            .map(Into::into)
            .collect::<Vec<ManagedHost>>();
        ssh_config_manager::apply(
            &paths,
            &desired,
            &expected_main_hash,
            &expected_managed_hash,
            allow_missing_ssh,
        )
        .map(Into::into)
        .map_err(command_error)
    })
    .await
    .map_err(|error| CommandError::new("ssh-config", error.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn restore_ssh_config(
    backup_id: String,
    expected_main_hash: String,
    expected_managed_hash: String,
) -> Result<(), CommandError> {
    tokio::task::spawn_blocking(move || {
        let paths = ManagerPaths::discover().map_err(command_error)?;
        ssh_config_manager::restore(
            &paths,
            &backup_id,
            &expected_main_hash,
            &expected_managed_hash,
        )
        .map_err(command_error)
    })
    .await
    .map_err(|error| CommandError::new("ssh-config", error.to_string()))?
}

#[tauri::command]
#[specta::specta]
pub async fn preview_ssh_config_restore(
    backup_id: String,
) -> Result<SshConfigPreviewDto, CommandError> {
    tokio::task::spawn_blocking(move || {
        let paths = ManagerPaths::discover().map_err(command_error)?;
        ssh_config_manager::preview_restore(&paths, &backup_id)
            .map(Into::into)
            .map_err(command_error)
    })
    .await
    .map_err(|error| CommandError::new("ssh-config", error.to_string()))?
}
