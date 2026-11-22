//! Engine configuration. Every memory-related value is configurable; the
//! defaults below are only starting points.

use std::path::PathBuf;

use super::error::{Result, VirgoError};
use crate::backend::BackendKind;

const GIB: u64 = 1024 * 1024 * 1024;
pub const DEFAULT_MEMORY_BUDGET_BYTES: u64 = 8 * GIB;
pub const DEFAULT_MAX_PREFETCH_BYTES: u64 = GIB;
pub const DEFAULT_PREFETCH_LAYERS: usize = 2;
pub const DEFAULT_MAX_CONTEXT_LENGTH: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ExecutionConfig {
    /// CPU worker threads. `None` lets the backend decide.
    pub num_threads: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineConfig {
    pub model_path: PathBuf,
    /// Ceiling for application-managed memory (weights, KV cache,
    /// activations, workspace, prefetch). OS file cache is not counted.
    pub memory_budget_bytes: u64,
    /// Portion of the budget prefetching may use.
    pub max_prefetch_bytes: u64,
    /// How many upcoming layers to prefetch.
    pub prefetch_layers: usize,
    pub backend: BackendKind,
    pub max_context_length: usize,
    pub execution: ExecutionConfig,
}

impl EngineConfig {
    pub fn new(model_path: impl Into<PathBuf>) -> Self {
        Self {
            model_path: model_path.into(),
            memory_budget_bytes: DEFAULT_MEMORY_BUDGET_BYTES,
            max_prefetch_bytes: DEFAULT_MAX_PREFETCH_BYTES,
            prefetch_layers: DEFAULT_PREFETCH_LAYERS,
            backend: BackendKind::default(),
            max_context_length: DEFAULT_MAX_CONTEXT_LENGTH,
            execution: ExecutionConfig::default(),
        }
    }

    pub fn with_memory_budget_bytes(mut self, bytes: u64) -> Self {
        self.memory_budget_bytes = bytes;
        self
    }

    pub fn with_max_prefetch_bytes(mut self, bytes: u64) -> Self {
        self.max_prefetch_bytes = bytes;
        self
    }

    pub fn with_prefetch_layers(mut self, layers: usize) -> Self {
        self.prefetch_layers = layers;
        self
    }

    pub fn with_backend(mut self, backend: BackendKind) -> Self {
        self.backend = backend;
        self
    }

    pub fn with_max_context_length(mut self, tokens: usize) -> Self {
        self.max_context_length = tokens;
        self
    }

    pub fn with_num_threads(mut self, threads: usize) -> Self {
        self.execution.num_threads = Some(threads);
        self
    }

    /// Checks internal consistency. Does not touch the filesystem.
    pub fn validate(&self) -> Result<()> {
        let bad = |msg: &str| Err(VirgoError::InvalidConfig(msg.to_string()));
        if self.model_path.as_os_str().is_empty() {
            return bad("model_path must not be empty");
        }
        if self.memory_budget_bytes == 0 {
            return bad("memory_budget_bytes must be greater than zero");
        }
        if self.max_prefetch_bytes > self.memory_budget_bytes {
            return bad("max_prefetch_bytes must not exceed memory_budget_bytes");
        }
        if self.prefetch_layers > 0 && self.max_prefetch_bytes == 0 {
            return bad("prefetch_layers > 0 requires max_prefetch_bytes > 0");
        }
        if self.max_context_length == 0 {
            return bad("max_context_length must be greater than zero");
        }
        if self.execution.num_threads == Some(0) {
            return bad("num_threads must be at least 1 when set");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_valid() {
        assert!(EngineConfig::new("model.gguf").validate().is_ok());
    }

    #[test]
    fn rejects_zero_budget() {
        let cfg = EngineConfig::new("m").with_memory_budget_bytes(0);
        assert!(matches!(cfg.validate(), Err(VirgoError::InvalidConfig(_))));
    }

    #[test]
    fn rejects_prefetch_larger_than_budget() {
        let cfg = EngineConfig::new("m")
            .with_memory_budget_bytes(GIB)
            .with_max_prefetch_bytes(2 * GIB);
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn rejects_prefetch_layers_without_prefetch_memory() {
        let cfg = EngineConfig::new("m")
            .with_max_prefetch_bytes(0)
            .with_prefetch_layers(1);
        assert!(cfg.validate().is_err());
        assert!(cfg.with_prefetch_layers(0).validate().is_ok());
    }

    #[test]
    fn rejects_zero_context_and_zero_threads() {
        assert!(EngineConfig::new("m").with_max_context_length(0).validate().is_err());
        assert!(EngineConfig::new("m").with_num_threads(0).validate().is_err());
    }
}
