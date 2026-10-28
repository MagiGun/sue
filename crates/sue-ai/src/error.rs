use std::fmt;

use sue_core::CoreError;

/// AI substrate errors.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum AiError {
    /// An arena error (stale handle, missing component, aliasing).
    Core(CoreError),
    /// A model id was registered twice.
    DuplicateModel(String),
    /// The file is not a recognised model format.
    UnknownFormat,
    /// A model header is inconsistent.
    MalformedHeader(&'static str),
    /// A model format version this build cannot read.
    UnsupportedVersion(u32),
    /// None of the task's backends is available.
    NoBackend {
        /// The task.
        task: String,
    },
    /// Input or output tensors have the wrong shape.
    ShapeMismatch(&'static str),
}

impl fmt::Display for AiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Core(error) => write!(f, "{error}"),
            Self::DuplicateModel(id) => write!(f, "model `{id}` is already registered"),
            Self::UnknownFormat => f.write_str("not a recognised model format"),
            Self::MalformedHeader(reason) => write!(f, "malformed model header: {reason}"),
            Self::UnsupportedVersion(version) => {
                write!(f, "unsupported model format version {version}")
            }
            Self::NoBackend { task } => write!(f, "no available compute backend can run {task}"),
            Self::ShapeMismatch(reason) => write!(f, "tensor shape mismatch: {reason}"),
        }
    }
}

impl std::error::Error for AiError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Core(error) => Some(error),
            _ => None,
        }
    }
}

impl From<CoreError> for AiError {
    fn from(error: CoreError) -> Self {
        Self::Core(error)
    }
}
