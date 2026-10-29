//! Printing: pages laid on paper as File, Print Settings says, written as
//! a PDF the system's viewer prints (File, Print).

use sue_core::RasterTile2D;
use sue_io::pdf::{PdfDocument, PdfImage, PdfItem, PdfPage, write_pdf};

/// Paper sizes: name, width and height in millimeters (portrait).
pub const PAPERS: [(&str, f32, f32); 8] = [
    ("A3", 297.0, 420.0),
    ("A4", 210.0, 297.0),
    ("A5", 148.0, 210.0),
    ("B4 (JIS)", 257.0, 364.0),
    ("B5 (JIS)", 182.0, 257.0),
    ("Letter", 215.9, 279.4),
    ("Legal", 215.9, 355.6),
    ("Tabloid", 279.4, 431.8),
];

/// How pictures go on paper.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PrintSettings {
    /// Which of [`PAPERS`].
    pub paper: usize,
    /// 0 turns the paper to suit the picture, 1 portrait, 2 landscape.
    pub orientation: u8,
    /// 0 fits the page, 1 prints at the picture's resolution (print
    /// size), 2 at `percent` of print size.
    pub size: u8,
    /// The scale for `size` 2, in percent.
    pub percent: f32,
    /// The margin all round, in millimeters.
    pub margin: f32,
    /// Every page of a story, not only the one shown.
    pub all_pages: bool,
}

impl Default for PrintSettings {
    fn default() -> Self {
        Self {
            paper: 1,
            orientation: 0,
            size: 0,
            percent: 100.0,
            margin: 10.0,
            all_pages: true,
        }
    }
}

/// Points in a millimeter.
const POINTS_PER_MM: f32 = 72.0 / 25.4;

/// The paper (in points) and where on it (`[x, y, width, height]` from
/// the top left, in points) a picture of `pixels` at `dpi` prints.
#[must_use]
pub fn place(settings: &PrintSettings, pixels: [u32; 2], dpi: f32) -> ([f32; 2], [f32; 4]) {
    let (_, w, h) = PAPERS[settings.paper.min(PAPERS.len() - 1)];
    let picture_wide = pixels[0] > pixels[1];
    let landscape = match settings.orientation {
        1 => false,
        2 => true,
        _ => picture_wide,
    };
    let paper = if landscape {
        [h * POINTS_PER_MM, w * POINTS_PER_MM]
    } else {
        [w * POINTS_PER_MM, h * POINTS_PER_MM]
    };
    let margin = (settings.margin.max(0.0) * POINTS_PER_MM).min(paper[0].min(paper[1]) / 2.0 - 1.0);
    let room = [paper[0] - 2.0 * margin, paper[1] - 2.0 * margin];
    let natural = [
        pixels[0] as f32 * 72.0 / dpi.max(1.0),
        pixels[1] as f32 * 72.0 / dpi.max(1.0),
    ];
    let scale = match settings.size {
        1 => 1.0,
        2 => settings.percent.max(1.0) / 100.0,
        _ => (room[0] / natural[0].max(1e-3)).min(room[1] / natural[1].max(1e-3)),
    };
    let size = [natural[0] * scale, natural[1] * scale];
    let rect = [
        (paper[0] - size[0]) / 2.0,
        (paper[1] - size[1]) / 2.0,
        size[0],
        size[1],
    ];
    (paper, rect)
}

/// `pages` (straight RGBA8, at `dpi`) as a PDF ready to print, one page
/// each on paper as `settings` says.
///
/// # Errors
/// If a page has no pixels or the PDF will not write.
pub fn pdf(
    settings: &PrintSettings,
    pages: &[RasterTile2D],
    dpi: f32,
    title: &str,
) -> Result<Vec<u8>, String> {
    let mut doc = PdfDocument {
        title: title.to_owned(),
        ..PdfDocument::default()
    };
    for page in pages {
        let bytes = page.bytes().ok_or("pages are 8-bit RGBA")?;
        let rgb = bytes
            .chunks(4)
            .flat_map(|pixel| {
                let alpha = u32::from(pixel[3]);
                [0, 1, 2].map(|channel| {
                    ((u32::from(pixel[channel]) * alpha + 255 * (255 - alpha)) / 255) as u8
                })
            })
            .collect();
        let (paper, rect) = place(settings, [page.width(), page.height()], dpi);
        doc.images.push(PdfImage {
            width: page.width(),
            height: page.height(),
            rgb,
        });
        doc.pages.push(PdfPage {
            size: paper,
            items: vec![PdfItem::Image {
                image: doc.images.len() - 1,
                rect,
            }],
        });
    }
    if doc.pages.is_empty() {
        return Err("There is nothing to print".to_owned());
    }
    write_pdf(&doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pictures_fit_print_at_size_and_turn_the_paper() {
        let a4 = PrintSettings::default();
        // A tall picture fits A4 portrait inside the margins.
        let (paper, rect) = place(&a4, [1000, 2000], 300.0);
        assert!((paper[0] - 210.0 * POINTS_PER_MM).abs() < 0.01);
        let margin = 10.0 * POINTS_PER_MM;
        assert!(
            (rect[3] - (paper[1] - 2.0 * margin)).abs() < 0.01,
            "as tall as the room"
        );
        // A wide one turns the paper.
        let (paper, _) = place(&a4, [2000, 1000], 300.0);
        assert!(paper[0] > paper[1]);
        // Print size: 300 pixels at 300 dpi is one inch.
        let actual = PrintSettings { size: 1, ..a4 };
        let (_, rect) = place(&actual, [300, 600], 300.0);
        assert!((rect[2] - 72.0).abs() < 0.01 && (rect[3] - 144.0).abs() < 0.01);
        let half = PrintSettings {
            size: 2,
            percent: 50.0,
            ..a4
        };
        assert!((place(&half, [300, 600], 300.0).1[2] - 36.0).abs() < 0.01);
        // The PDF has a page per picture.
        let page =
            RasterTile2D::from_bytes(2, 2, sue_core::PixelFormat::Rgba8, vec![0; 16]).unwrap();
        let bytes = pdf(&a4, &[page.clone(), page], 72.0, "Print").unwrap();
        assert!(bytes.starts_with(b"%PDF"));
        assert_eq!(
            String::from_utf8_lossy(&bytes)
                .matches("/Type /Page ")
                .count(),
            2
        );
        assert!(pdf(&a4, &[], 72.0, "Print").is_err());
    }
}
