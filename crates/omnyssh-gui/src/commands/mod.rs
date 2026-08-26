//! IPC command handlers, one module per domain (tech-gui.md §3.1). Commands are
//! thin: validate input, call the core, return a DTO or an error.

pub mod desktop_card;
pub mod hosts;
pub mod keysetup;
pub mod settings;
pub mod sftp;
pub mod snippets;
pub mod ssh_config;
pub mod ssh_keys;
pub mod terminal;
pub mod update;
