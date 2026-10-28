//! Color management. Photo, video grading, compositing, design and print
//! all need to agree on what a color is: a [`Color`] carries its color
//! space, and conversions go through CIE XYZ with each space's primaries,
//! white point (Bradford adaptation between D65 and the ACES white) and
//! transfer function. Matrices are derived from the published primaries, not
//! copied by hand.

/// The primaries and white point of an RGB space.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Primaries {
    red: [f64; 2],
    green: [f64; 2],
    blue: [f64; 2],
    white: [f64; 2],
}

const D65: [f64; 2] = [0.3127, 0.3290];
const ACES_WHITE: [f64; 2] = [0.32168, 0.33767];

const REC709: Primaries = Primaries {
    red: [0.64, 0.33],
    green: [0.30, 0.60],
    blue: [0.15, 0.06],
    white: D65,
};
const P3_D65: Primaries = Primaries {
    red: [0.680, 0.320],
    green: [0.265, 0.690],
    blue: [0.150, 0.060],
    white: D65,
};
const REC2020: Primaries = Primaries {
    red: [0.708, 0.292],
    green: [0.170, 0.797],
    blue: [0.131, 0.046],
    white: D65,
};
const AP1: Primaries = Primaries {
    red: [0.713, 0.293],
    green: [0.165, 0.830],
    blue: [0.128, 0.044],
    white: ACES_WHITE,
};
const AP0: Primaries = Primaries {
    red: [0.7347, 0.2653],
    green: [0.0, 1.0],
    blue: [0.0001, -0.0770],
    white: ACES_WHITE,
};

/// How code values relate to linear light.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Transfer {
    /// Linear light.
    Linear,
    /// The sRGB curve.
    Srgb,
    /// Pure power 2.4 (BT.1886 display).
    Gamma24,
    /// SMPTE ST 2084 perceptual quantizer (1.0 is 10 000 nits).
    Pq,
    /// ARIB STD-B67 hybrid log-gamma.
    Hlg,
}

impl Transfer {
    /// Code value to linear light.
    #[must_use]
    pub fn decode(self, value: f64) -> f64 {
        match self {
            Self::Linear => value,
            Self::Srgb => {
                if value <= 0.040_45 {
                    value / 12.92
                } else {
                    ((value + 0.055) / 1.055).powf(2.4)
                }
            }
            Self::Gamma24 => value.max(0.0).powf(2.4),
            Self::Pq => {
                let (m1, m2) = (0.159_301_757_812_5, 78.843_75);
                let (c1, c2, c3) = (0.835_937_5, 18.851_562_5, 18.6875);
                let p = value.max(0.0).powf(1.0 / m2);
                ((p - c1).max(0.0) / (c2 - c3 * p)).powf(1.0 / m1)
            }
            Self::Hlg => {
                let (a, b, c) = (0.178_832_77, 0.284_668_92, 0.559_910_73);
                if value <= 0.5 {
                    value * value / 3.0
                } else {
                    (((value - c) / a).exp() + b) / 12.0
                }
            }
        }
    }

    /// Linear light to code value.
    #[must_use]
    pub fn encode(self, value: f64) -> f64 {
        match self {
            Self::Linear => value,
            Self::Srgb => {
                if value <= 0.003_130_8 {
                    value * 12.92
                } else {
                    1.055 * value.powf(1.0 / 2.4) - 0.055
                }
            }
            Self::Gamma24 => value.max(0.0).powf(1.0 / 2.4),
            Self::Pq => {
                let (m1, m2) = (0.159_301_757_812_5, 78.843_75);
                let (c1, c2, c3) = (0.835_937_5, 18.851_562_5, 18.6875);
                let y = value.max(0.0).powf(m1);
                ((c1 + c2 * y) / (1.0 + c3 * y)).powf(m2)
            }
            Self::Hlg => {
                let (a, b, c) = (0.178_832_77, 0.284_668_92, 0.559_910_73);
                if value <= 1.0 / 12.0 {
                    (3.0 * value.max(0.0)).sqrt()
                } else {
                    a * (12.0 * value - b).ln() + c
                }
            }
        }
    }
}

/// A named color space.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ColorSpace {
    /// sRGB, as on most screens and the web.
    Srgb,
    /// sRGB primaries, linear light (compositing and rendering).
    LinearSrgb,
    /// Rec. 709 primaries with the BT.1886 2.4 display gamma (HD video).
    Rec709,
    /// Display P3 (Apple displays, wide-gamut photos).
    DisplayP3,
    /// Rec. 2020 primaries, linear light.
    Rec2020Linear,
    /// Rec. 2100 PQ (HDR video).
    Rec2100Pq,
    /// Rec. 2100 HLG (HDR broadcast).
    Rec2100Hlg,
    /// ACEScg (AP1, linear): the grading and VFX working space.
    AcesCg,
    /// ACES2065-1 (AP0, linear): the archival interchange space.
    Aces2065,
    /// CIE 1931 XYZ, D65.
    Xyz,
}

impl ColorSpace {
    /// Every space, for pickers and tests.
    pub const ALL: [Self; 10] = [
        Self::Srgb,
        Self::LinearSrgb,
        Self::Rec709,
        Self::DisplayP3,
        Self::Rec2020Linear,
        Self::Rec2100Pq,
        Self::Rec2100Hlg,
        Self::AcesCg,
        Self::Aces2065,
        Self::Xyz,
    ];

    /// A stable name for files.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Srgb => "srgb",
            Self::LinearSrgb => "linear-srgb",
            Self::Rec709 => "rec709",
            Self::DisplayP3 => "display-p3",
            Self::Rec2020Linear => "rec2020-linear",
            Self::Rec2100Pq => "rec2100-pq",
            Self::Rec2100Hlg => "rec2100-hlg",
            Self::AcesCg => "acescg",
            Self::Aces2065 => "aces2065-1",
            Self::Xyz => "xyz-d65",
        }
    }

    /// The space with a [`Self::name`].
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|space| space.name() == name)
    }

    fn parts(self) -> Option<(Primaries, Transfer)> {
        Some(match self {
            Self::Srgb => (REC709, Transfer::Srgb),
            Self::LinearSrgb => (REC709, Transfer::Linear),
            Self::Rec709 => (REC709, Transfer::Gamma24),
            Self::DisplayP3 => (P3_D65, Transfer::Srgb),
            Self::Rec2020Linear => (REC2020, Transfer::Linear),
            Self::Rec2100Pq => (REC2020, Transfer::Pq),
            Self::Rec2100Hlg => (REC2020, Transfer::Hlg),
            Self::AcesCg => (AP1, Transfer::Linear),
            Self::Aces2065 => (AP0, Transfer::Linear),
            Self::Xyz => return None,
        })
    }
}

type Matrix = [[f64; 3]; 3];

fn multiply(a: &Matrix, b: &Matrix) -> Matrix {
    let mut out = [[0.0; 3]; 3];
    for row in 0..3 {
        for column in 0..3 {
            out[row][column] = (0..3).map(|k| a[row][k] * b[k][column]).sum();
        }
    }
    out
}

fn apply(m: &Matrix, v: [f64; 3]) -> [f64; 3] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
    ]
}

fn invert(m: &Matrix) -> Matrix {
    let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
    let inv = 1.0 / det;
    [
        [
            (m[1][1] * m[2][2] - m[1][2] * m[2][1]) * inv,
            (m[0][2] * m[2][1] - m[0][1] * m[2][2]) * inv,
            (m[0][1] * m[1][2] - m[0][2] * m[1][1]) * inv,
        ],
        [
            (m[1][2] * m[2][0] - m[1][0] * m[2][2]) * inv,
            (m[0][0] * m[2][2] - m[0][2] * m[2][0]) * inv,
            (m[0][2] * m[1][0] - m[0][0] * m[1][2]) * inv,
        ],
        [
            (m[1][0] * m[2][1] - m[1][1] * m[2][0]) * inv,
            (m[0][1] * m[2][0] - m[0][0] * m[2][1]) * inv,
            (m[0][0] * m[1][1] - m[0][1] * m[1][0]) * inv,
        ],
    ]
}

fn xyz_of(xy: [f64; 2]) -> [f64; 3] {
    [xy[0] / xy[1], 1.0, (1.0 - xy[0] - xy[1]) / xy[1]]
}

/// Linear RGB to XYZ (in the space's own white).
fn rgb_to_xyz(p: &Primaries) -> Matrix {
    let (r, g, b) = (xyz_of(p.red), xyz_of(p.green), xyz_of(p.blue));
    let m = [[r[0], g[0], b[0]], [r[1], g[1], b[1]], [r[2], g[2], b[2]]];
    let s = apply(&invert(&m), xyz_of(p.white));
    [
        [m[0][0] * s[0], m[0][1] * s[1], m[0][2] * s[2]],
        [m[1][0] * s[0], m[1][1] * s[1], m[1][2] * s[2]],
        [m[2][0] * s[0], m[2][1] * s[1], m[2][2] * s[2]],
    ]
}

/// Bradford chromatic adaptation from one white to another.
fn adapt(from: [f64; 2], to: [f64; 2]) -> Matrix {
    const BRADFORD: Matrix = [
        [0.8951, 0.2664, -0.1614],
        [-0.7502, 1.7135, 0.0367],
        [0.0389, -0.0685, 1.0296],
    ];
    if from == to {
        return [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    }
    let (source, target) = (apply(&BRADFORD, xyz_of(from)), apply(&BRADFORD, xyz_of(to)));
    let scale = [
        [target[0] / source[0], 0.0, 0.0],
        [0.0, target[1] / source[1], 0.0],
        [0.0, 0.0, target[2] / source[2]],
    ];
    multiply(&invert(&BRADFORD), &multiply(&scale, &BRADFORD))
}

/// To XYZ with a D65 white.
fn to_xyz_d65(space: ColorSpace, rgb: [f64; 3]) -> [f64; 3] {
    match space.parts() {
        None => rgb,
        Some((primaries, transfer)) => {
            let linear = rgb.map(|value| transfer.decode(value));
            let xyz = apply(&rgb_to_xyz(&primaries), linear);
            apply(&adapt(primaries.white, D65), xyz)
        }
    }
}

fn from_xyz_d65(space: ColorSpace, xyz: [f64; 3]) -> [f64; 3] {
    match space.parts() {
        None => xyz,
        Some((primaries, transfer)) => {
            let adapted = apply(&adapt(D65, primaries.white), xyz);
            let linear = apply(&invert(&rgb_to_xyz(&primaries)), adapted);
            linear.map(|value| transfer.encode(value))
        }
    }
}

/// A color in a known space, with straight (not premultiplied) alpha.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    /// The three components, in `space`.
    pub rgb: [f32; 3],
    /// Opacity, 0 to 1.
    pub alpha: f32,
    /// The space `rgb` is in.
    pub space: ColorSpace,
}

impl Color {
    /// An opaque sRGB color from 8-bit components.
    #[must_use]
    pub fn srgb8(r: u8, g: u8, b: u8) -> Self {
        Self {
            rgb: [r, g, b].map(|value| f32::from(value) / 255.0),
            alpha: 1.0,
            space: ColorSpace::Srgb,
        }
    }

    /// The same color in `space` (out-of-gamut values are kept, not clipped).
    #[must_use]
    pub fn to_space(&self, space: ColorSpace) -> Self {
        if space == self.space {
            return *self;
        }
        let xyz = to_xyz_d65(self.space, self.rgb.map(f64::from));
        Self {
            rgb: from_xyz_d65(space, xyz).map(|value| value as f32),
            alpha: self.alpha,
            space,
        }
    }

    /// `true` if every component is in 0 to 1.
    #[must_use]
    pub fn in_gamut(&self) -> bool {
        self.rgb
            .iter()
            .all(|value| (-1e-4..=1.0 + 1e-4).contains(value))
    }

    /// Relative luminance (CIE Y), 0 for black and 1 for diffuse white.
    #[must_use]
    pub fn luminance(&self) -> f64 {
        to_xyz_d65(self.space, self.rgb.map(f64::from))[1]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 2e-4
    }

    #[test]
    fn white_is_white_everywhere_and_round_trips() {
        let white = Color::srgb8(255, 255, 255);
        for space in ColorSpace::ALL {
            if matches!(
                space,
                ColorSpace::Xyz | ColorSpace::Rec2100Pq | ColorSpace::Rec2100Hlg
            ) {
                continue;
            }
            let there = white.to_space(space);
            assert!(
                there.rgb.iter().all(|value| close(*value, 1.0)),
                "{space:?} {there:?}"
            );
            let back = there.to_space(ColorSpace::Srgb);
            assert!(back.rgb.iter().all(|value| close(*value, 1.0)));
        }
        let xyz = white.to_space(ColorSpace::Xyz);
        assert!(close(xyz.rgb[0], 0.9505) && close(xyz.rgb[1], 1.0) && close(xyz.rgb[2], 1.0890));
    }

    #[test]
    fn known_values_and_gamut() {
        let gray = Color::srgb8(128, 128, 128).to_space(ColorSpace::LinearSrgb);
        assert!(close(gray.rgb[0], 0.21586));
        let p3_red = Color {
            rgb: [1.0, 0.0, 0.0],
            alpha: 1.0,
            space: ColorSpace::DisplayP3,
        };
        assert!(
            !p3_red.to_space(ColorSpace::Srgb).in_gamut(),
            "P3 red is outside sRGB"
        );
        let srgb_red = Color::srgb8(255, 0, 0);
        assert!(srgb_red.to_space(ColorSpace::DisplayP3).in_gamut());
        let aces = srgb_red
            .to_space(ColorSpace::AcesCg)
            .to_space(ColorSpace::Srgb);
        assert!(close(aces.rgb[0], 1.0) && close(aces.rgb[1], 0.0) && close(aces.rgb[2], 0.0));
        assert!(close(Color::srgb8(0, 255, 0).luminance() as f32, 0.7152));
    }

    #[test]
    fn hdr_transfers_invert() {
        for transfer in [
            Transfer::Srgb,
            Transfer::Gamma24,
            Transfer::Pq,
            Transfer::Hlg,
        ] {
            for value in [0.0, 0.01, 0.18, 0.5, 1.0] {
                let back = transfer.decode(transfer.encode(value));
                assert!((back - value).abs() < 1e-6, "{transfer:?} {value} {back}");
            }
        }
        assert_eq!(ColorSpace::from_name("acescg"), Some(ColorSpace::AcesCg));
    }
}
