//! Desktop-only settings and system terminal launching.

use std::ffi::OsString;
use std::process::Command;

use tauri::State;

use omnyssh_core::config::app_config::{load_app_config, save_terminal_open_mode_to_config};
use omnyssh_core::ssh::client::{Host, HostSource};

use crate::error::CommandError;
use crate::state::GuiState;

#[tauri::command]
#[specta::specta]
pub async fn load_terminal_open_mode() -> Result<String, CommandError> {
    tauri::async_runtime::spawn_blocking(|| load_app_config(None))
        .await
        .map_err(command_error)?
        .map(|config| normalize_mode(&config.ui.terminal_open_mode).to_string())
        .map_err(command_error)
}

#[tauri::command]
#[specta::specta]
pub async fn save_terminal_open_mode(mode: String) -> Result<(), CommandError> {
    let mode = normalize_mode(&mode).to_string();
    tauri::async_runtime::spawn_blocking(move || save_terminal_open_mode_to_config(&mode))
        .await
        .map_err(command_error)?
        .map_err(command_error)
}

#[tauri::command]
#[specta::specta]
pub async fn open_system_terminal(
    state: State<'_, GuiState>,
    host_name: String,
) -> Result<(), CommandError> {
    let host = state
        .host_by_name(&host_name)
        .ok_or_else(|| CommandError::new("system-terminal", "unknown host"))?;
    tauri::async_runtime::spawn_blocking(move || launch_system_terminal(&host))
        .await
        .map_err(command_error)?
        .map_err(command_error)
}

fn normalize_mode(value: &str) -> &'static str {
    if value.eq_ignore_ascii_case("system") {
        "system"
    } else {
        "default"
    }
}

fn system_ssh_args(host: &Host) -> Vec<OsString> {
    if host.source == HostSource::SshConfig {
        return vec![host.name.clone().into()];
    }
    let mut args = Vec::new();
    args.push(format!("{}@{}", host.user, host.hostname).into());
    if host.port != 22 {
        args.push("-p".into());
        args.push(host.port.to_string().into());
    }
    if let Some(identity) = &host.identity_file {
        args.push("-i".into());
        args.push(identity.into());
    }
    if let Some(proxy) = &host.proxy_jump {
        args.push("-J".into());
        args.push(proxy.into());
    }
    if let Some(ciphers) = &host.ciphers {
        args.push("-o".into());
        args.push(format!("Ciphers={ciphers}").into());
    }
    args
}

fn launch_system_terminal(host: &Host) -> std::io::Result<()> {
    let args = system_ssh_args(host);
    #[cfg(target_os = "windows")]
    {
        Command::new("wt.exe")
            .args(windows_terminal_args(host, args))
            .spawn()
            .map(|_| ())
    }
    #[cfg(target_os = "macos")]
    {
        let ssh_command = std::iter::once(OsString::from("ssh"))
            .chain(args)
            .map(|value| shell_quote(&value.to_string_lossy()))
            .collect::<Vec<_>>()
            .join(" ");
        let command = format!(
            "printf '\\033]0;%s\\007' {}; exec {ssh_command}",
            shell_quote(&host.name)
        );
        let script = format!(
            "tell application \"Terminal\" to do script \"{}\"",
            command.replace('\\', "\\\\").replace('"', "\\\"")
        );
        Command::new("/usr/bin/osascript")
            .arg("-e")
            .arg(script)
            .arg("-e")
            .arg("tell application \"Terminal\" to activate")
            .spawn()
            .map(|_| ())
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("x-terminal-emulator")
            .arg("-T")
            .arg(&host.name)
            .arg("-e")
            .arg("ssh")
            .args(&args)
            .spawn()
            .map(|_| ())
    }
}

#[cfg(any(target_os = "windows", test))]
fn windows_terminal_args(host: &Host, ssh_args: Vec<OsString>) -> Vec<OsString> {
    [
        OsString::from("new-tab"),
        OsString::from("--title"),
        OsString::from(&host.name),
        OsString::from("ssh"),
    ]
    .into_iter()
    .chain(ssh_args)
    .collect()
}

#[cfg(target_os = "macos")]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn command_error(error: impl ToString) -> CommandError {
    CommandError::new("settings", error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imported_hosts_open_by_alias_to_preserve_user_ssh_policy() {
        let host = Host {
            name: "legacy".into(),
            hostname: "ignored.example.com".into(),
            source: HostSource::SshConfig,
            ..Host::default()
        };
        assert_eq!(system_ssh_args(&host), vec![OsString::from("legacy")]);
    }

    #[test]
    fn manual_hosts_pass_connection_options_without_a_password() {
        let host = Host {
            hostname: "legacy.example.com".into(),
            user: "admin".into(),
            port: 2222,
            identity_file: Some("C:/keys/id_demo".into()),
            proxy_jump: Some("jump".into()),
            ciphers: Some("+aes256-cbc".into()),
            password: Some("must-not-leak".into()),
            ..Host::default()
        };
        let rendered = system_ssh_args(&host)
            .into_iter()
            .map(|item| item.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            rendered,
            [
                "admin@legacy.example.com",
                "-p",
                "2222",
                "-i",
                "C:/keys/id_demo",
                "-J",
                "jump",
                "-o",
                "Ciphers=+aes256-cbc"
            ]
        );
        assert!(!rendered.join(" ").contains("must-not-leak"));
    }

    #[test]
    fn windows_terminal_tab_uses_the_host_name() {
        let host = Host {
            name: "legacy-db".into(),
            hostname: "legacy.example.com".into(),
            ..Host::default()
        };
        let rendered = windows_terminal_args(&host, system_ssh_args(&host))
            .into_iter()
            .map(|item| item.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(&rendered[..4], ["new-tab", "--title", "legacy-db", "ssh"]);
        assert_eq!(rendered[4], "root@legacy.example.com");
    }
}
