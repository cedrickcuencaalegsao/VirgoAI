use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub ollama_url: String,
    pub planner_model: String,
    pub executor_model: String,
    /// Empty means "use the default folder inside the app data dir".
    pub memory_dir: String,
    pub num_ctx: u32,
    pub keep_alive: String,
    pub use_planner: bool,
    pub planner_think: bool,
    pub executor_think: bool,
    /// Apply model edits to project files as soon as a reply finishes.
    pub auto_apply: bool,
    /// Folder reopened at startup (empty = none).
    pub last_project: String,
    /// Project (folder name) that new chats are saved into. Empty = none.
    pub active_workspace: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ollama_url: "http://localhost:11434".into(),
            planner_model: "qwen3:8b".into(),
            executor_model: "qwen2.5-coder:7b".into(),
            memory_dir: String::new(),
            num_ctx: 8192,
            keep_alive: "10m".into(),
            use_planner: true,
            planner_think: true,
            executor_think: false,
            auto_apply: true,
            last_project: String::new(),
            active_workspace: String::new(),
        }
    }
}

fn data_root() -> PathBuf {
    dirs::data_dir().unwrap_or_else(|| PathBuf::from("."))
}

/// `~/Library/Application Support/VirgoAI` on macOS.
pub fn app_dir() -> PathBuf {
    data_root().join("VirgoAI")
}

/// Before the rename the app stored its data in `local-agent`. Move it over once.
fn migrate_legacy_dir() {
    let old = data_root().join("local-agent");
    let new = app_dir();
    if old.exists() && !new.exists() {
        let _ = fs::rename(old, new);
    }
}

impl Config {
    pub fn path() -> PathBuf {
        app_dir().join("config.toml")
    }

    /// Load the config file, creating it with defaults on first run.
    pub fn load() -> Self {
        migrate_legacy_dir();
        match fs::read_to_string(Self::path()) {
            Ok(text) => toml::from_str::<Config>(&text).unwrap_or_default(),
            Err(_) => {
                let cfg = Config::default();
                let _ = cfg.save();
                cfg
            }
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let text = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(path, text).map_err(|e| e.to_string())
    }

    /// Resolve the memory folder, expanding a leading `~/`.
    pub fn memory_path(&self) -> PathBuf {
        let raw = self.memory_dir.trim();
        if raw.is_empty() {
            return app_dir().join("memory");
        }
        if let Some(rest) = raw.strip_prefix("~/") {
            if let Some(home) = dirs::home_dir() {
                return home.join(rest);
            }
        }
        PathBuf::from(raw)
    }
}
