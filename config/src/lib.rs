use std::fs;
use std::path::PathBuf;

use ollama::types::{Endpoint, Provider};
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

    // ---- external providers (empty key = disabled) ----
    pub openai_key: String,
    pub anthropic_key: String,
    pub xai_key: String,
    /// Change these to use a proxy or an OpenAI-compatible server.
    pub openai_url: String,
    pub anthropic_url: String,
    pub xai_url: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ollama_url: Provider::Ollama.default_url().into(),
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
            openai_key: String::new(),
            anthropic_key: String::new(),
            xai_key: String::new(),
            openai_url: Provider::OpenAi.default_url().into(),
            anthropic_url: Provider::Anthropic.default_url().into(),
            xai_url: Provider::Xai.default_url().into(),
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
        fs::write(&path, text).map_err(|e| e.to_string())?;
        // The file now holds API keys: make it private to this user.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o600));
        }
        Ok(())
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

    // ---- providers --------------------------------------------------------

    /// The key from settings, or from the provider's usual environment variable.
    fn api_key(&self, p: Provider) -> String {
        let (stored, var) = match p {
            Provider::Ollama => return String::new(),
            Provider::OpenAi => (&self.openai_key, "OPENAI_API_KEY"),
            Provider::Anthropic => (&self.anthropic_key, "ANTHROPIC_API_KEY"),
            Provider::Xai => (&self.xai_key, "XAI_API_KEY"),
        };
        let stored = stored.trim();
        if !stored.is_empty() {
            return stored.to_string();
        }
        std::env::var(var).unwrap_or_default().trim().to_string()
    }

    pub fn endpoint(&self, p: Provider) -> Endpoint {
        let url = match p {
            Provider::Ollama => &self.ollama_url,
            Provider::OpenAi => &self.openai_url,
            Provider::Anthropic => &self.anthropic_url,
            Provider::Xai => &self.xai_url,
        };
        let url = if url.trim().is_empty() { p.default_url() } else { url.as_str() };
        Endpoint { provider: p, base_url: url.to_string(), api_key: self.api_key(p) }
    }

    /// The endpoint that serves a model name such as `openai:gpt-4o` or `qwen3:8b`.
    pub fn endpoint_for(&self, model: &str) -> Endpoint {
        self.endpoint(Provider::split(model).0)
    }

    /// Ollama plus every provider that has a key.
    pub fn endpoints(&self) -> Vec<Endpoint> {
        [Provider::Ollama, Provider::OpenAi, Provider::Anthropic, Provider::Xai]
            .into_iter()
            .map(|p| self.endpoint(p))
            .filter(|e| e.provider == Provider::Ollama || !e.api_key.is_empty())
            .collect()
    }
}