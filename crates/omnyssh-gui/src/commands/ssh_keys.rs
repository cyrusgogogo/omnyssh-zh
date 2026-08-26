//! SSH key catalog commands. Private key contents stay in the core manager;
//! the IPC surface carries metadata and filesystem paths only.

use std::path::PathBuf;

use omnyssh_core::config::ssh_key_manager::{self, KeyManagerPaths};

use crate::dto::{SshKeyRecordDto, SshKeySnapshotDto};
use crate::error::CommandError;

fn command_error(error: impl ToString) -> CommandError {
    CommandError::new("ssh-keys", error.to_string())
}

#[tauri::command]
#[specta::specta]
pub async fn get_ssh_key_snapshot() -> Result<SshKeySnapshotDto, CommandError> {
    tauri::async_runtime::spawn_blocking(ssh_key_manager::inspect_default)
        .await
        .map_err(command_error)?
        .map(Into::into)
        .map_err(command_error)
}

#[tauri::command]
#[specta::specta]
pub async fn create_ssh_key(
    name: String,
    file_stem: String,
    key_type: String,
) -> Result<SshKeyRecordDto, CommandError> {
    let paths = KeyManagerPaths::discover().map_err(command_error)?;
    ssh_key_manager::create_key(&paths, &name, &file_stem, &key_type)
        .await
        .map(Into::into)
        .map_err(command_error)
}

#[tauri::command]
#[specta::specta]
pub async fn import_ssh_key(
    name: String,
    private_path: String,
) -> Result<SshKeyRecordDto, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let paths = KeyManagerPaths::discover()?;
        ssh_key_manager::import_key(&paths, &name, &PathBuf::from(private_path))
    })
    .await
    .map_err(command_error)?
    .map(Into::into)
    .map_err(command_error)
}

#[tauri::command]
#[specta::specta]
pub async fn rename_ssh_key(id: String, name: String) -> Result<(), CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let paths = KeyManagerPaths::discover()?;
        ssh_key_manager::rename_key(&paths, &id, &name)
    })
    .await
    .map_err(command_error)?
    .map_err(command_error)
}

#[tauri::command]
#[specta::specta]
pub async fn backup_ssh_key(id: String) -> Result<String, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let paths = KeyManagerPaths::discover()?;
        ssh_key_manager::backup_key(&paths, &id)
    })
    .await
    .map_err(command_error)?
    .map_err(command_error)
}

#[tauri::command]
#[specta::specta]
pub async fn read_ssh_public_key(id: String) -> Result<String, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let paths = KeyManagerPaths::discover()?;
        ssh_key_manager::read_public_key(&paths, &id)
    })
    .await
    .map_err(command_error)?
    .map_err(command_error)
}

#[tauri::command]
#[specta::specta]
pub async fn delete_ssh_key(id: String, confirmation: String) -> Result<String, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let paths = KeyManagerPaths::discover()?;
        let snapshot = ssh_key_manager::inspect(&paths)?;
        let record = snapshot
            .records
            .iter()
            .find(|record| record.id == id)
            .ok_or_else(|| anyhow::anyhow!("key record not found"))?;
        if confirmation != record.name {
            anyhow::bail!("confirmation does not match the key name");
        }
        ssh_key_manager::delete_key(&paths, &id)
    })
    .await
    .map_err(command_error)?
    .map_err(command_error)
}

#[tauri::command]
#[specta::specta]
pub async fn restore_ssh_key(backup_id: String) -> Result<SshKeyRecordDto, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let paths = KeyManagerPaths::discover()?;
        ssh_key_manager::restore_key(&paths, &backup_id)
    })
    .await
    .map_err(command_error)?
    .map(Into::into)
    .map_err(command_error)
}
