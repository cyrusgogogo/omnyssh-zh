//! Safe management of OmnySSH-owned OpenSSH client configuration.
//!
//! User-authored configuration is treated as read-only. OmnySSH writes exact,
//! single-alias `Host` blocks to `~/.ssh/omnyssh.conf` and installs one top-level
//! include in `~/.ssh/config` after an explicit preview.

use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, bail, Context};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::ssh::client::{Host, HostSource};

pub const MANAGED_INCLUDE: &str = "Include ~/.ssh/omnyssh.conf";
pub const MAX_INCLUDE_DEPTH: usize = 16;
pub const MAX_SOURCE_FILES: usize = 256;
pub const MAX_SOURCE_BYTES: u64 = 16 * 1024 * 1024;
pub const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
const BACKUP_KEEP: usize = 10;

#[derive(Debug, Clone)]
pub struct ManagerPaths {
    pub ssh_dir: PathBuf,
    pub main_config: PathBuf,
    pub managed_config: PathBuf,
    pub backups_dir: PathBuf,
    pub lock_file: PathBuf,
}

impl ManagerPaths {
    pub fn for_home(home: &Path) -> Self {
        let ssh_dir = home.join(".ssh");
        Self {
            main_config: ssh_dir.join("config"),
            managed_config: ssh_dir.join("omnyssh.conf"),
            backups_dir: ssh_dir.join(".omnyssh-backups"),
            lock_file: ssh_dir.join(".omnyssh.lock"),
            ssh_dir,
        }
    }

    pub fn discover() -> anyhow::Result<Self> {
        let home = dirs::home_dir().context("Cannot determine home directory")?;
        Ok(Self::for_home(&home))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedHost {
    pub alias: String,
    pub hostname: String,
    pub user: String,
    pub port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proxy_jump: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ciphers: Option<String>,
}

impl TryFrom<&Host> for ManagedHost {
    type Error = anyhow::Error;

    fn try_from(host: &Host) -> Result<Self, Self::Error> {
        if host.source != HostSource::Manual {
            bail!("only manual hosts can be synchronised");
        }
        let value = Self {
            alias: host.name.clone(),
            hostname: host.hostname.clone(),
            user: host.user.clone(),
            port: host.port,
            identity_file: host.identity_file.clone(),
            proxy_jump: host.proxy_jump.clone(),
            ciphers: host.ciphers.clone(),
        };
        validate_host(&value)?;
        Ok(value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceFile {
    pub path: String,
    pub content: Option<String>,
    pub read_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub code: String,
    pub message: String,
    pub path: Option<String>,
    pub line: Option<usize>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DiagnosticSeverity {
    Info,
    Warning,
    Blocking,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupEntry {
    pub id: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagerSnapshot {
    pub main_path: String,
    pub managed_path: String,
    pub installed: bool,
    pub writable: bool,
    pub main_hash: String,
    pub managed_hash: String,
    pub hosts: Vec<ManagedHost>,
    pub sources: Vec<SourceFile>,
    pub diagnostics: Vec<Diagnostic>,
    pub backups: Vec<BackupEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub main_hash: String,
    pub managed_hash: String,
    pub main_diff: String,
    pub managed_diff: String,
    pub warnings: Vec<Diagnostic>,
    pub can_apply: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyReport {
    pub backup_id: String,
    pub ssh_validation: String,
}

pub fn inspect_default() -> anyhow::Result<ManagerSnapshot> {
    inspect(&ManagerPaths::discover()?)
}

pub fn inspect(paths: &ManagerPaths) -> anyhow::Result<ManagerSnapshot> {
    let main = read_optional_text(&paths.main_config)?;
    let managed = read_optional_text(&paths.managed_config)?;
    let mut diagnostics = Vec::new();
    inspect_write_target(paths, &paths.main_config, &mut diagnostics)?;
    inspect_write_target(paths, &paths.managed_config, &mut diagnostics)?;
    let hosts = match managed.as_deref() {
        Some(text) => match parse_managed(text) {
            Ok(hosts) => hosts,
            Err(err) => {
                diagnostics.push(blocking(
                    "managed-invalid",
                    err.to_string(),
                    &paths.managed_config,
                ));
                Vec::new()
            }
        },
        None => Vec::new(),
    };
    let mut sources = Vec::new();
    let mut aliases = HashSet::new();
    scan_sources(paths, &mut sources, &mut aliases, &mut diagnostics)?;
    for host in &hosts {
        if aliases.contains(&host.alias) {
            diagnostics.push(blocking(
                "alias-conflict",
                format!(
                    "alias '{}' is also defined by user configuration",
                    host.alias
                ),
                &paths.main_config,
            ));
        }
    }
    let installed = main
        .as_deref()
        .is_some_and(|text| top_level_include_count(text) == 1);
    let writable = !diagnostics
        .iter()
        .any(|d| d.severity == DiagnosticSeverity::Blocking);
    Ok(ManagerSnapshot {
        main_path: paths.main_config.display().to_string(),
        managed_path: paths.managed_config.display().to_string(),
        installed,
        writable,
        main_hash: hash_optional(main.as_deref()),
        managed_hash: hash_optional(managed.as_deref()),
        hosts,
        sources,
        diagnostics,
        backups: list_backups(paths)?,
    })
}

pub fn preview(paths: &ManagerPaths, desired: &[ManagedHost]) -> anyhow::Result<Preview> {
    validate_hosts(desired)?;
    let snapshot = inspect(paths)?;
    let main = read_optional_text(&paths.main_config)?.unwrap_or_default();
    let managed = read_optional_text(&paths.managed_config)?.unwrap_or_default();
    let next_main = install_include(&main)?;
    let newline = if managed.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let next_managed = render_managed(desired, newline);
    let mut warnings = snapshot.diagnostics;
    let mut source_files = Vec::new();
    let mut user_aliases = HashSet::new();
    let mut source_diagnostics = Vec::new();
    scan_sources(
        paths,
        &mut source_files,
        &mut user_aliases,
        &mut source_diagnostics,
    )?;
    warnings.extend(source_diagnostics);
    for host in desired {
        if user_aliases.contains(&host.alias) {
            warnings.push(blocking(
                "alias-conflict",
                format!("alias '{}' is defined by user configuration", host.alias),
                &paths.main_config,
            ));
        }
        if let Some(path) = &host.identity_file {
            if !identity_path_exists(path, paths) {
                warnings.push(Diagnostic {
                    severity: DiagnosticSeverity::Warning,
                    code: "identity-missing".into(),
                    message: format!(
                        "IdentityFile for '{}' does not exist or is not readable",
                        host.alias
                    ),
                    path: Some(path.clone()),
                    line: None,
                });
            }
        }
    }
    let can_apply = !warnings
        .iter()
        .any(|d| d.severity == DiagnosticSeverity::Blocking);
    Ok(Preview {
        main_hash: hash_optional(if paths.main_config.exists() {
            Some(&main)
        } else {
            None
        }),
        managed_hash: hash_optional(if paths.managed_config.exists() {
            Some(&managed)
        } else {
            None
        }),
        main_diff: simple_diff(&main, &next_main),
        managed_diff: simple_diff(&managed, &next_managed),
        warnings,
        can_apply,
    })
}

pub fn apply(
    paths: &ManagerPaths,
    desired: &[ManagedHost],
    expected_main_hash: &str,
    expected_managed_hash: &str,
    allow_missing_ssh: bool,
) -> anyhow::Result<ApplyReport> {
    validate_hosts(desired)?;
    let safety_preview = preview(paths, desired)?;
    if !safety_preview.can_apply {
        bail!("candidate contains blocking SSH config diagnostics");
    }
    ensure_ssh_directory(paths)?;
    let _lock = LockGuard::acquire(&paths.lock_file)?;
    let current_main = read_optional_text(&paths.main_config)?;
    let current_managed = read_optional_text(&paths.managed_config)?;
    if hash_optional(current_main.as_deref()) != expected_main_hash
        || hash_optional(current_managed.as_deref()) != expected_managed_hash
    {
        bail!("configuration changed after preview; reload before saving");
    }
    let next_main = install_include(current_main.as_deref().unwrap_or_default())?;
    let newline = current_managed
        .as_deref()
        .filter(|s| s.contains("\r\n"))
        .map_or("\n", |_| "\r\n");
    let next_managed = render_managed(desired, newline);
    let ssh_validation =
        validate_with_system_ssh(paths, &next_managed, desired, allow_missing_ssh)?;
    let backup_id = create_backup(paths, current_main.as_deref(), current_managed.as_deref())?;
    let write_result = (|| {
        atomic_write(&paths.managed_config, next_managed.as_bytes())?;
        atomic_write(&paths.main_config, next_main.as_bytes())?;
        let reparsed =
            parse_managed(&read_optional_text(&paths.managed_config)?.unwrap_or_default())?;
        if reparsed != desired {
            bail!("post-write managed config verification failed");
        }
        Ok(())
    })();
    if let Err(err) = write_result {
        restore_backup_files(paths, &backup_id)?;
        return Err(err.context("write failed; original files restored"));
    }
    prune_backups(paths)?;
    Ok(ApplyReport {
        backup_id,
        ssh_validation,
    })
}

pub fn preview_restore(paths: &ManagerPaths, backup_id: &str) -> anyhow::Result<Preview> {
    validate_backup_id(backup_id)?;
    let current_main = read_optional_text(&paths.main_config)?;
    let current_managed = read_optional_text(&paths.managed_config)?;
    let (backup_main, backup_managed) = read_backup_values(paths, backup_id)?;
    if let Some(text) = &backup_managed {
        parse_managed(text)?;
    }
    if let Some(text) = &backup_main {
        reject_unsafe_text(text)?;
    }
    Ok(Preview {
        main_hash: hash_optional(current_main.as_deref()),
        managed_hash: hash_optional(current_managed.as_deref()),
        main_diff: simple_diff(
            current_main.as_deref().unwrap_or_default(),
            backup_main.as_deref().unwrap_or_default(),
        ),
        managed_diff: simple_diff(
            current_managed.as_deref().unwrap_or_default(),
            backup_managed.as_deref().unwrap_or_default(),
        ),
        warnings: Vec::new(),
        can_apply: true,
    })
}

pub fn restore(
    paths: &ManagerPaths,
    backup_id: &str,
    expected_main_hash: &str,
    expected_managed_hash: &str,
) -> anyhow::Result<()> {
    validate_backup_id(backup_id)?;
    if backup_id.contains(['/', '\\']) || backup_id.contains("..") {
        bail!("invalid backup id");
    }
    let _lock = LockGuard::acquire(&paths.lock_file)?;
    let current_main = read_optional_text(&paths.main_config)?;
    let current_managed = read_optional_text(&paths.managed_config)?;
    if hash_optional(current_main.as_deref()) != expected_main_hash
        || hash_optional(current_managed.as_deref()) != expected_managed_hash
    {
        bail!("configuration changed after restore preview; reload before restoring");
    }
    let safety = create_backup(paths, current_main.as_deref(), current_managed.as_deref())?;
    if let Err(err) = restore_backup_files(paths, backup_id) {
        let _ = restore_backup_files(paths, &safety);
        return Err(err.context("restore failed; current files restored"));
    }
    Ok(())
}

fn validate_backup_id(id: &str) -> anyhow::Result<()> {
    if id.is_empty() || id.contains(['/', '\\']) || id.contains("..") {
        bail!("invalid backup id");
    }
    Ok(())
}

fn read_backup_values(
    paths: &ManagerPaths,
    id: &str,
) -> anyhow::Result<(Option<String>, Option<String>)> {
    let dir = paths.backups_dir.join(id);
    if !dir.is_dir() {
        bail!("backup not found");
    }
    let read = |name: &str| -> anyhow::Result<Option<String>> {
        let file = dir.join(name);
        if file.exists() {
            return Ok(Some(fs::read_to_string(file)?));
        }
        if dir.join(format!("{name}.missing")).exists() {
            return Ok(None);
        }
        bail!("backup is incomplete")
    };
    Ok((read("config")?, read("omnyssh.conf")?))
}

pub fn parse_managed(content: &str) -> anyhow::Result<Vec<ManagedHost>> {
    reject_unsafe_text(content)?;
    let mut hosts = Vec::new();
    let mut current: Option<ManagedHost> = None;
    let mut seen = HashSet::new();
    for (index, raw) in content.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }
        let (key, value) =
            split_kv(line).ok_or_else(|| anyhow!("line {} is not a directive", index + 1))?;
        match key.to_ascii_lowercase().as_str() {
            "host" => {
                if let Some(host) = current.take() {
                    finish_host(host, &mut hosts, &mut seen)?;
                }
                current = Some(ManagedHost {
                    alias: value.into(),
                    hostname: String::new(),
                    user: String::new(),
                    port: 0,
                    identity_file: None,
                    proxy_jump: None,
                    ciphers: None,
                });
            }
            "hostname" => current.as_mut().context("HostName before Host")?.hostname = value.into(),
            "user" => current.as_mut().context("User before Host")?.user = value.into(),
            "port" => {
                current.as_mut().context("Port before Host")?.port =
                    value.parse().context("invalid Port")?
            }
            "identityfile" => {
                let host = current.as_mut().context("IdentityFile before Host")?;
                if host.identity_file.replace(value.into()).is_some() {
                    bail!("multiple IdentityFile values are not supported");
                }
            }
            "proxyjump" => {
                current
                    .as_mut()
                    .context("ProxyJump before Host")?
                    .proxy_jump = Some(value.into())
            }
            "ciphers" => {
                current.as_mut().context("Ciphers before Host")?.ciphers = Some(value.into())
            }
            _ => bail!("unsupported directive '{}' on line {}", key, index + 1),
        }
    }
    if let Some(host) = current {
        finish_host(host, &mut hosts, &mut seen)?;
    }
    Ok(hosts)
}

pub fn render_managed(hosts: &[ManagedHost], newline: &str) -> String {
    let mut lines = vec![
        "# Managed by OmnySSH. Complex OpenSSH directives belong in ~/.ssh/config.".to_string(),
        String::new(),
    ];
    for (i, host) in hosts.iter().enumerate() {
        if i > 0 {
            lines.push(String::new());
        }
        lines.push(format!("Host {}", host.alias));
        lines.push(format!("    HostName {}", host.hostname));
        lines.push(format!("    User {}", host.user));
        lines.push(format!("    Port {}", host.port));
        if let Some(value) = &host.identity_file {
            lines.push(format!("    IdentityFile {value}"));
        }
        if let Some(value) = &host.proxy_jump {
            lines.push(format!("    ProxyJump {value}"));
        }
        if let Some(value) = &host.ciphers {
            lines.push(format!("    Ciphers {value}"));
        }
    }
    lines.push(String::new());
    lines.join(newline)
}

pub fn validate_host(host: &ManagedHost) -> anyhow::Result<()> {
    if host.alias.is_empty()
        || !host
            .alias
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    {
        bail!("alias must contain only ASCII letters, digits, '.', '_' or '-'");
    }
    validate_scalar("HostName", &host.hostname)?;
    validate_scalar("User", &host.user)?;
    if host.port == 0 {
        bail!("Port must be between 1 and 65535");
    }
    if let Some(path) = &host.identity_file {
        validate_scalar("IdentityFile", path)?;
        if path == "none"
            || path.contains('%')
            || path.contains("${")
            || !(Path::new(path).is_absolute() || path.starts_with('~'))
        {
            bail!("IdentityFile must be absolute or start with '~' and cannot use tokens");
        }
    }
    if let Some(proxy) = &host.proxy_jump {
        validate_proxy_jump(proxy)?;
    }
    if let Some(ciphers) = &host.ciphers {
        validate_scalar("Ciphers", ciphers)?;
    }
    Ok(())
}

fn validate_hosts(hosts: &[ManagedHost]) -> anyhow::Result<()> {
    let mut aliases = HashSet::new();
    for host in hosts {
        validate_host(host)?;
        if !aliases.insert(&host.alias) {
            bail!("duplicate alias '{}'", host.alias);
        }
    }
    Ok(())
}

fn validate_scalar(name: &str, value: &str) -> anyhow::Result<()> {
    if value.is_empty()
        || value.chars().any(|c| c.is_whitespace() || c.is_control())
        || value.contains('%')
        || value.contains("${")
    {
        bail!("{name} must be one non-empty value without whitespace, controls, or tokens");
    }
    Ok(())
}

fn validate_proxy_jump(value: &str) -> anyhow::Result<()> {
    validate_scalar("ProxyJump", value)?;
    for hop in value.split(',') {
        if hop.is_empty() || hop.contains("://") || hop == "none" {
            bail!("invalid ProxyJump hop");
        }
        let host_port = hop.rsplit_once('@').map_or(hop, |(_, v)| v);
        if host_port.starts_with('[') {
            let close = host_port
                .find(']')
                .context("unclosed IPv6 address in ProxyJump")?;
            if close + 1 < host_port.len() && !host_port[close + 1..].starts_with(':') {
                bail!("invalid ProxyJump IPv6 port");
            }
        } else if let Some((_, port)) = host_port.rsplit_once(':') {
            let parsed: u16 = port.parse().context("invalid ProxyJump port")?;
            if parsed == 0 {
                bail!("invalid ProxyJump port");
            }
        }
    }
    Ok(())
}

fn finish_host(
    host: ManagedHost,
    out: &mut Vec<ManagedHost>,
    seen: &mut HashSet<String>,
) -> anyhow::Result<()> {
    validate_host(&host)?;
    if !seen.insert(host.alias.clone()) {
        bail!("duplicate alias '{}'", host.alias);
    }
    out.push(host);
    Ok(())
}

fn split_kv(line: &str) -> Option<(&str, &str)> {
    let pos = line.find(|c: char| c.is_whitespace() || c == '=')?;
    let key = &line[..pos];
    let value = line[pos..]
        .trim_start_matches(|c: char| c.is_whitespace() || c == '=')
        .trim();
    (!key.is_empty() && !value.is_empty()).then_some((key, value))
}

fn reject_unsafe_text(content: &str) -> anyhow::Result<()> {
    if content.as_bytes().starts_with(&[0xEF, 0xBB, 0xBF]) {
        bail!("UTF-8 BOM is not supported");
    }
    if content.contains('\0') {
        bail!("NUL bytes are not supported");
    }
    Ok(())
}

fn install_include(content: &str) -> anyhow::Result<String> {
    reject_unsafe_text(content)?;
    let count = top_level_include_count(content);
    if count > 1 {
        bail!("multiple top-level OmnySSH Include directives found");
    }
    if count == 1 {
        return Ok(content.to_string());
    }
    let newline = if content.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    Ok(format!(
        "# OmnySSH managed hosts{newline}{MANAGED_INCLUDE}{newline}{newline}{content}"
    ))
}

fn top_level_include_count(content: &str) -> usize {
    let mut conditional = false;
    content
        .lines()
        .filter(|raw| {
            let line = raw.split('#').next().unwrap_or_default().trim();
            let Some((key, value)) = split_kv(line) else {
                return false;
            };
            if key.eq_ignore_ascii_case("host") || key.eq_ignore_ascii_case("match") {
                conditional = true;
            }
            !conditional
                && key.eq_ignore_ascii_case("include")
                && value
                    .replace('\\', "/")
                    .eq_ignore_ascii_case("~/.ssh/omnyssh.conf")
        })
        .count()
}

fn scan_sources(
    paths: &ManagerPaths,
    sources: &mut Vec<SourceFile>,
    aliases: &mut HashSet<String>,
    diagnostics: &mut Vec<Diagnostic>,
) -> anyhow::Result<()> {
    let mut stack = vec![(paths.main_config.clone(), 0usize)];
    let mut visited = HashSet::new();
    let mut total = 0u64;
    while let Some((path, depth)) = stack.pop() {
        if !path.exists() {
            continue;
        }
        if depth > MAX_INCLUDE_DEPTH {
            diagnostics.push(blocking("include-depth", "Include depth exceeds 16", &path));
            break;
        }
        let canonical = path.canonicalize().unwrap_or(path.clone());
        let is_managed = canonical
            == paths
                .managed_config
                .canonicalize()
                .unwrap_or(paths.managed_config.clone());
        if !visited.insert(canonical) {
            diagnostics.push(blocking("include-cycle", "Include cycle detected", &path));
            continue;
        }
        if visited.len() > MAX_SOURCE_FILES {
            diagnostics.push(blocking(
                "include-count",
                "more than 256 source files",
                &path,
            ));
            break;
        }
        let metadata = fs::metadata(&path)?;
        if metadata.len() > MAX_FILE_BYTES {
            diagnostics.push(blocking(
                "file-size",
                "configuration file exceeds 2 MiB",
                &path,
            ));
            continue;
        }
        total += metadata.len();
        if total > MAX_SOURCE_BYTES {
            diagnostics.push(blocking(
                "total-size",
                "configuration sources exceed 16 MiB",
                &path,
            ));
            break;
        }
        let bytes = fs::read(&path)?;
        let text = match String::from_utf8(bytes) {
            Ok(text) => text,
            Err(_) => {
                diagnostics.push(blocking("encoding", "configuration is not UTF-8", &path));
                sources.push(SourceFile {
                    path: path.display().to_string(),
                    content: None,
                    read_only: true,
                });
                continue;
            }
        };
        if let Err(err) = reject_unsafe_text(&text) {
            diagnostics.push(blocking("encoding", err.to_string(), &path));
        }
        for (line_no, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or_default().trim();
            let Some((key, value)) = split_kv(line) else {
                continue;
            };
            if key.eq_ignore_ascii_case("host") && !is_managed {
                for pattern in value.split_whitespace() {
                    if !pattern.contains(['*', '?']) && !pattern.starts_with('!') {
                        aliases.insert(pattern.to_string());
                    }
                }
            }
            if key.eq_ignore_ascii_case("include") {
                for token in value.split_whitespace() {
                    let expanded = expand_include_token(token, &paths.ssh_dir);
                    match glob::glob(&expanded.to_string_lossy()) {
                        Ok(matches) => {
                            for item in matches.flatten() {
                                stack.push((item, depth + 1));
                            }
                        }
                        Err(err) => diagnostics.push(Diagnostic {
                            severity: DiagnosticSeverity::Blocking,
                            code: "include-glob".into(),
                            message: err.to_string(),
                            path: Some(path.display().to_string()),
                            line: Some(line_no + 1),
                        }),
                    }
                }
            }
        }
        sources.push(SourceFile {
            path: path.display().to_string(),
            content: Some(text),
            read_only: !is_managed,
        });
    }
    Ok(())
}

fn expand_include_token(token: &str, ssh_dir: &Path) -> PathBuf {
    let unquoted = token.trim_matches('"');
    if unquoted == "~" {
        return ssh_dir.parent().unwrap_or(ssh_dir).to_path_buf();
    }
    if let Some(rest) = unquoted.strip_prefix("~/") {
        return ssh_dir.parent().unwrap_or(ssh_dir).join(rest);
    }
    let path = PathBuf::from(unquoted);
    if path.is_absolute() {
        path
    } else {
        ssh_dir.join(path)
    }
}

fn read_optional_text(path: &Path) -> anyhow::Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        let target = path
            .canonicalize()
            .with_context(|| format!("broken symlink {}", path.display()))?;
        if !target.is_file() {
            bail!("symlink target is not a regular file: {}", target.display());
        }
    } else if !metadata.is_file() {
        bail!("not a regular file: {}", path.display());
    }
    let bytes = fs::read(path)?;
    let text = String::from_utf8(bytes).map_err(|_| anyhow!("{} is not UTF-8", path.display()))?;
    reject_unsafe_text(&text)?;
    Ok(Some(text))
}

fn hash_optional(content: Option<&str>) -> String {
    match content {
        None => "missing".into(),
        Some(content) => format!("{:x}", Sha256::digest(content.as_bytes())),
    }
}

fn simple_diff(old: &str, new: &str) -> String {
    if old == new {
        return String::new();
    }
    let mut out = String::new();
    for line in old.lines() {
        out.push_str("- ");
        out.push_str(line);
        out.push('\n');
    }
    for line in new.lines() {
        out.push_str("+ ");
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn identity_path_exists(value: &str, paths: &ManagerPaths) -> bool {
    let path = if let Some(rest) = value.strip_prefix("~/") {
        paths.ssh_dir.parent().unwrap_or(&paths.ssh_dir).join(rest)
    } else {
        PathBuf::from(value)
    };
    File::open(path).is_ok()
}

fn validate_with_system_ssh(
    paths: &ManagerPaths,
    content: &str,
    hosts: &[ManagedHost],
    allow_missing: bool,
) -> anyhow::Result<String> {
    let candidate = paths.ssh_dir.join(".omnyssh.validate.tmp");
    fs::write(&candidate, content)?;
    let version = Command::new("ssh").arg("-V").output();
    let result: anyhow::Result<String> = match version {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound && allow_missing => {
            Ok("ssh unavailable; internal validation only".into())
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Err(anyhow!(
            "system ssh is unavailable; explicit confirmation required"
        )),
        Err(err) => Err(err.into()),
        Ok(version) => {
            let version_text = String::from_utf8_lossy(&version.stderr).trim().to_string();
            let mut failure = None;
            for host in hosts {
                let output = Command::new("ssh")
                    .args(["-G", "-F"])
                    .arg(&candidate)
                    .arg(&host.alias)
                    .output()?;
                if !output.status.success() {
                    failure = Some(anyhow!(
                        "ssh -G rejected '{}': {}",
                        host.alias,
                        String::from_utf8_lossy(&output.stderr).trim()
                    ));
                    break;
                }
            }
            failure.map_or_else(|| Ok(version_text), Err)
        }
    };
    let _ = fs::remove_file(candidate);
    result
}

fn create_backup(
    paths: &ManagerPaths,
    main: Option<&str>,
    managed: Option<&str>,
) -> anyhow::Result<String> {
    let id = format!(
        "{}-{}",
        Utc::now().format("%Y%m%dT%H%M%S%.9fZ"),
        std::process::id()
    );
    let dir = paths.backups_dir.join(&id);
    fs::create_dir_all(&paths.backups_dir)?;
    fs::create_dir(&dir)?;
    if let Some(value) = main {
        fs::write(dir.join("config"), value)?;
    } else {
        fs::write(dir.join("config.missing"), [])?;
    }
    if let Some(value) = managed {
        fs::write(dir.join("omnyssh.conf"), value)?;
    } else {
        fs::write(dir.join("omnyssh.conf.missing"), [])?;
    }
    fs::write(
        dir.join("manifest"),
        format!(
            "main={}\nmanaged={}\n",
            hash_optional(main),
            hash_optional(managed)
        ),
    )?;
    set_private_dir_permissions(&dir)?;
    Ok(id)
}

fn restore_backup_files(paths: &ManagerPaths, id: &str) -> anyhow::Result<()> {
    let dir = paths.backups_dir.join(id);
    if !dir.is_dir() {
        bail!("backup not found");
    }
    restore_one(&dir, "config", &paths.main_config)?;
    restore_one(&dir, "omnyssh.conf", &paths.managed_config)?;
    Ok(())
}

fn restore_one(dir: &Path, name: &str, target: &Path) -> anyhow::Result<()> {
    let source = dir.join(name);
    if source.exists() {
        atomic_write(target, &fs::read(source)?)?;
    } else if dir.join(format!("{name}.missing")).exists() && target.exists() {
        fs::remove_file(target)?;
    } else {
        bail!("backup is incomplete");
    }
    Ok(())
}

fn list_backups(paths: &ManagerPaths) -> anyhow::Result<Vec<BackupEntry>> {
    if !paths.backups_dir.exists() {
        return Ok(Vec::new());
    }
    let mut entries = fs::read_dir(&paths.backups_dir)?
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .map(|e| BackupEntry {
            id: e.file_name().to_string_lossy().into_owned(),
            path: e.path().display().to_string(),
        })
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| b.id.cmp(&a.id));
    Ok(entries)
}

fn prune_backups(paths: &ManagerPaths) -> anyhow::Result<()> {
    for entry in list_backups(paths)?.into_iter().skip(BACKUP_KEEP) {
        fs::remove_dir_all(entry.path)?;
    }
    Ok(())
}

pub(super) fn atomic_write(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let target = if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        path.canonicalize()
            .with_context(|| format!("broken symlink {}", path.display()))?
    } else {
        path.to_path_buf()
    };
    let parent = target.parent().context("file has no parent")?;
    fs::create_dir_all(parent)?;
    let tmp = parent.join(format!(
        ".{}.omnyssh-tmp-{}",
        target.file_name().unwrap_or_default().to_string_lossy(),
        std::process::id()
    ));
    let result = (|| {
        let mut file = OpenOptions::new().create_new(true).write(true).open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        // ReplaceFileW rejects an open replacement file with ERROR_SHARING_VIOLATION.
        drop(file);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))?;
        }
        #[cfg(not(windows))]
        fs::rename(&tmp, &target)?;
        #[cfg(windows)]
        atomic_replace_windows(&tmp, &target)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(windows)]
fn atomic_replace_windows(source: &Path, target: &Path) -> anyhow::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, ReplaceFileW, MOVEFILE_WRITE_THROUGH, REPLACEFILE_WRITE_THROUGH,
    };
    let wide = |path: &Path| {
        path.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>()
    };
    let source_w = wide(source);
    let target_w = wide(target);
    let ok = unsafe {
        if target.exists() {
            ReplaceFileW(
                target_w.as_ptr(),
                source_w.as_ptr(),
                std::ptr::null(),
                REPLACEFILE_WRITE_THROUGH,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        } else {
            MoveFileExW(source_w.as_ptr(), target_w.as_ptr(), MOVEFILE_WRITE_THROUGH)
        }
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error())
            .context("atomic Windows file replacement failed");
    }
    Ok(())
}

fn set_private_dir_permissions(_path: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(_path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn ensure_ssh_directory(paths: &ManagerPaths) -> anyhow::Result<()> {
    if paths.ssh_dir.exists() {
        if !paths.ssh_dir.is_dir() {
            bail!("{} is not a directory", paths.ssh_dir.display());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            let metadata = fs::metadata(&paths.ssh_dir)?;
            if metadata.uid() != unsafe { libc::geteuid() } {
                bail!(
                    "{} is not owned by the current user",
                    paths.ssh_dir.display()
                );
            }
            if metadata.permissions().mode() & 0o022 != 0 {
                bail!(
                    "{} is writable by another user; fix its permissions before saving",
                    paths.ssh_dir.display()
                );
            }
        }
        return Ok(());
    }
    fs::create_dir_all(&paths.ssh_dir)
        .with_context(|| format!("Failed to create {}", paths.ssh_dir.display()))?;
    set_private_dir_permissions(&paths.ssh_dir)
}

fn inspect_write_target(
    paths: &ManagerPaths,
    path: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) -> anyhow::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let link_metadata = fs::symlink_metadata(path)?;
    let target = if link_metadata.file_type().is_symlink() {
        let resolved = path.canonicalize()?;
        if !resolved.starts_with(&paths.ssh_dir) {
            diagnostics.push(Diagnostic {
                severity: DiagnosticSeverity::Warning,
                code: "external-symlink".into(),
                message: "write target is outside ~/.ssh and requires explicit review".into(),
                path: Some(resolved.display().to_string()),
                line: None,
            });
        }
        resolved
    } else {
        path.to_path_buf()
    };
    let metadata = fs::metadata(&target)?;
    if !metadata.is_file() {
        diagnostics.push(blocking(
            "not-regular-file",
            "write target is not a regular file",
            &target,
        ));
        return Ok(());
    }
    if metadata.permissions().readonly() {
        diagnostics.push(blocking("read-only", "write target is read-only", &target));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if metadata.uid() != unsafe { libc::geteuid() } {
            diagnostics.push(blocking(
                "wrong-owner",
                "write target is not owned by the current user",
                &target,
            ));
        }
        if metadata.permissions().mode() & 0o022 != 0 {
            diagnostics.push(blocking(
                "unsafe-permissions",
                "write target is writable by another user",
                &target,
            ));
        }
    }
    Ok(())
}

fn blocking(code: &str, message: impl Into<String>, path: &Path) -> Diagnostic {
    Diagnostic {
        severity: DiagnosticSeverity::Blocking,
        code: code.into(),
        message: message.into(),
        path: Some(path.display().to_string()),
        line: None,
    }
}

struct LockGuard {
    path: PathBuf,
    _file: File,
}
impl LockGuard {
    fn acquire(path: &Path) -> anyhow::Result<Self> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)
            .with_context(|| format!("SSH config is locked: {}", path.display()))?;
        writeln!(
            file,
            "pid={}\ntime={}",
            std::process::id(),
            Utc::now().to_rfc3339()
        )?;
        Ok(Self {
            path: path.to_path_buf(),
            _file: file,
        })
    }
}
impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn sample(alias: &str) -> ManagedHost {
        ManagedHost {
            alias: alias.into(),
            hostname: "example.com".into(),
            user: "deploy".into(),
            port: 22,
            identity_file: None,
            proxy_jump: None,
            ciphers: None,
        }
    }

    #[test]
    fn managed_round_trip_is_stable() {
        let mut legacy = sample("prod-1");
        legacy.ciphers = Some("+aes256-cbc".into());
        let hosts = vec![legacy];
        let text = render_managed(&hosts, "\n");
        assert_eq!(parse_managed(&text).unwrap(), hosts);
    }

    #[test]
    fn rejects_complex_or_injectable_managed_config() {
        assert!(parse_managed("Host *\n  User root\n").is_err());
        let mut host = sample("bad name");
        assert!(validate_host(&host).is_err());
        host.alias = "ok".into();
        host.hostname = "x\nProxyCommand bad".into();
        assert!(validate_host(&host).is_err());
    }

    #[test]
    fn preview_never_touches_real_home_and_detects_stale_write() {
        let dir = tempdir().unwrap();
        let paths = ManagerPaths::for_home(dir.path());
        let previewed = preview(&paths, &[sample("prod")]).unwrap();
        fs::create_dir_all(&paths.ssh_dir).unwrap();
        fs::write(&paths.main_config, "# external\n").unwrap();
        let err = apply(
            &paths,
            &[sample("prod")],
            &previewed.main_hash,
            &previewed.managed_hash,
            true,
        )
        .unwrap_err();
        assert!(err.to_string().contains("changed after preview"));
    }

    #[test]
    fn preview_blocks_a_user_owned_exact_alias() {
        let dir = tempdir().unwrap();
        let paths = ManagerPaths::for_home(dir.path());
        fs::create_dir_all(&paths.ssh_dir).unwrap();
        fs::write(&paths.main_config, "Host prod\n  HostName user.example\n").unwrap();
        let prepared = preview(&paths, &[sample("prod")]).unwrap();
        assert!(!prepared.can_apply);
        assert!(prepared
            .warnings
            .iter()
            .any(|item| item.code == "alias-conflict"));
    }

    #[test]
    fn include_is_inserted_at_top_once() {
        let original = "Host *\n  ServerAliveInterval 30\n";
        let installed = install_include(original).unwrap();
        assert!(installed.starts_with("# OmnySSH managed hosts\nInclude"));
        assert_eq!(install_include(&installed).unwrap(), installed);
    }

    #[test]
    fn inspection_exposes_main_and_managed_config_sources() {
        let dir = tempdir().unwrap();
        let paths = ManagerPaths::for_home(dir.path());
        fs::create_dir_all(&paths.ssh_dir).unwrap();
        fs::write(
            &paths.main_config,
            format!("{MANAGED_INCLUDE}\nHost legacy\n  HostName legacy.example.com\n"),
        )
        .unwrap();
        fs::write(
            &paths.managed_config,
            "Host managed\n  HostName managed.example.com\n  User root\n  Port 22\n",
        )
        .unwrap();

        let snapshot = inspect(&paths).unwrap();

        assert!(snapshot.sources.iter().any(|source| {
            source.path == paths.main_config.display().to_string()
                && source
                    .content
                    .as_deref()
                    .is_some_and(|content| content.contains("Host legacy"))
        }));
        assert!(snapshot.sources.iter().any(|source| {
            source.path == paths.managed_config.display().to_string()
                && source
                    .content
                    .as_deref()
                    .is_some_and(|content| content.contains("Host managed"))
        }));
    }

    #[cfg(windows)]
    #[test]
    fn atomic_write_replaces_an_existing_windows_file() {
        let dir = tempdir().unwrap();
        let target = dir.path().join("config");
        fs::write(&target, "old contents\n").unwrap();

        atomic_write(&target, b"new contents\n").unwrap_or_else(|error| panic!("{error:#}"));

        assert_eq!(fs::read_to_string(target).unwrap(), "new contents\n");
    }

    #[test]
    fn apply_uses_only_temporary_home_and_creates_recoverable_backup() {
        let dir = tempdir().unwrap();
        let paths = ManagerPaths::for_home(dir.path());
        let desired = vec![sample("prod")];
        let prepared = preview(&paths, &desired).unwrap();
        let report = apply(
            &paths,
            &desired,
            &prepared.main_hash,
            &prepared.managed_hash,
            true,
        )
        .unwrap();
        assert!(fs::read_to_string(&paths.main_config)
            .unwrap()
            .starts_with("# OmnySSH"));
        assert_eq!(
            parse_managed(&fs::read_to_string(&paths.managed_config).unwrap()).unwrap(),
            desired
        );
        assert!(paths
            .backups_dir
            .join(&report.backup_id)
            .join("config.missing")
            .exists());

        let restore_preview = preview_restore(&paths, &report.backup_id).unwrap();
        restore(
            &paths,
            &report.backup_id,
            &restore_preview.main_hash,
            &restore_preview.managed_hash,
        )
        .unwrap();
        assert!(!paths.main_config.exists());
        assert!(!paths.managed_config.exists());
    }
}
