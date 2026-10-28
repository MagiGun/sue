//! Hardware profiles: the frame-rate, shader and animation budget.

use std::fmt;
use std::str::FromStr;

use sue_platform::{GpuTier, HardwareCapabilities};

use crate::RiceError;

/// How much of the machine Sue may use. Chosen automatically from the
/// hardware, and overridable with `profile.hardware` in rice.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HardwareProfile {
    /// Match the display (240 Hz+), full shader stack.
    HighRefresh,
    /// 60 FPS cap, moderate shader stack.
    Balanced,
    /// Old or low-power machines: 30 FPS cap, no shader passes, instant snaps.
    LegacyLowSpec,
    /// Battery saver: 30 FPS cap, minimal shaders, no blur.
    MobileBatterySaver,
}

/// The budget a profile grants the renderer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameBudget {
    /// Frame-rate cap; `None` follows the display refresh rate.
    pub max_fps: Option<u32>,
    /// Maximum post-processing shader passes per frame.
    pub shader_passes: u8,
    /// `true` if spring physics may animate; `false` snaps instantly.
    pub spring_physics: bool,
    /// `true` if blur effects are allowed.
    pub blur: bool,
}

impl HardwareProfile {
    /// Every profile.
    pub const ALL: [Self; 4] = [
        Self::HighRefresh,
        Self::Balanced,
        Self::LegacyLowSpec,
        Self::MobileBatterySaver,
    ];

    /// The rice value that asks for automatic selection.
    pub const AUTO: &'static str = "auto";

    /// Picks a profile for the hardware: low-power mode → battery saver; a
    /// software or legacy GPU, or two cores or fewer → legacy; a 120 Hz+
    /// display with a high-end GPU → high refresh; otherwise balanced.
    #[must_use]
    pub fn recommend(capabilities: &HardwareCapabilities) -> Self {
        if capabilities.power.is_low_power() {
            Self::MobileBatterySaver
        } else if matches!(capabilities.gpu_tier, GpuTier::Software | GpuTier::Legacy)
            || capabilities.logical_cores <= 2
        {
            Self::LegacyLowSpec
        } else if capabilities.display_refresh_hz >= 120
            && capabilities.gpu_tier == GpuTier::HighEnd
        {
            Self::HighRefresh
        } else {
            Self::Balanced
        }
    }

    /// Resolves the rice setting: `"auto"` (or an unknown name) recommends a
    /// profile, any profile name selects it.
    #[must_use]
    pub fn resolve(setting: &str, capabilities: &HardwareCapabilities) -> Self {
        setting
            .parse()
            .unwrap_or_else(|_| Self::recommend(capabilities))
    }

    /// The budget this profile grants.
    #[must_use]
    pub const fn budget(self) -> FrameBudget {
        match self {
            Self::HighRefresh => FrameBudget {
                max_fps: None,
                shader_passes: 8,
                spring_physics: true,
                blur: true,
            },
            Self::Balanced => FrameBudget {
                max_fps: Some(60),
                shader_passes: 4,
                spring_physics: true,
                blur: true,
            },
            Self::LegacyLowSpec => FrameBudget {
                max_fps: Some(30),
                shader_passes: 0,
                spring_physics: false,
                blur: false,
            },
            Self::MobileBatterySaver => FrameBudget {
                max_fps: Some(30),
                shader_passes: 1,
                spring_physics: true,
                blur: false,
            },
        }
    }

    /// Frame rate to render at on a display refreshing at `display_hz`
    /// (`0` means unknown and is treated as 60 Hz).
    #[must_use]
    pub fn target_fps(self, display_hz: u32) -> u32 {
        let display = if display_hz == 0 {
            HardwareCapabilities::DEFAULT_REFRESH_HZ
        } else {
            display_hz
        };
        self.budget()
            .max_fps
            .map_or(display, |cap| cap.min(display))
    }

    /// The name used in rice files.
    #[must_use]
    pub const fn rice_name(self) -> &'static str {
        match self {
            Self::HighRefresh => "high-refresh",
            Self::Balanced => "balanced",
            Self::LegacyLowSpec => "legacy-low-spec",
            Self::MobileBatterySaver => "mobile-battery-saver",
        }
    }
}

impl fmt::Display for HardwareProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.rice_name())
    }
}

impl FromStr for HardwareProfile {
    type Err = RiceError;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|profile| profile.rice_name() == name)
            .ok_or_else(|| RiceError::InvalidChoice {
                path: "profile.hardware".to_owned(),
                value: name.to_owned(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_platform::PowerSource;

    fn capabilities(
        cores: usize,
        hz: u32,
        tier: GpuTier,
        power: PowerSource,
    ) -> HardwareCapabilities {
        HardwareCapabilities {
            logical_cores: cores,
            display_refresh_hz: hz,
            gpu_tier: tier,
            power,
            ..HardwareCapabilities::probe_portable()
        }
    }

    #[test]
    fn recommends_by_hardware() {
        let rig = capabilities(16, 240, GpuTier::HighEnd, PowerSource::Mains);
        assert_eq!(
            HardwareProfile::recommend(&rig),
            HardwareProfile::HighRefresh
        );
        let laptop = capabilities(
            8,
            60,
            GpuTier::Mainstream,
            PowerSource::Battery {
                low_power_mode: false,
            },
        );
        assert_eq!(
            HardwareProfile::recommend(&laptop),
            HardwareProfile::Balanced
        );
        let netbook = capabilities(2, 60, GpuTier::Mainstream, PowerSource::Mains);
        assert_eq!(
            HardwareProfile::recommend(&netbook),
            HardwareProfile::LegacyLowSpec
        );
        let phone = capabilities(
            8,
            120,
            GpuTier::HighEnd,
            PowerSource::Battery {
                low_power_mode: true,
            },
        );
        assert_eq!(
            HardwareProfile::recommend(&phone),
            HardwareProfile::MobileBatterySaver
        );
        let headless = HardwareCapabilities {
            logical_cores: 4,
            ..HardwareCapabilities::probe_portable()
        };
        assert_eq!(
            HardwareProfile::recommend(&headless),
            HardwareProfile::Balanced
        );
    }

    #[test]
    fn rice_names_round_trip_and_auto_recommends() {
        for profile in HardwareProfile::ALL {
            assert_eq!(profile.rice_name().parse::<HardwareProfile>(), Ok(profile));
        }
        let rig = capabilities(16, 240, GpuTier::HighEnd, PowerSource::Mains);
        assert_eq!(
            HardwareProfile::resolve("auto", &rig),
            HardwareProfile::HighRefresh
        );
        assert_eq!(
            HardwareProfile::resolve("legacy-low-spec", &rig),
            HardwareProfile::LegacyLowSpec
        );
        assert!("turbo".parse::<HardwareProfile>().is_err());
    }

    #[test]
    fn budgets_cap_the_frame_rate() {
        assert_eq!(HardwareProfile::HighRefresh.target_fps(240), 240);
        assert_eq!(HardwareProfile::Balanced.target_fps(144), 60);
        assert_eq!(HardwareProfile::Balanced.target_fps(0), 60);
        assert_eq!(HardwareProfile::MobileBatterySaver.target_fps(120), 30);
        assert!(!HardwareProfile::LegacyLowSpec.budget().spring_physics);
    }
}
