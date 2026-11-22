//! VirgoEngine: a native Rust LLM inference engine built around
//! intelligent layer streaming.
//!
//! **Status: Phase 1 (foundation).** The crate currently provides
//! configuration, error types, model metadata, tensor abstractions,
//! model-format detection and a reference CPU backend. It cannot yet load
//! tensors, stream layers or generate text.

pub mod backend;
pub mod engine;
pub mod inference;
pub mod memory;
pub mod model;
pub mod streaming;

pub use backend::cpu::CpuBackend;
pub use backend::{Backend, BackendKind};
pub use engine::config::{EngineConfig, ExecutionConfig};
pub use engine::engine::Engine;
pub use engine::error::{Result, VirgoError};
pub use model::loader::{ModelFormat, detect_format};
pub use model::metadata::ModelMetadata;
pub use model::tensor::{DType, TensorInfo, TensorLocation};
