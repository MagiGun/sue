use std::fmt;

use sue_core::TxnError;

use crate::{AppletError, AppletId, MediaClass};

/// Window manager errors.
#[derive(Debug)]
#[non_exhaustive]
pub enum WmError {
    /// An applet id that is not `[a-z][a-z0-9-]*`.
    InvalidAppletId(String),
    /// Two applets share an id.
    DuplicateApplet(AppletId),
    /// No applet is registered with this id.
    UnknownApplet(AppletId),
    /// No installed Center Stage applet opens this kind of asset.
    NothingOpens(MediaClass),
    /// A rice parameter the window manager needs is missing.
    MissingRice(&'static str),
    /// An applet failed.
    Applet(AppletError),
    /// Undo or redo failed.
    History(TxnError),
}

impl fmt::Display for WmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidAppletId(id) => write!(f, "`{id}` is not a valid applet id"),
            Self::DuplicateApplet(id) => write!(f, "applet `{id}` is already registered"),
            Self::UnknownApplet(id) => write!(f, "no applet named `{id}` is installed"),
            Self::NothingOpens(class) => write!(f, "no installed applet opens {class:?} assets"),
            Self::MissingRice(path) => write!(
                f,
                "rice parameter `{path}` is missing; call sue_wm::register_rice"
            ),
            Self::Applet(error) => write!(f, "applet error: {error}"),
            Self::History(error) => write!(f, "history error: {error}"),
        }
    }
}

impl std::error::Error for WmError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Applet(error) => Some(error),
            Self::History(error) => Some(error),
            _ => None,
        }
    }
}

impl From<AppletError> for WmError {
    fn from(error: AppletError) -> Self {
        Self::Applet(error)
    }
}

impl From<TxnError> for WmError {
    fn from(error: TxnError) -> Self {
        Self::History(error)
    }
}
