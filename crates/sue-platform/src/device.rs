//! Operating system, form factor and hardware capability descriptions.

use std::fmt;

/// The operating system family Sue is running on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OsFamily {
    /// Microsoft Windows.
    Windows,
    /// Linux and other freedesktop systems.
    Linux,
    /// Apple macOS.
    MacOs,
    /// Google Android.
    Android,
    /// Apple iOS / iPadOS.
    Ios,
    /// Anything else (BSDs, WASM hosts, …); treated like Linux for paths.
    Other,
}

impl OsFamily {
    /// The family this binary was compiled for.
    #[must_use]
    pub const fn current() -> Self {
        if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "android") {
            Self::Android
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(target_os = "ios") {
            Self::Ios
        } else {
            Self::Other
        }
    }

    /// `true` for Android and iOS.
    #[must_use]
    pub const fn is_mobile(self) -> bool {
        matches!(self, Self::Android | Self::Ios)
    }

    /// Display name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Windows => "Windows",
            Self::Linux => "Linux",
            Self::MacOs => "macOS",
            Self::Android => "Android",
            Self::Ios => "iOS",
            Self::Other => "Other",
        }
    }
}

impl fmt::Display for OsFamily {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The physical shape of the device, which selects the layout topology.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FormFactor {
    /// Mouse, keyboard and a large screen.
    Desktop,
    /// Large touch screen, often with a stylus.
    Tablet,
    /// Phone: small touch screen, thumb reach.
    Mobile,
}

impl FormFactor {
    /// Smallest screen side, in density-independent pixels, from which a mobile
    /// device counts as a tablet (Android's `sw600dp` convention).
    pub const TABLET_MIN_SMALLEST_SIDE_DP: f32 = 600.0;

    /// Classifies a device from its OS and the smallest side of its screen.
    #[must_use]
    pub fn classify(os: OsFamily, smallest_side_dp: f32) -> Self {
        if !os.is_mobile() {
            Self::Desktop
        } else if smallest_side_dp >= Self::TABLET_MIN_SMALLEST_SIDE_DP {
            Self::Tablet
        } else {
            Self::Mobile
        }
    }

    /// The layout people chose in Settings (`display.interface`):
    /// `desktop`, `tablet` or `phone`, or this device's own for `auto`
    /// and anything else.
    #[must_use]
    pub fn chosen(self, interface: &str) -> Self {
        match interface {
            "desktop" => Self::Desktop,
            "tablet" => Self::Tablet,
            "phone" => Self::Mobile,
            _ => self,
        }
    }

    /// `true` for the touch layouts (tablets and phones).
    #[must_use]
    pub const fn is_touch(self) -> bool {
        matches!(self, Self::Tablet | Self::Mobile)
    }

    /// Display name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Desktop => "Desktop",
            Self::Tablet => "Tablet",
            Self::Mobile => "Mobile",
        }
    }
}

impl fmt::Display for FormFactor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// The graphics API the renderer can target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GraphicsApi {
    /// No GPU surface (headless or software only).
    None,
    /// Desktop OpenGL.
    OpenGl,
    /// OpenGL ES.
    Gles,
    /// Vulkan.
    Vulkan,
    /// Direct3D 12.
    Dx12,
    /// Apple Metal.
    Metal,
}

/// Coarse GPU performance class.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GpuTier {
    /// Not probed.
    Unknown,
    /// CPU rasterizer.
    Software,
    /// Old or very low-power GPU: fixed-function-era features only.
    Legacy,
    /// Typical integrated or entry discrete GPU.
    Mainstream,
    /// Recent discrete or high-end mobile GPU.
    HighEnd,
}

/// GPU vendor, used to gate vendor-specific compute backends.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GpuVendor {
    /// Not probed.
    Unknown,
    /// NVIDIA.
    Nvidia,
    /// AMD.
    Amd,
    /// Intel.
    Intel,
    /// Apple.
    Apple,
    /// Qualcomm.
    Qualcomm,
    /// Arm (Mali).
    Arm,
    /// Any other vendor.
    Other,
}

/// Where the device draws power from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PowerSource {
    /// Not probed.
    Unknown,
    /// Wall power.
    Mains,
    /// Battery, optionally with the OS low-power mode enabled.
    Battery {
        /// `true` when the OS battery saver is on.
        low_power_mode: bool,
    },
}

impl PowerSource {
    /// `true` when the OS battery saver is on.
    #[must_use]
    pub const fn is_low_power(self) -> bool {
        matches!(
            self,
            Self::Battery {
                low_power_mode: true
            }
        )
    }
}

/// What the hardware can do. Hosts fill in what they can probe; the rest stays
/// at the conservative defaults of [`HardwareCapabilities::probe_portable`].
#[derive(Clone, Debug, PartialEq)]
pub struct HardwareCapabilities {
    /// Logical CPU cores.
    pub logical_cores: usize,
    /// Display refresh rate in Hz.
    pub display_refresh_hz: u32,
    /// Graphics API available to the renderer.
    pub graphics_api: GraphicsApi,
    /// GPU performance class.
    pub gpu_tier: GpuTier,
    /// GPU vendor.
    pub gpu_vendor: GpuVendor,
    /// Physical memory in bytes, if known.
    pub memory_bytes: Option<u64>,
    /// Power source.
    pub power: PowerSource,
    /// A touch screen is present.
    pub touch: bool,
    /// A pressure-sensitive stylus is present.
    pub stylus: bool,
}

impl HardwareCapabilities {
    /// Fallback refresh rate when the display cannot be queried.
    pub const DEFAULT_REFRESH_HZ: u32 = 60;

    /// Capabilities discoverable through the portable standard library alone:
    /// the core count. Everything else is reported as unknown.
    #[must_use]
    pub fn probe_portable() -> Self {
        Self {
            logical_cores: std::thread::available_parallelism().map_or(1, usize::from),
            display_refresh_hz: Self::DEFAULT_REFRESH_HZ,
            graphics_api: GraphicsApi::None,
            gpu_tier: GpuTier::Unknown,
            gpu_vendor: GpuVendor::Unknown,
            memory_bytes: None,
            power: PowerSource::Unknown,
            touch: false,
            stylus: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn people_can_choose_another_layout_than_their_devices() {
        assert_eq!(FormFactor::Mobile.chosen("desktop"), FormFactor::Desktop);
        assert_eq!(FormFactor::Desktop.chosen("phone"), FormFactor::Mobile);
        assert_eq!(FormFactor::Tablet.chosen("auto"), FormFactor::Tablet);
        assert_eq!(FormFactor::Mobile.chosen("nonsense"), FormFactor::Mobile);
        assert!(FormFactor::Tablet.is_touch() && !FormFactor::Desktop.is_touch());
    }

    #[test]
    fn classifies_form_factors() {
        assert_eq!(
            FormFactor::classify(OsFamily::Windows, 300.0),
            FormFactor::Desktop
        );
        assert_eq!(
            FormFactor::classify(OsFamily::Android, 411.0),
            FormFactor::Mobile
        );
        assert_eq!(
            FormFactor::classify(OsFamily::Ios, 820.0),
            FormFactor::Tablet
        );
        assert_eq!(
            FormFactor::classify(OsFamily::Android, 600.0),
            FormFactor::Tablet
        );
    }

    #[test]
    fn current_os_matches_target() {
        let os = OsFamily::current();
        assert_eq!(os == OsFamily::Windows, cfg!(target_os = "windows"));
        assert!(!os.name().is_empty());
    }

    #[test]
    fn portable_probe_is_conservative() {
        let caps = HardwareCapabilities::probe_portable();
        assert!(caps.logical_cores >= 1);
        assert_eq!(caps.gpu_tier, GpuTier::Unknown);
        assert!(!caps.power.is_low_power());
        assert!(
            PowerSource::Battery {
                low_power_mode: true
            }
            .is_low_power()
        );
    }
}
