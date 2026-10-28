//! The platform host trait and the headless reference host.

use std::collections::{BTreeMap, VecDeque};
use std::fmt;

use crate::{FormFactor, HardwareCapabilities, InputEvent, OsFamily, PlatformPaths};

/// Errors raised by a platform host.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PlatformError {
    /// A required environment variable is unset or empty.
    MissingEnvironment(&'static str),
    /// A mobile host was created without the app sandbox root.
    MissingSandbox,
    /// A surface size or scale factor is not positive and finite.
    InvalidSurface(&'static str),
    /// No surface has this id.
    UnknownSurface(SurfaceId),
    /// The host cannot do this.
    Unsupported(&'static str),
    /// No usable GPU adapter, device or surface.
    Gpu(String),
    /// The windowing system failed.
    Window(String),
    /// The application failed to start.
    App(String),
    /// No usable audio device or stream.
    Audio(String),
}

impl fmt::Display for PlatformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingEnvironment(name) => write!(f, "environment variable {name} is not set"),
            Self::MissingSandbox => f.write_str("mobile hosts need the app sandbox directory"),
            Self::InvalidSurface(reason) => write!(f, "invalid surface: {reason}"),
            Self::UnknownSurface(id) => write!(f, "unknown surface {}", id.0),
            Self::Unsupported(what) => write!(f, "unsupported: {what}"),
            Self::Gpu(reason) => write!(f, "GPU: {reason}"),
            Self::Window(reason) => write!(f, "window: {reason}"),
            Self::App(reason) => write!(f, "application: {reason}"),
            Self::Audio(reason) => write!(f, "audio: {reason}"),
        }
    }
}

impl std::error::Error for PlatformError {}

/// Identifies a window or swap-chain surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SurfaceId(pub u32);

/// Areas covered by notches, rounded corners or system bars, in logical pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SafeAreaInsets {
    /// Top inset.
    pub top: f32,
    /// Right inset.
    pub right: f32,
    /// Bottom inset.
    pub bottom: f32,
    /// Left inset.
    pub left: f32,
}

impl SafeAreaInsets {
    /// No insets.
    pub const ZERO: Self = Self {
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
        left: 0.0,
    };
}

/// A request for a new surface.
#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceDescriptor {
    /// Window title (ignored on mobile).
    pub title: String,
    /// Width in logical pixels.
    pub width: f32,
    /// Height in logical pixels.
    pub height: f32,
    /// Physical pixels per logical pixel.
    pub scale_factor: f32,
}

/// A live surface.
#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceInfo {
    /// Its id.
    pub id: SurfaceId,
    /// Window title.
    pub title: String,
    /// Width in logical pixels.
    pub width: f32,
    /// Height in logical pixels.
    pub height: f32,
    /// Physical pixels per logical pixel.
    pub scale_factor: f32,
    /// Safe-area insets.
    pub safe_area: SafeAreaInsets,
}

/// Everything Sue needs from an operating system. Native hosts for Windows,
/// Linux, macOS, Android and iOS implement this trait, and nothing above
/// `sue-platform` may bypass it.
pub trait PlatformHost {
    /// Operating system family.
    fn os(&self) -> OsFamily;

    /// Device form factor.
    fn form_factor(&self) -> FormFactor;

    /// Probed hardware capabilities.
    fn capabilities(&self) -> &HardwareCapabilities;

    /// Configuration, data and cache directories.
    fn paths(&self) -> &PlatformPaths;

    /// Creates a window or swap-chain surface.
    ///
    /// # Errors
    /// [`PlatformError::InvalidSurface`] for a bad size or scale factor.
    fn create_surface(
        &mut self,
        descriptor: &SurfaceDescriptor,
    ) -> Result<SurfaceInfo, PlatformError>;

    /// Resizes a surface.
    ///
    /// # Errors
    /// [`PlatformError::UnknownSurface`] or [`PlatformError::InvalidSurface`].
    fn resize_surface(
        &mut self,
        id: SurfaceId,
        width: f32,
        height: f32,
    ) -> Result<SurfaceInfo, PlatformError>;

    /// Destroys a surface.
    ///
    /// # Errors
    /// [`PlatformError::UnknownSurface`].
    fn destroy_surface(&mut self, id: SurfaceId) -> Result<(), PlatformError>;

    /// A live surface.
    fn surface(&self, id: SurfaceId) -> Option<&SurfaceInfo>;

    /// Drains pending input events, oldest first.
    fn poll_events(&mut self) -> Vec<InputEvent>;
}

/// A complete [`PlatformHost`] with virtual surfaces and an injectable input
/// queue: for CI, the self-check, and remote or scripted control.
#[derive(Clone, Debug)]
pub struct HeadlessHost {
    os: OsFamily,
    form_factor: FormFactor,
    capabilities: HardwareCapabilities,
    paths: PlatformPaths,
    safe_area: SafeAreaInsets,
    surfaces: BTreeMap<SurfaceId, SurfaceInfo>,
    next_surface: u32,
    events: VecDeque<InputEvent>,
}

impl HeadlessHost {
    /// A host with explicit properties.
    #[must_use]
    pub fn new(
        os: OsFamily,
        form_factor: FormFactor,
        capabilities: HardwareCapabilities,
        paths: PlatformPaths,
    ) -> Self {
        Self {
            os,
            form_factor,
            capabilities,
            paths,
            safe_area: SafeAreaInsets::ZERO,
            surfaces: BTreeMap::new(),
            next_surface: 1,
            events: VecDeque::new(),
        }
    }

    /// A host for the machine this process runs on, using only portable probes.
    ///
    /// # Errors
    /// If the platform directories cannot be resolved from the environment.
    pub fn detect() -> Result<Self, PlatformError> {
        let os = OsFamily::current();
        // Headless hosts have no screen to measure; mobile defaults to a phone.
        let form_factor = if os.is_mobile() {
            FormFactor::Mobile
        } else {
            FormFactor::Desktop
        };
        let paths = PlatformPaths::detect()?;
        Ok(Self::new(
            os,
            form_factor,
            HardwareCapabilities::probe_portable(),
            paths,
        ))
    }

    /// Sets the safe-area insets applied to new surfaces.
    #[must_use]
    pub fn with_safe_area(mut self, safe_area: SafeAreaInsets) -> Self {
        self.safe_area = safe_area;
        self
    }

    /// Queues a synthetic input event.
    pub fn inject(&mut self, event: InputEvent) {
        self.events.push_back(event);
    }

    fn validate(width: f32, height: f32, scale_factor: f32) -> Result<(), PlatformError> {
        let positive = |value: f32| value.is_finite() && value > 0.0;
        if !positive(width) || !positive(height) {
            return Err(PlatformError::InvalidSurface(
                "size must be positive and finite",
            ));
        }
        if !positive(scale_factor) {
            return Err(PlatformError::InvalidSurface(
                "scale factor must be positive and finite",
            ));
        }
        Ok(())
    }
}

impl PlatformHost for HeadlessHost {
    fn os(&self) -> OsFamily {
        self.os
    }

    fn form_factor(&self) -> FormFactor {
        self.form_factor
    }

    fn capabilities(&self) -> &HardwareCapabilities {
        &self.capabilities
    }

    fn paths(&self) -> &PlatformPaths {
        &self.paths
    }

    fn create_surface(
        &mut self,
        descriptor: &SurfaceDescriptor,
    ) -> Result<SurfaceInfo, PlatformError> {
        Self::validate(descriptor.width, descriptor.height, descriptor.scale_factor)?;
        let id = SurfaceId(self.next_surface);
        self.next_surface += 1;
        let info = SurfaceInfo {
            id,
            title: descriptor.title.clone(),
            width: descriptor.width,
            height: descriptor.height,
            scale_factor: descriptor.scale_factor,
            safe_area: self.safe_area,
        };
        self.surfaces.insert(id, info.clone());
        Ok(info)
    }

    fn resize_surface(
        &mut self,
        id: SurfaceId,
        width: f32,
        height: f32,
    ) -> Result<SurfaceInfo, PlatformError> {
        let surface = self
            .surfaces
            .get_mut(&id)
            .ok_or(PlatformError::UnknownSurface(id))?;
        Self::validate(width, height, surface.scale_factor)?;
        surface.width = width;
        surface.height = height;
        Ok(surface.clone())
    }

    fn destroy_surface(&mut self, id: SurfaceId) -> Result<(), PlatformError> {
        self.surfaces
            .remove(&id)
            .map(drop)
            .ok_or(PlatformError::UnknownSurface(id))
    }

    fn surface(&self, id: SurfaceId) -> Option<&SurfaceInfo> {
        self.surfaces.get(&id)
    }

    fn poll_events(&mut self) -> Vec<InputEvent> {
        self.events.drain(..).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::KeyEvent;
    use std::path::PathBuf;

    fn host() -> HeadlessHost {
        let paths = PlatformPaths {
            config_dir: PathBuf::from("cfg"),
            data_dir: PathBuf::from("data"),
            cache_dir: PathBuf::from("cache"),
            home_dir: PathBuf::from("home"),
        };
        HeadlessHost::new(
            OsFamily::Android,
            FormFactor::Mobile,
            HardwareCapabilities::probe_portable(),
            paths,
        )
        .with_safe_area(SafeAreaInsets {
            top: 47.0,
            bottom: 34.0,
            ..SafeAreaInsets::ZERO
        })
    }

    fn descriptor(width: f32, height: f32) -> SurfaceDescriptor {
        SurfaceDescriptor {
            title: "Sue".into(),
            width,
            height,
            scale_factor: 3.0,
        }
    }

    #[test]
    fn surface_lifecycle() {
        let mut host = host();
        let surface = host.create_surface(&descriptor(390.0, 844.0)).unwrap();
        assert!((surface.safe_area.top - 47.0).abs() < f32::EPSILON);
        let resized = host.resize_surface(surface.id, 844.0, 390.0).unwrap();
        assert!((resized.width - 844.0).abs() < f32::EPSILON);
        host.destroy_surface(surface.id).unwrap();
        assert!(host.surface(surface.id).is_none());
        assert_eq!(
            host.destroy_surface(surface.id),
            Err(PlatformError::UnknownSurface(surface.id))
        );
    }

    #[test]
    fn rejects_degenerate_surfaces() {
        let mut host = host();
        assert!(host.create_surface(&descriptor(0.0, 100.0)).is_err());
        assert!(host.create_surface(&descriptor(f32::NAN, 100.0)).is_err());
    }

    #[test]
    fn injected_events_drain_in_order() {
        let mut host = host();
        host.inject(InputEvent::Keyboard(KeyEvent::typed('a')));
        host.inject(InputEvent::Keyboard(KeyEvent::typed('b')));
        let events = host.poll_events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0], InputEvent::Keyboard(KeyEvent::typed('a')));
        assert!(host.poll_events().is_empty());
    }

    #[test]
    fn detect_describes_this_machine() {
        // Resolving paths needs the usual environment, which CI provides.
        if let Ok(host) = HeadlessHost::detect() {
            assert_eq!(host.os(), OsFamily::current());
            assert!(host.capabilities().logical_cores >= 1);
        }
    }
}
