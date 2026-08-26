//! Safe catalog and file lifecycle for user SSH key pairs.
//!
//! Private key contents never leave this module. Callers receive only metadata
//! and paths. Every destructive delete creates a recoverable backup first.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::ssh_config_manager::atomic_write;

#[derive(Debug, Clone)]
pub struct KeyManagerPaths {
    pub ssh_dir: PathBuf,
    pub catalog: PathBuf,
    pub backups_dir: PathBuf,
}

impl KeyManagerPaths {
    pub fn for_home(home: &Path, app_dir: &Path) -> Self {
        Self {
            ssh_dir: home.join(".ssh"),
            catalog: app_dir.join("ssh_keys.toml"),
            backups_dir: app_dir.join("ssh-key-backups"),
        }
    }

    pub fn discover() -> anyhow::Result<Self> {
        let home = dirs::home_dir().context("Cannot determine home directory")?;
        let app_dir = crate::utils::platform::app_config_dir()
            .context("Cannot determine OmnySSH config directory")?;
        Ok(Self::for_home(&home, &app_dir))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SshKeyRecord {
    pub id: String,
    pub name: String,
    pub private_path: String,
    pub public_path: String,
    pub key_type: String,
    #[serde(default)]
    pub available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredKeyPair {
    pub private_path: String,
    pub public_path: String,
    pub suggested_name: String,
    pub key_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshKeyBackup {
    pub id: String,
    pub key_id: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct KeyManagerSnapshot {
    pub records: Vec<SshKeyRecord>,
    pub discovered: Vec<DiscoveredKeyPair>,
    pub backups: Vec<SshKeyBackup>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct Catalog {
    keys: Vec<SshKeyRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BackupManifest {
    record: SshKeyRecord,
}

pub fn inspect_default() -> anyhow::Result<KeyManagerSnapshot> {
    inspect(&KeyManagerPaths::discover()?)
}

pub fn inspect(paths: &KeyManagerPaths) -> anyhow::Result<KeyManagerSnapshot> {
    let mut catalog = load_catalog(paths)?;
    for record in &mut catalog.keys {
        record.available =
            Path::new(&record.private_path).is_file() && Path::new(&record.public_path).is_file();
    }
    let registered = catalog
        .keys
        .iter()
        .map(|record| normal_path(Path::new(&record.private_path)))
        .collect::<std::collections::HashSet<_>>();
    let discovered = discover_pairs(paths)?
        .into_iter()
        .filter(|pair| !registered.contains(&normal_path(Path::new(&pair.private_path))))
        .collect();
    Ok(KeyManagerSnapshot {
        records: catalog.keys,
        discovered,
        backups: list_backups(paths)?,
    })
}

pub async fn create_key(
    paths: &KeyManagerPaths,
    name: &str,
    file_stem: &str,
    key_type: &str,
) -> anyhow::Result<SshKeyRecord> {
    let name = validate_name(name)?;
    validate_file_stem(file_stem)?;
    let (kind, bits, label) = match key_type {
        "ed25519" => ("ed25519", None, "ed25519"),
        "rsa4096" => ("rsa", Some("4096"), "rsa4096"),
        _ => bail!("unsupported key type"),
    };
    ensure_ssh_dir(paths)?;
    let private = paths.ssh_dir.join(file_stem);
    let public = paths.ssh_dir.join(format!("{file_stem}.pub"));
    if private.exists() || public.exists() {
        bail!("a key file with that name already exists");
    }

    let mut command = tokio::process::Command::new("ssh-keygen");
    command.arg("-t").arg(kind);
    if let Some(bits) = bits {
        command.arg("-b").arg(bits);
    }
    command
        .arg("-f")
        .arg(&private)
        .arg("-N")
        .arg("")
        .arg("-C")
        .arg(format!("OmnySSH {name}"));
    #[cfg(windows)]
    {
        command.creation_flags(0x0800_0000);
    }
    let output = command.output().await.context("Failed to run ssh-keygen")?;
    if !output.status.success() {
        bail!(
            "ssh-keygen failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    register_pair(paths, &name, &private, label)
}

pub fn import_key(
    paths: &KeyManagerPaths,
    name: &str,
    private_path: &Path,
) -> anyhow::Result<SshKeyRecord> {
    let name = validate_name(name)?;
    let private = validate_pair_path(paths, private_path)?;
    let key_type = public_key_type(&public_path_for(&private))?;
    register_pair(paths, &name, &private, &key_type)
}

pub fn rename_key(paths: &KeyManagerPaths, id: &str, name: &str) -> anyhow::Result<()> {
    let name = validate_name(name)?;
    let mut catalog = load_catalog(paths)?;
    ensure_unique_name(&catalog, &name, Some(id))?;
    let record = catalog
        .keys
        .iter_mut()
        .find(|record| record.id == id)
        .context("key record not found")?;
    record.name = name;
    save_catalog(paths, &catalog)
}

pub fn backup_key(paths: &KeyManagerPaths, id: &str) -> anyhow::Result<String> {
    let catalog = load_catalog(paths)?;
    let record = catalog
        .keys
        .iter()
        .find(|record| record.id == id)
        .context("key record not found")?;
    backup_record(paths, record)
}

/// Returns only the public OpenSSH line. Private key bytes never leave this module.
pub fn read_public_key(paths: &KeyManagerPaths, id: &str) -> anyhow::Result<String> {
    let catalog = load_catalog(paths)?;
    let record = catalog
        .keys
        .iter()
        .find(|record| record.id == id)
        .context("key record not found")?;
    let private = validate_pair_path(paths, Path::new(&record.private_path))?;
    let public = public_path_for(&private);
    public_key_type(&public)?;
    let value = fs::read_to_string(&public)?;
    Ok(value.trim().to_string())
}

pub fn delete_key(paths: &KeyManagerPaths, id: &str) -> anyhow::Result<String> {
    let mut catalog = load_catalog(paths)?;
    let index = catalog
        .keys
        .iter()
        .position(|record| record.id == id)
        .context("key record not found")?;
    let record = catalog.keys[index].clone();
    let private = validate_pair_path(paths, Path::new(&record.private_path))?;
    let public = public_path_for(&private);
    let backup_id = backup_record(paths, &record)?;
    if let Err(error) = fs::remove_file(&private) {
        return Err(error).with_context(|| format!("Failed to delete {}", private.display()));
    }
    if let Err(error) = fs::remove_file(&public) {
        let _ = restore_pair_files(paths, &backup_id, &private, &public);
        return Err(error).with_context(|| format!("Failed to delete {}", public.display()));
    }
    catalog.keys.remove(index);
    if let Err(error) = save_catalog(paths, &catalog) {
        let _ = restore_pair_files(paths, &backup_id, &private, &public);
        return Err(error).context("Failed to update SSH key catalog after deletion");
    }
    Ok(backup_id)
}

pub fn restore_key(paths: &KeyManagerPaths, backup_id: &str) -> anyhow::Result<SshKeyRecord> {
    validate_backup_id(backup_id)?;
    let dir = paths.backups_dir.join(backup_id);
    let manifest: BackupManifest = toml::from_str(
        &fs::read_to_string(dir.join("manifest.toml")).context("backup manifest is missing")?,
    )?;
    let private = PathBuf::from(&manifest.record.private_path);
    let public = PathBuf::from(&manifest.record.public_path);
    ensure_direct_child(paths, &private)?;
    ensure_direct_child(paths, &public)?;
    if private.exists() || public.exists() {
        bail!("target key files already exist; delete them before restoring");
    }
    let mut catalog = load_catalog(paths)?;
    ensure_unique_name(&catalog, &manifest.record.name, Some(&manifest.record.id))?;
    ensure_ssh_dir(paths)?;
    atomic_write(&private, &fs::read(dir.join("private"))?)?;
    atomic_write(&public, &fs::read(dir.join("public"))?)?;
    let mut record = manifest.record;
    record.available = true;
    catalog.keys.retain(|item| item.id != record.id);
    catalog.keys.push(record.clone());
    save_catalog(paths, &catalog)?;
    Ok(record)
}

fn register_pair(
    paths: &KeyManagerPaths,
    name: &str,
    private: &Path,
    key_type: &str,
) -> anyhow::Result<SshKeyRecord> {
    let private = validate_pair_path(paths, private)?;
    let public = public_path_for(&private);
    let mut catalog = load_catalog(paths)?;
    let id = path_id(&private);
    if catalog.keys.iter().any(|record| record.id == id) {
        bail!("this key pair is already loaded");
    }
    ensure_unique_name(&catalog, name, None)?;
    let record = SshKeyRecord {
        id,
        name: name.to_string(),
        private_path: private.display().to_string(),
        public_path: public.display().to_string(),
        key_type: key_type.to_string(),
        available: true,
    };
    catalog.keys.push(record.clone());
    save_catalog(paths, &catalog)?;
    Ok(record)
}

fn discover_pairs(paths: &KeyManagerPaths) -> anyhow::Result<Vec<DiscoveredKeyPair>> {
    if !paths.ssh_dir.exists() {
        return Ok(Vec::new());
    }
    let mut pairs = Vec::new();
    for entry in fs::read_dir(&paths.ssh_dir)? {
        let entry = entry?;
        let private = entry.path();
        if !private.is_file()
            || private.extension().is_some_and(|ext| ext == "pub")
            || fs::symlink_metadata(&private)?.file_type().is_symlink()
        {
            continue;
        }
        let public = public_path_for(&private);
        if !public.is_file() {
            continue;
        }
        let key_type = match public_key_type(&public) {
            Ok(value) => value,
            Err(_) => continue,
        };
        pairs.push(DiscoveredKeyPair {
            private_path: private.display().to_string(),
            public_path: public.display().to_string(),
            suggested_name: private
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            key_type,
        });
    }
    pairs.sort_by(|a, b| a.private_path.cmp(&b.private_path));
    Ok(pairs)
}

fn backup_record(paths: &KeyManagerPaths, record: &SshKeyRecord) -> anyhow::Result<String> {
    let private = validate_pair_path(paths, Path::new(&record.private_path))?;
    let public = public_path_for(&private);
    let id = format!(
        "{}-{}-{}",
        Utc::now().format("%Y%m%dT%H%M%S%.6fZ"),
        std::process::id(),
        record.id
    );
    let dir = paths.backups_dir.join(&id);
    fs::create_dir_all(&dir)?;
    atomic_write(&dir.join("private"), &fs::read(&private)?)?;
    atomic_write(&dir.join("public"), &fs::read(&public)?)?;
    atomic_write(
        &dir.join("manifest.toml"),
        toml::to_string_pretty(&BackupManifest {
            record: record.clone(),
        })?
        .as_bytes(),
    )?;
    set_private_dir(&dir)?;
    Ok(id)
}

fn restore_pair_files(
    paths: &KeyManagerPaths,
    backup_id: &str,
    private: &Path,
    public: &Path,
) -> anyhow::Result<()> {
    let dir = paths.backups_dir.join(backup_id);
    atomic_write(private, &fs::read(dir.join("private"))?)?;
    atomic_write(public, &fs::read(dir.join("public"))?)?;
    Ok(())
}

fn list_backups(paths: &KeyManagerPaths) -> anyhow::Result<Vec<SshKeyBackup>> {
    if !paths.backups_dir.exists() {
        return Ok(Vec::new());
    }
    let mut backups = Vec::new();
    for entry in fs::read_dir(&paths.backups_dir)? {
        let entry = entry?;
        if !entry.path().is_dir() {
            continue;
        }
        let Ok(content) = fs::read_to_string(entry.path().join("manifest.toml")) else {
            continue;
        };
        let Ok(manifest) = toml::from_str::<BackupManifest>(&content) else {
            continue;
        };
        backups.push(SshKeyBackup {
            id: entry.file_name().to_string_lossy().into_owned(),
            key_id: manifest.record.id,
            name: manifest.record.name,
        });
    }
    backups.sort_by(|a, b| b.id.cmp(&a.id));
    Ok(backups)
}

fn load_catalog(paths: &KeyManagerPaths) -> anyhow::Result<Catalog> {
    if !paths.catalog.exists() {
        return Ok(Catalog::default());
    }
    toml::from_str(&fs::read_to_string(&paths.catalog)?).context("Invalid SSH key catalog")
}

fn save_catalog(paths: &KeyManagerPaths, catalog: &Catalog) -> anyhow::Result<()> {
    atomic_write(&paths.catalog, toml::to_string_pretty(catalog)?.as_bytes())
}

fn validate_pair_path(paths: &KeyManagerPaths, private: &Path) -> anyhow::Result<PathBuf> {
    ensure_direct_child(paths, private)?;
    if private.extension().is_some_and(|ext| ext == "pub") {
        bail!("select the private key, not the .pub file");
    }
    let metadata = fs::symlink_metadata(private)
        .with_context(|| format!("private key not found: {}", private.display()))?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        bail!("private key must be a regular file");
    }
    let public = public_path_for(private);
    if !public.is_file() || fs::symlink_metadata(&public)?.file_type().is_symlink() {
        bail!("matching public key file is missing");
    }
    Ok(private.to_path_buf())
}

fn ensure_direct_child(paths: &KeyManagerPaths, path: &Path) -> anyhow::Result<()> {
    let parent = path.parent().context("key path has no parent")?;
    let expected = normal_path(&paths.ssh_dir);
    if normal_path(parent) != expected || path.file_name().is_none() {
        bail!("key files must be direct children of ~/.ssh");
    }
    Ok(())
}

fn public_path_for(private: &Path) -> PathBuf {
    let mut value = private.as_os_str().to_os_string();
    value.push(".pub");
    PathBuf::from(value)
}

fn public_key_type(public: &Path) -> anyhow::Result<String> {
    let content = fs::read_to_string(public)?;
    let kind = content
        .split_whitespace()
        .next()
        .context("empty public key")?;
    match kind {
        "ssh-ed25519" => Ok("ed25519".into()),
        "ssh-rsa" => Ok("rsa".into()),
        "ecdsa-sha2-nistp256" | "ecdsa-sha2-nistp384" | "ecdsa-sha2-nistp521" => Ok("ecdsa".into()),
        value if value.starts_with("sk-") => Ok("security-key".into()),
        _ => bail!("unsupported public key type"),
    }
}

fn path_id(path: &Path) -> String {
    let hash = Sha256::digest(normal_path(path).as_bytes());
    hash[..12]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn validate_name(name: &str) -> anyhow::Result<String> {
    let value = name.trim();
    if value.is_empty() || value.chars().count() > 64 || value.chars().any(char::is_control) {
        bail!("key name must contain 1 to 64 printable characters");
    }
    Ok(value.to_string())
}

fn ensure_unique_name(
    catalog: &Catalog,
    name: &str,
    exclude_id: Option<&str>,
) -> anyhow::Result<()> {
    if catalog.keys.iter().any(|record| {
        Some(record.id.as_str()) != exclude_id && record.name.eq_ignore_ascii_case(name)
    }) {
        bail!("a key record with that name already exists");
    }
    Ok(())
}

fn validate_file_stem(value: &str) -> anyhow::Result<()> {
    if value.is_empty()
        || value.len() > 96
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        || matches!(
            value,
            "." | ".." | "config" | "known_hosts" | "authorized_keys"
        )
    {
        bail!("invalid key file name");
    }
    Ok(())
}

fn validate_backup_id(value: &str) -> anyhow::Result<()> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        bail!("invalid backup id");
    }
    Ok(())
}

fn ensure_ssh_dir(paths: &KeyManagerPaths) -> anyhow::Result<()> {
    fs::create_dir_all(&paths.ssh_dir)?;
    set_private_dir(&paths.ssh_dir)
}

fn set_private_dir(_path: &Path) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(_path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn normal_path(path: &Path) -> String {
    let value = path.to_string_lossy().replace('\\', "/");
    if cfg!(windows) {
        value.to_ascii_lowercase()
    } else {
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, KeyManagerPaths) {
        let temp = tempfile::tempdir().unwrap();
        let paths = KeyManagerPaths::for_home(temp.path(), &temp.path().join("app"));
        fs::create_dir_all(&paths.ssh_dir).unwrap();
        (temp, paths)
    }

    fn pair(paths: &KeyManagerPaths, stem: &str) -> PathBuf {
        let private = paths.ssh_dir.join(stem);
        fs::write(&private, "PRIVATE FIXTURE").unwrap();
        fs::write(public_path_for(&private), "ssh-ed25519 AAAATEST fixture\n").unwrap();
        private
    }

    #[test]
    fn importing_and_renaming_changes_only_catalog_metadata() {
        let (_temp, paths) = fixture();
        let private = pair(&paths, "id_demo");
        let record = import_key(&paths, "Demo", &private).unwrap();

        rename_key(&paths, &record.id, "Production key").unwrap();

        assert!(private.exists());
        assert!(public_path_for(&private).exists());
        let snapshot = inspect(&paths).unwrap();
        assert_eq!(snapshot.records[0].name, "Production key");
        assert_eq!(
            snapshot.records[0].private_path,
            private.display().to_string()
        );
    }

    #[test]
    fn deletion_always_backs_up_and_restore_recreates_the_pair() {
        let (_temp, paths) = fixture();
        let private = pair(&paths, "id_delete");
        let record = import_key(&paths, "Delete me", &private).unwrap();

        let backup = delete_key(&paths, &record.id).unwrap();
        assert!(!private.exists());
        assert!(!public_path_for(&private).exists());
        assert!(inspect(&paths).unwrap().records.is_empty());

        let restored = restore_key(&paths, &backup).unwrap();
        assert_eq!(restored.name, "Delete me");
        assert!(private.exists());
        assert!(public_path_for(&private).exists());
    }

    #[test]
    fn discovery_finds_pairs_without_reading_private_contents() {
        let (_temp, paths) = fixture();
        let private = pair(&paths, "id_existing");

        let snapshot = inspect(&paths).unwrap();

        assert_eq!(snapshot.discovered.len(), 1);
        assert_eq!(
            snapshot.discovered[0].private_path,
            private.display().to_string()
        );
        assert_eq!(snapshot.discovered[0].key_type, "ed25519");
    }

    #[test]
    fn public_key_copy_returns_only_the_public_line() {
        let (_temp, paths) = fixture();
        let private = pair(&paths, "id_copy");
        let record = import_key(&paths, "copy", &private).unwrap();

        let copied = read_public_key(&paths, &record.id).unwrap();
        assert_eq!(copied, "ssh-ed25519 AAAATEST fixture");
        assert!(!copied.contains("PRIVATE"));
    }

    #[test]
    fn record_names_are_unique_without_renaming_files() {
        let (_temp, paths) = fixture();
        let first = pair(&paths, "id_first");
        let second = pair(&paths, "id_second");
        let first = import_key(&paths, "Production", &first).unwrap();

        let duplicate = import_key(&paths, "production", &second).unwrap_err();
        assert!(duplicate.to_string().contains("name already exists"));
        rename_key(&paths, &first.id, "Production").unwrap();
    }

    #[test]
    fn import_rejects_a_pair_outside_the_ssh_directory() {
        let (temp, paths) = fixture();
        let private = temp.path().join("outside");
        fs::write(&private, "PRIVATE").unwrap();
        fs::write(public_path_for(&private), "ssh-ed25519 AAAATEST fixture\n").unwrap();

        let error = import_key(&paths, "Outside", &private).unwrap_err();

        assert!(error.to_string().contains("direct children"));
    }
}
