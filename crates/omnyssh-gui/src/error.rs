//! Command error carried across the IPC boundary (tech-gui.md §4.2). Commands
//! return `Result<T, CommandError>`; the frontend localizes `code` and keeps
//! `raw_detail` byte-for-byte for diagnostics.

use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub code: String,
    pub args: HashMap<String, String>,
    pub raw_detail: Option<String>,
}

impl CommandError {
    pub fn new(code: impl Into<String>, detail: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            args: HashMap::new(),
            raw_detail: Some(detail.into()),
        }
    }

    pub fn with_arg(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.args.insert(name.into(), value.into());
        self
    }
}
