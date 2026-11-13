// ZeroTier-GUI — a cross-platform front-end for ZeroTier.
//
// Rust + GPUI rewrite Copyright (C) 2026 Senasgr.
// Based on the original ZeroTier-GUI by Tomás Ralph (GPL-3.0-or-later).
// See NOTICE and LICENSE for attribution and licensing details.

//! Persistent list of networks the user has joined before.
//!
//! The on-disk format is deliberately the same JSON object the Tkinter
//! version used, so an existing `network_history.json` keeps working:
//!
//! ```json
//! {
//!   "8056c2e21c000001": {
//!     "name": "Home",
//!     "join_date": "2024/04/19 18:32"
//!   }
//! }
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

pub const FILE_NAME: &str = "network_history.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct HistoryEntry {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub join_date: String,
}

impl HistoryEntry {
    pub fn display_name(&self) -> String {
        if self.name.trim().is_empty() {
            "Unknown Name".to_string()
        } else {
            self.name.clone()
        }
    }
}

pub type NetworkHistory = BTreeMap<String, HistoryEntry>;

pub fn directory() -> PathBuf {
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from(".local/share"));

    data_home.join("zerotier-gui")
}

pub fn file_path() -> PathBuf {
    directory().join(FILE_NAME)
}

/// Load the history, creating the data directory and an empty file when this
/// is the first run.  Corrupt or unreadable files are treated as empty rather
/// than preventing the application from starting.
pub fn load() -> NetworkHistory {
    let path = file_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    match std::fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => {
            let _ = std::fs::File::create(&path);
            NetworkHistory::default()
        }
    }
}

pub fn save(history: &NetworkHistory) -> std::io::Result<()> {
    let path = file_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let json = serde_json::to_string(history)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;

    // Write to a sibling file first so a crash cannot truncate the history.
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, json)?;
    std::fs::rename(temporary, path)
}

/// The timestamp format used by the original GUI: `YYYY/MM/DD HH:MM`.
pub fn timestamp() -> String {
    chrono::Local::now().format("%Y/%m/%d %H:%M").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_original_format() {
        let json = r#"{
            "8056c2e21c000001": {
                "name": "Home",
                "join_date": "2024/04/19 18:32"
            },
            "abcdef0123456789": {
                "name": "",
                "join_date": "2023/01/02 03:04"
            }
        }"#;

        let history: NetworkHistory = serde_json::from_str(json).unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history["8056c2e21c000001"].display_name(), "Home");
        assert_eq!(history["abcdef0123456789"].display_name(), "Unknown Name");
    }

    #[test]
    fn round_trips() {
        let mut history = NetworkHistory::new();
        history.insert(
            "8056c2e21c000001".to_string(),
            HistoryEntry {
                name: "Home".to_string(),
                join_date: "2024/04/19 18:32".to_string(),
            },
        );

        let json = serde_json::to_string(&history).unwrap();
        let decoded: NetworkHistory = serde_json::from_str(&json).unwrap();
        assert_eq!(history, decoded);
    }

    #[test]
    fn timestamp_has_the_expected_shape() {
        let stamp = timestamp();
        assert_eq!(stamp.len(), 16, "unexpected timestamp: {stamp}");
        assert_eq!(&stamp[4..5], "/");
        assert_eq!(&stamp[7..8], "/");
        assert_eq!(&stamp[10..11], " ");
        assert_eq!(&stamp[13..14], ":");
    }
}
