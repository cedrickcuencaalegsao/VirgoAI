//! Model file access. Phase 1 only identifies the container format by
//! reading a few leading bytes; it never reads tensor data.
//!
//! Phase 2 will add GGUF / Safetensors header parsing and tensor access
//! (investigating `memmap2` and bounded reads).

use std::fs::File;
use std::io::{ErrorKind, Read};
use std::path::Path;

use crate::engine::error::{Result, VirgoError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModelFormat {
    Gguf,
    Safetensors,
}

/// Identifies the file format from its content, not its extension.
///
/// GGUF starts with the magic bytes `GGUF`. Safetensors starts with a
/// little-endian u64 header length followed by a JSON header beginning
/// with `{`; the length must fit inside the file.
pub fn detect_format(path: &Path) -> Result<ModelFormat> {
    let io_err = |source| VirgoError::StorageReadFailed { path: path.to_path_buf(), source };

    let mut file = File::open(path).map_err(|e| {
        if e.kind() == ErrorKind::NotFound {
            VirgoError::ModelNotFound(path.to_path_buf())
        } else {
            io_err(e)
        }
    })?;
    let file_len = file.metadata().map_err(io_err)?.len();

    let mut head = [0u8; 9];
    let mut filled = 0;
    while filled < head.len() {
        match file.read(&mut head[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) => return Err(io_err(e)),
        }
    }

    if filled >= 4 && &head[..4] == b"GGUF" {
        return Ok(ModelFormat::Gguf);
    }
    if filled == head.len() {
        let header_len = u64::from_le_bytes(head[..8].try_into().expect("8 bytes"));
        if head[8] == b'{' && header_len > 0 && header_len <= file_len - 8 {
            return Ok(ModelFormat::Safetensors);
        }
    }
    Err(VirgoError::UnsupportedModelFormat(format!(
        "{} is neither GGUF nor Safetensors",
        path.display()
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn write_temp(tag: &str, bytes: &[u8]) -> PathBuf {
        let path = std::env::temp_dir().join(format!("virgo_{}_{tag}", std::process::id()));
        fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn detects_gguf() {
        let p = write_temp("gguf", b"GGUF\x03\x00\x00\x00rest");
        assert_eq!(detect_format(&p).unwrap(), ModelFormat::Gguf);
        fs::remove_file(p).unwrap();
    }

    #[test]
    fn detects_safetensors() {
        let json = br#"{"__metadata__":{}}"#;
        let mut bytes = (json.len() as u64).to_le_bytes().to_vec();
        bytes.extend_from_slice(json);
        let p = write_temp("st", &bytes);
        assert_eq!(detect_format(&p).unwrap(), ModelFormat::Safetensors);
        fs::remove_file(p).unwrap();
    }

    #[test]
    fn safetensors_header_longer_than_file_is_rejected() {
        let mut bytes = 1000u64.to_le_bytes().to_vec();
        bytes.extend_from_slice(b"{}");
        let p = write_temp("st_bad", &bytes);
        assert!(matches!(detect_format(&p), Err(VirgoError::UnsupportedModelFormat(_))));
        fs::remove_file(p).unwrap();
    }

    #[test]
    fn unknown_and_tiny_files_are_rejected() {
        let p = write_temp("junk", b"hello world, not a model");
        assert!(matches!(detect_format(&p), Err(VirgoError::UnsupportedModelFormat(_))));
        fs::remove_file(&p).unwrap();
        let p = write_temp("tiny", b"GG");
        assert!(detect_format(&p).is_err());
        fs::remove_file(p).unwrap();
    }

    #[test]
    fn missing_file_reports_model_not_found() {
        let p = std::env::temp_dir().join("virgo_definitely_missing.gguf");
        assert!(matches!(detect_format(&p), Err(VirgoError::ModelNotFound(_))));
    }
}
