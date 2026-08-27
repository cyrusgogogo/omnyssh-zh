<div align="center">

# OmnySSH（中文优化版）

### 在一个窗口中管理所有服务器：概览、终端、SFTP 和命令片段。

<img src="assets/gui.webp" alt="OmnySSH GUI 概览" width="900">

[![Downloads](https://img.shields.io/github/downloads/cyrusgogogo/omnyssh/total?label=total%20installs&color=2ea44f)](https://github.com/cyrusgogogo/omnyssh/releases)
[![Latest release](https://img.shields.io/github/v/release/cyrusgogogo/omnyssh?label=latest)](https://github.com/cyrusgogogo/omnyssh/releases/latest)
[![Stars](https://img.shields.io/github/stars/cyrusgogogo/omnyssh?style=flat)](https://github.com/cyrusgogogo/omnyssh/stargazers)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Build](https://img.shields.io/github/actions/workflow/status/cyrusgogogo/omnyssh/ci.yml?branch=main)](https://github.com/cyrusgogogo/omnyssh/actions)

**简体中文** | [English](README.en.md)

**[安装](#安装)** • **[功能](#功能)** • **[SSH 密钥配置](#ssh-密钥配置)** •
**[对比](#对比)** • **[TUI 版本](#tui-版本)** • **[中文优化版说明](#中文优化版说明)**

</div>

---

## 安装

macOS 和 Linux 使用一条命令安装：

```bash
curl -fsSL https://raw.githubusercontent.com/cyrusgogogo/omnyssh/main/install.sh | sh
```

脚本会检测操作系统和架构，并安装本仓库的最新桌面版：macOS 安装到 `/Applications`，Linux 添加到应用菜单。只需要终端版或希望同时安装两者时，使用 `curl … | sh -s -- --tui` 或 `--both`。也可以从 [Releases](https://github.com/cyrusgogogo/omnyssh/releases/latest) 手动下载。

| 平台 | 文件 |
|---|---|
| macOS Apple Silicon | `OmnySSH-aarch64-apple-darwin.dmg` |
| macOS Intel | `OmnySSH-x86_64-apple-darwin.dmg` |
| Linux x86_64 | `OmnySSH-x86_64.AppImage` / `.deb` / `.rpm` |
| Windows x86_64 | `OmnySSH-x86_64-setup.exe` |

无需账户、登录或遥测。应用首次打开时概览为空；若存在 `~/.ssh/config`，OmnySSH 会读取其中的主机，包括使用 `ProxyJump` 或为旧设备单独配置 `Ciphers` 的主机。

桌面端还提供需用户主动操作的 **SSH 配置** 页面。它不会重排已有文件：OmnySSH 只把精确 Host 条目写入 `~/.ssh/omnyssh.conf`，展示差异并获得确认后，才在主配置顶部加入一条 `Include ~/.ssh/omnyssh.conf`。这些条目与软件自身的 `hosts.toml` 主机独立维护。包括删除在内的每次写入都会先预览、校验和备份，并检测外部并发修改。

其中的**密钥管理**页签可管理 `~/.ssh/` 直属目录下的密钥对：使用唯一的 `id_omnyssh_<随机码>` 文件名创建、加载已有密钥、修改仅供 OmnySSH 显示的记录名称、复制公钥/私钥路径，以及备份、恢复和输入名称二次确认后删除。私钥内容不会进入前端，身份文件通过已管理密钥下拉选择。关闭桌面窗口后 OmnySSH 会留在系统托盘，需使用托盘“退出”才会彻底结束；设置中也可选择继续使用内置终端，或改由 Windows Terminal、macOS“终端”和 Linux 系统终端打开连接，系统终端的页签或窗口标题会使用主机名称。

---

## 功能

主机只需添加一次。概览卡片会自动刷新 CPU、内存、磁盘、运行时间、系统版本、CPU 占用最高的进程和服务状态。点击 `sh` 打开真实 PTY 终端，点击 `files` 打开双面板 SFTP 浏览器。

### 实时概览

每台主机都有 CPU、内存和磁盘进度条，并显示运行时间、系统版本、进程与 Docker 容器数量。指标达到阈值时会变为黄色或红色。可从概览把一台或多台主机加入独立桌面卡片；卡片可自由拖动、切换是否固定在最前，通过与概览状态同色的圆点直接切换主机，并显示概览中的进程信息，还可打开当前主机的终端或 SFTP。桌面卡片还可缩小为仅显示主机圆点的窄条；悬停圆点会临时浮现实时状态，默认向下展开，卡片靠近屏幕底部时自动改为向上展开；点击圆点可锁定状态面板，并可从面板直接进入该主机的终端或 SFTP。如果同一台机器同时维护了局域网和公网地址，可以在主机编辑器中勾选“在概览中隐藏”；该地址仍可用于终端、SFTP、片段和命令面板。

### 真实终端

支持完整 PTY、多标签页和并行会话。切换到概览或其他功能时，终端会话仍保持运行。

### 双面板 SFTP

左侧为本地，右侧为远程。支持多选、上传、下载、进度显示和常用文件操作。

### 命令片段

保存常用命令并在一台或多台主机上运行。参数化片段如 `sudo systemctl restart {{service}}` 会在运行前提示填写参数。

### 全局搜索

按 ⌘K 搜索所有主机和已打开的会话，按 Enter 连接主机或返回现有会话。

### 演示模式

将界面中的真实 IP 替换为虚构地址，适合录屏或共享屏幕。

### 浅色与深色主题

两套主题均随应用提供，可从侧边栏切换。

### 多语言

桌面版可在“设置 → 外观 → 语言”中选择跟随系统、English 或简体中文。TUI 按 `Shift+L` 切换，也可使用 `[ui].language` 或临时参数 `--language zh-CN`。远程终端、命令输出、主机名和路径不会被翻译。

### 轻量

多个会话打开时约占用 130 MB 内存，下载约 20 MB。详细数据见[对比](#对比)。

---

## SSH 密钥配置

选择一台使用密码且尚未配置密钥的主机，点击 **配置 SSH 密钥**。OmnySSH 会生成 Ed25519 密钥，将公钥追加到 `authorized_keys`，再使用新密钥建立连接验证；验证通过后才禁用密码登录。启动流程即表示同意完成这些步骤，中途没有额外确认。

修改 `sshd_config` 前会在服务器上创建备份。任何步骤失败时都会恢复备份。私钥不会离开本机，数据也不会发送到所选服务器之外。

实现位于 [`crates/omnyssh-core/src/ssh/key_setup.rs`](crates/omnyssh-core/src/ssh/key_setup.rs)。在生产服务器上使用前建议先阅读代码。

---

## 对比

内存和 CPU 数据在 M4 Mac 上测得，两款应用均处于打开且空闲状态。

| | OmnySSH | Termius | tmux + ssh |
|---|---|---|---|
| 空闲内存 | ~130 MB | ~649 MB | 很低 |
| 进程数 | 4 | 9 | 1 |
| 实时指标概览 | ✅ | ✅ | ❌ |
| 双面板 SFTP | ✅ | ✅ | ❌ |
| 命令片段与批量运行 | ✅ | ✅ | ❌ |
| 一键密钥配置 | ✅ | ❌ | ❌ |
| 需要账户 | ❌ | ✅ | ❌ |
| 遥测 | ❌ | ✅ | ❌ |
| 开源 | ✅ | ❌ | ✅ |
| 价格 | 免费 | 💰 | 免费 |

tmux 最轻量，但不提供图形化管理能力。

---

## TUI 版本

OmnySSH 最初是终端应用，TUI 版本仍在维护并持续发布。

![演示](assets/demo.gif)

底层使用同一套引擎：`crates/omnyssh-core` 提供前端无关逻辑，GUI 与 TUI 构建在其上。TUI 包含概览、SFTP、命令片段、多会话标签页、模糊搜索、四套主题（`default`、`dracula`、`nord`、`gruvbox`）和可配置快捷键。

```bash
# 使用本仓库安装脚本安装 TUI
curl -fsSL https://raw.githubusercontent.com/cyrusgogogo/omnyssh/main/install.sh | sh -s -- --tui

# 或从 v1.0.0 源码安装
cargo install --git https://github.com/cyrusgogogo/omnyssh.git --tag v1.0.0 --locked --bin omny

# Nix
nix run github:cyrusgogogo/omnyssh/v1.0.0
```

运行 `omny`。按 `a` 添加主机、`/` 搜索、`?` 查看帮助、`Shift+K` 配置 SSH 密钥、`Shift+L` 选择语言。

Linux、macOS、Windows 和 Termux 的预编译 TUI 位于 [Releases](https://github.com/cyrusgogogo/omnyssh/releases) 的 `omny-*` 文件。配置目录分别为 Linux 的 `~/.config/omnyssh/`、macOS 的 `~/Library/Application Support/omnyssh/` 和 Windows 的 `%APPDATA%\omnyssh\`。TUI 启动时只读 `~/.ssh/config`；只有桌面端用户明确确认的 SSH 配置流程才会安装上述托管 Include。

完整选项、快捷键和配置说明请运行 `man omny`；中文手册可用 `man -L zh_CN omny` 查看。

---

## 中文优化版说明

本仓库是基于 [timhartmann7/omnyssh](https://github.com/timhartmann7/omnyssh) 的独立修改发行版，重点补充简体中文体验、SSH 配置与密钥管理、系统终端、系统托盘及桌面主机卡片等功能。本修改版不代表上游作者为其提供背书。

为遵守 Apache License 2.0，本仓库保留完整的 [LICENSE](LICENSE)，并在 [NOTICE](NOTICE) 中说明上游来源和本发行版的主要修改。分发源码或二进制时请一并保留许可证与归属声明。

---

## 参与贡献

欢迎提交 Pull Request。环境配置、约定和检查清单见 [CONTRIBUTING.md](CONTRIBUTING.md)。计划较大的改动前请先创建 Issue。

```text
crates/omnyssh-core   前端无关引擎
crates/omnyssh        TUI 应用（可执行文件：omny）
crates/omnyssh-gui    Tauri 桌面应用
```

## 许可证

本项目依据 Apache License 2.0 分发，详见 [LICENSE](LICENSE) 与 [NOTICE](NOTICE)。本发行版包含对上游 OmnySSH 的修改；贡献者提交代码即表示同意按同一许可证提供其贡献。

<div align="center">

### ⭐ 如果 OmnySSH 帮你少开了一个终端标签页，欢迎点亮 Star

[报告问题](https://github.com/cyrusgogogo/omnyssh/issues) •
[提出功能建议](https://github.com/cyrusgogogo/omnyssh/issues) •
[参与讨论](https://github.com/cyrusgogogo/omnyssh/discussions)

</div>
