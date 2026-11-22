//! Crate-wide error type.
//!
//! Placed in `engine/` (the public-facing subsystem) rather than in a new
//! top-level module so the directory layout from the README stays intact.

use std::path::PathBuf;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, VirgoError>;

#[derive(Debug, Error)]
pub enum VirgoError {
    #[error("invalid engine configuration: {0}")]
    InvalidConfig(String),

    #[error("model file not found: {}", .0.display())]
    ModelNotFound(PathBuf),
    #[error("unsupported model format: {0}")]
    UnsupportedModelFormat(String),
    #[error("unsupported architecture: {0}")]
    UnsupportedArchitecture(String),
    #[error("invalid model metadata: {0}")]
    InvalidModelMetadata(String),

    #[error("tensor not found: {0}")]
    TensorNotFound(String),
    #[error("invalid tensor offset for `{tensor}`: {reason}")]
    InvalidTensorOffset { tensor: String, reason: String },
    #[error("unsupported tensor type: {0}")]
    UnsupportedTensorType(String),

    #[error("storage read failed for {}: {source}", path.display())]
    StorageReadFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("memory budget exceeded: requested {requested} bytes, {available} available")]
    MemoryBudgetExceeded { requested: u64, available: u64 },
    #[error("allocation failed: {0}")]
    AllocationFailed(String),

    #[error("failed to load layer {layer}: {reason}")]
    LayerLoadingFailed { layer: usize, reason: String },
    #[error("prefetch failed: {0}")]
    PrefetchFailed(String),
    #[error("cache operation failed: {0}")]
    CacheOperationFailed(String),

    #[error("backend unavailable: {0}")]
    BackendUnavailable(String),
    #[error("kernel execution failed: {0}")]
    KernelExecutionFailed(String),

    #[error("generation cancelled")]
    GenerationCancelled,
    #[error("context limit exceeded: {requested} tokens requested, maximum is {max}")]
    ContextLimitExceeded { requested: usize, max: usize },
}
