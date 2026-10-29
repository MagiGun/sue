//! Picture books: comics, manga and picture e-books, one picture a page,
//! as a PDF or as a fixed-layout EPUB 3 that keeps every page as drawn and
//! turns pages right to left for manga.

use std::fmt::Write as _;

use sue_core::RasterTile2D;

use super::xml::escape;
use crate::pdf::{PdfBookmark, PdfDocument, PdfImage, PdfItem, PdfPage, write_pdf};
use crate::zip::ZipWriter;

/// A book of pictures.
#[derive(Clone, Debug, Default)]
pub struct PictureBook {
    /// Its title.
    pub title: String,
    /// Its author.
    pub author: String,
    /// A language tag, e.g. `ja`.
    pub language: String,
    /// A unique id (an e-book's identifier).
    pub identifier: String,
    /// The pages, flattened, in reading order.
    pub pages: Vec<RasterTile2D>,
    /// Pages turn right to left (manga).
    pub right_to_left: bool,
    /// The pages' resolution, for their size on paper.
    pub dpi: f32,
}

/// A page as 8-bit RGB over white.
fn rgb_of(page: &RasterTile2D) -> Result<Vec<u8>, String> {
    let bytes = page.bytes().ok_or("pages are 8-bit RGBA")?;
    Ok(bytes
        .chunks(4)
        .flat_map(|pixel| {
            let alpha = u32::from(pixel[3]);
            [0, 1, 2].map(|channel| {
                ((u32::from(pixel[channel]) * alpha + 255 * (255 - alpha)) / 255) as u8
            })
        })
        .collect())
}

/// The book as a PDF: a page a picture, at its size on paper.
///
/// # Errors
/// If a page is not 8-bit RGBA or the PDF will not write.
pub fn picture_pdf(book: &PictureBook) -> Result<Vec<u8>, String> {
    let points = 72.0 / book.dpi.max(1.0);
    let mut doc = PdfDocument {
        title: book.title.clone(),
        author: book.author.clone(),
        ..PdfDocument::default()
    };
    for (index, page) in book.pages.iter().enumerate() {
        let size = [page.width() as f32 * points, page.height() as f32 * points];
        doc.images.push(PdfImage {
            width: page.width(),
            height: page.height(),
            rgb: rgb_of(page)?,
        });
        doc.pages.push(PdfPage {
            size,
            items: vec![PdfItem::Image {
                image: index,
                rect: [0.0, 0.0, size[0], size[1]],
            }],
        });
        doc.bookmarks.push(PdfBookmark {
            title: format!("Page {}", index + 1),
            level: 1,
            page: index,
            y: 0.0,
        });
    }
    write_pdf(&doc)
}

/// The book as a fixed-layout EPUB 3: each page a JPEG at `quality` on a
/// page of its own size, the spine read right to left for manga.
///
/// # Errors
/// If a page will not encode.
pub fn picture_epub(book: &PictureBook, quality: u8) -> Result<Vec<u8>, String> {
    let mut zip = ZipWriter::new();
    // The mimetype comes first, stored, as readers expect.
    zip.add("mimetype", b"application/epub+zip", true);
    zip.add_text(
        "META-INF/container.xml",
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<container version=\"1.0\" xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\"><rootfiles><rootfile full-path=\"OEBPS/content.opf\" media-type=\"application/oebps-package+xml\"/></rootfiles></container>",
    );
    let (mut manifest, mut spine, mut nav) = (String::new(), String::new(), String::new());
    for (index, page) in book.pages.iter().enumerate() {
        let number = index + 1;
        let jpeg = crate::encode_as(page, "jpg", quality, 1)?;
        zip.add(&format!("OEBPS/images/page-{number:04}.jpg"), &jpeg, true);
        let (w, h) = (page.width(), page.height());
        zip.add_text(
            &format!("OEBPS/page-{number:04}.xhtml"),
            &format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE html>\n<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\"><head><title>Page {number}</title><meta name=\"viewport\" content=\"width={w}, height={h}\"/><style>html,body{{margin:0;padding:0}}img{{display:block;width:{w}px;height:{h}px}}</style></head><body><img src=\"images/page-{number:04}.jpg\" alt=\"Page {number}\"/></body></html>"
            ),
        );
        let _ = write!(
            manifest,
            "<item id=\"page{number}\" href=\"page-{number:04}.xhtml\" media-type=\"application/xhtml+xml\"/><item id=\"image{number}\" href=\"images/page-{number:04}.jpg\" media-type=\"image/jpeg\"{}/>",
            if index == 0 {
                " properties=\"cover-image\""
            } else {
                ""
            }
        );
        let _ = write!(spine, "<itemref idref=\"page{number}\"/>");
        let _ = write!(
            nav,
            "<li><a href=\"page-{number:04}.xhtml\">Page {number}</a></li>"
        );
    }
    zip.add_text(
        "OEBPS/nav.xhtml",
        &format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE html>\n<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\"><head><title>{}</title></head><body><nav epub:type=\"toc\"><ol>{nav}</ol></nav></body></html>",
            escape(&book.title)
        ),
    );
    let direction = if book.right_to_left { "rtl" } else { "ltr" };
    zip.add_text(
        "OEBPS/content.opf",
        &format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"bookid\"><metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:identifier id=\"bookid\">{}</dc:identifier><dc:title>{}</dc:title><dc:creator>{}</dc:creator><dc:language>{}</dc:language><meta property=\"dcterms:modified\">2000-01-01T00:00:00Z</meta><meta property=\"rendition:layout\">pre-paginated</meta><meta property=\"rendition:spread\">landscape</meta></metadata><manifest><item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>{manifest}</manifest><spine page-progression-direction=\"{direction}\">{spine}</spine></package>",
            escape(&book.identifier),
            escape(&book.title),
            escape(&book.author),
            escape(if book.language.is_empty() { "en" } else { &book.language })
        ),
    );
    Ok(zip.finish())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_core::PixelFormat;

    fn book() -> PictureBook {
        let page = |shade: u8| {
            RasterTile2D::from_bytes(
                30,
                40,
                PixelFormat::Rgba8,
                [shade, shade, shade, 255].repeat(30 * 40),
            )
            .unwrap()
        };
        PictureBook {
            title: "Walk & Run".to_owned(),
            author: "Sue".to_owned(),
            language: "ja".to_owned(),
            identifier: "urn:sue:test".to_owned(),
            pages: vec![page(250), page(20)],
            right_to_left: true,
            dpi: 72.0,
        }
    }

    #[test]
    fn picture_books_write_a_pdf_page_and_a_fixed_layout_page_each() {
        let pdf = picture_pdf(&book()).unwrap();
        assert!(pdf.starts_with(b"%PDF"));
        let text = String::from_utf8_lossy(&pdf);
        assert_eq!(
            text.matches("/Type /Page /Parent").count(),
            2,
            "a page a picture"
        );
        assert!(
            text.contains("/MediaBox [0 0 30 40]"),
            "a page the picture's size"
        );

        let epub = picture_epub(&book(), 90).unwrap();
        let files = crate::zip::read_zip(&epub).unwrap();
        assert_eq!(files[0].0, "mimetype", "first");
        assert_eq!(files[0].1, b"application/epub+zip");
        let names: Vec<&str> = files.iter().map(|(name, _)| name.as_str()).collect();
        for name in [
            "META-INF/container.xml",
            "OEBPS/content.opf",
            "OEBPS/nav.xhtml",
            "OEBPS/page-0001.xhtml",
            "OEBPS/page-0002.xhtml",
            "OEBPS/images/page-0002.jpg",
        ] {
            assert!(names.contains(&name), "{name}");
        }
        let opf = files
            .iter()
            .find(|(name, _)| name == "OEBPS/content.opf")
            .unwrap();
        let opf = String::from_utf8_lossy(&opf.1);
        assert!(opf.contains("pre-paginated"));
        assert!(
            opf.contains("page-progression-direction=\"rtl\""),
            "manga reads right to left"
        );
        assert!(opf.contains("Walk &amp; Run"));
        let page = files
            .iter()
            .find(|(name, _)| name == "OEBPS/page-0001.xhtml")
            .unwrap();
        assert!(String::from_utf8_lossy(&page.1).contains("width=30, height=40"));
        let image = files
            .iter()
            .find(|(name, _)| name == "OEBPS/images/page-0002.jpg")
            .unwrap();
        let decoded = crate::decode_jpeg(&image.1).unwrap();
        assert_eq!((decoded.tile.width(), decoded.tile.height()), (30, 40));
    }
}
