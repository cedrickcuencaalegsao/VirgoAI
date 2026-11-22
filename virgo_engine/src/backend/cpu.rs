//! Reference CPU backend: straightforward scalar f32 kernels whose purpose
//! is correctness. Threading, NEON and quantized kernels come in Phase 6
//! (quantized formats only once they are verified).

use super::{Backend, BackendKind};
use crate::engine::error::{Result, VirgoError};

#[derive(Debug, Default, Clone, Copy)]
pub struct CpuBackend;

impl CpuBackend {
    pub fn new() -> Self {
        Self
    }
}

fn kernel_err<T>(msg: String) -> Result<T> {
    Err(VirgoError::KernelExecutionFailed(msg))
}

fn expect_len(what: &str, actual: usize, expected: usize) -> Result<()> {
    if actual != expected {
        return kernel_err(format!("{what} has length {actual}, expected {expected}"));
    }
    Ok(())
}

impl Backend for CpuBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::Cpu
    }

    fn matmul_transposed_b(
        &self,
        a: &[f32],
        b: &[f32],
        out: &mut [f32],
        m: usize,
        k: usize,
        n: usize,
    ) -> Result<()> {
        let size = |x: usize, y: usize| {
            x.checked_mul(y)
                .ok_or_else(|| VirgoError::KernelExecutionFailed("matrix size overflows usize".into()))
        };
        expect_len("a", a.len(), size(m, k)?)?;
        expect_len("b", b.len(), size(n, k)?)?;
        expect_len("out", out.len(), size(m, n)?)?;
        if k == 0 {
            out.fill(0.0);
            return Ok(());
        }
        for (i, a_row) in a.chunks_exact(k).enumerate() {
            for (j, b_row) in b.chunks_exact(k).enumerate() {
                out[i * n + j] = a_row.iter().zip(b_row).map(|(x, y)| x * y).sum();
            }
        }
        Ok(())
    }

    fn rms_norm(&self, x: &[f32], weight: &[f32], eps: f32, out: &mut [f32]) -> Result<()> {
        if x.is_empty() {
            return kernel_err("rms_norm input is empty".into());
        }
        expect_len("weight", weight.len(), x.len())?;
        expect_len("out", out.len(), x.len())?;
        let mean_sq = x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32;
        let inv_rms = 1.0 / (mean_sq + eps).sqrt();
        for ((o, &xi), &wi) in out.iter_mut().zip(x).zip(weight) {
            *o = xi * inv_rms * wi;
        }
        Ok(())
    }

    fn softmax_inplace(&self, x: &mut [f32]) -> Result<()> {
        if x.is_empty() {
            return Ok(());
        }
        let max = x.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        if !max.is_finite() {
            return kernel_err("softmax input has no finite maximum".into());
        }
        let mut sum = 0.0f32;
        for v in x.iter_mut() {
            *v = (*v - max).exp();
            sum += *v;
        }
        for v in x.iter_mut() {
            *v /= sum;
        }
        Ok(())
    }

    fn silu_inplace(&self, x: &mut [f32]) -> Result<()> {
        for v in x.iter_mut() {
            *v /= 1.0 + (-*v).exp();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn matmul_transposed_b_matches_hand_computation() {
        // a = [[1,2,3],[4,5,6]], b = [[1,0,1],[0,1,0]] (stored [n=2,k=3])
        let a = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let b = [1.0, 0.0, 1.0, 0.0, 1.0, 0.0];
        let mut out = [0.0; 4];
        CpuBackend.matmul_transposed_b(&a, &b, &mut out, 2, 3, 2).unwrap();
        assert_eq!(out, [4.0, 2.0, 10.0, 5.0]);
    }

    #[test]
    fn matmul_rejects_wrong_buffer_sizes() {
        let mut out = [0.0; 4];
        let r = CpuBackend.matmul_transposed_b(&[1.0; 5], &[1.0; 6], &mut out, 2, 3, 2);
        assert!(matches!(r, Err(VirgoError::KernelExecutionFailed(_))));
    }

    #[test]
    fn rms_norm_known_values() {
        let mut out = [0.0; 2];
        CpuBackend.rms_norm(&[3.0, 4.0], &[1.0, 2.0], 0.0, &mut out).unwrap();
        let rms = (12.5f32).sqrt();
        assert!(close(out[0], 3.0 / rms));
        assert!(close(out[1], 8.0 / rms));
    }

    #[test]
    fn rms_norm_rejects_bad_input() {
        let mut out = [0.0; 2];
        assert!(CpuBackend.rms_norm(&[], &[], 1e-6, &mut []).is_err());
        assert!(CpuBackend.rms_norm(&[1.0, 2.0], &[1.0], 1e-6, &mut out).is_err());
    }

    #[test]
    fn softmax_sums_to_one_and_is_stable() {
        let mut x = [1000.0, 1001.0, 1002.0];
        CpuBackend.softmax_inplace(&mut x).unwrap();
        assert!(close(x.iter().sum(), 1.0));
        assert!(x[0] < x[1] && x[1] < x[2]);
        // reference: softmax([0,1,2])
        assert!(close(x[2], 0.665_240_96));
    }

    #[test]
    fn softmax_handles_masked_entries() {
        let mut x = [0.0, f32::NEG_INFINITY];
        CpuBackend.softmax_inplace(&mut x).unwrap();
        assert!(close(x[0], 1.0) && close(x[1], 0.0));
        assert!(CpuBackend.softmax_inplace(&mut [f32::NEG_INFINITY]).is_err());
    }

    #[test]
    fn silu_known_values() {
        let mut x = [0.0, 1.0, -1.0, 20.0];
        CpuBackend.silu_inplace(&mut x).unwrap();
        assert!(close(x[0], 0.0));
        assert!(close(x[1], 0.731_058_6));
        assert!(close(x[2], -0.268_941_4));
        assert!(close(x[3], 20.0));
    }
}
