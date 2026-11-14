//! Color profile preview (View, Color Profile, Preview): the picture as it
//! would come out printed, simulated with ink on paper. Each color is
//! separated into cyan, magenta, yellow and black with a total ink limit,
//! then laid as those inks over the paper's color, so colors a press
//! cannot print (bright blues, greens, oranges) dull as they would.

/// The profiles: name, paper color, total ink limit (0 to 4) and whether
/// it prints in gray only.
pub const PROFILES: [(&str, [f32; 3], f32, bool); 4] = [
    (
        "Coated paper (Japan Color 2001 like)",
        [0.97, 0.97, 0.96],
        3.5,
        false,
    ),
    (
        "Uncoated paper (US Web Uncoated like)",
        [0.94, 0.93, 0.90],
        2.6,
        false,
    ),
    ("Newsprint", [0.88, 0.87, 0.83], 2.4, false),
    (
        "Grayscale (monochrome print)",
        [0.97, 0.97, 0.97],
        1.0,
        true,
    ),
];

/// The inks' colors as they print alone on white: cyan, magenta, yellow,
/// black.
const INKS: [[f32; 3]; 4] = [
    [0.0, 0.63, 0.91],
    [0.90, 0.0, 0.49],
    [1.0, 0.94, 0.0],
    [0.14, 0.13, 0.13],
];

/// One straight sRGB color (0 to 1) as printed with profile `profile`.
#[must_use]
pub fn proof_color(rgb: [f32; 3], profile: usize) -> [f32; 3] {
    let (_, paper, limit, gray) = PROFILES[profile.min(PROFILES.len() - 1)];
    if gray {
        let luma = 0.299 * rgb[0] + 0.587 * rgb[1] + 0.114 * rgb[2];
        // Black ink over the paper, with a little dot gain.
        let ink = (1.0 - luma).clamp(0.0, 1.0).powf(0.9);
        return paper.map(|channel| {
            channel * (1.0 - ink * (1.0 - INKS[3][0] / channel.max(1e-3)).clamp(0.0, 1.0))
        });
    }
    let k = 1.0 - rgb[0].max(rgb[1]).max(rgb[2]);
    let left = (1.0 - k).max(1e-4);
    let mut inks = [
        (1.0 - rgb[0] - k) / left,
        (1.0 - rgb[1] - k) / left,
        (1.0 - rgb[2] - k) / left,
        k,
    ]
    .map(|ink| ink.clamp(0.0, 1.0));
    // More black and less color in the shadows, as presses separate.
    let under = inks[0].min(inks[1]).min(inks[2]) * 0.5;
    for ink in &mut inks[..3] {
        *ink -= under;
    }
    inks[3] = (inks[3] + under * (1.0 - inks[3])).min(1.0);
    let total: f32 = inks.iter().sum();
    if total > limit {
        let scale = limit / total;
        for ink in &mut inks[..3] {
            *ink *= scale;
        }
    }
    // Dot gain: midtones print a little darker.
    let gained = inks.map(|ink| 1.0 - (1.0 - ink).powf(1.15));
    let mut out = paper;
    for (ink, color) in gained.iter().zip(INKS) {
        for channel in 0..3 {
            let filter = 1.0 - ink * (1.0 - color[channel]);
            out[channel] *= filter;
        }
    }
    out.map(|channel| channel.clamp(0.0, 1.0))
}

/// Straight RGBA8 `pixels` as printed with `profile`, alpha kept. Each
/// color is worked out once.
#[must_use]
pub fn proof(pixels: &[u8], profile: usize) -> Vec<u8> {
    let mut known: std::collections::HashMap<[u8; 3], [u8; 3]> = std::collections::HashMap::new();
    let mut out = Vec::with_capacity(pixels.len());
    for pixel in pixels.chunks(4) {
        let key = [pixel[0], pixel[1], pixel[2]];
        let printed = *known.entry(key).or_insert_with(|| {
            proof_color(key.map(|channel| f32::from(channel) / 255.0), profile)
                .map(|channel| (channel * 255.0).round() as u8)
        });
        out.extend_from_slice(&printed);
        out.push(pixel.get(3).copied().unwrap_or(255));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chroma(rgb: [f32; 3]) -> f32 {
        rgb[0].max(rgb[1]).max(rgb[2]) - rgb[0].min(rgb[1]).min(rgb[2])
    }

    #[test]
    fn print_dulls_screen_colors_and_white_is_the_paper() {
        let white = proof_color([1.0, 1.0, 1.0], 0);
        assert!(
            (white[0] - PROFILES[0].1[0]).abs() < 1e-3,
            "white is the paper"
        );
        let blue = proof_color([0.0, 0.0, 1.0], 0);
        assert!(chroma(blue) < 0.9, "a screen blue cannot print as bright");
        let black = proof_color([0.0, 0.0, 0.0], 0);
        assert!(black.iter().all(|channel| *channel < 0.2));
        // Newsprint is duller than coated paper.
        let red = [1.0, 0.0, 0.0];
        assert!(chroma(proof_color(red, 2)) < chroma(proof_color(red, 0)));
        // Grayscale prints gray.
        let gray = proof_color([1.0, 0.0, 0.0], 3);
        assert!((gray[0] - gray[1]).abs() < 1e-3 && (gray[1] - gray[2]).abs() < 1e-3);
        let pixels = proof(&[255, 255, 255, 128, 0, 0, 255, 255], 0);
        assert_eq!(pixels.len(), 8);
        assert_eq!(pixels[3], 128, "alpha is kept");
    }
}
