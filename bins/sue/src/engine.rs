//! Boot: wires a platform host, the live rice, the undoable world and the
//! window manager together.

use std::fmt;
use std::io;
use std::path::PathBuf;

use sue_core::UndoStack;
use sue_platform::{FormFactor, HardwareCapabilities, OsFamily, PlatformHost, PlatformPaths};
use sue_rice::{DEFAULT_RICE, HardwareProfile, RiceDeclaration, RiceParameter, RiceSchema};
use sue_wm::{AppletContext, ShellRequest, WmConfig, Workspace, World};

use crate::applets;

/// The live rice file in the platform rice directory. It starts as a byte-for-
/// byte copy of the built-in `default.suerice` and is written back
/// format-preservingly after every edit.
pub const RICE_FILE: &str = "default.suerice";

/// Where the live rice came from.
#[derive(Debug)]
pub enum RiceSource {
    /// The rice embedded in the binary.
    Default,
    /// A file on disk.
    File(PathBuf),
}

impl fmt::Display for RiceSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Default => f.write_str("built-in default.suerice"),
            Self::File(path) => write!(f, "{}", path.display()),
        }
    }
}

/// Riceables of the ingest pipeline (LAW 3).
pub fn io_declarations() -> Vec<RiceDeclaration> {
    let int = |value, min, max| RiceParameter::IntRange {
        value,
        min: Some(min),
        max: Some(max),
    };
    vec![
        RiceDeclaration::new(
            "io.byte_map_width",
            int(64, 16, 1024),
            "Columns in the hex / byte-map view of unknown files",
        ),
        RiceDeclaration::new(
            "io.max_ingest_mib",
            int(512, 1, 4096),
            "Largest file accepted by drag and drop (MiB)",
        ),
    ]
}

/// Every subsystem, booted and connected.
pub struct Engine {
    /// Operating system.
    pub os: OsFamily,
    /// Device form factor.
    pub form_factor: FormFactor,
    /// Hardware capabilities.
    pub capabilities: HardwareCapabilities,
    /// Platform directories.
    pub paths: PlatformPaths,
    /// The arena and the live rice.
    pub world: World,
    /// The global undo tree.
    pub history: UndoStack<World>,
    /// Applet requests waiting for the shell.
    pub requests: Vec<ShellRequest>,
    /// Where the rice came from.
    pub rice_source: RiceSource,
    /// The shell.
    pub workspace: Workspace,
}

impl Engine {
    /// Boots against `host`: loads the live rice file if it exists (the
    /// built-in default otherwise), registers every riceable, installs the
    /// built-in applets and boots the workspace.
    ///
    /// # Errors
    /// A message naming the subsystem that failed.
    pub fn boot(host: &dyn PlatformHost) -> Result<Self, String> {
        let paths = host.paths().clone();
        let rice_path = paths.rice_dir().join(RICE_FILE);
        let (text, rice_source) = match std::fs::read_to_string(&rice_path) {
            Ok(text) => (text, RiceSource::File(rice_path)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                (DEFAULT_RICE.to_owned(), RiceSource::Default)
            }
            Err(error) => {
                return Err(format!(
                    "rice: cannot read {}: {error}",
                    rice_path.display()
                ));
            }
        };
        let mut rice = RiceSchema::parse(&text).map_err(|error| format!("rice: {error}"))?;
        let launchpad = applets::launchpad().map_err(|error| format!("launchpad: {error}"))?;
        sue_wm::register_rice(&mut rice).map_err(|error| format!("rice: {error}"))?;
        rice.register_all(&io_declarations())
            .map_err(|error| format!("rice: {error}"))?;
        rice.register_all(&launchpad.rice_declarations())
            .map_err(|error| format!("rice: {error}"))?;

        let config =
            WmConfig::from_rice(&rice).map_err(|error| format!("window manager: {error}"))?;
        let profile = HardwareProfile::resolve(&config.hardware_profile, host.capabilities());
        // "hub", or an applet that is not installed, boots into the Hub
        // (`Workspace::new` resolves `launchpad.boot_into`).
        let workspace = Workspace::new(
            launchpad,
            config,
            profile,
            host.capabilities().display_refresh_hz,
            host.form_factor(),
        );
        let mut engine = Self {
            os: host.os(),
            form_factor: host.form_factor(),
            capabilities: host.capabilities().clone(),
            paths,
            world: World {
                rice,
                ..World::default()
            },
            history: UndoStack::new(),
            requests: Vec::new(),
            rice_source,
            workspace,
        };
        let (workspace, mut ctx) = engine.shell();
        workspace
            .boot(&mut ctx)
            .map_err(|error| format!("window manager: {error}"))?;
        Ok(engine)
    }

    /// The live rice file.
    pub fn rice_path(&self) -> PathBuf {
        self.paths.rice_dir().join(RICE_FILE)
    }

    /// The workspace plus an applet context over the rest of the engine.
    pub fn shell(&mut self) -> (&mut Workspace, AppletContext<'_>) {
        let ctx = AppletContext {
            world: &mut self.world,
            history: &mut self.history,
            form_factor: self.form_factor,
            profile: self.workspace.profile(),
            requests: &mut self.requests,
            focused: false,
        };
        (&mut self.workspace, ctx)
    }

    /// Re-reads the window-manager config and hardware profile from the rice,
    /// after an edit, undo or redo.
    ///
    /// # Errors
    /// If a window-manager parameter is missing.
    pub fn refresh_config(&mut self) -> Result<(), String> {
        let config = WmConfig::from_rice(&self.world.rice).map_err(|error| error.to_string())?;
        let profile = HardwareProfile::resolve(&config.hardware_profile, &self.capabilities);
        self.workspace.apply_config(config);
        if profile != self.workspace.profile() {
            self.workspace.set_profile(profile);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_rice_matches_the_binary_declarations() {
        let shipped = RiceSchema::parse(DEFAULT_RICE).unwrap();
        let mut declarations = applets::launchpad().unwrap().rice_declarations();
        declarations.extend(io_declarations());
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
