//! Drawing colors: HSV for the color wheel, the default color set and the
//! recent-color history.

/// A color as hue (0 to 360), saturation and value (0 to 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hsv {
    /// Hue in degrees.
    pub h: f32,
    /// Saturation.
    pub s: f32,
    /// Value.
    pub v: f32,
}

impl Hsv {
    /// The HSV of an sRGB color (0 to 1). Gray keeps hue 0.
    #[must_use]
    pub fn from_rgb([r, g, b]: [f32; 3]) -> Self {
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;
        let h = if delta <= 0.0 {
            0.0
        } else if max == r {
            60.0 * ((g - b) / delta).rem_euclid(6.0)
        } else if max == g {
            60.0 * ((b - r) / delta + 2.0)
        } else {
            60.0 * ((r - g) / delta + 4.0)
        };
        Self {
            h,
            s: if max <= 0.0 { 0.0 } else { delta / max },
            v: max,
        }
    }

    /// The sRGB color (0 to 1).
    #[must_use]
    pub fn to_rgb(self) -> [f32; 3] {
        let h = self.h.rem_euclid(360.0) / 60.0;
        let c = self.v * self.s;
        let x = c * (1.0 - (h % 2.0 - 1.0).abs());
        let (r, g, b) = match h as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        let m = self.v - c;
        [r + m, g + m, b + m]
    }
}

/// sRGB bytes as 0 to 1.
#[must_use]
pub fn unit(rgb: [u8; 3]) -> [f32; 3] {
    rgb.map(|value| f32::from(value) / 255.0)
}

/// 0 to 1 as sRGB bytes.
#[must_use]
pub fn bytes(rgb: [f32; 3]) -> [u8; 3] {
    rgb.map(|value| (value.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// `#rrggbb`.
#[must_use]
pub fn hex([r, g, b]: [u8; 3]) -> String {
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// Reads `#rrggbb` or `rrggbb`.
#[must_use]
pub fn parse_hex(text: &str) -> Option<[u8; 3]> {
    let digits = text.trim().trim_start_matches('#');
    if digits.len() != 6 || !digits.is_ascii() {
        return None;
    }
    let part = |at: usize| u8::from_str_radix(&digits[at..at + 2], 16).ok();
    Some([part(0)?, part(2)?, part(4)?])
}

/// The swatches of the default color set, eight to a row.
pub const DEFAULT_SET: [[u8; 3]; 56] = [
    [0, 0, 0],
    [255, 255, 255],
    [240, 60, 90],
    [250, 110, 70],
    [255, 140, 0],
    [255, 205, 0],
    [255, 250, 210],
    [255, 255, 235],
    [255, 255, 0],
    [255, 255, 140],
    [190, 240, 90],
    [40, 170, 40],
    [30, 110, 50],
    [60, 190, 150],
    [0, 230, 230],
    [180, 255, 255],
    [255, 255, 255],
    [190, 230, 255],
    [70, 170, 250],
    [50, 120, 240],
    [0, 70, 255],
    [130, 120, 255],
    [130, 40, 230],
    [250, 160, 255],
    [190, 70, 230],
    [255, 180, 255],
    [255, 210, 250],
    [250, 100, 200],
    [235, 60, 110],
    [235, 150, 140],
    [160, 70, 50],
    [110, 60, 40],
    [255, 160, 110],
    [255, 200, 170],
    [255, 225, 210],
    [255, 245, 230],
    [255, 235, 200],
    [255, 210, 160],
    [250, 185, 120],
    [240, 160, 90],
    [255, 140, 150],
    [255, 170, 160],
    [255, 200, 185],
    [250, 220, 200],
    [230, 190, 160],
    [210, 160, 130],
    [180, 130, 100],
    [150, 100, 80],
    [64, 64, 64],
    [96, 96, 96],
    [128, 128, 128],
    [160, 160, 160],
    [192, 192, 192],
    [210, 210, 210],
    [230, 230, 230],
    [245, 245, 245],
];

/// Recently used colors, newest first, without repeats.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct History {
    colors: Vec<[u8; 3]>,
}

impl History {
    /// How many colors it keeps.
    pub const LIMIT: usize = 48;

    /// Remembers `color` as the newest.
    pub fn push(&mut self, color: [u8; 3]) {
        self.colors.retain(|old| *old != color);
        self.colors.insert(0, color);
        self.colors.truncate(Self::LIMIT);
    }

    /// The colors, newest first.
    #[must_use]
    pub fn colors(&self) -> &[[u8; 3]] {
        &self.colors
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsv_round_trips_through_rgb() {
        for rgb in [
            [255, 0, 0],
            [0, 128, 255],
            [12, 200, 90],
            [255, 255, 255],
            [0, 0, 0],
        ] {
            let back = bytes(Hsv::from_rgb(unit(rgb)).to_rgb());
            assert_eq!(back, rgb);
        }
        let red = Hsv::from_rgb([1.0, 0.0, 0.0]);
        assert_eq!((red.h, red.s, red.v), (0.0, 1.0, 1.0));
        let blue = Hsv::from_rgb([0.0, 0.0, 1.0]);
        assert!((blue.h - 240.0).abs() < 1e-3);
    }

    #[test]
    fn hex_text_reads_and_writes() {
        assert_eq!(hex([255, 16, 0]), "#ff1000");
        assert_eq!(parse_hex("#FF1000"), Some([255, 16, 0]));
        assert_eq!(parse_hex("ff1000"), Some([255, 16, 0]));
        assert_eq!(parse_hex("#ff10"), None);
        assert_eq!(parse_hex("#gg0000"), None);
    }

    #[test]
    fn history_keeps_the_newest_once() {
        let mut history = History::default();
        history.push([1, 1, 1]);
        history.push([2, 2, 2]);
        history.push([1, 1, 1]);
        assert_eq!(history.colors(), &[[1, 1, 1], [2, 2, 2]]);
        for value in 0..100 {
            history.push([value, 0, 0]);
        }
        assert_eq!(history.colors().len(), History::LIMIT);
    }
}
