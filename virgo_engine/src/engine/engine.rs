//! The engine coordinates subsystems; it does not implement their logic.
//!
//! Phase 1 only validates configuration, identifies the model format and
//! instantiates the compute backend. Model loading, streaming and
//! generation arrive in later phases.

use tracing::info;

use super::config::EngineConfig;
use super::error::Result;
use crate::backend::{self, Backend};
use crate::model::loader::{ModelFormat, detect_format};

pub struct Engine {
    config: EngineConfig,
    format: ModelFormat,
    backend: Box<dyn Backend>,
}

impl Engine {
    pub fn new(config: EngineConfig) -> Result<Self> {
        config.validate()?;
        let format = detect_format(&config.model_path)?;
        let backend = backend::create(config.backend)?;
        info!(
            path = %config.model_path.display(),
            ?format,
            backend = ?config.backend,
            "VirgoEngine initialised"
        );
        Ok(Self { config, format, backend })
    }

    pub fn config(&self) -> &EngineConfig {
        &self.config
    }

    pub fn model_format(&self) -> ModelFormat {
        self.format
    }

    pub fn backend(&self) -> &dyn Backend {
        self.backend.as_ref()
    }
}
