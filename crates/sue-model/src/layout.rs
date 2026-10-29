//! Page layout. Desktop publishing, books, magazines, comics, slideshows,
//! posters, PDF pages and drawing sheets (title blocks, viewports) are pages
//! of a physical size holding frames. A [`Frame`] shows another entity (a
//! text, a picture, a table, a 3D view, a vector drawing) at a place on the
//! page, so layout never copies content. Lengths are in points (1/72 inch),
//! the unit of PDF and print; [`sue_core::Quantity`] converts from any unit.

use sue_core::codec::{CodecError, Reader, Writer};
use sue_core::{ComponentType, EntityId, Properties, Quantity};

/// A rectangle in points: x, y (from the top left), width, height.
pub type Rect = [f64; 4];

/// Named page sizes, in points (width, height, portrait).
pub const PAGE_SIZES: &[(&str, [f64; 2])] = &[
    ("A3", [841.89, 1190.55]),
    ("A4", [595.28, 841.89]),
    ("A5", [419.53, 595.28]),
    ("B5", [498.9, 708.66]),
    ("Letter", [612.0, 792.0]),
    ("Legal", [612.0, 1008.0]),
    ("Tabloid", [792.0, 1224.0]),
    ("Trade paperback", [432.0, 648.0]),
    ("Slide 16:9", [960.0, 540.0]),
    ("Slide 4:3", [720.0, 540.0]),
    ("ARCH D", [1728.0, 2592.0]),
    ("ISO A1", [1683.78, 2383.94]),
];

/// The size named `name`, in points.
#[must_use]
pub fn page_size(name: &str) -> Option<[f64; 2]> {
    PAGE_SIZES
        .iter()
        .find(|(known, _)| known.eq_ignore_ascii_case(name))
        .map(|(_, size)| *size)
}

/// Converts a length quantity to points.
#[must_use]
pub fn points(length: &Quantity) -> Option<f64> {
    length.in_unit("pt")
}

/// A placed view of an entity.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    /// What it shows.
    pub content: EntityId,
    /// Where, in points.
    pub rect: Rect,
    /// Rotation in degrees, about the frame's center.
    pub rotation: f64,
    /// Frame settings (fit, crop, text flow to the next frame, opacity).
    pub properties: Properties,
}

impl Frame {
    /// A frame showing `content` at `rect`.
    #[must_use]
    pub fn new(content: EntityId, rect: Rect) -> Self {
        Self {
            content,
            rect,
            rotation: 0.0,
            properties: Properties::new(),
        }
    }
}

/// One page.
#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    /// Width and height in points.
    pub size: [f64; 2],
    /// Top, right, bottom and left margins in points.
    pub margins: [f64; 4],
    /// Bleed beyond the trim, in points.
    pub bleed: f64,
    /// The master page it inherits from, by index.
    pub master: Option<usize>,
    /// Frames, back to front.
    pub frames: Vec<Frame>,
    /// Page settings (background, transition for slides, section start).
    pub properties: Properties,
}

impl Page {
    /// An empty page of `size` with half-inch margins.
    #[must_use]
    pub fn new(size: [f64; 2]) -> Self {
        Self {
            size,
            margins: [36.0; 4],
            bleed: 0.0,
            master: None,
            frames: Vec::new(),
            properties: Properties::new(),
        }
    }

    /// The area inside the margins.
    #[must_use]
    pub fn content_rect(&self) -> Rect {
        let [top, right, bottom, left] = self.margins;
        [
            left,
            top,
            (self.size[0] - left - right).max(0.0),
            (self.size[1] - top - bottom).max(0.0),
        ]
    }

    /// Frames under the point `(x, y)`, front first.
    pub fn frames_at(&self, x: f64, y: f64) -> impl Iterator<Item = &Frame> {
        self.frames.iter().rev().filter(move |frame| {
            let [left, top, width, height] = frame.rect;
            x >= left && x < left + width && y >= top && y < top + height
        })
    }
}

/// Pages and master pages.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PageLayout {
    /// The pages, in reading order.
    pub pages: Vec<Page>,
    /// Master pages that pages inherit frames from.
    pub masters: Vec<Page>,
    /// `true` for facing (spread) pages.
    pub facing: bool,
}

impl PageLayout {
    /// `count` empty pages of `size`.
    #[must_use]
    pub fn with_pages(size: [f64; 2], count: usize) -> Self {
        Self {
            pages: vec![Page::new(size); count],
            ..Self::default()
        }
    }

    /// The frames page `index` shows: its master's, then its own.
    #[must_use]
    pub fn visible_frames(&self, index: usize) -> Vec<&Frame> {
        let Some(page) = self.pages.get(index) else {
            return Vec::new();
        };
        let master = page.master.and_then(|master| self.masters.get(master));
        master
            .into_iter()
            .flat_map(|master| master.frames.iter())
            .chain(&page.frames)
            .collect()
    }

    /// The pages that show `content`.
    #[must_use]
    pub fn pages_showing(&self, content: EntityId) -> Vec<usize> {
        (0..self.pages.len())
            .filter(|index| {
                self.visible_frames(*index)
                    .iter()
                    .any(|frame| frame.content == content)
            })
            .collect()
    }
}

fn encode_page(out: &mut Writer, page: &Page) {
    out.f64(page.size[0]);
    out.f64(page.size[1]);
    page.margins.iter().for_each(|margin| out.f64(*margin));
    out.f64(page.bleed);
    out.varint(page.master.map_or(0, |master| master as u64 + 1));
    out.len(page.frames.len());
    for frame in &page.frames {
        out.raw(&frame.content.to_bytes());
        frame.rect.iter().for_each(|value| out.f64(*value));
        out.f64(frame.rotation);
        frame.properties.encode(out);
    }
    page.properties.encode(out);
}

fn decode_page(input: &mut Reader<'_>) -> Result<Page, CodecError> {
    let mut page = Page::new([input.f64()?, input.f64()?]);
    for margin in &mut page.margins {
        *margin = input.f64()?;
    }
    page.bleed = input.f64()?;
    let master = input.varint()?;
    page.master = (master > 0)
        .then(|| usize::try_from(master - 1))
        .transpose()
        .map_err(|_| input.error("master index overflows"))?;
    for _ in 0..input.len(57)? {
        let mut id = [0; 16];
        id.copy_from_slice(input.raw(16)?);
        let mut frame = Frame::new(
            EntityId::from_bytes(id),
            [input.f64()?, input.f64()?, input.f64()?, input.f64()?],
        );
        frame.rotation = input.f64()?;
        frame.properties = Properties::decode(input)?;
        page.frames.push(frame);
    }
    page.properties = Properties::decode(input)?;
    Ok(page)
}

impl ComponentType for PageLayout {
    const NAME: &'static str = "sue.layout";
    const VERSION: u32 = 1;

    fn encode(&self, out: &mut Writer) {
        out.bool(self.facing);
        for pages in [&self.masters, &self.pages] {
            out.len(pages.len());
            pages.iter().for_each(|page| encode_page(out, page));
        }
    }

    fn decode(_version: u32, input: &mut Reader<'_>) -> Result<Self, CodecError> {
        let facing = input.bool()?;
        let read_pages = |input: &mut Reader<'_>| -> Result<Vec<Page>, CodecError> {
            let count = input.len(58)?;
            (0..count).map(|_| decode_page(input)).collect()
        };
        let masters = read_pages(input)?;
        let pages = read_pages(input)?;
        if pages
            .iter()
            .any(|page| page.master.is_some_and(|master| master >= masters.len()))
        {
            return Err(input.error("a page names a missing master"));
        }
        Ok(Self {
            pages,
            masters,
            facing,
        })
    }

    fn heap_bytes(&self) -> usize {
        self.pages
            .iter()
            .chain(&self.masters)
            .map(|page| page.frames.len() * size_of::<Frame>())
            .sum()
    }

    fn remap_entities(&mut self, map: &mut dyn FnMut(EntityId) -> EntityId) {
        for page in self.pages.iter_mut().chain(&mut self.masters) {
            for frame in &mut page.frames {
                frame.content = map(frame.content);
            }
        }
    }
}

/// A running header or footer: what goes at the left, the middle and the
/// right of every page's top or bottom margin. Each part is text with
/// fields in braces that each page fills in: `{page}`, `{pages}`, `{title}`,
/// `{author}` and `{date}`. Stored as one text, the parts parted by tabs
/// (a text without a tab is the middle part).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Running {
    /// At the left margin.
    pub left: String,
    /// In the middle.
    pub center: String,
    /// At the right margin.
    pub right: String,
}

/// The fields a page fills into its [`Running`] header and footer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PageFields<'a> {
    /// The page's number, from 1.
    pub page: usize,
    /// The number of pages.
    pub pages: usize,
    /// The document's title.
    pub title: &'a str,
    /// Its author.
    pub author: &'a str,
    /// Today's date, as the document writes dates.
    pub date: &'a str,
}

/// Every field a [`Running`] part may hold, and what people read for it.
pub const FIELDS: &[(&str, &str)] = &[
    ("page", "Page Number"),
    ("pages", "Page Count"),
    ("title", "Title"),
    ("author", "Author"),
    ("date", "Date"),
];

/// A piece of a [`Running`] part: plain text or a field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunningPiece<'a> {
    /// Text as it is.
    Text(&'a str),
    /// A field by its name, e.g. `page`.
    Field(&'a str),
}

impl Running {
    /// The parts of a stored header or footer (see [`Self::store`]).
    #[must_use]
    pub fn parse(stored: &str) -> Self {
        let parts: Vec<&str> = stored.split('\t').collect();
        match parts.as_slice() {
            [center] => Self {
                center: (*center).to_owned(),
                ..Self::default()
            },
            [left, center] => Self {
                left: (*left).to_owned(),
                center: (*center).to_owned(),
                right: String::new(),
            },
            [left, center, right, ..] => Self {
                left: (*left).to_owned(),
                center: (*center).to_owned(),
                right: (*right).to_owned(),
            },
            [] => Self::default(),
        }
    }

    /// One text holding the three parts, parted by tabs (just the middle
    /// part when the others are empty).
    #[must_use]
    pub fn store(&self) -> String {
        if self.left.is_empty() && self.right.is_empty() {
            self.center.clone()
        } else {
            format!("{}\t{}\t{}", self.left, self.center, self.right)
        }
    }

    /// `true` when all three parts are empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.left.trim().is_empty() && self.center.trim().is_empty() && self.right.trim().is_empty()
    }

    /// The left, middle and right parts with `fields` filled in.
    #[must_use]
    pub fn fill(&self, fields: &PageFields<'_>) -> [String; 3] {
        [&self.left, &self.center, &self.right].map(|part| fill(part, fields))
    }
}

/// `part` cut into its text and its fields. An unknown name in braces
/// stays text.
#[must_use]
pub fn running_pieces(part: &str) -> Vec<RunningPiece<'_>> {
    let mut out = Vec::new();
    let mut rest = part;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            break;
        };
        let name = &after[..close];
        if FIELDS.iter().any(|(known, _)| *known == name) {
            if open > 0 {
                out.push(RunningPiece::Text(&rest[..open]));
            }
            out.push(RunningPiece::Field(name));
        } else {
            out.push(RunningPiece::Text(&rest[..open + close + 2]));
        }
        rest = &after[close + 1..];
    }
    if !rest.is_empty() {
        out.push(RunningPiece::Text(rest));
    }
    out
}

/// `part` with `fields` filled in.
#[must_use]
pub fn fill(part: &str, fields: &PageFields<'_>) -> String {
    running_pieces(part)
        .into_iter()
        .map(|piece| match piece {
            RunningPiece::Text(text) => text.to_owned(),
            RunningPiece::Field("page") => fields.page.to_string(),
            RunningPiece::Field("pages") => fields.pages.to_string(),
            RunningPiece::Field("title") => fields.title.to_owned(),
            RunningPiece::Field("author") => fields.author.to_owned(),
            RunningPiece::Field("date") => fields.date.to_owned(),
            RunningPiece::Field(other) => format!("{{{other}}}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_frames_and_masters() {
        let (logo, story) = (EntityId::new(), EntityId::new());
        let a4 = page_size("a4").unwrap();
        assert!((points(&Quantity::new(210.0, "mm")).unwrap() - a4[0]).abs() < 0.01);
        let mut layout = PageLayout::with_pages(a4, 3);
        let mut master = Page::new(a4);
        master
            .frames
            .push(Frame::new(logo, [36.0, 36.0, 72.0, 36.0]));
        layout.masters.push(master);
        layout.pages[1].master = Some(0);
        let body = layout.pages[1].content_rect();
        layout.pages[1].frames.push(Frame::new(story, body));
        assert_eq!(layout.visible_frames(1).len(), 2);
        assert_eq!(layout.pages_showing(logo), vec![1]);
        assert_eq!(layout.pages[1].frames_at(300.0, 400.0).count(), 1);
        assert_eq!(layout.pages[1].content_rect()[2], a4[0] - 72.0);

        let mut out = Writer::new();
        ComponentType::encode(&layout, &mut out);
        let bytes = out.into_bytes();
        let back = PageLayout::decode(1, &mut Reader::new(&bytes)).unwrap();
        assert_eq!(back, layout);
    }

    #[test]
    fn running_heads_fill_their_fields_by_page() {
        let footer = Running::parse("{title}	Page {page} of {pages}	{date}");
        assert_eq!(footer.left, "{title}");
        assert_eq!(Running::parse(&footer.store()), footer);
        let fields = PageFields {
            page: 3,
            pages: 12,
            title: "Book",
            author: "Me",
            date: "2026-10-29",
        };
        assert_eq!(
            footer.fill(&fields),
            ["Book", "Page 3 of 12", "2026-10-29"].map(String::from)
        );
        let head = Running::parse("{author}: {nope}");
        assert_eq!(head.center, "{author}: {nope}");
        assert_eq!(head.store(), "{author}: {nope}");
        assert_eq!(head.fill(&fields)[1], "Me: {nope}");
        assert_eq!(
            running_pieces("p{page}/{pages}"),
            vec![
                RunningPiece::Text("p"),
                RunningPiece::Field("page"),
                RunningPiece::Text("/"),
                RunningPiece::Field("pages"),
            ]
        );
        assert!(Running::parse("	 	").is_empty());
    }
}
