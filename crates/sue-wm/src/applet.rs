//! The applet contract every tool, utility and studio implements (LAW 1).

use std::fmt;

use sue_core::{Arena, CoreError, TxnError, UndoStack};
use sue_platform::{FormFactor, InputEvent};
use sue_rice::{HardwareProfile, RiceDeclaration, RiceSchema};

use crate::{RenderDemand, WmError, Zone};

/// An applet's unique id: `[a-z][a-z0-9-]*`, e.g. `quick-notes`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AppletId(String);

impl AppletId {
    /// Validates and wraps an id.
    ///
    /// # Errors
    /// [`WmError::InvalidAppletId`].
    pub fn new(id: &str) -> Result<Self, WmError> {
        let starts_with_letter = id
            .bytes()
            .next()
            .is_some_and(|byte| byte.is_ascii_lowercase());
        let body_ok = id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
        if starts_with_letter && body_ok {
            Ok(Self(id.to_owned()))
        } else {
            Err(WmError::InvalidAppletId(id.to_owned()))
        }
    }

    /// The id text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AppletId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Where an applet is listed on the Hub.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AppletCategory {
    /// A small everyday tool (file explorer, player, notes, converter).
    Utility,
    /// A full creative environment (DAW, CAD, IDE, …).
    Studio,
}

/// Static facts about an applet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppletManifest {
    /// Unique id.
    pub id: AppletId,
    /// Display name.
    pub name: String,
    /// Hub category.
    pub category: AppletCategory,
    /// Zone the applet lives in by default.
    pub home_zone: Zone,
    /// `true` if the applet works on time-based media, which shows the timeline.
    pub uses_timeline: bool,
    /// One-line description for the Hub.
    pub summary: String,
}

/// What an applet may touch while handling a call. Applets hold handles, not
/// data, and change the arena only through transactions on `history`.
pub struct AppletContext<'a> {
    /// The shared arena.
    pub arena: &'a mut Arena,
    /// The shared undo tree.
    pub history: &'a mut UndoStack<Arena>,
    /// The live rice.
    pub rice: &'a RiceSchema,
    /// Device form factor.
    pub form_factor: FormFactor,
    /// Active hardware profile.
    pub profile: HardwareProfile,
}

impl fmt::Debug for AppletContext<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AppletContext")
            .field("entities", &self.arena.len())
            .field("history", &self.history)
            .field("form_factor", &self.form_factor)
            .field("profile", &self.profile)
            .finish_non_exhaustive()
    }
}

/// How an applet reacted to an input event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputResponse {
    /// Not handled; the event may go elsewhere.
    Ignored,
    /// Handled.
    Consumed {
        /// `true` if something visible changed.
        redraw: bool,
    },
}

/// An error raised by an applet.
#[derive(Debug)]
pub struct AppletError {
    message: String,
}

impl AppletError {
    /// An error with a message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for AppletError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AppletError {}

impl From<CoreError> for AppletError {
    fn from(error: CoreError) -> Self {
        Self::new(error.to_string())
    }
}

impl From<TxnError> for AppletError {
    fn from(error: TxnError) -> Self {
        Self::new(error.to_string())
    }
}

/// The contract every tool, utility and studio implements. The window manager
/// knows applets only through this trait.
pub trait SueApplet: Send {
    /// Static facts about the applet.
    fn manifest(&self) -> AppletManifest;

    /// Riceable parameters the applet declares (LAW 3).
    fn rice_declarations(&self) -> Vec<RiceDeclaration> {
        Vec::new()
    }

    /// Called when the applet is opened in a tab.
    ///
    /// # Errors
    /// Any reason the applet cannot start.
    fn mount(&mut self, ctx: &mut AppletContext<'_>) -> Result<(), AppletError>;

    /// Handles input the window manager did not consume.
    ///
    /// # Errors
    /// Any failure while handling the event.
    fn handle_input(
        &mut self,
        event: &InputEvent,
        ctx: &mut AppletContext<'_>,
    ) -> Result<InputResponse, AppletError>;

    /// Advances animations by `dt` seconds. Static applets keep the default,
    /// which never asks for frames.
    fn update(&mut self, _dt: f32, _ctx: &mut AppletContext<'_>) -> RenderDemand {
        RenderDemand::Sleep
    }

    /// Called when the applet's tab closes or switches to another applet.
    fn unmount(&mut self, _ctx: &mut AppletContext<'_>) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_applet_ids() {
        assert_eq!(
            AppletId::new("quick-notes").unwrap().as_str(),
            "quick-notes"
        );
        assert!(AppletId::new("cad2").is_ok());
        for bad in ["", "Quick", "1st", "with space", "under_score"] {
            assert!(AppletId::new(bad).is_err(), "{bad}");
        }
    }
}
