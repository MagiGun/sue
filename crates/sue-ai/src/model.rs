//! Model formats, header parsing, tasks and descriptors.

use std::fmt;

use sue_core::ComponentKind;

use crate::{AiError, ComputeBackend};

/// The safetensors format caps its JSON header at 100 MB.
const SAFETENSORS_MAX_HEADER: u64 = 100_000_000;

/// How a model is stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ModelFormat {
    /// GGUF (quantized transformer weights).
    Gguf,
    /// ONNX graph.
    Onnx,
    /// safetensors weights.
    Safetensors,
    /// Sue's native serialized compute graph (`.magic`).
    Magic,
    /// A kernel compiled into the engine.
    Builtin,
}

impl ModelFormat {
    /// The format implied by a file name's extension.
    #[must_use]
    pub fn from_extension(file_name: &str) -> Option<Self> {
        let (_, extension) = file_name.rsplit_once('.')?;
        match extension.to_ascii_lowercase().as_str() {
            "gguf" => Some(Self::Gguf),
            "onnx" => Some(Self::Onnx),
            "safetensors" => Some(Self::Safetensors),
            "magic" => Some(Self::Magic),
            _ => None,
        }
    }

    /// The format recognised from the first bytes of a file. ONNX and `.magic`
    /// have no magic number and are only known by extension.
    #[must_use]
    pub fn sniff(header: &[u8]) -> Option<Self> {
        if GgufHeader::parse(header).is_ok() {
            Some(Self::Gguf)
        } else if safetensors_header_len(header).is_ok() {
            Some(Self::Safetensors)
        } else {
            None
        }
    }

    /// Display name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Gguf => "GGUF",
            Self::Onnx => "ONNX",
            Self::Safetensors => "safetensors",
            Self::Magic => "magic graph",
            Self::Builtin => "built-in kernel",
        }
    }
}

impl fmt::Display for ModelFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The fixed part of a GGUF header.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GgufHeader {
    /// Format version (1–3).
    pub version: u32,
    /// Number of tensors.
    pub tensor_count: u64,
    /// Number of metadata key/value pairs.
    pub metadata_kv_count: u64,
}

impl GgufHeader {
    /// Parses `GGUF`, the version, and the tensor and metadata counts (32-bit
    /// in version 1, 64-bit after).
    ///
    /// # Errors
    /// [`AiError::UnknownFormat`], [`AiError::MalformedHeader`] or
    /// [`AiError::UnsupportedVersion`].
    pub fn parse(bytes: &[u8]) -> Result<Self, AiError> {
        if !bytes.starts_with(b"GGUF") {
            return Err(AiError::UnknownFormat);
        }
        let truncated = AiError::MalformedHeader("GGUF header is truncated");
        let u32_at = |offset: usize| -> Option<u32> {
            Some(u32::from_le_bytes(
                bytes.get(offset..offset + 4)?.try_into().ok()?,
            ))
        };
        let u64_at = |offset: usize| -> Option<u64> {
            Some(u64::from_le_bytes(
                bytes.get(offset..offset + 8)?.try_into().ok()?,
            ))
        };
        let version = u32_at(4).ok_or(truncated.clone())?;
        let (tensor_count, metadata_kv_count) = match version {
            1 => (
                u64::from(u32_at(8).ok_or(truncated.clone())?),
                u64::from(u32_at(12).ok_or(truncated)?),
            ),
            2 | 3 => (
                u64_at(8).ok_or(truncated.clone())?,
                u64_at(16).ok_or(truncated)?,
            ),
            other => return Err(AiError::UnsupportedVersion(other)),
        };
        Ok(Self {
            version,
            tensor_count,
            metadata_kv_count,
        })
    }
}

/// The length of a safetensors JSON header: a little-endian `u64` of at most
/// 100 MB, followed by `{`.
///
/// # Errors
/// [`AiError::MalformedHeader`].
pub fn safetensors_header_len(bytes: &[u8]) -> Result<u64, AiError> {
    let prefix: [u8; 8] = bytes
        .get(..8)
        .and_then(|prefix| prefix.try_into().ok())
        .ok_or(AiError::MalformedHeader("safetensors header is truncated"))?;
    let length = u64::from_le_bytes(prefix);
    if length == 0 || length > SAFETENSORS_MAX_HEADER {
        return Err(AiError::MalformedHeader(
            "safetensors header length is out of range",
        ));
    }
    if bytes.get(8).is_some_and(|&byte| byte != b'{') {
        return Err(AiError::MalformedHeader(
            "safetensors header is not a JSON object",
        ));
    }
    Ok(length)
}

/// What a model does, which fixes the components it reads and writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AiTask {
    /// Speech to text.
    Transcription,
    /// Split a mix into stems.
    StemSeparation,
    /// Image super-resolution.
    Upscaling,
    /// Text recognition in images.
    Ocr,
    /// Fill masked image regions.
    Inpainting,
    /// Shader source from a description.
    ShaderGeneration,
    /// Refactoring and completion over syntax trees.
    CodeAssist,
    /// Image understanding and captioning.
    Vision,
}

impl AiTask {
    /// The component the task reads.
    #[must_use]
    pub const fn input_kind(self) -> ComponentKind {
        match self {
            Self::Transcription | Self::StemSeparation => ComponentKind::PCMFloat1D,
            Self::Upscaling | Self::Ocr | Self::Inpainting | Self::Vision => {
                ComponentKind::RasterTile2D
            }
            Self::ShaderGeneration => ComponentKind::StringBuffer,
            Self::CodeAssist => ComponentKind::ASTNode,
        }
    }

    /// The component the task writes.
    #[must_use]
    pub const fn output_kind(self) -> ComponentKind {
        match self {
            Self::StemSeparation => ComponentKind::PCMFloat1D,
            Self::Upscaling | Self::Inpainting => ComponentKind::RasterTile2D,
            Self::Transcription | Self::Ocr | Self::Vision | Self::ShaderGeneration => {
                ComponentKind::StringBuffer
            }
            Self::CodeAssist => ComponentKind::ASTNode,
        }
    }
}

/// A model available on this device.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelDescriptor {
    /// Unique id, e.g. `whisper-small-q5`.
    pub id: String,
    /// What it does.
    pub task: AiTask,
    /// How it is stored.
    pub format: ModelFormat,
    /// Size on disk in bytes.
    pub size_bytes: u64,
    /// Quantization label, e.g. `Q4_K_M`.
    pub quantization: Option<String>,
    /// Memory needed to run it, in bytes.
    pub min_memory_bytes: u64,
    /// Backends that can execute it.
    pub backends: Vec<ComputeBackend>,
}

impl ModelDescriptor {
    /// A descriptor that runs on the CPU, with no size or memory requirement.
    pub fn new(id: impl Into<String>, task: AiTask, format: ModelFormat) -> Self {
        Self {
            id: id.into(),
            task,
            format,
            size_bytes: 0,
            quantization: None,
            min_memory_bytes: 0,
            backends: vec![ComputeBackend::CpuSimd],
        }
    }

    /// Describes a model file from its name and first bytes. The header wins
    /// over the extension; GGUF and safetensors headers are validated.
    ///
    /// # Errors
    /// [`AiError::UnknownFormat`], or the header's parse error.
    pub fn from_file_header(
        id: impl Into<String>,
        task: AiTask,
        file_name: &str,
        header: &[u8],
        size_bytes: u64,
    ) -> Result<Self, AiError> {
        let format = ModelFormat::sniff(header)
            .or_else(|| ModelFormat::from_extension(file_name))
            .ok_or(AiError::UnknownFormat)?;
        match format {
            ModelFormat::Gguf => {
                GgufHeader::parse(header)?;
            }
            ModelFormat::Safetensors => {
                safetensors_header_len(header)?;
            }
            ModelFormat::Onnx | ModelFormat::Magic | ModelFormat::Builtin => {}
        }
        Ok(Self {
            size_bytes,
            ..Self::new(id, task, format)
        })
    }

    /// Sets the size on disk.
    #[must_use]
    pub fn with_size(mut self, size_bytes: u64) -> Self {
        self.size_bytes = size_bytes;
        self
    }

    /// Sets the memory requirement.
    #[must_use]
    pub fn with_memory(mut self, min_memory_bytes: u64) -> Self {
        self.min_memory_bytes = min_memory_bytes;
        self
    }

    /// Sets the quantization label.
    #[must_use]
    pub fn with_quantization(mut self, quantization: impl Into<String>) -> Self {
        self.quantization = Some(quantization.into());
        self
    }

    /// Sets the supported backends.
    #[must_use]
    pub fn with_backends(mut self, backends: impl IntoIterator<Item = ComputeBackend>) -> Self {
        self.backends = backends.into_iter().collect();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gguf(version: u32, tensors: u64, kvs: u64) -> Vec<u8> {
        let mut bytes = b"GGUF".to_vec();
        bytes.extend_from_slice(&version.to_le_bytes());
        if version == 1 {
            bytes.extend_from_slice(&u32::try_from(tensors).unwrap().to_le_bytes());
            bytes.extend_from_slice(&u32::try_from(kvs).unwrap().to_le_bytes());
        } else {
            bytes.extend_from_slice(&tensors.to_le_bytes());
            bytes.extend_from_slice(&kvs.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn parses_gguf_headers() {
        let header = GgufHeader::parse(&gguf(3, 291, 24)).unwrap();
        assert_eq!(
            header,
            GgufHeader {
                version: 3,
                tensor_count: 291,
                metadata_kv_count: 24
            }
        );
        assert_eq!(GgufHeader::parse(&gguf(1, 7, 2)).unwrap().tensor_count, 7);
        assert_eq!(
            GgufHeader::parse(&gguf(9, 0, 0)),
            Err(AiError::UnsupportedVersion(9))
        );
        assert!(matches!(
            GgufHeader::parse(&gguf(3, 1, 1)[..12]),
            Err(AiError::MalformedHeader(_))
        ));
        assert_eq!(GgufHeader::parse(b"GGML"), Err(AiError::UnknownFormat));
    }

    #[test]
    fn parses_safetensors_headers() {
        let mut bytes = 2_u64.to_le_bytes().to_vec();
        bytes.extend_from_slice(b"{}");
        assert_eq!(safetensors_header_len(&bytes), Ok(2));
        assert!(safetensors_header_len(&u64::MAX.to_le_bytes()).is_err());
        let mut not_json = 2_u64.to_le_bytes().to_vec();
        not_json.extend_from_slice(b"[]");
        assert!(safetensors_header_len(&not_json).is_err());
        assert!(safetensors_header_len(b"short").is_err());
    }

    #[test]
    fn identifies_formats() {
        assert_eq!(
            ModelFormat::from_extension("llama-3.Q4_K_M.GGUF"),
            Some(ModelFormat::Gguf)
        );
        assert_eq!(
            ModelFormat::from_extension("demucs.onnx"),
            Some(ModelFormat::Onnx)
        );
        assert_eq!(
            ModelFormat::from_extension("sdxl.safetensors"),
            Some(ModelFormat::Safetensors)
        );
        assert_eq!(
            ModelFormat::from_extension("upscale.magic"),
            Some(ModelFormat::Magic)
        );
        assert_eq!(ModelFormat::from_extension("README"), None);
        assert_eq!(ModelFormat::sniff(&gguf(2, 0, 0)), Some(ModelFormat::Gguf));

        let descriptor = ModelDescriptor::from_file_header(
            "whisper",
            AiTask::Transcription,
            "whisper.bin",
            &gguf(3, 1, 1),
            42,
        )
        .unwrap();
        assert_eq!(
            (descriptor.format, descriptor.size_bytes),
            (ModelFormat::Gguf, 42)
        );
        assert!(ModelDescriptor::from_file_header("x", AiTask::Ocr, "x.bin", b"????", 1).is_err());
        assert!(
            ModelDescriptor::from_file_header("x", AiTask::Ocr, "x.gguf", b"GGUF\x09\0\0\0", 1)
                .is_err()
        );
    }

    #[test]
    fn tasks_fix_their_components() {
        assert_eq!(
            AiTask::Transcription.output_kind(),
            ComponentKind::StringBuffer
        );
        assert_eq!(
            AiTask::StemSeparation.input_kind(),
            ComponentKind::PCMFloat1D
        );
        assert_eq!(AiTask::CodeAssist.output_kind(), ComponentKind::ASTNode);
    }
}
