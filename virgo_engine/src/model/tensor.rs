//! Tensor abstractions: element types, shapes and where tensor bytes live
//! in the model file. No tensor data is held here.
//!
//! Shapes are row-major, outermost dimension first. GGUF stores dimensions
//! innermost-first, so the Phase 2 GGUF parser must reverse them.
//!
//! Quantized types are described only far enough to compute byte sizes.
//! Dequantization and quantized kernels are not implemented.

use std::fmt;

use crate::engine::error::{Result, VirgoError};

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DType {
    F32,
    F16,
    BF16,
    Q8_0,
    Q4_0,
    Q4_K,
}

impl DType {
    /// Elements per storage block (1 for plain float types).
    pub const fn block_elements(self) -> usize {
        match self {
            DType::F32 | DType::F16 | DType::BF16 => 1,
            DType::Q8_0 | DType::Q4_0 => 32,
            DType::Q4_K => 256,
        }
    }

    /// Bytes per storage block.
    ///
    /// Q8_0: f16 scale + 32 x i8 = 34. Q4_0: f16 scale + 16 packed bytes = 18.
    /// Q4_K: f16 d + f16 dmin + 12 scale bytes + 128 packed bytes = 144.
    pub const fn block_bytes(self) -> usize {
        match self {
            DType::F32 => 4,
            DType::F16 | DType::BF16 => 2,
            DType::Q8_0 => 34,
            DType::Q4_0 => 18,
            DType::Q4_K => 144,
        }
    }

    pub const fn is_quantized(self) -> bool {
        self.block_elements() > 1
    }

    pub const fn name(self) -> &'static str {
        match self {
            DType::F32 => "F32",
            DType::F16 => "F16",
            DType::BF16 => "BF16",
            DType::Q8_0 => "Q8_0",
            DType::Q4_0 => "Q4_0",
            DType::Q4_K => "Q4_K",
        }
    }

    /// Storage size in bytes for `n_elements`, which must be a whole number
    /// of blocks.
    pub fn byte_size(self, n_elements: u64) -> Result<u64> {
        let block = self.block_elements() as u64;
        if n_elements % block != 0 {
            return Err(VirgoError::InvalidModelMetadata(format!(
                "{n_elements} elements is not a multiple of the {} block size {block}",
                self.name()
            )));
        }
        (n_elements / block)
            .checked_mul(self.block_bytes() as u64)
            .ok_or_else(|| VirgoError::InvalidModelMetadata("tensor byte size overflows u64".into()))
    }
}

impl fmt::Display for DType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Absolute byte range of a tensor's data within the model file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TensorLocation {
    pub offset: u64,
    pub len: u64,
}

impl TensorLocation {
    pub fn end(&self) -> Option<u64> {
        self.offset.checked_add(self.len)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TensorInfo {
    pub name: String,
    pub shape: Vec<usize>,
    pub dtype: DType,
    pub location: TensorLocation,
}

impl TensorInfo {
    /// Creates a tensor description, validating that the shape, dtype and
    /// byte length agree.
    pub fn new(
        name: impl Into<String>,
        shape: Vec<usize>,
        dtype: DType,
        location: TensorLocation,
    ) -> Result<Self> {
        let info = Self { name: name.into(), shape, dtype, location };
        info.validate()?;
        Ok(info)
    }

    pub fn num_elements(&self) -> Result<u64> {
        self.shape
            .iter()
            .try_fold(1u64, |acc, &d| acc.checked_mul(d as u64))
            .ok_or_else(|| {
                VirgoError::InvalidModelMetadata(format!("shape of `{}` overflows u64", self.name))
            })
    }

    pub fn expected_byte_size(&self) -> Result<u64> {
        self.dtype.byte_size(self.num_elements()?)
    }

    fn validate(&self) -> Result<()> {
        if self.shape.contains(&0) {
            return Err(VirgoError::InvalidModelMetadata(format!(
                "tensor `{}` has a zero-sized dimension",
                self.name
            )));
        }
        if self.dtype.is_quantized() {
            let block = self.dtype.block_elements();
            match self.shape.last() {
                Some(&row) if row % block == 0 => {}
                _ => {
                    return Err(VirgoError::InvalidModelMetadata(format!(
                        "tensor `{}`: innermost dimension must be a multiple of {block} for {}",
                        self.name, self.dtype
                    )));
                }
            }
        }
        let expected = self.expected_byte_size()?;
        if self.location.len != expected {
            return Err(VirgoError::InvalidTensorOffset {
                tensor: self.name.clone(),
                reason: format!(
                    "stored length {} does not match {expected} bytes expected from shape and dtype",
                    self.location.len
                ),
            });
        }
        if self.location.end().is_none() {
            return Err(VirgoError::InvalidTensorOffset {
                tensor: self.name.clone(),
                reason: "offset + length overflows u64".into(),
            });
        }
        Ok(())
    }

    /// Verifies the tensor's byte range lies inside a file of `file_len` bytes.
    pub fn check_within_file(&self, file_len: u64) -> Result<()> {
        match self.location.end() {
            Some(end) if end <= file_len => Ok(()),
            _ => Err(VirgoError::InvalidTensorOffset {
                tensor: self.name.clone(),
                reason: format!(
                    "range {}..+{} exceeds file length {file_len}",
                    self.location.offset, self.location.len
                ),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loc(offset: u64, len: u64) -> TensorLocation {
        TensorLocation { offset, len }
    }

    #[test]
    fn block_sizes() {
        assert_eq!(DType::F32.byte_size(10).unwrap(), 40);
        assert_eq!(DType::F16.byte_size(10).unwrap(), 20);
        assert_eq!(DType::BF16.byte_size(10).unwrap(), 20);
        assert_eq!(DType::Q8_0.byte_size(64).unwrap(), 68);
        assert_eq!(DType::Q4_0.byte_size(64).unwrap(), 36);
        assert_eq!(DType::Q4_K.byte_size(512).unwrap(), 288);
    }

    #[test]
    fn partial_block_is_rejected() {
        assert!(DType::Q8_0.byte_size(33).is_err());
    }

    #[test]
    fn valid_tensor() {
        let t = TensorInfo::new("w", vec![4, 8], DType::F32, loc(100, 128)).unwrap();
        assert_eq!(t.num_elements().unwrap(), 32);
        assert!(t.check_within_file(228).is_ok());
    }

    #[test]
    fn length_mismatch_is_rejected() {
        let err = TensorInfo::new("w", vec![4, 8], DType::F32, loc(0, 100)).unwrap_err();
        assert!(matches!(err, VirgoError::InvalidTensorOffset { .. }));
    }

    #[test]
    fn zero_dimension_is_rejected() {
        assert!(TensorInfo::new("w", vec![4, 0], DType::F32, loc(0, 0)).is_err());
    }

    #[test]
    fn quantized_row_must_be_block_aligned() {
        assert!(TensorInfo::new("w", vec![2, 48], DType::Q8_0, loc(0, 0)).is_err());
        assert!(TensorInfo::new("w", vec![2, 64], DType::Q8_0, loc(0, 136)).is_ok());
    }

    #[test]
    fn range_beyond_file_is_rejected() {
        let t = TensorInfo::new("w", vec![4], DType::F32, loc(90, 16)).unwrap();
        assert!(t.check_within_file(105).is_err());
        assert!(t.check_within_file(106).is_ok());
    }

    #[test]
    fn offset_overflow_is_rejected() {
        let t = TensorInfo::new("w", vec![4], DType::F32, loc(u64::MAX - 4, 16));
        assert!(t.is_err());
    }
}
