//! Writing PDF: pages of positioned glyphs, rectangles, lines, pictures and
//! links, with the fonts embedded whole (TrueType and OpenType, a face of a
//! collection pulled out on its own), text that copies out as Unicode, and
//! bookmarks. Whoever sets the type (`sue-type`) decides where every glyph
//! goes; this only writes it down.
//!
//! Coordinates are in points from the top left of a page.

use std::fmt::Write as _;
use std::sync::Arc;

/// A font to embed: the file and the face in it.
#[derive(Clone, Debug)]
pub struct PdfFont {
    /// The font file.
    pub bytes: Arc<Vec<u8>>,
    /// The face in a collection, else 0.
    pub index: u32,
}

/// One glyph placed on a line.
#[derive(Clone, Debug, PartialEq)]
pub struct PdfGlyph {
    /// The glyph id in its font.
    pub id: u16,
    /// Its left edge, in points from the page's left.
    pub x: f32,
    /// The text it stands for (for copying).
    pub text: String,
}

/// Something drawn on a page.
#[derive(Clone, Debug, PartialEq)]
pub enum PdfItem {
    /// A run of glyphs of one font, size and color on one baseline.
    Text {
        /// The font, an index into [`PdfDocument::fonts`].
        font: usize,
        /// Size in points.
        size: f32,
        /// The baseline, in points from the top.
        baseline: f32,
        /// The fill color.
        color: [u8; 3],
        /// Slant it (the face has no italic).
        slant: bool,
        /// Thicken it (the face has no bold).
        embolden: bool,
        /// The glyphs.
        glyphs: Vec<PdfGlyph>,
    },
    /// A filled rectangle: x, y, width, height.
    Rect {
        /// Where.
        rect: [f32; 4],
        /// Its color.
        color: [u8; 3],
    },
    /// A straight line.
    Line {
        /// From.
        from: [f32; 2],
        /// To.
        to: [f32; 2],
        /// Its width.
        width: f32,
        /// Its color.
        color: [u8; 3],
    },
    /// A picture, an index into [`PdfDocument::images`], filling `rect`.
    Image {
        /// The picture.
        image: usize,
        /// Where.
        rect: [f32; 4],
    },
    /// A clickable area opening a web address.
    Link {
        /// Where.
        rect: [f32; 4],
        /// The address.
        uri: String,
    },
}

/// One page.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PdfPage {
    /// Width and height in points.
    pub size: [f32; 2],
    /// What is on it, back to front.
    pub items: Vec<PdfItem>,
}

/// A picture: 8-bit RGB pixels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PdfImage {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// `width * height * 3` bytes.
    pub rgb: Vec<u8>,
}

/// A bookmark.
#[derive(Clone, Debug, PartialEq)]
pub struct PdfBookmark {
    /// What it says.
    pub title: String,
    /// 1 for the top level.
    pub level: u8,
    /// The page, from 0.
    pub page: usize,
    /// Where on the page, in points from the top.
    pub y: f32,
}

/// A whole PDF.
#[derive(Clone, Debug, Default)]
pub struct PdfDocument {
    /// Its title.
    pub title: String,
    /// Its author.
    pub author: String,
    /// The fonts the text uses.
    pub fonts: Vec<PdfFont>,
    /// The pictures.
    pub images: Vec<PdfImage>,
    /// The pages.
    pub pages: Vec<PdfPage>,
    /// Bookmarks, in order.
    pub bookmarks: Vec<PdfBookmark>,
}

/// A text string for the document's info and bookmarks (UTF-16, so any
/// language shows).
fn text_string(text: &str) -> String {
    let mut out = String::from("<FEFF");
    for unit in text.encode_utf16() {
        let _ = write!(out, "{unit:04X}");
    }
    out.push('>');
    out
}

fn deflate(bytes: &[u8]) -> Vec<u8> {
    miniz_oxide::deflate::compress_to_vec_zlib(bytes, 6)
}

/// Pulls face `index` out of a font collection as a font file of its own
/// (a PDF embeds one face per file).
fn single_face(bytes: &[u8], index: u32) -> Option<Vec<u8>> {
    let be32 = |at: usize| -> Option<u32> {
        Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
    };
    let be16 = |at: usize| -> Option<u16> {
        Some(u16::from_be_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
    };
    if bytes.get(..4) != Some(b"ttcf") {
        return Some(bytes.to_vec());
    }
    let offset = be32(12 + index as usize * 4)? as usize;
    let version = be32(offset)?;
    let count = usize::from(be16(offset + 4)?);
    let mut tables = Vec::with_capacity(count);
    for table in 0..count {
        let record = offset + 12 + table * 16;
        let tag = bytes.get(record..record + 4)?;
        let at = be32(record + 8)? as usize;
        let len = be32(record + 12)? as usize;
        tables.push((tag.to_vec(), bytes.get(at..at + len)?.to_vec()));
    }
    let mut out = Vec::new();
    out.extend_from_slice(&version.to_be_bytes());
    out.extend_from_slice(&(count as u16).to_be_bytes());
    let power = (count.max(1) as f32).log2().floor() as u32;
    let range = (1_u16 << power) * 16;
    out.extend_from_slice(&range.to_be_bytes());
    out.extend_from_slice(&(power as u16).to_be_bytes());
    out.extend_from_slice(&((count as u16) * 16 - range).to_be_bytes());
    let mut at = 12 + count * 16;
    let mut body = Vec::new();
    for (tag, data) in &tables {
        let checksum = data.chunks(4).fold(0_u32, |sum, chunk| {
            let mut word = [0_u8; 4];
            word[..chunk.len()].copy_from_slice(chunk);
            sum.wrapping_add(u32::from_be_bytes(word))
        });
        out.extend_from_slice(tag);
        out.extend_from_slice(&checksum.to_be_bytes());
        out.extend_from_slice(&(at as u32).to_be_bytes());
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        body.extend_from_slice(data);
        let padded = data.len().div_ceil(4) * 4;
        body.resize(body.len() + padded - data.len(), 0);
        at += padded;
    }
    out.extend_from_slice(&body);
    Some(out)
}

/// The PostScript name of a face, made safe for a PDF name.
fn postscript_name(face: &ttf_parser::Face<'_>, fallback: usize) -> String {
    let name = face
        .names()
        .into_iter()
        .filter(|name| name.name_id == ttf_parser::name_id::POST_SCRIPT_NAME)
        .find_map(|name| name.to_string())
        .unwrap_or_default();
    let clean: String = name
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '-')
        .collect();
    if clean.is_empty() {
        format!("SueFont{fallback}")
    } else {
        clean
    }
}

struct Objects {
    out: Vec<u8>,
    offsets: Vec<usize>,
}

impl Objects {
    fn reserve(&mut self) -> usize {
        self.offsets.push(0);
        self.offsets.len()
    }

    fn put(&mut self, id: usize, body: &str) {
        self.offsets[id - 1] = self.out.len();
        self.out
            .extend_from_slice(format!("{id} 0 obj\n{body}\nendobj\n").as_bytes());
    }

    fn stream(&mut self, id: usize, dict: &str, data: &[u8]) {
        self.offsets[id - 1] = self.out.len();
        self.out.extend_from_slice(
            format!("{id} 0 obj\n<< {dict} /Length {} >>\nstream\n", data.len()).as_bytes(),
        );
        self.out.extend_from_slice(data);
        self.out.extend_from_slice(b"\nendstream\nendobj\n");
    }
}

fn number(value: f32) -> String {
    let rounded = (value * 1000.0).round() / 1000.0;
    if rounded == rounded.trunc() {
        format!("{}", rounded as i64)
    } else {
        format!("{rounded}")
    }
}

fn rgb(color: [u8; 3]) -> String {
    format!(
        "{} {} {}",
        number(f32::from(color[0]) / 255.0),
        number(f32::from(color[1]) / 255.0),
        number(f32::from(color[2]) / 255.0)
    )
}

/// Writes `doc` as PDF 1.7.
///
/// # Errors
/// If a font cannot be read.
#[allow(clippy::too_many_lines, reason = "one PDF, object by object")]
pub fn write_pdf(doc: &PdfDocument) -> Result<Vec<u8>, String> {
    let mut objects = Objects {
        out: b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n".to_vec(),
        offsets: Vec::new(),
    };
    let catalog = objects.reserve();
    let pages_id = objects.reserve();
    let info = objects.reserve();
    // Which glyphs of each font are used, with their text, for the widths
    // and the copy-out map.
    let mut used: Vec<std::collections::BTreeMap<u16, String>> =
        vec![Default::default(); doc.fonts.len()];
    for page in &doc.pages {
        for item in &page.items {
            if let PdfItem::Text { font, glyphs, .. } = item
                && let Some(map) = used.get_mut(*font)
            {
                for glyph in glyphs {
                    map.entry(glyph.id).or_insert_with(|| glyph.text.clone());
                }
            }
        }
    }
    // Fonts.
    let mut font_ids = Vec::with_capacity(doc.fonts.len());
    let mut widths: Vec<std::collections::HashMap<u16, f32>> = Vec::new();
    for (index, font) in doc.fonts.iter().enumerate() {
        let data = single_face(&font.bytes, font.index).ok_or("a font collection is broken")?;
        let face = ttf_parser::Face::parse(&data, 0)
            .map_err(|error| format!("a font cannot be read: {error}"))?;
        let em = f32::from(face.units_per_em()).max(1.0);
        let scale = |value: f32| value * 1000.0 / em;
        let name = postscript_name(&face, index);
        let cff = face
            .raw_face()
            .table(ttf_parser::Tag::from_bytes(b"CFF "))
            .is_some();
        let bbox = face.global_bounding_box();
        let mut glyph_widths = std::collections::HashMap::new();
        let mut w = String::from("[");
        for id in used[index].keys() {
            let advance = face
                .glyph_hor_advance(ttf_parser::GlyphId(*id))
                .map_or(0.0, f32::from);
            glyph_widths.insert(*id, scale(advance));
            let _ = write!(w, " {id} [{}]", number(scale(advance)));
        }
        w.push_str(" ]");
        widths.push(glyph_widths);
        let file = objects.reserve();
        let descriptor = objects.reserve();
        let cid = objects.reserve();
        let to_unicode = objects.reserve();
        let type0 = objects.reserve();
        let packed = deflate(&data);
        if cff {
            objects.stream(file, "/Subtype /OpenType /Filter /FlateDecode", &packed);
        } else {
            objects.stream(
                file,
                &format!("/Length1 {} /Filter /FlateDecode", data.len()),
                &packed,
            );
        }
        let italic = face.is_italic();
        let flags = 32 | if italic { 64 } else { 0 };
        objects.put(
            descriptor,
            &format!(
                "<< /Type /FontDescriptor /FontName /{name} /Flags {flags} /FontBBox [{} {} {} {}] /ItalicAngle {} /Ascent {} /Descent {} /CapHeight {} /StemV 80 /{} {file} 0 R >>",
                number(scale(f32::from(bbox.x_min))),
                number(scale(f32::from(bbox.y_min))),
                number(scale(f32::from(bbox.x_max))),
                number(scale(f32::from(bbox.y_max))),
                number(face.italic_angle()),
                number(scale(f32::from(face.ascender()))),
                number(scale(f32::from(face.descender()))),
                number(scale(f32::from(face.capital_height().unwrap_or(face.ascender())))),
                if cff { "FontFile3" } else { "FontFile2" }
            ),
        );
        objects.put(
            cid,
            &format!(
                "<< /Type /Font /Subtype /{} /BaseFont /{name} /CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> /FontDescriptor {descriptor} 0 R /DW 0 /W {w}{} >>",
                if cff { "CIDFontType0" } else { "CIDFontType2" },
                if cff { "" } else { " /CIDToGIDMap /Identity" }
            ),
        );
        let mut cmap = String::from(
            "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
        );
        let entries: Vec<(&u16, &String)> = used[index]
            .iter()
            .filter(|(_, text)| !text.is_empty())
            .collect();
        for chunk in entries.chunks(100) {
            let _ = writeln!(cmap, "{} beginbfchar", chunk.len());
            for (id, text) in chunk {
                let mut hex = String::new();
                for unit in text.encode_utf16() {
                    let _ = write!(hex, "{unit:04X}");
                }
                let _ = writeln!(cmap, "<{id:04X}> <{hex}>");
            }
            cmap.push_str("endbfchar\n");
        }
        cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
        objects.stream(
            to_unicode,
            "/Filter /FlateDecode",
            &deflate(cmap.as_bytes()),
        );
        objects.put(
            type0,
            &format!(
                "<< /Type /Font /Subtype /Type0 /BaseFont /{name} /Encoding /Identity-H /DescendantFonts [{cid} 0 R] /ToUnicode {to_unicode} 0 R >>"
            ),
        );
        font_ids.push(type0);
    }
    // Pictures.
    let mut image_ids = Vec::with_capacity(doc.images.len());
    for image in &doc.images {
        let id = objects.reserve();
        objects.stream(
            id,
            &format!(
                "/Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode",
                image.width, image.height
            ),
            &deflate(&image.rgb),
        );
        image_ids.push(id);
    }
    // Pages.
    let mut page_ids = Vec::with_capacity(doc.pages.len());
    let mut page_heights = Vec::with_capacity(doc.pages.len());
    for page in &doc.pages {
        let [width, height] = page.size;
        let y = |top: f32| height - top;
        let mut content = String::new();
        let mut links = Vec::new();
        for item in &page.items {
            match item {
                PdfItem::Rect { rect, color } => {
                    let _ = writeln!(
                        content,
                        "{} rg {} {} {} {} re f",
                        rgb(*color),
                        number(rect[0]),
                        number(y(rect[1] + rect[3])),
                        number(rect[2]),
                        number(rect[3])
                    );
                }
                PdfItem::Line {
                    from,
                    to,
                    width,
                    color,
                } => {
                    let _ = writeln!(
                        content,
                        "{} RG {} w {} {} m {} {} l S",
                        rgb(*color),
                        number(*width),
                        number(from[0]),
                        number(y(from[1])),
                        number(to[0]),
                        number(y(to[1]))
                    );
                }
                PdfItem::Image { image, rect } => {
                    if *image < image_ids.len() {
                        let _ = writeln!(
                            content,
                            "q {} 0 0 {} {} {} cm /Im{image} Do Q",
                            number(rect[2]),
                            number(rect[3]),
                            number(rect[0]),
                            number(y(rect[1] + rect[3]))
                        );
                    }
                }
                PdfItem::Link { rect, uri } => links.push((*rect, uri.clone())),
                PdfItem::Text {
                    font,
                    size,
                    baseline,
                    color,
                    slant,
                    embolden,
                    glyphs,
                } => {
                    let Some(first) = glyphs.first() else {
                        continue;
                    };
                    let Some(widths) = widths.get(*font) else {
                        continue;
                    };
                    let _ = write!(
                        content,
                        "BT /F{font} {} Tf {} rg",
                        number(*size),
                        rgb(*color)
                    );
                    if *embolden {
                        let _ = write!(
                            content,
                            " 2 Tr {} w {} RG",
                            number(size * 0.03),
                            rgb(*color)
                        );
                    }
                    let skew = if *slant { "0.21" } else { "0" };
                    let _ = write!(
                        content,
                        " 1 0 {skew} 1 {} {} Tm [",
                        number(first.x),
                        number(y(*baseline))
                    );
                    let mut pen = first.x;
                    for glyph in glyphs {
                        let shift = glyph.x - pen;
                        if shift.abs() > 0.01 {
                            let _ = write!(content, "{}", number(-shift * 1000.0 / size));
                        }
                        let _ = write!(content, "<{:04X}>", glyph.id);
                        pen =
                            glyph.x + widths.get(&glyph.id).copied().unwrap_or(0.0) * size / 1000.0;
                    }
                    content.push_str("] TJ");
                    if *embolden {
                        content.push_str(" 0 Tr");
                    }
                    content.push_str(" ET\n");
                }
            }
        }
        let stream = objects.reserve();
        objects.stream(stream, "/Filter /FlateDecode", &deflate(content.as_bytes()));
        let mut annots = String::new();
        for (rect, uri) in links {
            let id = objects.reserve();
            let escaped: String = uri
                .chars()
                .flat_map(|ch| match ch {
                    '(' | ')' | '\\' => vec!['\\', ch],
                    ch if ch.is_ascii() => vec![ch],
                    _ => vec![],
                })
                .collect();
            objects.put(
                id,
                &format!(
                    "<< /Type /Annot /Subtype /Link /Rect [{} {} {} {}] /Border [0 0 0] /A << /S /URI /URI ({escaped}) >> >>",
                    number(rect[0]),
                    number(y(rect[1] + rect[3])),
                    number(rect[0] + rect[2]),
                    number(y(rect[1]))
                ),
            );
            let _ = write!(annots, " {id} 0 R");
        }
        let mut fonts = String::new();
        for (index, id) in font_ids.iter().enumerate() {
            let _ = write!(fonts, " /F{index} {id} 0 R");
        }
        let mut xobjects = String::new();
        for (index, id) in image_ids.iter().enumerate() {
            let _ = write!(xobjects, " /Im{index} {id} 0 R");
        }
        let page_id = objects.reserve();
        objects.put(
            page_id,
            &format!(
                "<< /Type /Page /Parent {pages_id} 0 R /MediaBox [0 0 {} {}] /Contents {stream} 0 R /Resources << /Font <<{fonts} >> /XObject <<{xobjects} >> >>{} >>",
                number(width),
                number(height),
                if annots.is_empty() { String::new() } else { format!(" /Annots [{annots} ]") }
            ),
        );
        page_ids.push(page_id);
        page_heights.push(height);
    }
    // Bookmarks, nested by level.
    let mut outline_root = None;
    if !doc.bookmarks.is_empty() && !page_ids.is_empty() {
        let root = objects.reserve();
        let ids: Vec<usize> = doc.bookmarks.iter().map(|_| objects.reserve()).collect();
        let mut parents = vec![None; ids.len()];
        let mut stack: Vec<(u8, usize)> = Vec::new();
        for (index, mark) in doc.bookmarks.iter().enumerate() {
            while stack.last().is_some_and(|(level, _)| *level >= mark.level) {
                stack.pop();
            }
            parents[index] = stack.last().map(|(_, parent)| *parent);
            stack.push((mark.level, index));
        }
        let children = |parent: Option<usize>| -> Vec<usize> {
            (0..ids.len())
                .filter(|index| parents[*index] == parent)
                .collect()
        };
        for index in 0..ids.len() {
            let mark = &doc.bookmarks[index];
            let siblings = children(parents[index]);
            let at = siblings
                .iter()
                .position(|known| *known == index)
                .unwrap_or(0);
            let mut body = format!(
                "<< /Title {} /Parent {} 0 R",
                text_string(&mark.title),
                parents[index].map_or(root, |parent| ids[parent])
            );
            if at > 0 {
                let _ = write!(body, " /Prev {} 0 R", ids[siblings[at - 1]]);
            }
            if let Some(next) = siblings.get(at + 1) {
                let _ = write!(body, " /Next {} 0 R", ids[*next]);
            }
            let kids = children(Some(index));
            if let (Some(first), Some(last)) = (kids.first(), kids.last()) {
                let _ = write!(
                    body,
                    " /First {} 0 R /Last {} 0 R /Count -{}",
                    ids[*first],
                    ids[*last],
                    kids.len()
                );
            }
            let page = mark.page.min(page_ids.len() - 1);
            let _ = write!(
                body,
                " /Dest [{} 0 R /XYZ 0 {} 0] >>",
                page_ids[page],
                number(page_heights[page] - mark.y)
            );
            objects.put(ids[index], &body);
        }
        let top = children(None);
        objects.put(
            root,
            &format!(
                "<< /Type /Outlines /First {} 0 R /Last {} 0 R /Count {} >>",
                ids[top[0]],
                ids[*top.last().unwrap_or(&top[0])],
                top.len()
            ),
        );
        outline_root = Some(root);
    }
    let kids: String = page_ids.iter().map(|id| format!(" {id} 0 R")).collect();
    objects.put(
        pages_id,
        &format!(
            "<< /Type /Pages /Kids [{kids} ] /Count {} >>",
            page_ids.len()
        ),
    );
    objects.put(
        catalog,
        &format!(
            "<< /Type /Catalog /Pages {pages_id} 0 R{} >>",
            outline_root.map_or(String::new(), |root| format!(
                " /Outlines {root} 0 R /PageMode /UseOutlines"
            ))
        ),
    );
    objects.put(
        info,
        &format!(
            "<< /Title {} /Author {} /Producer {} >>",
            text_string(&doc.title),
            text_string(&doc.author),
            text_string("Sue")
        ),
    );
    let xref = objects.out.len();
    let mut tail = format!(
        "xref\n0 {}\n0000000000 65535 f \n",
        objects.offsets.len() + 1
    );
    for offset in &objects.offsets {
        let _ = writeln!(tail, "{offset:010} 00000 n ");
    }
    let _ = write!(
        tail,
        "trailer\n<< /Size {} /Root {catalog} 0 R /Info {info} 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.offsets.len() + 1
    );
    objects.out.extend_from_slice(tail.as_bytes());
    Ok(objects.out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_page_of_text_writes_a_whole_pdf() {
        // Any TrueType font will do; the test makes its own tiny one from
        // nothing, so it reads the font bytes the PDF is given.
        let font = PdfFont {
            bytes: Arc::new(tiny_font()),
            index: 0,
        };
        let doc = PdfDocument {
            title: "Test".to_owned(),
            author: "Sue".to_owned(),
            fonts: vec![font],
            images: vec![PdfImage {
                width: 1,
                height: 1,
                rgb: vec![255, 0, 0],
            }],
            pages: vec![PdfPage {
                size: [612.0, 792.0],
                items: vec![
                    PdfItem::Text {
                        font: 0,
                        size: 12.0,
                        baseline: 72.0,
                        color: [0, 0, 0],
                        slant: false,
                        embolden: true,
                        glyphs: vec![
                            PdfGlyph {
                                id: 1,
                                x: 72.0,
                                text: "A".to_owned(),
                            },
                            PdfGlyph {
                                id: 1,
                                x: 80.0,
                                text: "A".to_owned(),
                            },
                        ],
                    },
                    PdfItem::Image {
                        image: 0,
                        rect: [72.0, 100.0, 50.0, 50.0],
                    },
                    PdfItem::Link {
                        rect: [72.0, 60.0, 20.0, 14.0],
                        uri: "https://example.com".to_owned(),
                    },
                ],
            }],
            bookmarks: vec![
                PdfBookmark {
                    title: "One".to_owned(),
                    level: 1,
                    page: 0,
                    y: 72.0,
                },
                PdfBookmark {
                    title: "One a".to_owned(),
                    level: 2,
                    page: 0,
                    y: 90.0,
                },
            ],
        };
        let pdf = write_pdf(&doc).unwrap();
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.starts_with("%PDF-1.7"));
        assert!(text.ends_with("%%EOF\n"));
        assert!(text.contains("/Type /Outlines"));
        assert!(text.contains("/Subtype /Type0"));
        assert!(text.contains("/URI (https://example.com)"));
        // Every object the cross-reference table points at starts there.
        let xref = text.rfind("xref\n").unwrap();
        for line in text[xref..]
            .lines()
            .skip(3)
            .take_while(|line| line.ends_with(" n "))
        {
            let offset: usize = line[..10].parse().unwrap();
            assert!(
                text[offset..].split_whitespace().nth(2) == Some("obj"),
                "{line}"
            );
        }
    }

    /// A TrueType font with one glyph, built table by table.
    fn tiny_font() -> Vec<u8> {
        let mut tables: Vec<(&[u8; 4], Vec<u8>)> = Vec::new();
        let mut head = vec![0_u8; 54];
        head[..4].copy_from_slice(&0x0001_0000_u32.to_be_bytes());
        head[12..16].copy_from_slice(&0x5F0F_3CF5_u32.to_be_bytes());
        head[18..20].copy_from_slice(&1000_u16.to_be_bytes());
        tables.push((b"head", head));
        let mut hhea = vec![0_u8; 36];
        hhea[..4].copy_from_slice(&0x0001_0000_u32.to_be_bytes());
        hhea[4..6].copy_from_slice(&800_i16.to_be_bytes());
        hhea[6..8].copy_from_slice(&(-200_i16).to_be_bytes());
        hhea[34..36].copy_from_slice(&2_u16.to_be_bytes());
        tables.push((b"hhea", hhea));
        let mut maxp = vec![0_u8; 6];
        maxp[..4].copy_from_slice(&0x0000_5000_u32.to_be_bytes());
        maxp[4..6].copy_from_slice(&2_u16.to_be_bytes());
        tables.push((b"maxp", maxp));
        let mut hmtx = Vec::new();
        for advance in [500_u16, 600] {
            hmtx.extend_from_slice(&advance.to_be_bytes());
            hmtx.extend_from_slice(&0_i16.to_be_bytes());
        }
        tables.push((b"hmtx", hmtx));
        let mut out = Vec::new();
        out.extend_from_slice(&0x0001_0000_u32.to_be_bytes());
        out.extend_from_slice(&(tables.len() as u16).to_be_bytes());
        out.extend_from_slice(&[0; 6]);
        let mut at = 12 + tables.len() * 16;
        let mut body = Vec::new();
        for (tag, data) in &tables {
            out.extend_from_slice(*tag);
            out.extend_from_slice(&0_u32.to_be_bytes());
            out.extend_from_slice(&(at as u32).to_be_bytes());
            out.extend_from_slice(&(data.len() as u32).to_be_bytes());
            body.extend_from_slice(data);
            let padded = data.len().div_ceil(4) * 4;
            body.resize(body.len() + padded - data.len(), 0);
            at += padded;
        }
        out.extend_from_slice(&body);
        out
    }
}
