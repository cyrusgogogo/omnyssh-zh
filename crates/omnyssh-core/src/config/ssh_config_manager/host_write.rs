//! 从软件自身主机定向写回配置。只改一个明确的 Host 块，不使用有损导入器回写。

use super::*;

#[cfg(windows)]
mod windows_permissions;

fn private_directory(path: &Path) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        windows_permissions::private_directory(path)
    }
    #[cfg(not(windows))]
    {
        set_private_dir_permissions(path)
    }
}

#[derive(Debug)]
pub struct HostWritePreview {
    pub alias: String,
    pub target_path: String,
    pub existing: bool,
    pub diff: String,
    pub fingerprint: String,
    pub requires_key_warning: bool,
}

#[derive(Debug)]
pub struct HostWriteReport {
    pub backup_path: String,
    pub ssh_validation: String,
}

struct Source {
    path: PathBuf,
    canonical: PathBuf,
    text: Option<String>,
    includes: Vec<(usize, Vec<usize>)>,
}

struct Plan {
    sources: Vec<Source>,
    target: usize,
    next: String,
    desired: ManagedHost,
    preview: HostWritePreview,
}

pub fn preview(
    paths: &ManagerPaths,
    host: &Host,
    replace_identity: bool,
) -> anyhow::Result<HostWritePreview> {
    Ok(prepare(paths, host, replace_identity)?.preview)
}

pub fn apply(
    paths: &ManagerPaths,
    host: &Host,
    replace_identity: bool,
    expected_fingerprint: &str,
    confirm_non_key: bool,
) -> anyhow::Result<HostWriteReport> {
    ensure_ssh_directory(paths)?;
    let _lock = LockGuard::acquire(&paths.lock_file)?;
    let plan = prepare(paths, host, replace_identity)?;
    if plan.preview.fingerprint != expected_fingerprint {
        bail!("SSH configuration or host changed after preview; preview again before writing");
    }
    if plan.preview.requires_key_warning && !confirm_non_key {
        bail!("confirm the key-login warning before writing; passwords cannot be stored in SSH config");
    }
    if plan.preview.diff.is_empty() {
        bail!("SSH configuration already matches this host");
    }
    let ssh_validation = validate_candidate(paths, &plan)?;
    // ssh -G 期间用户也可能编辑文件，写入前再核对全部配置源和 Include 展开结果。
    if prepare(paths, host, replace_identity)?.preview.fingerprint != expected_fingerprint {
        bail!("SSH configuration changed during validation; preview again before writing");
    }
    let source = &plan.sources[plan.target];
    let backup = backup_source(paths, source)?;
    atomic_write(&source.path, plan.next.as_bytes())
        .with_context(|| format!("write failed; original saved in {}", backup.display()))?;
    if read_optional_text(&source.path)?.as_deref() != Some(plan.next.as_str()) {
        // 不覆盖可能来自外部编辑器的新内容；完整原件已备份，返回可恢复位置。
        bail!(
            "post-write verification failed; original saved in {}",
            backup.display()
        );
    }
    Ok(HostWriteReport {
        backup_path: backup.display().to_string(),
        ssh_validation,
    })
}

fn prepare(paths: &ManagerPaths, host: &Host, replace_identity: bool) -> anyhow::Result<Plan> {
    if host.source != HostSource::Manual {
        bail!("only an existing manual host can be written to SSH config");
    }
    let mut desired = ManagedHost {
        // 采用 SSH 主机后，即使软件内重命名，也优先找原来的别名。
        alias: host
            .original_ssh_host
            .as_deref()
            .unwrap_or(&host.name)
            .into(),
        hostname: host.hostname.clone(),
        user: host.user.clone(),
        port: host.port,
        identity_file: host.identity_file.clone(),
        proxy_jump: host.proxy_jump.clone(),
        ciphers: host.ciphers.clone(),
    };
    let mut sources = Vec::new();
    load_sources(paths, paths.main_config.clone(), &mut sources, 0)?;
    let mut matches = find_blocks(&sources, &desired.alias)?;
    if matches.is_empty() && desired.alias != host.name {
        desired.alias = host.name.clone();
        matches = find_blocks(&sources, &desired.alias)?;
    }
    if matches.len() > 1 {
        bail!("multiple Host blocks define '{}'; edit the SSH configuration manually to resolve ambiguity", desired.alias);
    }
    let existing = !matches.is_empty();
    let target = matches.first().map_or(0, |item| item.0);
    let source = &sources[target];
    let mut diagnostics = Vec::new();
    inspect_write_target(paths, &source.path, &mut diagnostics)?;
    if let Some(problem) = diagnostics
        .iter()
        .find(|d| d.severity == DiagnosticSeverity::Blocking)
    {
        bail!("{}: {}", source.path.display(), problem.message);
    }
    if source.path.exists() && !source.canonical.starts_with(paths.ssh_dir.canonicalize()?) {
        bail!(
            "target is outside ~/.ssh; update this external Include file manually: {}",
            source.path.display()
        );
    }
    let old = source.text.as_deref().unwrap_or_default();
    let mut written_fields = desired.clone();
    if !replace_identity
        && matches.first().is_some_and(|(_, start, end)| {
            old.lines().take(*end).skip(start + 1).any(|line| {
                directive(line).is_some_and(|(key, _)| key.eq_ignore_ascii_case("identityfile"))
            })
        })
    {
        // 未改密钥时不重新解释原身份文件（它可能含空格、令牌或多个值）。
        written_fields.identity_file = None;
    }
    validate_host(&written_fields)?;
    for value in [
        Some(written_fields.alias.as_str()),
        Some(&written_fields.hostname),
        Some(&written_fields.user),
        written_fields.identity_file.as_deref(),
        written_fields.proxy_jump.as_deref(),
        written_fields.ciphers.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if value.contains(['#', '\'', '"']) || value.ends_with('\\') {
            bail!("SSH config values cannot contain comments, quotes or trailing escapes");
        }
    }
    if desired.alias.starts_with('-') {
        bail!("SSH Host alias cannot begin with '-'");
    }
    let next = if let Some((_, start, end)) = matches.first() {
        patch_block(old, *start, *end, &desired, replace_identity)?
    } else {
        let newline = if old.contains("\r\n") { "\r\n" } else { "\n" };
        let block = render_managed(std::slice::from_ref(&desired), newline);
        let block = block
            .split_once(&format!("{newline}{newline}"))
            .context("missing Host block")?
            .1;
        // 保留顶部 Include 的全局作用域及管理器识别方式；新 Host 放在第一个
        // Host 块前。若更早的全局值遮蔽本次修改，ssh -G 会拒绝写入而非假成功。
        let insertion = old
            .split_inclusive('\n')
            .take_while(|line| {
                !directive(line).is_some_and(|(key, _)| key.eq_ignore_ascii_case("host"))
            })
            .map(str::len)
            .sum::<usize>();
        let (before, after) = old.split_at(insertion);
        let separator = if before.is_empty() || before.ends_with('\n') {
            ""
        } else {
            newline
        };
        format!("{before}{separator}{block}{newline}{after}")
    };
    let requires_key_warning = host.identity_file.is_none() || host.password.is_some();
    let mut hash = Sha256::new();
    for item in &sources {
        hash.update(format!(
            "{:?}\0{:?}\0{}\0",
            item.path,
            item.canonical,
            hash_optional(item.text.as_deref())
        ));
    }
    hash.update(format!("{target}\0{requires_key_warning}\0{next}"));
    let preview = HostWritePreview {
        alias: desired.alias.clone(),
        target_path: source.path.display().to_string(),
        existing,
        diff: simple_diff(old, &next),
        fingerprint: format!("{:x}", hash.finalize()),
        requires_key_warning,
    };
    Ok(Plan {
        sources,
        target,
        next,
        desired,
        preview,
    })
}

/// 返回指令名和值的起点；未知指令保持原样，不解析其中的 shell 命令。
fn directive(line: &str) -> Option<(&str, usize)> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let offset = line.len() - trimmed.len();
    let end = trimmed.find(|c: char| c.is_whitespace() || c == '=')?;
    let value = trimmed[end..].trim_start_matches(|c: char| c.is_whitespace() || c == '=');
    Some((&trimmed[..end], offset + trimmed.len() - value.len()))
}

fn comment_start(value: &str) -> usize {
    let mut quoted = false;
    let mut escaped = false;
    for (index, c) in value.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' {
            escaped = true;
        } else if c == '"' {
            quoted = !quoted;
        } else if c == '#' && !quoted {
            return index;
        }
    }
    value.len()
}

fn arguments(value: &str) -> anyhow::Result<Vec<String>> {
    let value = &value[..comment_start(value)];
    if value.contains("\\\"")
        || value
            .as_bytes()
            .windows(2)
            .any(|pair| pair[0] == b'\\' && pair[1].is_ascii_whitespace())
    {
        bail!("escaped Host or Include arguments require manual SSH config editing");
    }
    let mut out = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    for c in value.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !word.is_empty() {
                    out.push(std::mem::take(&mut word));
                }
            }
            _ => word.push(c),
        }
    }
    if quoted {
        bail!("unclosed quote in Host or Include directive");
    }
    if !word.is_empty() {
        out.push(word);
    }
    if out.is_empty() {
        bail!("empty Host or Include directive");
    }
    Ok(out)
}

fn load_sources(
    paths: &ManagerPaths,
    path: PathBuf,
    sources: &mut Vec<Source>,
    depth: usize,
) -> anyhow::Result<usize> {
    if depth > MAX_INCLUDE_DEPTH || sources.len() >= MAX_SOURCE_FILES {
        bail!("SSH Include depth or file count exceeds the safety limit");
    }
    // 大文件在读取前拒绝；缺失的主配置是合法的新增目标。
    if path.exists() && fs::metadata(&path)?.len() > MAX_FILE_BYTES {
        bail!("SSH configuration exceeds 2 MiB: {}", path.display());
    }
    let canonical = if path.exists() {
        path.canonicalize()?
    } else {
        path.clone()
    };
    if sources.iter().any(|s| s.canonical == canonical) {
        bail!("repeated or cyclic SSH Include: {}", path.display());
    }
    let text = read_optional_text(&path)?;
    let size = sources
        .iter()
        .map(|s| s.text.as_ref().map_or(0, String::len))
        .sum::<usize>()
        + text.as_ref().map_or(0, String::len);
    if size as u64 > MAX_SOURCE_BYTES {
        bail!("SSH configuration sources exceed 16 MiB");
    }
    let index = sources.len();
    sources.push(Source {
        path: path.clone(),
        canonical,
        text: text.clone(),
        includes: Vec::new(),
    });
    let mut conditional = false;
    for (line_no, line) in text.as_deref().unwrap_or_default().lines().enumerate() {
        let Some((key, value_start)) = directive(line) else {
            continue;
        };
        // 不把带引号/转义的关键字漏当成普通选项，避免绕过 Match/Include 的边界。
        if !key.bytes().all(|c| c.is_ascii_alphabetic()) {
            bail!(
                "quoted or escaped SSH directive keywords require manual editing: {}:{}",
                path.display(),
                line_no + 1
            );
        }
        if key.eq_ignore_ascii_case("match") {
            // 不评估 Match（特别是 exec），校验阶段不允许触发用户命令。
            bail!(
                "Match rules require manual SSH config editing: {}:{}",
                path.display(),
                line_no + 1
            );
        }
        if key.eq_ignore_ascii_case("host") {
            conditional = arguments(&line[value_start..])? != ["*"];
        }
        if !key.eq_ignore_ascii_case("include") {
            continue;
        }
        if conditional {
            bail!(
                "conditional Include requires manual SSH config editing: {}:{}",
                path.display(),
                line_no + 1
            );
        }
        let mut children = Vec::new();
        for token in arguments(&line[value_start..])? {
            if token.contains(['%', '$', '"'])
                || (token.starts_with('~') && !token.starts_with("~/"))
            {
                bail!(
                    "dynamic Include paths require manual editing: {}",
                    path.display()
                );
            }
            let expanded = expand_include_token(&token, &paths.ssh_dir);
            let mut files =
                glob::glob(&expanded.to_string_lossy())?.collect::<Result<Vec<_>, _>>()?;
            files.sort();
            for file in files {
                children.push(load_sources(paths, file, sources, depth + 1)?);
            }
        }
        sources[index].includes.push((line_no, children));
    }
    Ok(index)
}

fn find_blocks(sources: &[Source], alias: &str) -> anyhow::Result<Vec<(usize, usize, usize)>> {
    let mut found = Vec::new();
    for (file, source) in sources.iter().enumerate() {
        let lines = source
            .text
            .as_deref()
            .unwrap_or_default()
            .split_inclusive('\n')
            .collect::<Vec<_>>();
        for (start, line) in lines.iter().enumerate() {
            let Some((key, value)) = directive(line) else {
                continue;
            };
            if !key.eq_ignore_ascii_case("host") {
                continue;
            }
            let patterns = arguments(&line[value..])?;
            if !patterns.iter().any(|p| p == alias) {
                continue;
            }
            if patterns.len() != 1 {
                bail!(
                    "Host '{}' shares a multi-pattern block; split it manually before writing",
                    alias
                );
            }
            let end = (start + 1..lines.len())
                .find(|i| {
                    directive(lines[*i]).is_some_and(|(key, _)| {
                        key.eq_ignore_ascii_case("host") || key.eq_ignore_ascii_case("match")
                    })
                })
                .unwrap_or(lines.len());
            found.push((file, start, end));
        }
    }
    Ok(found)
}

fn patch_block(
    text: &str,
    start: usize,
    end: usize,
    host: &ManagedHost,
    replace_identity: bool,
) -> anyhow::Result<String> {
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut lines = text
        .split_inclusive('\n')
        .map(str::to_string)
        .collect::<Vec<_>>();
    let mut fields = vec![
        ("HostName", host.hostname.clone()),
        ("User", host.user.clone()),
        ("Port", host.port.to_string()),
    ];
    for (key, value) in [
        ("IdentityFile", &host.identity_file),
        ("ProxyJump", &host.proxy_jump),
        ("Ciphers", &host.ciphers),
    ] {
        if let Some(value) = value {
            fields.push((key, value.clone()));
        }
    }
    let mut additions = String::new();
    for (key, value) in fields {
        let matching = (start + 1..end)
            .filter(|i| directive(&lines[*i]).is_some_and(|(k, _)| k.eq_ignore_ascii_case(key)))
            .collect::<Vec<_>>();
        if key == "IdentityFile" && !replace_identity && !matching.is_empty() {
            // 导入 DTO 不包含完整身份列表；没明确换密钥时保留全部 IdentityFile。
            continue;
        }
        if matching.len() > 1 {
            bail!(
                "multiple {key} directives in Host '{}'; edit this field manually",
                host.alias
            );
        }
        if let Some(index) = matching.first() {
            let line = &lines[*index];
            let (_, value_start) = directive(line).context("missing directive")?;
            let value_end = value_start
                + line[value_start..value_start + comment_start(&line[value_start..])]
                    .trim_end()
                    .len();
            lines[*index] = format!("{}{value}{}", &line[..value_start], &line[value_end..]);
        } else {
            additions.push_str(&format!("    {key} {value}{newline}"));
        }
    }
    if !additions.is_empty() {
        // 新字段紧随 Host 头部，不改变块尾注释与下一个主机的相对位置。
        if !lines[start].ends_with('\n') {
            lines[start].push_str(newline);
        }
        lines[start].push_str(&additions);
    }
    Ok(lines.concat())
}

struct ValidationDir(PathBuf);
impl Drop for ValidationDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn unique_id() -> String {
    format!(
        "{}-{}",
        Utc::now().format("%Y%m%dT%H%M%S%.9fZ"),
        std::process::id()
    )
}

fn validate_candidate(paths: &ManagerPaths, plan: &Plan) -> anyhow::Result<String> {
    if paths
        .ssh_dir
        .to_string_lossy()
        .chars()
        .any(|c| c == '"' || c.is_control())
    {
        bail!("SSH directory path cannot contain quotes or control characters");
    }
    let path = paths
        .ssh_dir
        .join(format!(".omnyssh-host-check-{}", unique_id()));
    fs::create_dir(&path)?;
    let dir = ValidationDir(path);
    private_directory(&dir.0)?;
    // 镜像 Include 图而非执行原文件，也不使用真实主目录作为测试数据。
    for (index, source) in plan.sources.iter().enumerate() {
        let text = if index == plan.target {
            &plan.next
        } else {
            source.text.as_deref().unwrap_or_default()
        };
        let mut lines = text
            .split_inclusive('\n')
            .map(str::to_string)
            .collect::<Vec<_>>();
        // 新增 Host 会移动主文件行号，按 Include 的出现次序替换，不依赖旧行号。
        let include_lines = lines
            .iter()
            .enumerate()
            .filter_map(|(i, line)| {
                directive(line)
                    .filter(|(key, _)| key.eq_ignore_ascii_case("include"))
                    .map(|_| i)
            })
            .collect::<Vec<_>>();
        for (line, (_, children)) in include_lines.into_iter().zip(&source.includes) {
            lines[line] = children
                .iter()
                .map(|child| {
                    format!(
                        "Include \"{}\"\n",
                        dir.0
                            .join(format!("{child}.conf"))
                            .to_string_lossy()
                            .replace('\\', "/")
                    )
                })
                .collect();
        }
        atomic_write(
            &dir.0.join(format!("{index}.conf")),
            lines.concat().as_bytes(),
        )?;
    }
    let mut command = Command::new("ssh");
    command
        .args(["-G", "-o", "CanonicalizeHostname=no", "-F"])
        .arg(dir.0.join("0.conf"))
        .arg(&plan.desired.alias);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let output = command
        .output()
        .context("system ssh is required to validate SSH config")?;
    if !output.status.success() {
        bail!(
            "ssh -G rejected the candidate: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let effective = String::from_utf8(output.stdout)?;
    for (key, value) in [
        ("hostname", plan.desired.hostname.clone()),
        ("user", plan.desired.user.clone()),
        ("port", plan.desired.port.to_string()),
    ] {
        let actual = effective
            .lines()
            .find_map(|line| line.strip_prefix(&format!("{key} ")));
        if actual != Some(value.as_str()) {
            bail!(
                "{key} is overridden by an earlier SSH rule; adjust configuration order manually"
            );
        }
    }
    Ok("ssh -G: validated without connecting".into())
}

fn backup_source(paths: &ManagerPaths, source: &Source) -> anyhow::Result<PathBuf> {
    let base = paths.ssh_dir.join(".omnyssh-host-backups");
    if fs::symlink_metadata(&base).is_ok_and(|m| m.file_type().is_symlink()) {
        bail!("SSH host backup directory cannot be a symlink");
    }
    fs::create_dir_all(&base)?;
    private_directory(&base)?;
    let dir = base.join(unique_id());
    fs::create_dir(&dir)?;
    private_directory(&dir)?;
    atomic_write(
        &dir.join("target.txt"),
        source.path.to_string_lossy().as_bytes(),
    )?;
    match &source.text {
        Some(text) => atomic_write(&dir.join("original.conf"), text.as_bytes())?,
        None => atomic_write(&dir.join("original.missing"), b"")?,
    }
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::{tempdir, TempDir};

    fn fixture(text: &str) -> (TempDir, ManagerPaths, Host) {
        let home = tempdir().unwrap();
        let paths = ManagerPaths::for_home(home.path());
        ensure_ssh_directory(&paths).unwrap();
        atomic_write(&paths.main_config, text.as_bytes()).unwrap();
        let host = Host {
            name: "lan".into(),
            hostname: "192.168.1.20".into(),
            user: "root".into(),
            port: 22,
            source: HostSource::Manual,
            ..Host::default()
        };
        (home, paths, host)
    }

    #[test]
    fn updates_original_alias_preserving_comments_unknown_options_and_crlf() {
        let (_home, paths, mut host) = fixture("# 我的主机\r\nHost \"lan\" # alias\r\n\tHostName = 192.168.1.10  # IP\r\n  User root\r\n  Port 22\r\n  Ciphers +aes256-cbc\r\n  ServerAliveInterval 30\r\n\r\nHost other\r\n  HostName untouched\r\n");
        host.name = "renamed".into();
        host.original_ssh_host = Some("lan".into());
        let plan = prepare(&paths, &host, false).unwrap();
        assert!(plan.preview.existing);
        assert_eq!(plan.preview.alias, "lan");
        assert_eq!(
            plan.next,
            fs::read_to_string(&paths.main_config)
                .unwrap()
                .replace("192.168.1.10", "192.168.1.20")
        );
    }

    #[test]
    fn adds_missing_alias_without_changing_existing_global_defaults() {
        let (_home, paths, host) =
            fixture("ServerAliveInterval 30\nHost other\n  HostName untouched\n");
        let plan = prepare(&paths, &host, false).unwrap();
        assert!(!plan.preview.existing);
        assert!(plan.next.starts_with("ServerAliveInterval 30\nHost lan\n"));
        assert!(plan.next.ends_with("Host other\n  HostName untouched\n"));
        assert_eq!(plan.next.matches("Host lan\n").count(), 1);
    }

    #[test]
    fn preview_neither_creates_ssh_directory_nor_exposes_password() {
        let home = tempdir().unwrap();
        let paths = ManagerPaths::for_home(home.path());
        let host = Host {
            name: "lan".into(),
            hostname: "192.168.1.20".into(),
            user: "root".into(),
            password: Some("secret-not-exported".into()),
            source: HostSource::Manual,
            ..Host::default()
        };
        let preview = preview(&paths, &host, false).unwrap();
        assert!(preview.requires_key_warning);
        assert!(!format!("{preview:?}").contains("secret-not-exported"));
        assert!(!paths.ssh_dir.exists());
    }

    #[test]
    fn locates_alias_in_quoted_include_and_in_managed_file() {
        let (_home, paths, host) =
            fixture("Include \"lan configs/*.conf\"\nInclude ~/.ssh/omnyssh.conf\n");
        atomic_write(
            &paths.ssh_dir.join("lan configs/one.conf"),
            b"Host lan\n  HostName old\n",
        )
        .unwrap();
        atomic_write(&paths.managed_config, b"Host managed\n  HostName old\n").unwrap();
        let plan = prepare(&paths, &host, false).unwrap();
        assert!(plan.preview.target_path.ends_with("one.conf"));
        let host = Host {
            name: "managed".into(),
            ..host
        };
        assert_eq!(
            prepare(&paths, &host, false).unwrap().preview.target_path,
            paths.managed_config.display().to_string()
        );
    }

    #[test]
    fn refuses_duplicate_aliases_shared_blocks_match_and_conditional_includes() {
        for text in [
            "Host lan\nHost lan\n",
            "Host lan other\n",
            "Match exec \"echo never\"\n",
            "\"Match\" exec \"echo never\"\n",
            "\"Include\" dynamic.conf\n",
            "Host lan\n Include other.conf\n",
            "Include config\n",
        ] {
            let (_home, paths, host) = fixture(text);
            assert!(preview(&paths, &host, false).is_err(), "{text}");
            assert_eq!(fs::read_to_string(paths.main_config).unwrap(), text);
        }
    }

    #[test]
    fn preserves_identity_list_unless_user_explicitly_replaces_it() {
        let (_home, paths, mut host) =
            fixture("Host lan\n  IdentityFile ~/.ssh/one\n  IdentityFile ~/.ssh/two\n");
        host.identity_file = Some("~/.ssh/two".into());
        let plan = prepare(&paths, &host, false).unwrap();
        assert!(plan
            .next
            .contains("  IdentityFile ~/.ssh/one\n  IdentityFile ~/.ssh/two\n"));
        assert!(preview(&paths, &host, true)
            .unwrap_err()
            .to_string()
            .contains("multiple IdentityFile"));
    }

    #[test]
    fn address_edit_preserves_quoted_and_tokenized_identity_paths() {
        let (_home, paths, mut host) =
            fixture("Host lan\n  IdentityFile \"%d/.ssh/key with spaces\" # keep\n");
        host.identity_file = Some("\"%d/.ssh/key with spaces\"".into());
        let plan = prepare(&paths, &host, false).unwrap();
        assert!(plan
            .next
            .contains("  IdentityFile \"%d/.ssh/key with spaces\" # keep\n"));
        assert!(preview(&paths, &host, true).is_err());
    }

    #[test]
    fn refuses_read_only_targets_and_includes_outside_ssh_directory() {
        let (_home, paths, host) = fixture("Include ../outside.conf\n");
        atomic_write(
            &paths.ssh_dir.parent().unwrap().join("outside.conf"),
            b"Host lan\n",
        )
        .unwrap();
        assert!(preview(&paths, &host, false)
            .unwrap_err()
            .to_string()
            .contains("outside ~/.ssh"));
        atomic_write(&paths.main_config, b"Host lan\n").unwrap();
        let original = fs::metadata(&paths.main_config).unwrap().permissions();
        let mut readonly = original.clone();
        readonly.set_readonly(true);
        fs::set_permissions(&paths.main_config, readonly).unwrap();
        assert!(preview(&paths, &host, false).is_err());
        fs::set_permissions(&paths.main_config, original).unwrap();
    }

    #[test]
    fn rejects_non_manual_hosts_invalid_aliases_and_directive_injection() {
        let (_home, paths, mut host) = fixture("");
        host.source = HostSource::SshConfig;
        assert!(preview(&paths, &host, false).is_err());
        host.source = HostSource::Manual;
        for value in ["a\nProxyCommand malicious", "host#comment", "\"host\""] {
            host.hostname = value.into();
            assert!(preview(&paths, &host, false).is_err());
        }
        host.hostname = "valid".into();
        host.name = "-option".into();
        assert!(preview(&paths, &host, false).is_err());
    }

    #[test]
    fn checks_password_warning_on_backend_and_rejects_stale_form() {
        let (_home, paths, mut host) = fixture("");
        let p = preview(&paths, &host, false).unwrap();
        assert!(apply(&paths, &host, false, &p.fingerprint, false)
            .unwrap_err()
            .to_string()
            .contains("key-login"));
        host.hostname = "192.168.1.21".into();
        assert!(apply(&paths, &host, false, &p.fingerprint, true)
            .unwrap_err()
            .to_string()
            .contains("changed after preview"));
        assert_eq!(fs::read_to_string(&paths.main_config).unwrap(), "");
    }

    #[test]
    fn rejects_external_edit_and_new_glob_match_after_preview() {
        let (_home, paths, host) = fixture("Include parts/*.conf\n");
        let p = preview(&paths, &host, false).unwrap();
        atomic_write(&paths.ssh_dir.join("parts/a.conf"), b"Host unrelated\n").unwrap();
        assert!(apply(&paths, &host, false, &p.fingerprint, true)
            .unwrap_err()
            .to_string()
            .contains("changed after preview"));
    }

    // 这些用例用真实 ssh -G，但只读临时目录，不登录、不访问用户密钥或 known_hosts。
    #[test]
    fn writes_and_backs_up_exact_include_source_without_duplicates() {
        let (_home, paths, host) = fixture("Include \"lan configs/one.conf\"\n");
        let target = paths.ssh_dir.join("lan configs/one.conf");
        let old =
            "Host lan\n  HostName 192.168.1.10\n  User root\n  Port 22\n  Ciphers +aes256-cbc\n";
        atomic_write(&target, old.as_bytes()).unwrap();
        let p = preview(&paths, &host, false).unwrap();
        let report = apply(&paths, &host, false, &p.fingerprint, true).unwrap();
        assert_eq!(
            fs::read_to_string(Path::new(&report.backup_path).join("original.conf")).unwrap(),
            old
        );
        assert_eq!(
            fs::read_to_string(&target).unwrap(),
            old.replace("192.168.1.10", "192.168.1.20")
        );
        assert!(preview(&paths, &host, false).unwrap().diff.is_empty());
        assert!(fs::read_dir(&paths.ssh_dir).unwrap().all(|item| !item
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".omnyssh-host-check")));
    }

    #[test]
    fn new_host_is_effective_and_can_be_updated_again() {
        let (_home, paths, mut host) =
            fixture("Include ~/.ssh/omnyssh.conf\nServerAliveInterval 30\n");
        atomic_write(
            &paths.managed_config,
            b"Host other\n  HostName other.example\n",
        )
        .unwrap();
        let p = preview(&paths, &host, false).unwrap();
        apply(&paths, &host, false, &p.fingerprint, true).unwrap();
        assert_eq!(
            top_level_include_count(&fs::read_to_string(&paths.main_config).unwrap()),
            1
        );
        host.hostname = "192.168.1.21".into();
        let p = preview(&paths, &host, false).unwrap();
        assert!(p.existing);
        apply(&paths, &host, false, &p.fingerprint, true).unwrap();
        assert_eq!(
            fs::read_to_string(&paths.main_config)
                .unwrap()
                .matches("Host lan\n")
                .count(),
            1
        );
    }

    #[test]
    fn failed_ssh_validation_does_not_write_or_create_backup() {
        for text in [
            "UnknownDirective value\nHost lan\n HostName old\n",
            "Host *\n HostName overridden\nHost lan\n HostName old\n",
        ] {
            let (_home, paths, host) = fixture(text);
            let p = preview(&paths, &host, false).unwrap();
            assert!(apply(&paths, &host, false, &p.fingerprint, true).is_err());
            assert_eq!(fs::read_to_string(&paths.main_config).unwrap(), text);
            assert!(!paths.ssh_dir.join(".omnyssh-host-backups").exists());
        }
    }
}
