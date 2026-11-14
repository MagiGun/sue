//! Layer blend modes and the compositing every raster feature shares.
//!
//! Painting, photo editing, compositing, video and slides all stack layers
//! the same way: a layer's color is mixed with what lies below by a
//! [`BlendMode`], then laid over it by its alpha (the W3C compositing model,
//! straight alpha, components 0 to 1). Keeping the math here means a picture
//! looks the same in every studio that shows it.
//!
//! Modes are stored by name (`style.blend`), so files stay readable when a
//! later Sue adds modes: an unknown name falls back to [`BlendMode::Normal`].
//! The list follows Clip Studio Paint's order; the two glow modes lay the
//! layer's color on by its alpha before mixing, so soft edges glow
//! brighter than with Add and Color Dodge. Folders take one more mode,
//! [`BlendMode::Through`], in which their layers mix with what is under
//! the folder as if the folder were not there.

/// How a layer's color mixes with the color below it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BlendMode {
    /// The layer's own color.
    #[default]
    Normal,
    /// The darker of the two, per channel.
    Darken,
    /// Multiplies: never lighter.
    Multiply,
    /// Darkens the backdrop to reflect the layer, with more contrast.
    ColorBurn,
    /// Adds and subtracts one: darkens linearly.
    LinearBurn,
    /// Backdrop minus layer.
    Subtract,
    /// The darker of the two colors as a whole.
    DarkerColor,
    /// The lighter of the two, per channel.
    Lighten,
    /// Inverse multiply: never darker.
    Screen,
    /// Brightens the backdrop to reflect the layer.
    ColorDodge,
    /// Color dodge with the layer's color taken by its alpha first:
    /// brighter where it is soft.
    GlowDodge,
    /// Adds the two, clipped at white.
    Add,
    /// Adds the layer's color taken by its alpha, then clips at white:
    /// brighter than Add where the layer is soft.
    AddGlow,
    /// The lighter of the two colors as a whole.
    LighterColor,
    /// Multiply or screen, by the backdrop.
    Overlay,
    /// A gentle overlay.
    SoftLight,
    /// Multiply or screen, by the layer.
    HardLight,
    /// Color burn or dodge, by the layer.
    VividLight,
    /// Linear burn or dodge, by the layer.
    LinearLight,
    /// Darken or lighten, by the layer.
    PinLight,
    /// Every channel to black or white.
    HardMix,
    /// The absolute difference.
    Difference,
    /// A softer difference.
    Exclusion,
    /// Backdrop divided by layer.
    Divide,
    /// The layer's hue, the backdrop's saturation and luminosity.
    Hue,
    /// The layer's saturation.
    Saturation,
    /// The layer's hue and saturation.
    Color,
    /// The layer's luminosity (shown as Brightness).
    Luminosity,
    /// A folder's layers mix with what is under the folder, as if it were
    /// not there (folders only; on a layer it is Normal).
    Through,
}

impl BlendMode {
    /// Every mode a layer takes, in menu order (Clip Studio Paint's).
    pub const ALL: [Self; 28] = [
        Self::Normal,
        Self::Darken,
        Self::Multiply,
        Self::ColorBurn,
        Self::LinearBurn,
        Self::Subtract,
        Self::Lighten,
        Self::Screen,
        Self::ColorDodge,
        Self::GlowDodge,
        Self::Add,
        Self::AddGlow,
        Self::Overlay,
        Self::SoftLight,
        Self::HardLight,
        Self::Difference,
        Self::VividLight,
        Self::LinearLight,
        Self::PinLight,
        Self::HardMix,
        Self::Exclusion,
        Self::DarkerColor,
        Self::LighterColor,
        Self::Divide,
        Self::Hue,
        Self::Saturation,
        Self::Color,
        Self::Luminosity,
    ];

    /// Every mode a folder takes, in menu order: [`Self::Through`] first,
    /// then [`Self::ALL`].
    pub const FOLDER: [Self; 29] = [
        Self::Through,
        Self::Normal,
        Self::Darken,
        Self::Multiply,
        Self::ColorBurn,
        Self::LinearBurn,
        Self::Subtract,
        Self::Lighten,
        Self::Screen,
        Self::ColorDodge,
        Self::GlowDodge,
        Self::Add,
        Self::AddGlow,
        Self::Overlay,
        Self::SoftLight,
        Self::HardLight,
        Self::Difference,
        Self::VividLight,
        Self::LinearLight,
        Self::PinLight,
        Self::HardMix,
        Self::Exclusion,
        Self::DarkerColor,
        Self::LighterColor,
        Self::Divide,
        Self::Hue,
        Self::Saturation,
        Self::Color,
        Self::Luminosity,
    ];

    /// The stored name, e.g. `color-burn`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Darken => "darken",
            Self::Multiply => "multiply",
            Self::ColorBurn => "color-burn",
            Self::LinearBurn => "linear-burn",
            Self::Subtract => "subtract",
            Self::DarkerColor => "darker-color",
            Self::Lighten => "lighten",
            Self::Screen => "screen",
            Self::ColorDodge => "color-dodge",
            Self::GlowDodge => "glow-dodge",
            Self::Add => "add",
            Self::AddGlow => "add-glow",
            Self::LighterColor => "lighter-color",
            Self::Overlay => "overlay",
            Self::SoftLight => "soft-light",
            Self::HardLight => "hard-light",
            Self::VividLight => "vivid-light",
            Self::LinearLight => "linear-light",
            Self::PinLight => "pin-light",
            Self::HardMix => "hard-mix",
            Self::Difference => "difference",
            Self::Exclusion => "exclusion",
            Self::Divide => "divide",
            Self::Hue => "hue",
            Self::Saturation => "saturation",
            Self::Color => "color",
            Self::Luminosity => "luminosity",
            Self::Through => "through",
        }
    }

    /// The mode stored as `name`, if this build knows it.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::FOLDER.into_iter().find(|mode| mode.name() == name)
    }

    /// The mode stored as `name`, or [`Self::Normal`] for names from a newer
    /// Sue.
    #[must_use]
    pub fn from_name_or_normal(name: &str) -> Self {
        Self::from_name(name).unwrap_or_default()
    }

    /// What people read, in English, e.g. `Color Burn` (studios translate
    /// it through the `model` language file).
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Darken => "Darken",
            Self::Multiply => "Multiply",
            Self::ColorBurn => "Color Burn",
            Self::LinearBurn => "Linear Burn",
            Self::Subtract => "Subtract",
            Self::DarkerColor => "Darker Color",
            Self::Lighten => "Lighten",
            Self::Screen => "Screen",
            Self::ColorDodge => "Color Dodge",
            Self::GlowDodge => "Glow Dodge",
            Self::Add => "Add",
            Self::AddGlow => "Add (Glow)",
            Self::LighterColor => "Lighter Color",
            Self::Overlay => "Overlay",
            Self::SoftLight => "Soft Light",
            Self::HardLight => "Hard Light",
            Self::VividLight => "Vivid Light",
            Self::LinearLight => "Linear Light",
            Self::PinLight => "Pin Light",
            Self::HardMix => "Hard Mix",
            Self::Difference => "Difference",
            Self::Exclusion => "Exclusion",
            Self::Divide => "Divide",
            Self::Hue => "Hue",
            Self::Saturation => "Saturation",
            Self::Color => "Color",
            Self::Luminosity => "Brightness",
            Self::Through => "Through",
        }
    }

    /// The menu group: modes with the same number belong together
    /// (through, normal, darkening, lightening, contrast, comparison,
    /// component), in the order of [`Self::FOLDER`].
    #[must_use]
    pub const fn group(self) -> u8 {
        match self {
            Self::Through => 0,
            Self::Normal => 1,
            Self::Darken | Self::Multiply | Self::ColorBurn | Self::LinearBurn | Self::Subtract => {
                2
            }
            Self::Lighten
            | Self::Screen
            | Self::ColorDodge
            | Self::GlowDodge
            | Self::Add
            | Self::AddGlow => 3,
            Self::Overlay
            | Self::SoftLight
            | Self::HardLight
            | Self::Difference
            | Self::VividLight
            | Self::LinearLight
            | Self::PinLight
            | Self::HardMix => 4,
            Self::Exclusion | Self::DarkerColor | Self::LighterColor | Self::Divide => 5,
            Self::Hue | Self::Saturation | Self::Color | Self::Luminosity => 6,
        }
    }

    /// `true` for the modes that take the layer's color by its alpha
    /// before mixing ([`Self::GlowDodge`], [`Self::AddGlow`]).
    #[must_use]
    pub const fn glows(self) -> bool {
        matches!(self, Self::GlowDodge | Self::AddGlow)
    }

    /// The mixed color of `source` over `backdrop`, both opaque RGB.
    #[must_use]
    pub fn mix(self, backdrop: [f32; 3], source: [f32; 3]) -> [f32; 3] {
        let per_channel = |f: fn(f32, f32) -> f32| {
            [
                f(backdrop[0], source[0]),
                f(backdrop[1], source[1]),
                f(backdrop[2], source[2]),
            ]
        };
        match self {
            Self::Normal | Self::Through => source,
            Self::Darken => per_channel(f32::min),
            Self::Multiply => per_channel(|b, s| b * s),
            Self::ColorBurn => per_channel(color_burn),
            Self::LinearBurn => per_channel(|b, s| (b + s - 1.0).max(0.0)),
            Self::Subtract => per_channel(|b, s| (b - s).max(0.0)),
            Self::DarkerColor => {
                if lum(source) < lum(backdrop) {
                    source
                } else {
                    backdrop
                }
            }
            Self::Lighten => per_channel(f32::max),
            Self::Screen => per_channel(screen),
            Self::ColorDodge | Self::GlowDodge => per_channel(color_dodge),
            Self::Add | Self::AddGlow => per_channel(|b, s| (b + s).min(1.0)),
            Self::LighterColor => {
                if lum(source) > lum(backdrop) {
                    source
                } else {
                    backdrop
                }
            }
            Self::Overlay => per_channel(|b, s| hard_light(s, b)),
            Self::SoftLight => per_channel(soft_light),
            Self::HardLight => per_channel(hard_light),
            Self::VividLight => per_channel(vivid_light),
            Self::LinearLight => per_channel(|b, s| (b + 2.0 * s - 1.0).clamp(0.0, 1.0)),
            Self::PinLight => per_channel(|b, s| {
                if s <= 0.5 {
                    b.min(2.0 * s)
                } else {
                    b.max(2.0 * s - 1.0)
                }
            }),
            Self::HardMix => per_channel(|b, s| if b + s >= 1.0 { 1.0 } else { 0.0 }),
            Self::Difference => per_channel(|b, s| (b - s).abs()),
            Self::Exclusion => per_channel(|b, s| b + s - 2.0 * b * s),
            Self::Divide => per_channel(|b, s| {
                if s <= 0.0 {
                    if b <= 0.0 { 0.0 } else { 1.0 }
                } else {
                    (b / s).min(1.0)
                }
            }),
            Self::Hue => set_lum(set_sat(source, sat(backdrop)), lum(backdrop)),
            Self::Saturation => set_lum(set_sat(backdrop, sat(source)), lum(backdrop)),
            Self::Color => set_lum(source, lum(backdrop)),
            Self::Luminosity => set_lum(backdrop, lum(source)),
        }
    }

    /// `source` (RGBA, straight alpha) laid over `backdrop` with this mode:
    /// the mixed color where both are opaque, each one alone where the other
    /// is clear. The glow modes mix the backdrop with the layer's color
    /// taken by its alpha, so where both are there the result is
    /// `mix(backdrop, color * alpha)` and not a share of
    /// `mix(backdrop, color)`.
    #[must_use]
    pub fn composite(self, backdrop: [f32; 4], source: [f32; 4]) -> [f32; 4] {
        let (alpha_b, alpha_s) = (backdrop[3], source[3]);
        if alpha_s <= 0.0 {
            return backdrop;
        }
        let alpha = alpha_s + alpha_b * (1.0 - alpha_s);
        if alpha <= 0.0 {
            return [0.0; 4];
        }
        let back = [backdrop[0], backdrop[1], backdrop[2]];
        let front = [source[0], source[1], source[2]];
        let mut out = [0.0; 4];
        if self.glows() {
            // Where only the layer is, its own color; where the backdrop
            // is, the backdrop mixed with the layer's color by its alpha.
            let lit = self.mix(back, front.map(|value| value * alpha_s));
            for channel in 0..3 {
                let premultiplied =
                    alpha_s * (1.0 - alpha_b) * front[channel] + alpha_b * lit[channel];
                out[channel] = (premultiplied / alpha).clamp(0.0, 1.0);
            }
            out[3] = alpha.min(1.0);
            return out;
        }
        let mixed = if matches!(self, Self::Normal | Self::Through) || alpha_b <= 0.0 {
            front
        } else {
            self.mix(back, front)
        };
        for channel in 0..3 {
            let shown = (1.0 - alpha_b) * front[channel] + alpha_b * mixed[channel];
            let premultiplied = alpha_s * shown + alpha_b * back[channel] * (1.0 - alpha_s);
            out[channel] = (premultiplied / alpha).clamp(0.0, 1.0);
        }
        out[3] = alpha.min(1.0);
        out
    }
}

fn screen(b: f32, s: f32) -> f32 {
    b + s - b * s
}

fn color_burn(b: f32, s: f32) -> f32 {
    if b >= 1.0 {
        1.0
    } else if s <= 0.0 {
        0.0
    } else {
        1.0 - ((1.0 - b) / s).min(1.0)
    }
}

fn color_dodge(b: f32, s: f32) -> f32 {
    if b <= 0.0 {
        0.0
    } else if s >= 1.0 {
        1.0
    } else {
        (b / (1.0 - s)).min(1.0)
    }
}

fn hard_light(b: f32, s: f32) -> f32 {
    if s <= 0.5 {
        b * 2.0 * s
    } else {
        screen(b, 2.0 * s - 1.0)
    }
}

fn soft_light(b: f32, s: f32) -> f32 {
    if s <= 0.5 {
        b - (1.0 - 2.0 * s) * b * (1.0 - b)
    } else {
        let d = if b <= 0.25 {
            ((16.0 * b - 12.0) * b + 4.0) * b
        } else {
            b.sqrt()
        };
        b + (2.0 * s - 1.0) * (d - b)
    }
}

fn vivid_light(b: f32, s: f32) -> f32 {
    if s <= 0.5 {
        color_burn(b, 2.0 * s)
    } else {
        color_dodge(b, 2.0 * (s - 0.5))
    }
}

fn lum(c: [f32; 3]) -> f32 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}

fn clip_color(c: [f32; 3]) -> [f32; 3] {
    let l = lum(c);
    let n = c[0].min(c[1]).min(c[2]);
    let x = c[0].max(c[1]).max(c[2]);
    let mut out = c;
    if n < 0.0 {
        out = out.map(|v| l + (v - l) * l / (l - n));
    }
    if x > 1.0 {
        out = out.map(|v| l + (v - l) * (1.0 - l) / (x - l));
    }
    out.map(|v| v.clamp(0.0, 1.0))
}

fn set_lum(c: [f32; 3], l: f32) -> [f32; 3] {
    let d = l - lum(c);
    clip_color(c.map(|v| v + d))
}

fn sat(c: [f32; 3]) -> f32 {
    c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2])
}

fn set_sat(c: [f32; 3], s: f32) -> [f32; 3] {
    let max = c[0].max(c[1]).max(c[2]);
    let min = c[0].min(c[1]).min(c[2]);
    if max - min <= 0.0 {
        return [0.0; 3];
    }
    c.map(|v| (v - min) * s / (max - min))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f32; 4], b: [f32; 4]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4)
    }

    #[test]
    fn names_round_trip_and_unknown_names_fall_back() {
        for mode in BlendMode::FOLDER {
            assert_eq!(BlendMode::from_name(mode.name()), Some(mode));
            assert!(!mode.label().is_empty());
        }
        // The two glow modes are known by name, and so is Through.
        assert_eq!(
            BlendMode::from_name("glow-dodge"),
            Some(BlendMode::GlowDodge)
        );
        assert_eq!(BlendMode::from_name("add-glow"), Some(BlendMode::AddGlow));
        assert_eq!(BlendMode::from_name("through"), Some(BlendMode::Through));
        assert_eq!(BlendMode::GlowDodge.label(), "Glow Dodge");
        assert_eq!(BlendMode::AddGlow.label(), "Add (Glow)");
        // A name from a newer Sue is Normal.
        assert_eq!(BlendMode::from_name("sparkle"), None);
        assert_eq!(BlendMode::from_name_or_normal("sparkle"), BlendMode::Normal);
        // Stored names stay as they were; only the label reads Brightness.
        assert_eq!(BlendMode::Luminosity.name(), "luminosity");
        assert_eq!(BlendMode::Luminosity.label(), "Brightness");
        let mut groups: Vec<u8> = BlendMode::FOLDER.iter().map(|mode| mode.group()).collect();
        let sorted = groups.clone();
        groups.sort_unstable();
        assert_eq!(groups, sorted, "FOLDER lists the modes group by group");
        assert_eq!(&BlendMode::FOLDER[1..], &BlendMode::ALL[..]);
        assert!(!BlendMode::ALL.contains(&BlendMode::Through));
        let mut names: Vec<&str> = BlendMode::FOLDER.iter().map(|mode| mode.name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), BlendMode::FOLDER.len(), "every name once");
    }

    #[test]
    fn the_list_follows_clip_studio_paints_order() {
        let labels: Vec<&str> = BlendMode::ALL.iter().map(|mode| mode.label()).collect();
        assert_eq!(
            labels,
            [
                "Normal",
                "Darken",
                "Multiply",
                "Color Burn",
                "Linear Burn",
                "Subtract",
                "Lighten",
                "Screen",
                "Color Dodge",
                "Glow Dodge",
                "Add",
                "Add (Glow)",
                "Overlay",
                "Soft Light",
                "Hard Light",
                "Difference",
                "Vivid Light",
                "Linear Light",
                "Pin Light",
                "Hard Mix",
                "Exclusion",
                "Darker Color",
                "Lighter Color",
                "Divide",
                "Hue",
                "Saturation",
                "Color",
                "Brightness",
            ]
        );
    }

    #[test]
    fn the_glow_modes_take_the_layers_color_by_its_alpha() {
        let back = [0.8, 0.4, 0.0, 1.0];
        let soft = [0.6, 0.6, 0.6, 0.5];
        // Add: halfway to the clipped sum. Add (Glow): the sum with the
        // color taken by its alpha, clipped: brighter where it clips.
        let add = BlendMode::Add.composite(back, soft);
        let glow = BlendMode::AddGlow.composite(back, soft);
        assert!(close(add, [0.9, 0.7, 0.3, 1.0]), "{add:?}");
        assert!(close(glow, [1.0, 0.7, 0.3, 1.0]), "{glow:?}");
        // Color Dodge: halfway to the dodge. Glow Dodge: a dodge by the
        // color taken by its alpha.
        let dodge = BlendMode::ColorDodge.composite(back, soft);
        let glow_dodge = BlendMode::GlowDodge.composite(back, soft);
        assert!(close(dodge, [0.9, 0.7, 0.0, 1.0]), "{dodge:?}");
        assert!(
            close(glow_dodge, [1.0, 0.4 / 0.7, 0.0, 1.0]),
            "{glow_dodge:?}"
        );
        // Opaque, the glow modes are their plain twins.
        let solid = [0.3, 0.2, 0.1, 1.0];
        assert!(close(
            BlendMode::AddGlow.composite(back, solid),
            BlendMode::Add.composite(back, solid)
        ));
        assert!(close(
            BlendMode::GlowDodge.composite(back, solid),
            BlendMode::ColorDodge.composite(back, solid)
        ));
        // Over nothing they keep the layer's own color and alpha.
        for mode in [BlendMode::AddGlow, BlendMode::GlowDodge] {
            assert!(close(mode.composite([0.0; 4], soft), soft), "{mode:?}");
        }
        // Over a half-clear backdrop: alpha as in any mode, the color
        // (0.5 * 0.5 * 0.6 + 0.5 * min(1, 0.2 + 0.3)) / 0.75.
        let out = BlendMode::AddGlow.composite([0.2, 0.2, 0.2, 0.5], soft);
        assert!(
            close(out, [0.533_333, 0.533_333, 0.533_333, 0.75]),
            "{out:?}"
        );
        // Through on a layer is Normal.
        assert_eq!(
            BlendMode::Through.composite(back, soft),
            BlendMode::Normal.composite(back, soft)
        );
    }

    #[test]
    fn modes_follow_their_definitions() {
        let gray = [0.5, 0.25, 0.75];
        assert_eq!(BlendMode::Multiply.mix(gray, [1.0; 3]), gray);
        assert_eq!(BlendMode::Screen.mix(gray, [0.0; 3]), gray);
        assert_eq!(BlendMode::Difference.mix(gray, gray), [0.0; 3]);
        assert_eq!(BlendMode::Add.mix([0.75; 3], [0.5; 3]), [1.0; 3]);
        assert_eq!(
            BlendMode::Darken.mix([0.2, 0.8, 0.5], [0.6, 0.1, 0.5]),
            [0.2, 0.1, 0.5]
        );
        assert_eq!(BlendMode::HardMix.mix([0.6; 3], [0.5; 3]), [1.0; 3]);
        // Luminosity keeps the backdrop's hue and takes the layer's lightness.
        let mixed = BlendMode::Luminosity.mix([1.0, 0.0, 0.0], [0.5; 3]);
        assert!((lum(mixed) - 0.5).abs() < 1e-4);
        assert!(mixed[0] > mixed[1] && mixed[1] == mixed[2]);
        // Overlay on mid gray leaves the layer's own value.
        assert!((BlendMode::Overlay.mix([0.5; 3], [0.3; 3])[0] - 0.3).abs() < 1e-6);
    }

    #[test]
    fn compositing_lays_layers_over_each_other() {
        let paper = [1.0, 1.0, 1.0, 1.0];
        let ink = [0.0, 0.0, 0.0, 0.5];
        // Half-transparent black over white is mid gray, opaque.
        assert!(close(
            BlendMode::Normal.composite(paper, ink),
            [0.5, 0.5, 0.5, 1.0]
        ));
        // Over nothing, a layer keeps its own color and alpha in any mode.
        let clear = [0.0; 4];
        let red = [1.0, 0.0, 0.0, 0.25];
        for mode in BlendMode::ALL {
            assert!(close(mode.composite(clear, red), red), "{mode:?}");
        }
        // A clear layer changes nothing.
        assert_eq!(
            BlendMode::Multiply.composite(paper, [0.2, 0.2, 0.2, 0.0]),
            paper
        );
        // Opaque multiply over opaque is the plain product.
        assert!(close(
            BlendMode::Multiply.composite([0.5, 0.5, 0.5, 1.0], [0.5, 1.0, 0.0, 1.0]),
            [0.25, 0.5, 0.0, 1.0]
        ));
    }
}
