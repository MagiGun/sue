use std::fmt;

use sue_core::TxnError;

/// Errors from parsing or editing a rice schema. Line numbers are 1-based.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RiceError {
    /// Malformed TOML.
    Syntax {
        /// Line of the error.
        line: usize,
        /// What is wrong.
        message: String,
    },
    /// Valid TOML that `.suerice` does not support.
    Unsupported {
        /// Line of the construct.
        line: usize,
        /// The unsupported feature.
        feature: &'static str,
    },
    /// Metadata that does not fit the value, or is malformed.
    Metadata {
        /// Line of the error.
        line: usize,
        /// What is wrong.
        message: String,
    },
    /// A key was defined twice.
    Duplicate {
        /// Line of the second definition.
        line: usize,
        /// The duplicated path.
        path: String,
    },
    /// No parameter has this path.
    UnknownKey(String),
    /// A path that is not `section.key` made of bare keys.
    InvalidPath(String),
    /// A value of the wrong type was supplied.
    TypeMismatch {
        /// Parameter path.
        path: String,
        /// Type the parameter holds.
        expected: &'static str,
        /// Type that was supplied.
        found: &'static str,
    },
    /// A number outside the parameter's range.
    OutOfRange {
        /// Parameter path.
        path: String,
        /// Supplied value.
        value: String,
        /// Allowed range, e.g. `[0..64]`.
        range: String,
    },
    /// A string that is not one of the parameter's choices.
    InvalidChoice {
        /// Parameter path.
        path: String,
        /// Supplied value.
        value: String,
    },
    /// A float that is NaN or infinite.
    NonFinite(String),
    /// A string that is not a hex color.
    InvalidColor(String),
    /// A string that is not a keybind.
    InvalidKeybind(String),
}

impl fmt::Display for RiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Syntax { line, message } => write!(f, "line {line}: {message}"),
            Self::Unsupported { line, feature } => {
                write!(
                    f,
                    "line {line}: {feature} are not supported in .suerice files"
                )
            }
            Self::Metadata { line, message } => write!(f, "line {line}: {message}"),
            Self::Duplicate { line, path } => write!(f, "line {line}: `{path}` is defined twice"),
            Self::UnknownKey(path) => write!(f, "no rice parameter named `{path}`"),
            Self::InvalidPath(path) => write!(f, "`{path}` is not a valid rice path"),
            Self::TypeMismatch {
                path,
                expected,
                found,
            } => write!(f, "`{path}` holds {expected}, not {found}"),
            Self::OutOfRange { path, value, range } => {
                write!(f, "{value} is outside the range {range} of `{path}`")
            }
            Self::InvalidChoice { path, value } => {
                write!(f, "\"{value}\" is not one of the choices of `{path}`")
            }
            Self::NonFinite(path) => write!(f, "`{path}` must be a finite number"),
            Self::InvalidColor(text) => write!(f, "\"{text}\" is not a hex color"),
            Self::InvalidKeybind(text) => write!(f, "\"{text}\" is not a keybind"),
        }
    }
}

impl std::error::Error for RiceError {}

impl From<RiceError> for TxnError {
    fn from(error: RiceError) -> Self {
        Self::with_source(error.to_string(), error)
    }
}

/// A recoverable problem found while loading: the value was adjusted and
/// loading continued.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RiceDiagnostic {
    /// 1-based line, if the parameter came from a file.
    pub line: Option<usize>,
    /// Parameter path.
    pub path: String,
    /// What was adjusted and why.
    pub message: String,
}

impl fmt::Display for RiceDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(f, "line {line}: `{}`: {}", self.path, self.message),
            None => write!(f, "`{}`: {}", self.path, self.message),
        }
    }
}
