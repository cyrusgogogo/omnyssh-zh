use omnyssh_core::config::ssh_config_manager::{self, ManagedHost, ManagerPaths};

use crate::dto::{
    HostInputDto, ManagedSshHostDto, SshApplyReportDto, SshConfigPreviewDto, SshConfigSnapshotDto,
    SshHostWritePreviewDto, SshHostWriteReportDto,
};
use crate::error::CommandError;

/// 从持久化的软件自身主机补齐编辑表单看不到的字段，不向前端回传密码。
fn host_for_write(
    input: HostInputDto,
    mut stored: Vec<omnyssh_core::ssh::client::Host>,
) -> anyhow::Result<omnyssh_core::ssh::client::Host> {
    use omnyssh_core::ssh::client::HostSource;
    let index = stored
        .iter()
        .position(|h| h.name == input.name && h.source == HostSource::Manual)
        .ok_or_else(|| {
            anyhow::anyhow!("only an existing manual host can be written to SSH config")
        })?;
    super::hosts::upsert(&mut stored, input, None);
    Ok(stored.remove(index))
}

#[tauri::command]
#[specta::specta]
pub async fn preview_host_ssh_config(
    input: HostInputDto,
) -> Result<SshHostWritePreviewDto, CommandError> {
    tokio::task::spawn_blocking(move || {
        let replace_identity = input
            .identity_file
            .as_ref()
            .is_some_and(|s| !s.trim().is_empty());
        let stored = omnyssh_core::config::load_hosts().map_err(command_error)?;
        let host = host_for_write(input, stored).map_err(command_error)?;
        let paths = ManagerPaths::discover().map_err(command_error)?;
        ssh_config_manager::host_write::preview(&paths, &host, replace_identity)
            .map(Into::into)
            .map_err(command_error)
    })
    .await
    .map_err(command_error)?
}

#[tauri::command]
#[specta::specta]
pub async fn write_host_ssh_config(
    input: HostInputDto,
    expected_fingerprint: String,
    confirm_non_key: bool,
) -> Result<SshHostWriteReportDto, CommandError> {
    tokio::task::spawn_blocking(move || {
        let replace_identity = input
            .identity_file
            .as_ref()
            .is_some_and(|s| !s.trim().is_empty());
        let stored = omnyssh_core::config::load_hosts().map_err(command_error)?;
        let host = host_for_write(input, stored).map_err(command_error)?;
        let paths = ManagerPaths::discover().map_err(command_error)?;
        let report = ssh_config_manager::host_write::apply(
            &paths,
            &host,
            replace_identity,
            &expected_fingerprint,
            confirm_non_key,
        )
        .map_err(command_error)?;
        Ok(SshHostWriteReportDto {
            backup_path: report.backup_path,
            ssh_validation: report.ssh_validation,
        })
    })
    .await
    .map_err(command_error)?
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use omnyssh_core::ssh::client::{Host, HostSource};

    fn input() -> HostInputDto {
        HostInputDto {
            name: "lan".into(),
            hostname: "192.168.1.20".into(),
            user: "root".into(),
            port: 22,
            identity_file: None,
            password: None,
            proxy_jump: None,
            tags: vec![],
            notes: None,
            hidden_from_overview: false,
            monitoring: None,
            monitor_port: None,
        }
    }

    #[test]
    fn write_uses_unsaved_form_with_stored_identity_policy_and_original_alias() {
        let stored = Host {
            name: "lan".into(),
            hostname: "192.168.1.10".into(),
            source: HostSource::Manual,
            identity_file: Some("~/.ssh/id_test".into()),
            password: Some("stored-secret".into()),
            proxy_jump: Some("bastion".into()),
            ciphers: Some("+aes256-cbc".into()),
            original_ssh_host: Some("original-lan".into()),
            ..Host::default()
        };
        let host = host_for_write(input(), vec![stored.clone()]).unwrap();
        assert_eq!(host.hostname, "192.168.1.20");
        assert_eq!(stored.hostname, "192.168.1.10");
        assert_eq!(host.identity_file, stored.identity_file);
        assert_eq!(host.password, stored.password);
        assert_eq!(host.proxy_jump, stored.proxy_jump);
        assert_eq!(host.ciphers, stored.ciphers);
        assert_eq!(host.original_ssh_host, stored.original_ssh_host);
    }

    #[test]
    fn write_rejects_missing_and_imported_hosts_even_when_called_directly() {
        assert!(host_for_write(input(), vec![]).is_err());
        let imported = Host {
            name: "lan".into(),
            source: HostSource::SshConfig,
            ..Host::default()
        };
        assert!(host_for_write(input(), vec![imported]).is_err());
    }
}
