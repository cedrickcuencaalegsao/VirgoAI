//! Model metadata. Values must come from the model file itself (GGUF
//! key/values or the Safetensors header plus config); nothing here is
//! specific to any model family.

use std::collections::{BTreeMap, BTreeSet};

use super::tensor::{DType, TensorInfo};
use crate::engine::error::{Result, VirgoError};

#[derive(Debug, Clone, PartialEq)]
pub struct ModelMetadata {
    pub architecture: String,
    pub name: Option<String>,
    pub hidden_size: usize,
    pub num_layers: usize,
    pub num_attention_heads: usize,
    pub num_kv_heads: usize,
    /// Explicit per-head dimension when the file provides one. Some
    /// architectures decouple it from `hidden_size / num_attention_heads`.
    pub head_dim: Option<usize>,
    pub feed_forward_size: usize,
    pub vocab_size: usize,
    pub context_length: usize,
    tensors: BTreeMap<String, TensorInfo>,
}

/// Plain architecture hyper-parameters, used to construct [`ModelMetadata`].
#[derive(Debug, Clone, PartialEq)]
pub struct ModelParams {
    pub architecture: String,
    pub name: Option<String>,
    pub hidden_size: usize,
    pub num_layers: usize,
    pub num_attention_heads: usize,
    pub num_kv_heads: usize,
    pub head_dim: Option<usize>,
    pub feed_forward_size: usize,
    pub vocab_size: usize,
    pub context_length: usize,
}

impl ModelMetadata {
    /// Builds validated metadata with an empty tensor index.
    pub fn new(p: ModelParams) -> Result<Self> {
        let meta = Self {
            architecture: p.architecture,
            name: p.name,
            hidden_size: p.hidden_size,
            num_layers: p.num_layers,
            num_attention_heads: p.num_attention_heads,
            num_kv_heads: p.num_kv_heads,
            head_dim: p.head_dim,
            feed_forward_size: p.feed_forward_size,
            vocab_size: p.vocab_size,
            context_length: p.context_length,
            tensors: BTreeMap::new(),
        };
        meta.validate()?;
        Ok(meta)
    }

    fn validate(&self) -> Result<()> {
        let bad = |msg: String| Err(VirgoError::InvalidModelMetadata(msg));
        if self.architecture.trim().is_empty() {
            return bad("architecture is empty".into());
        }
        let counts = [
            ("hidden_size", self.hidden_size),
            ("num_layers", self.num_layers),
            ("num_attention_heads", self.num_attention_heads),
            ("num_kv_heads", self.num_kv_heads),
            ("feed_forward_size", self.feed_forward_size),
            ("vocab_size", self.vocab_size),
            ("context_length", self.context_length),
        ];
        for (field, value) in counts {
            if value == 0 {
                return bad(format!("{field} must be greater than zero"));
            }
        }
        if self.head_dim == Some(0) {
            return bad("head_dim must be greater than zero when present".into());
        }
        if self.num_attention_heads % self.num_kv_heads != 0 {
            return bad(format!(
                "num_attention_heads ({}) must be a multiple of num_kv_heads ({})",
                self.num_attention_heads, self.num_kv_heads
            ));
        }
        self.head_dim()?;
        Ok(())
    }

    /// Per-head dimension: the explicit value if present, otherwise
    /// `hidden_size / num_attention_heads` (which must divide evenly).
    pub fn head_dim(&self) -> Result<usize> {
        if let Some(d) = self.head_dim {
            return Ok(d);
        }
        if self.hidden_size % self.num_attention_heads != 0 {
            return Err(VirgoError::InvalidModelMetadata(format!(
                "hidden_size ({}) is not divisible by num_attention_heads ({}) and no head_dim was given",
                self.hidden_size, self.num_attention_heads
            )));
        }
        Ok(self.hidden_size / self.num_attention_heads)
    }

    /// Number of query heads sharing each KV head (grouped-query attention).
    pub fn gqa_group_size(&self) -> usize {
        self.num_attention_heads / self.num_kv_heads
    }

    /// Adds a tensor to the index. Duplicate names are an error.
    pub fn add_tensor(&mut self, info: TensorInfo) -> Result<()> {
        if self.tensors.contains_key(&info.name) {
            return Err(VirgoError::InvalidModelMetadata(format!(
                "duplicate tensor name `{}`",
                info.name
            )));
        }
        self.tensors.insert(info.name.clone(), info);
        Ok(())
    }

    pub fn tensor(&self, name: &str) -> Result<&TensorInfo> {
        self.tensors
            .get(name)
            .ok_or_else(|| VirgoError::TensorNotFound(name.to_string()))
    }

    pub fn tensors(&self) -> impl Iterator<Item = &TensorInfo> {
        self.tensors.values()
    }

    pub fn tensor_count(&self) -> usize {
        self.tensors.len()
    }

    /// Distinct element types present in the tensor index.
    pub fn weight_dtypes(&self) -> BTreeSet<DType> {
        self.tensors.values().map(|t| t.dtype).collect()
    }

    /// Sum of all indexed tensor byte lengths (file bytes, not RAM).
    pub fn total_tensor_bytes(&self) -> u64 {
        self.tensors.values().map(|t| t.location.len).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::tensor::TensorLocation;

    fn params() -> ModelParams {
        ModelParams {
            architecture: "test".into(),
            name: None,
            hidden_size: 64,
            num_layers: 2,
            num_attention_heads: 8,
            num_kv_heads: 2,
            head_dim: None,
            feed_forward_size: 128,
            vocab_size: 100,
            context_length: 512,
        }
    }

    fn tensor(name: &str, offset: u64) -> TensorInfo {
        TensorInfo::new(name, vec![4], DType::F32, TensorLocation { offset, len: 16 }).unwrap()
    }

    #[test]
    fn valid_metadata_derives_head_dim_and_group_size() {
        let m = ModelMetadata::new(params()).unwrap();
        assert_eq!(m.head_dim().unwrap(), 8);
        assert_eq!(m.gqa_group_size(), 4);
    }

    #[test]
    fn explicit_head_dim_overrides_derivation() {
        let mut p = params();
        p.hidden_size = 60; // not divisible by 8 heads
        assert!(ModelMetadata::new(p.clone()).is_err());
        p.head_dim = Some(16);
        assert_eq!(ModelMetadata::new(p).unwrap().head_dim().unwrap(), 16);
    }

    #[test]
    fn rejects_zero_and_inconsistent_values() {
        let mut p = params();
        p.num_layers = 0;
        assert!(ModelMetadata::new(p).is_err());

        let mut p = params();
        p.num_kv_heads = 3; // 8 % 3 != 0
        assert!(ModelMetadata::new(p).is_err());

        let mut p = params();
        p.architecture = "  ".into();
        assert!(ModelMetadata::new(p).is_err());
    }

    #[test]
    fn tensor_index_lookup_and_duplicates() {
        let mut m = ModelMetadata::new(params()).unwrap();
        m.add_tensor(tensor("a", 0)).unwrap();
        m.add_tensor(tensor("b", 16)).unwrap();
        assert!(m.add_tensor(tensor("a", 32)).is_err());
        assert_eq!(m.tensor("b").unwrap().location.offset, 16);
        assert!(matches!(m.tensor("zzz"), Err(VirgoError::TensorNotFound(_))));
        assert_eq!(m.tensor_count(), 2);
        assert_eq!(m.total_tensor_bytes(), 32);
        assert_eq!(m.weight_dtypes().len(), 1);
    }
}
