//! Global (per user, not per workspace) application settings.
//!
//! Stored as JSON in `<config dir>/wurzelwerk/settings.json`:
//! * Linux and Windows: `~/.config` (Linux honours `$XDG_CONFIG_HOME`)
//! * macOS: `~/Library/Application Support`

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::i18n::Lang;

const APP_DIR: &str = "wurzelwerk";
const FILE: &str = "settings.json";
const MAX_RECENT: usize = 8;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub language: Lang,
    /// Workspace opened at start-up.
    pub workspace: Option<PathBuf>,
    /// Most recently used first.
    pub recent: Vec<PathBuf>,
    pub check_updates: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: Lang::detect(),
            workspace: None,
            recent: Vec::new(),
            check_updates: true,
        }
    }
}

impl Settings {
    /// Missing or unreadable files fall back to the defaults.
    pub fn load() -> Self {
        std::fs::read_to_string(settings_path())
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        let path = settings_path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&path, json).map_err(|e| format!("cannot write {}: {e}", path.display()))
    }

    /// Moves `path` to the front of the recent list.
    pub fn remember(&mut self, path: &Path) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_path_buf());
        self.recent.truncate(MAX_RECENT);
    }
}

fn home() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var)
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

/// Platform config root (the folder that contains `wurzelwerk/`).
pub fn config_root() -> PathBuf {
    let home = home().unwrap_or_else(|| PathBuf::from("."));
    if cfg!(target_os = "macos") {
        home.join("Library").join("Application Support")
    } else if cfg!(windows) {
        home.join(".config")
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| home.join(".config"))
    }
}

pub fn config_dir() -> PathBuf {
    config_root().join(APP_DIR)
}

pub fn settings_path() -> PathBuf {
    config_dir().join(FILE)
}

/// Suggested location for the first workspace.
pub fn default_workspace() -> PathBuf {
    let base = home().unwrap_or_else(|| PathBuf::from("."));
    let docs = base.join("Documents");
    if docs.is_dir() { docs } else { base }.join("Wurzelwerk")
}
