//! Compute backend interface. Operates on plain `f32` slices and knows
//! nothing about model file formats.

pub mod cpu;

use crate::engine::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BackendKind {
    #[default]
    Cpu,
}

/// Numeric kernels needed by the transformer. All buffers are row-major.
pub trait Backend: Send + Sync {
    fn kind(&self) -> BackendKind;

    /// `out[m, n] = a[m, k] x b[n, k]^T`.
    ///
    /// `b` is stored as `[n, k]` because linear-layer weights are stored
    /// `[out_features, in_features]`.
    fn matmul_transposed_b(
        &self,
        a: &[f32],
        b: &[f32],
        out: &mut [f32],
        m: usize,
        k: usize,
        n: usize,
    ) -> Result<()>;

    /// `out[i] = x[i] / sqrt(mean(x^2) + eps) * weight[i]`.
    fn rms_norm(&self, x: &[f32], weight: &[f32], eps: f32, out: &mut [f32]) -> Result<()>;

    /// Numerically stable softmax over the whole slice.
    fn softmax_inplace(&self, x: &mut [f32]) -> Result<()>;

    /// `x[i] = x[i] * sigmoid(x[i])`.
    fn silu_inplace(&self, x: &mut [f32]) -> Result<()>;
}

pub fn create(kind: BackendKind) -> Result<Box<dyn Backend>> {
    match kind {
        BackendKind::Cpu => Ok(Box::new(cpu::CpuBackend::new())),
    }
}
