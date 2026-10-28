//! Boot: wires the platform host, rice, arena, undo trees and window manager
//! together.

use std::fmt;
use std::io;
use std::path::PathBuf;

use sue_core::{Arena, UndoStack};
use sue_platform::{HeadlessHost, PlatformHost};
use sue_rice::{DEFAULT_RICE, HardwareProfile, RiceDeclaration, RiceParameter, RiceSchema};
use sue_wm::{AppletContext, AppletId, Launchpad, WmConfig, Workspace};

use crate::quick_notes::QuickNotes;

/// The user's rice, looked up in the platform rice directory.
pub const USER_RICE_FILE: &str = "user.suerice";

/// Where the live rice came from.
#[derive(Debug)]
pub enum RiceSource {
    /// The rice embedded in the binary.
    Default,
    /// A user file.
    User(PathBuf),
}

impl fmt::Display for RiceSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Default => f.write_str("built-in default.suerice"),
            Self::User(path) => write!(f, "{}", path.display()),
        }
    }
}

/// The riceable width of the byte-map view (LAW 3).
pub fn byte_map_declaration() -> RiceDeclaration {
    RiceDeclaration::new(
        "io.byte_map_width",
        RiceParameter::IntRange {
            value: 64,
            min: Some(16),
            max: Some(1024),
        },
        "Columns in the hex / byte-map view of unknown files",
    )
}

/// Every subsystem, booted and connected.
pub struct Engine {
    /// Platform host.
    pub host: HeadlessHost,
    /// The live rice.
    pub rice: RiceSchema,
    /// Undo tree for Settings edits.
    pub rice_history: UndoStack<RiceSchema>,
    /// Where the rice came from.
    pub rice_source: RiceSource,
    /// The shared data arena.
    pub arena: Arena,
    /// Undo tree for arena edits.
    pub history: UndoStack<Arena>,
    /// Active hardware profile.
    pub profile: HardwareProfile,
    /// The shell.
    pub workspace: Workspace,
}

impl Engine {
    /// Detects the platform, loads rice (the user's file if present), registers
    /// every riceable, installs the built-in applets and boots the workspace.
    ///
    /// # Errors
    /// A message naming the subsystem that failed.
    pub fn boot() -> Result<Self, String> {
        let host = HeadlessHost::detect().map_err(|error| format!("platform: {error}"))?;

        let user_rice = host.paths().rice_dir().join(USER_RICE_FILE);
        let (text, rice_source) = match std::fs::read_to_string(&user_rice) {
            Ok(text) => (text, RiceSource::User(user_rice)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                (DEFAULT_RICE.to_owned(), RiceSource::Default)
            }
            Err(error) => {
                return Err(format!(
                    "rice: cannot read {}: {error}",
                    user_rice.display()
                ));
            }
        };
        let mut rice = RiceSchema::parse(&text).map_err(|error| format!("rice: {error}"))?;

        let mut launchpad = Launchpad::new();
        launchpad
            .register(|| Box::new(QuickNotes::default()))
            .map_err(|error| format!("launchpad: {error}"))?;

        sue_wm::register_rice(&mut rice).map_err(|error| format!("rice: {error}"))?;
        rice.register(&byte_map_declaration())
            .map_err(|error| format!("rice: {error}"))?;
        rice.register_all(&launchpad.rice_declarations())
            .map_err(|error| format!("rice: {error}"))?;

        let config =
            WmConfig::from_rice(&rice).map_err(|error| format!("window manager: {error}"))?;
        // "hub", or an applet that is not installed, boots into the Hub.
        let pinned = AppletId::new(&config.boot_into)
            .ok()
            .filter(|id| launchpad.manifest(id).is_some());
        if let Some(id) = pinned {
            launchpad
                .pin_boot(&id)
                .map_err(|error| format!("launchpad: {error}"))?;
        }
        let profile = HardwareProfile::resolve(&config.hardware_profile, host.capabilities());
        let mut workspace = Workspace::new(
            launchpad,
            config,
            profile,
            host.capabilities().display_refresh_hz,
            host.form_factor(),
        );

        let mut arena = Arena::new();
        let mut history = UndoStack::new();
        workspace
            .boot(&mut AppletContext {
                arena: &mut arena,
                history: &mut history,
                rice: &rice,
                form_factor: host.form_factor(),
                profile,
            })
            .map_err(|error| format!("window manager: {error}"))?;

        Ok(Self {
            host,
            rice,
            rice_history: UndoStack::new(),
            rice_source,
            arena,
            history,
            profile,
            workspace,
        })
    }

    /// The workspace plus an applet context over the rest of the engine.
    pub fn shell(&mut self) -> (&mut Workspace, AppletContext<'_>) {
        let ctx = AppletContext {
            arena: &mut self.arena,
            history: &mut self.history,
            rice: &self.rice,
            form_factor: self.host.form_factor(),
            profile: self.profile,
        };
        (&mut self.workspace, ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_rice_matches_the_binary_declarations() {
        let shipped = RiceSchema::parse(DEFAULT_RICE).unwrap();
        let mut launchpad = Launchpad::new();
        launchpad
            .register(|| Box::new(QuickNotes::default()))
            .unwrap();
        let mut declarations = launchpad.rice_declarations();
        declarations.push(byte_map_declaration());
        for declaration in declarations {
            assert_eq!(
                shipped.parameter(&declaration.path),
                Some(&declaration.default),
                "{} differs from its declaration",
                declaration.path
            );
        }
    }
}
