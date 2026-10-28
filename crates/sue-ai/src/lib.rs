//! # sue-ai: the offline, engine-level AI substrate
//!
//! [`LocalModelRegistry`] tracks models stored on the device (GGUF, ONNX,
//! safetensors, `.magic` graphs and built-in kernels) and selects one for a
//! task given the available [`ComputeBackend`]s and a memory budget.
//! [`ZeroCopyTensorTask`] runs a kernel straight on arena memory: it takes an
//! input and an output [`AssetHandle`](sue_core::AssetHandle), borrows both
//! entities at once, and never copies a buffer or touches the disk.
//!
//! Nothing in this crate opens a network connection.
//!
//! Specification: `HUMANZ.md` §6.

mod backend;
mod error;
mod model;
mod registry;
mod task;

pub use backend::ComputeBackend;
pub use error::AiError;
pub use model::{AiTask, GgufHeader, ModelDescriptor, ModelFormat, safetensors_header_len};
pub use registry::{LocalModelRegistry, ModelSelection};
pub use task::{MidSideStemSplit, TaskReport, ZeroCopyTensorTask};
