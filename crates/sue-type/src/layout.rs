//! Laying rich text out: each paragraph becomes one egui galley (shaped
//! with the fonts it asks for), then the galleys are stacked and, in page
//! view, broken across pages between their lines. The same blocks serve
//! hit testing, the caret, selection and painting.

use std::ops::Range;
use std::sync::Arc;

use egui::text::{ByteIndex, LayoutJob, LayoutSection, TextFormat};
use egui::{Color32, FontFamily, FontId, Galley, Rect, Shadow, Stroke, Vec2, pos2, vec2};
use sue_core::{EntityId, Value};
use sue_model::contents::Contents;
use sue_model::{Mark, PageFields, Running, TextMarks, mark_kind as kind};
use sue_wm::Theme;
use sue_wm::ui::{EDGE_CONTRAST, Elevation, TEXT_CONTRAST, contrast};

use crate::style::{
    Align, CharFormat, ListKind, ParagraphLook, StyleSheet, marks_by_paragraph, paragraph_look,
    runs,
};
use crate::tables::{TableBlock, TableSource, lay_table};

/// Screen pixels per point at 100% zoom (a 96 dpi screen).
pub const PX_PER_PT: f32 = 96.0 / 72.0;

/// A page's size and margins, in points, and what its margins show.
#[derive(Clone, Debug, PartialEq)]
pub struct PageSetup {
    /// Width and height.
    pub size: [f32; 2],
    /// Top, right, bottom and left margins.
    pub margins: [f32; 4],
    /// The running header, in the top margin.
    pub header: Running,
    /// The running footer, in the bottom margin (the page number in the
    /// middle, unless set otherwise).
    pub footer: Running,
    /// No header or footer on the first page (a title page).
    pub different_first: bool,
    /// The title, author and date their fields show.
    pub title: String,
    /// The author.
    pub author: String,
    /// The date.
    pub date: String,
}

impl Default for PageSetup {
    fn default() -> Self {
        Self {
            size: [612.0, 792.0],
            margins: [72.0; 4],
            header: Running::default(),
            footer: Running::parse("{page}"),
            different_first: false,
            title: String::new(),
            author: String::new(),
            date: String::new(),
        }
    }
}

impl PageSetup {
    /// What page `page` (from 0) of `pages` shows in its header and in its
    /// footer: left, middle and right, fields filled in. `None` for none.
    #[must_use]
    pub fn furniture(&self, page: usize, pages: usize) -> [Option<[String; 3]>; 2] {
        if page == 0 && self.different_first {
            return [None, None];
        }
        let fields = PageFields {
            page: page + 1,
            pages,
            title: &self.title,
            author: &self.author,
            date: &self.date,
        };
        [&self.header, &self.footer]
            .map(|running| (!running.is_empty()).then(|| running.fill(&fields)))
    }

    /// The width text runs across, in points.
    #[must_use]
    pub fn text_width(&self) -> f32 {
        (self.size[0] - self.margins[1] - self.margins[3]).max(36.0)
    }

    /// The height text fills, in points.
    #[must_use]
    pub fn text_height(&self) -> f32 {
        (self.size[1] - self.margins[0] - self.margins[2]).max(36.0)
    }
}

/// How the text is shown.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ViewMode {
    /// Pages, as they print.
    Page,
    /// One long column of `width` points, no pages.
    Draft {
        /// The line length in points.
        width: f32,
    },
}

/// The colors text is set in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ink {
    /// The page.
    pub paper: Color32,
    /// Text.
    pub text: Color32,
    /// Lighter text (subtitles, captions).
    pub soft: Color32,
    /// Links.
    pub link: Color32,
    /// Behind code and comments.
    pub shade: Color32,
    /// Whether the marks' own text colors show (off on dark paper).
    pub colors: bool,
}

impl Ink {
    /// Black on white, as on paper.
    #[must_use]
    pub fn paper() -> Self {
        Self {
            paper: Color32::WHITE,
            text: Color32::from_gray(0x18),
            soft: Color32::from_gray(0x6a),
            link: Color32::from_rgb(0x1a, 0x55, 0xb0),
            shade: Color32::from_gray(0xee),
            colors: true,
        }
    }
}

/// The colors of what the editor marks on a page: the selection and its
/// handles, the spelling wave, tracked changes, commented words and the
/// words of a note, and the shadow under a page.
///
/// Each is a semantic role of the [`Theme`] (its selection, focus,
/// danger, warning and info colors and its shadow) resolved for the
/// page's own [`Ink`], so that it reads on that paper: the theme decides
/// what a mark is, the paper decides how light or dark it has to be. Sue's
/// shipped themes are monochrome, so there the marks are told apart by
/// their shape (a wave, an underline, a strike, a wash), never by hue
/// alone; a theme that gives its roles a hue colors them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageMarks {
    /// Behind selected text while the editor has the keyboard. Like
    /// every wash here it is opaque, the role mixed into the paper, so it
    /// shows as worked out whatever space the screen blends in.
    pub selection: Color32,
    /// Behind selected text while something else has the keyboard.
    pub selection_idle: Color32,
    /// A finger selection's handles.
    pub handle: Color32,
    /// The wave under a word spelling does not know.
    pub misspelt: Color32,
    /// Text a tracked change inserted (also underlined).
    pub inserted: Color32,
    /// Text a tracked change deleted (also struck through).
    pub deleted: Color32,
    /// Behind words a comment is on.
    pub comment: Color32,
    /// Behind the words of a footnote or an endnote.
    pub note: Color32,
    /// The shadow under a page in page view.
    pub page_shadow: Shadow,
}

/// How far the selection mixes its color into the paper, at most: less
/// where the text over it would no longer read.
const SELECTION_WASH: f32 = 0.24;
/// The least wash that still shows on the paper.
const LEAST_WASH: f32 = 0.08;
/// An idle selection, a comment and a note mix the text color into the
/// paper this much of the selection's wash.
const IDLE_SHARE: f32 = 0.5;
/// See [`IDLE_SHARE`].
const COMMENT_SHARE: f32 = 0.55;
/// See [`IDLE_SHARE`].
const NOTE_SHARE: f32 = 0.28;
/// How far a hued theme's comment and note colors mix into the paper.
const HUED_COMMENT: f32 = 0.33;
/// See [`HUED_COMMENT`].
const HUED_NOTE: f32 = 0.17;

/// `from` mixed toward `to` by `t` (0 is `from`), opaque.
fn mix(from: Color32, to: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let channel = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t).round() as u8;
    Color32::from_rgb(
        channel(from.r(), to.r()),
        channel(from.g(), to.g()),
        channel(from.b(), to.b()),
    )
}

/// `role` moved toward `text` by as little as it takes to reach `ratio`
/// on `paper`.
fn reading(role: Color32, text: Color32, paper: Color32, ratio: f32) -> Color32 {
    (0..=20)
        .map(|step| mix(role, text, step as f32 / 20.0))
        .find(|color| contrast(*color, paper) >= ratio)
        .unwrap_or(text)
}

impl PageMarks {
    /// The marks of `theme` on a page of `ink`.
    #[must_use]
    pub fn resolve(theme: &Theme, ink: &Ink) -> Self {
        // A role the theme leaves at its text color gives no hue: the
        // page's own ink stands in for it.
        let hue = |role: Color32| (role != theme.text).then_some(role);
        Self::with(
            ink,
            Some(theme.selection),
            Some(theme.focus()),
            hue(theme.danger()),
            hue(theme.info()),
            hue(theme.warning()),
            theme.elevation(Elevation::Raised),
        )
    }

    /// The marks of a page of `ink` with no theme to ask (tests, and
    /// pages set away from the screen): all from the ink.
    #[must_use]
    pub fn plain(ink: &Ink) -> Self {
        let shadow = Shadow {
            offset: [0, 2],
            blur: 8,
            spread: 0,
            color: Color32::from_black_alpha(64),
        };
        Self::with(ink, None, None, None, None, None, shadow)
    }

    fn with(
        ink: &Ink,
        selection: Option<Color32>,
        focus: Option<Color32>,
        danger: Option<Color32>,
        info: Option<Color32>,
        warning: Option<Color32>,
        page_shadow: Shadow,
    ) -> Self {
        let paper = ink.paper;
        let text = ink.text;
        // The selection's color, where it stands apart from the paper;
        // else the text's own.
        let base = selection
            .filter(|color| contrast(*color, paper) >= EDGE_CONTRAST)
            .unwrap_or(text);
        // As strong a wash as keeps the text over it readable.
        let mut wash = SELECTION_WASH;
        while wash > LEAST_WASH && contrast(text, mix(paper, base, wash)) < TEXT_CONTRAST {
            wash -= 0.02;
        }
        let tint = |role: Option<Color32>, share: f32, hued: f32| match role {
            Some(role) => mix(paper, role, hued),
            None => mix(paper, text, wash * share),
        };
        Self {
            selection: mix(paper, base, wash),
            selection_idle: mix(paper, text, wash * IDLE_SHARE),
            handle: reading(focus.unwrap_or(text), text, paper, EDGE_CONTRAST),
            misspelt: danger.map_or(text, |role| reading(role, text, paper, EDGE_CONTRAST)),
            inserted: info.map_or(text, |role| reading(role, text, paper, TEXT_CONTRAST)),
            deleted: danger.map_or(ink.soft, |role| reading(role, text, paper, TEXT_CONTRAST)),
            comment: tint(warning, COMMENT_SHARE, HUED_COMMENT),
            note: tint(info, NOTE_SHARE, HUED_NOTE),
            page_shadow,
        }
    }

    /// What of the marks goes into laid out text (tracked changes,
    /// comments and notes), for a layout cache to tell when it is stale.
    #[must_use]
    pub fn text_key(&self) -> [[u8; 4]; 4] {
        [self.inserted, self.deleted, self.comment, self.note].map(|color| color.to_array())
    }
}

/// One paragraph, laid out.
#[derive(Clone, Debug)]
pub struct Block {
    /// The document it is in (an index into the shown documents).
    pub doc: usize,
    /// Its paragraph's index in that document.
    pub index: usize,
    /// Its bytes in the document, without the newline.
    pub range: Range<usize>,
    /// How it looks.
    pub look: ParagraphLook,
    /// Its text, shaped.
    pub galley: Arc<Galley>,
    /// Where the galley's x = 0 sits, in pixels from the text's left edge
    /// (the left indent, the middle or the right end, by alignment).
    pub origin: f32,
    /// The left indent in pixels.
    pub indent: f32,
    /// The width its lines run across, in pixels.
    pub width: f32,
    /// A list bullet or number.
    pub label: Option<Arc<Galley>>,
    /// An embedded picture filling the paragraph: the entity and its size
    /// in pixels.
    pub figure: Option<(EntityId, Vec2)>,
    /// An embedded table filling the paragraph; its rows are the block's.
    pub table: Option<Arc<TableBlock>>,
    /// A table of contents filling the paragraph: its entity and settings
    /// (its entries, in `table`, come with the pages,
    /// [`crate::contents::fill`]).
    pub contents: Option<(EntityId, Contents)>,
    /// Each row's top and bottom, in galley pixels.
    pub rows: Vec<(f32, f32)>,
    /// Space above and below, in pixels.
    pub space: (f32, f32),
}

impl Block {
    /// The number of characters in its galley.
    #[must_use]
    pub fn chars(&self) -> usize {
        self.galley
            .rows
            .iter()
            .map(|row| row.row.glyphs.len())
            .sum()
    }
}

/// The rows of one block that sit on one page.
#[derive(Clone, Debug, PartialEq)]
pub struct Piece {
    /// The block.
    pub block: usize,
    /// Its rows here.
    pub rows: Range<usize>,
    /// The page (0 in draft view).
    pub page: usize,
    /// Where the first row's top is, in pixels below the top of the text
    /// area.
    pub top: f32,
    /// The first row's top in galley pixels (subtract to place the galley).
    pub shift: f32,
}

/// Several documents, laid out.
#[derive(Clone, Debug, Default)]
pub struct Laid {
    /// Every paragraph, documents one after another.
    pub blocks: Vec<Block>,
    /// Where the blocks go.
    pub pieces: Vec<Piece>,
    /// The pages (1 in draft view).
    pub pages: usize,
    /// A page's size in pixels (the column in draft view).
    pub page: Vec2,
    /// The text area's top left inside a page, in pixels.
    pub inset: Vec2,
    /// The text area's width, in pixels.
    pub text_width: f32,
    /// The text area's height on a page (the whole column in draft view).
    pub text_height: f32,
    /// The first block of each document.
    pub doc_starts: Vec<usize>,
}

/// A document to lay out.
pub struct Source<'a> {
    /// Its text.
    pub text: &'a str,
    /// Its formatting.
    pub marks: &'a TextMarks,
    /// Pictures it shows: entity and size in pixels at 100%.
    pub figure_size: &'a dyn Fn(EntityId) -> Option<Vec2>,
    /// Tables it shows ([`crate::tables`]).
    pub tables: &'a dyn Fn(EntityId) -> Option<TableSource>,
    /// Tables of contents it shows ([`crate::contents`]).
    pub contents: &'a dyn Fn(EntityId) -> Option<Contents>,
}

/// What a paragraph's text is laid out with.
pub struct Setting<'a> {
    /// The styles.
    pub sheet: &'a StyleSheet,
    /// Pixels per point.
    pub scale: f32,
    /// The text area's width in points.
    pub width: f32,
    /// The colors.
    pub ink: Ink,
    /// The colors of tracked changes, comments and notes in the text.
    pub marks: PageMarks,
}

fn family(ctx: &egui::Context, name: &str, bold: bool, italic: bool) -> (FontFamily, bool, bool) {
    let resolved = sue_wm::fonts::resolve(ctx, name, bold, italic);
    (resolved.family, resolved.fake_bold, resolved.fake_italic)
}

fn color(rgb: [u8; 3]) -> Color32 {
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

/// Where an underline sits below the baseline, as a share of the font's
/// descent.
const UNDERLINE_DROP: f32 = 0.45;
/// Where a strike sits above the baseline, as a share of the font's
/// ascent: through the middle of the small letters.
const STRIKE_RISE: f32 = 0.28;

/// Lays `job` out with its underlines and strikes on the text itself.
///
/// egui draws them from the line box: an underline at its bottom and a
/// strike through its middle. With line spacing above single, the box
/// reaches well below the letters, so an underline floats toward the next
/// line and a strike runs along the baseline, where it reads as an
/// underline. Here the lines are left out of the layout and put in after,
/// from each glyph's baseline and font. Where the glyphs cannot be matched
/// to the text (which egui's layout does not do today), egui's own lines
/// are kept.
pub(crate) fn lay_out(ctx: &egui::Context, job: LayoutJob) -> Arc<Galley> {
    let lines: Vec<(Stroke, Stroke)> = job
        .sections
        .iter()
        .map(|section| (section.format.underline, section.format.strikethrough))
        .collect();
    let spaced = job
        .sections
        .iter()
        .any(|section| section.format.line_height.is_some());
    if !spaced
        || lines
            .iter()
            .all(|(under, strike)| under.is_empty() && strike.is_empty())
    {
        return ctx.fonts_mut(|fonts| fonts.layout_job(job));
    }
    let mut bare = job.clone();
    for section in &mut bare.sections {
        section.format.underline = Stroke::NONE;
        section.format.strikethrough = Stroke::NONE;
    }
    let galley = ctx.fonts_mut(|fonts| fonts.layout_job(bare));
    match place_lines(&galley, &job, &lines) {
        Some(placed) => Arc::new(placed),
        None => ctx.fonts_mut(|fonts| fonts.layout_job(job)),
    }
}

/// A line being drawn along a row: its stroke, its height and where it
/// runs across.
type Run = (Stroke, f32, f32, f32);

/// `galley` (laid out from `job` without lines) with `lines`, the
/// underline and strike of each section of `job`, drawn on its text.
/// `None` when a glyph does not match the text.
fn place_lines(galley: &Galley, job: &LayoutJob, lines: &[(Stroke, Stroke)]) -> Option<Galley> {
    let section_of = |byte: usize| {
        job.sections.iter().position(|section| {
            section.byte_range.start.0 <= byte && byte < section.byte_range.end.0
        })
    };
    let pixels = galley.pixels_per_point.max(0.1);
    let snap = |y: f32| (y * pixels).round() / pixels;
    let mut out = galley.clone();
    let mut chars = job.text.char_indices();
    let (mut vertices, mut indices) = (0, 0);
    for placed in &mut out.rows {
        let mut rects: Vec<(Rect, Color32)> = Vec::new();
        let mut finish = |run: &mut Option<Run>| {
            if let Some((stroke, y, left, right)) = run.take() {
                let width = stroke.width.max(1.0 / pixels);
                let top = snap(y - width / 2.0);
                rects.push((
                    Rect::from_min_max(pos2(left, top), pos2(right, top + width)),
                    stroke.color,
                ));
            }
        };
        let (mut under_run, mut strike_run): (Option<Run>, Option<Run>) = (None, None);
        for glyph in &placed.row.glyphs {
            let (byte, ch) = chars.next()?;
            if ch != glyph.chr {
                return None;
            }
            let (under, strike) = lines[section_of(byte)?];
            let base = glyph.pos.y;
            let descent = (glyph.font_height - glyph.font_ascent).max(0.0);
            for (stroke, y, run) in [
                (under, base + descent * UNDERLINE_DROP, &mut under_run),
                (
                    strike,
                    base - glyph.font_ascent * STRIKE_RISE,
                    &mut strike_run,
                ),
            ] {
                match run {
                    Some((same, at, _, right))
                        if !stroke.is_empty() && *same == stroke && (*at - y).abs() < 0.01 =>
                    {
                        *right = glyph.max_x();
                    }
                    _ => {
                        finish(run);
                        if !stroke.is_empty() {
                            *run = Some((stroke, y, glyph.pos.x, glyph.max_x()));
                        }
                    }
                }
            }
        }
        finish(&mut under_run);
        finish(&mut strike_run);
        if placed.ends_with_newline && chars.next()?.1 != '\n' {
            return None;
        }
        if rects.is_empty() {
            continue;
        }
        let row = Arc::make_mut(&mut placed.row);
        let mesh = &mut row.visuals.mesh;
        let (before_vertices, before_indices) = (mesh.vertices.len(), mesh.indices.len());
        for (rect, color) in rects {
            mesh.add_colored_rect(rect, color);
            row.visuals.mesh_bounds = row.visuals.mesh_bounds.union(rect);
            out.mesh_bounds = out.mesh_bounds.union(rect.translate(placed.pos.to_vec2()));
        }
        vertices += mesh.vertices.len() - before_vertices;
        indices += mesh.indices.len() - before_indices;
    }
    out.num_vertices += vertices;
    out.num_indices += indices;
    Some(out)
}

/// The bullet of list level `level`.
fn bullet(level: usize) -> &'static str {
    ["\u{2022}", "\u{25E6}", "\u{25AA}"][level % 3]
}

/// A paragraph's text format for one run.
pub(crate) fn format(
    ctx: &egui::Context,
    look: &ParagraphLook,
    run: &CharFormat,
    setting: &Setting<'_>,
    line: Option<f32>,
) -> TextFormat {
    let size_pt = run.size.unwrap_or(look.size);
    let script = run.script != 0;
    let size = size_pt
        * setting.scale
        * if script { 0.66 } else { 1.0 }
        * if run.small_caps { 0.8 } else { 1.0 };
    let bold = look.bold || run.bold;
    let italic = look.italic || run.italic;
    let name = if run.code {
        setting.sheet.mono_font.as_str()
    } else {
        run.font.as_deref().unwrap_or(&look.font)
    };
    let (family, fake_bold, fake_italic) = family(ctx, name, bold, italic);
    let ink = &setting.ink;
    let mut text = if run.link.is_some() {
        ink.link
    } else if look.soft {
        ink.soft
    } else {
        ink.text
    };
    if ink.colors
        && let Some(rgb) = run.color.or(look.color)
    {
        text = color(rgb);
    }
    // Tracked changes show in the theme's colors for them (struck
    // through and underlined below, so they read without color too).
    if run.deleted.is_some() {
        text = setting.marks.deleted;
    } else if run.inserted.is_some() {
        text = setting.marks.inserted;
    }
    let mut background = Color32::TRANSPARENT;
    if let Some(rgb) = run.highlight {
        background = color(rgb);
    } else if run.comment.is_some() {
        background = setting.marks.comment;
    } else if run.code {
        background = ink.shade;
    } else if run.footnote.is_some() || run.endnote.is_some() {
        // A footnote's words, faintly: the note itself prints at the end.
        background = setting.marks.note;
    }
    let stroke = Stroke::new((setting.scale * 0.75).max(1.0), text);
    TextFormat {
        font_id: FontId::new(size.max(1.0), family),
        extra_letter_spacing: if fake_bold { size * 0.045 } else { 0.0 },
        line_height: if script { None } else { line },
        color: text,
        background,
        italics: fake_italic,
        underline: if look.underline
            || run.underline
            || run.link.is_some()
            || run.inserted.is_some()
        {
            stroke
        } else {
            Stroke::NONE
        },
        strikethrough: if run.strike || run.deleted.is_some() {
            stroke
        } else {
            Stroke::NONE
        },
        valign: match run.script {
            1 => egui::Align::TOP,
            -1 => egui::Align::BOTTOM,
            _ => egui::Align::BOTTOM,
        },
        ..TextFormat::default()
    }
}

/// Lays out every paragraph of one document. `doc` is its index among the
/// shown documents.
#[must_use]
pub fn lay_document(
    ctx: &egui::Context,
    doc: usize,
    source: &Source<'_>,
    setting: &Setting<'_>,
) -> Vec<Block> {
    let text = source.text;
    let paragraphs = sue_model::paragraphs(text);
    let by_paragraph = marks_by_paragraph(source.marks.marks(), &paragraphs);
    let mut blocks = Vec::with_capacity(paragraphs.len().max(1));
    let mut numbers = [0_usize; 9];
    let width_px = setting.width * setting.scale;
    let empty: Vec<&Mark> = Vec::new();
    // An empty text still has one (empty) paragraph to put the caret in.
    let ranges: Vec<Range<usize>> = if paragraphs.is_empty() {
        std::iter::once(0..0).collect()
    } else {
        paragraphs
    };
    for (index, range) in ranges.iter().enumerate() {
        let marks = by_paragraph.get(index).unwrap_or(&empty);
        let look = paragraph_look(setting.sheet, marks);
        let body = range.start..range.end - usize::from(text[range.clone()].ends_with('\n'));
        let indent = look.indent * setting.scale;
        let width = (width_px - indent - look.indent_right * setting.scale).max(24.0);
        // List numbering runs through consecutive list paragraphs.
        let label = match look.list {
            Some(list) => {
                numbers[look.level + 1..].iter_mut().for_each(|n| *n = 0);
                numbers[look.level] += 1;
                let label = match list {
                    ListKind::Bullet => bullet(look.level).to_owned(),
                    ListKind::Number => format!("{}.", numbers[look.level]),
                };
                let base = format(ctx, &look, &CharFormat::default(), setting, None);
                let job = LayoutJob::single_section(label, base);
                Some(ctx.fonts_mut(|fonts| fonts.layout_job(job)))
            }
            None => {
                numbers = [0; 9];
                None
            }
        };
        let base_font = format(ctx, &look, &CharFormat::default(), setting, None).font_id;
        let natural = ctx.fonts_mut(|fonts| fonts.row_height(&base_font));
        let line = Some(natural * look.line_spacing);
        let embed = (text[body.clone()] == sue_model::OBJECT.to_string())
            .then(|| {
                marks
                    .iter()
                    .find(|mark| mark.kind == kind::EMBED)
                    .and_then(|mark| match mark.value {
                        Value::Entity(id) => Some(id),
                        _ => None,
                    })
            })
            .flatten();
        let table = embed.and_then(|id| {
            let source = (source.tables)(id)?;
            Some(Arc::new(lay_table(ctx, id, &source, &look, setting, width)))
        });
        let contents = embed.and_then(|id| Some((id, (source.contents)(id)?)));
        let figure = embed
            .filter(|_| table.is_none() && contents.is_none())
            .and_then(|id| {
                let size = (source.figure_size)(id)? * setting.scale;
                let fit = (width / size.x.max(1.0)).min(1.0);
                Some((id, size * fit))
            });
        let mut job = LayoutJob::default();
        job.wrap.max_width = width;
        job.halign = match look.align {
            Align::Left | Align::Justify => egui::Align::LEFT,
            Align::Center => egui::Align::Center,
            Align::Right => egui::Align::RIGHT,
        };
        job.justify = look.align == Align::Justify;
        let first_indent = (look.first_indent * setting.scale).max(0.0);
        let pieces = runs(marks, body.clone());
        if pieces.is_empty() || body.is_empty() {
            let base = format(ctx, &look, &CharFormat::default(), setting, line);
            job.sections.push(LayoutSection {
                leading_space: first_indent,
                byte_range: ByteIndex(0)..ByteIndex(0),
                format: base,
            });
        } else {
            for (piece, run) in pieces {
                // A line break ends the row (egui breaks at a newline; one
                // character for one, so the caret's places stay the same).
                let mut chunk = text[piece.clone()].replace(sue_model::LINE_BREAK, "\n");
                if (run.small_caps || run.caps)
                    && chunk.chars().all(|ch| ch.to_uppercase().count() == 1)
                {
                    chunk = chunk.to_uppercase();
                }
                let start = job.text.len();
                job.text.push_str(&chunk);
                job.sections.push(LayoutSection {
                    leading_space: if start == 0 { first_indent } else { 0.0 },
                    byte_range: ByteIndex(start)..ByteIndex(job.text.len()),
                    format: format(ctx, &look, &run, setting, line),
                });
            }
        }
        let galley = lay_out(ctx, job);
        let mut rows: Vec<(f32, f32)> = galley
            .rows
            .iter()
            .map(|row| (row.min_y(), row.max_y()))
            .collect();
        if let Some((_, size)) = figure {
            rows = vec![(0.0, size.y)];
        }
        if let Some(table) = &table
            && !table.rows.is_empty()
        {
            rows.clone_from(&table.rows);
        }
        if rows.is_empty() {
            rows.push((0.0, natural));
        }
        let origin = match look.align {
            Align::Left | Align::Justify => indent,
            Align::Center => indent + width / 2.0,
            Align::Right => indent + width,
        };
        blocks.push(Block {
            doc,
            index,
            range: body,
            space: if table.is_some() {
                (
                    look.space_before.max(crate::tables::TABLE_SPACE) * setting.scale,
                    look.space_after.max(crate::tables::TABLE_SPACE) * setting.scale,
                )
            } else {
                (
                    look.space_before * setting.scale,
                    look.space_after * setting.scale,
                )
            },
            look,
            galley,
            origin,
            indent,
            width,
            label,
            figure,
            table,
            contents,
            rows,
        });
    }
    blocks
}

/// Places `blocks` on pages (or one column), filling in `laid`.
pub fn paginate(laid: &mut Laid, mode: ViewMode, page: &PageSetup, scale: f32, doc_gap: f32) {
    laid.pieces.clear();
    let (page_size, inset, text_height) = match mode {
        ViewMode::Page => (
            vec2(page.size[0], page.size[1]) * scale,
            vec2(page.margins[3], page.margins[0]) * scale,
            page.text_height() * scale,
        ),
        ViewMode::Draft { width } => (vec2(width * scale, 0.0), Vec2::ZERO, f32::INFINITY),
    };
    laid.page = page_size;
    laid.inset = inset;
    laid.text_width = match mode {
        ViewMode::Page => page.text_width() * scale,
        ViewMode::Draft { width } => width * scale,
    };
    let paged = matches!(mode, ViewMode::Page);
    let mut page_index = 0;
    let mut y = 0.0_f32;
    let mut last_doc = None;
    let count = laid.blocks.len();
    for index in 0..count {
        let block = &laid.blocks[index];
        if last_doc.is_some_and(|doc| doc != block.doc) && y > 0.0 {
            y += doc_gap;
        }
        last_doc = Some(block.doc);
        if paged && block.look.page_break && y > 0.0 {
            page_index += 1;
            y = 0.0;
        }
        if y > 0.0 {
            y += block.space.0;
        }
        let rows = block.rows.clone();
        let height = |from: usize, to: usize| rows[to - 1].1 - rows[from].0;
        // A table that runs onto another page repeats its heading rows
        // there first.
        let (repeat, headings) = block.table.as_ref().map_or((0.0, 0), |table| {
            if paged {
                (table.repeat_height(), table.header_rows)
            } else {
                (0.0, 0)
            }
        });
        // Where this page's rows of the block start (below any headings).
        let mut page_top = 0.0;
        // A heading stays with the first line after it.
        if paged && block.look.keep_with_next && y > 0.0 {
            let next_line = laid
                .blocks
                .get(index + 1)
                .map_or(0.0, |next| next.space.0 + (next.rows[0].1 - next.rows[0].0));
            if y + height(0, rows.len()) + next_line > text_height
                && height(0, rows.len()) + next_line <= text_height
            {
                page_index += 1;
                y = 0.0;
            }
        }
        let mut from = 0;
        while from < rows.len() {
            let mut to = from;
            while to < rows.len() && y + height(from, to + 1) <= text_height + 0.5 {
                to += 1;
            }
            if to == from {
                if y > page_top {
                    page_index += 1;
                    page_top = if from >= headings.max(1) { repeat } else { 0.0 };
                    y = page_top;
                    continue;
                }
                // Taller than a page: it goes on one anyway.
                to = from + 1;
            }
            // A lone first line at the foot of a page moves on with the rest.
            if paged && from == 0 && to == 1 && rows.len() > 2 && y > 0.0 {
                page_index += 1;
                y = 0.0;
                continue;
            }
            laid.pieces.push(Piece {
                block: index,
                rows: from..to,
                page: page_index,
                top: y,
                shift: rows[from].0,
            });
            y += height(from, to);
            from = to;
            if from < rows.len() {
                page_index += 1;
                page_top = if from >= headings.max(1) { repeat } else { 0.0 };
                y = page_top;
            }
        }
        y += laid.blocks[index].space.1;
    }
    laid.pages = page_index + 1;
    laid.text_height = if paged { text_height } else { y.max(1.0) };
    if !paged {
        laid.page.y = laid.text_height;
    }
}

/// The byte offset, in its document, of character `chars` of `block`.
#[must_use]
pub fn byte_of_char(text: &str, block: &Block, chars: usize) -> usize {
    let body = &text[block.range.clone()];
    block.range.start
        + body
            .char_indices()
            .nth(chars)
            .map_or(body.len(), |(byte, _)| byte)
}

/// The character index in `block` of byte `offset` of its document.
#[must_use]
pub fn char_of_byte(text: &str, block: &Block, offset: usize) -> usize {
    let offset = offset.clamp(block.range.start, block.range.end);
    text[block.range.start..offset].chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::Fonts;

    /// Sue's theme with each built-in preset's colors.
    fn presets() -> Vec<(String, Theme)> {
        let mut rice = sue_rice::RiceSchema::parse(sue_rice::DEFAULT_RICE).unwrap();
        sue_wm::register_rice(&mut rice).unwrap();
        let sue = Theme::from_rice(&rice);
        sue_rice::builtin_presets()
            .iter()
            .map(|preset| (preset.id.clone(), sue.with_preset(preset)))
            .collect()
    }

    /// The pages Sue shows: white paper, and dark paper in each theme's
    /// own colors (as Sue Writer's dark paper sets it).
    fn pages(theme: &Theme) -> [Ink; 2] {
        let dark = Ink {
            paper: theme.pill,
            text: theme.text,
            soft: theme.muted,
            link: theme.text,
            shade: theme.hover,
            colors: false,
        };
        [Ink::paper(), dark]
    }

    /// `color` laid over `paper`, opaque.
    fn over(paper: Color32, color: Color32) -> Color32 {
        let [r, g, b, a] = color.to_srgba_unmultiplied();
        mix(paper, Color32::from_rgb(r, g, b), f32::from(a) / 255.0)
    }

    #[test]
    fn every_mark_reads_on_every_page_of_every_theme() {
        let presets = presets();
        assert!(presets.len() >= 3);
        for (id, theme) in &presets {
            for ink in pages(theme) {
                let marks = PageMarks::resolve(theme, &ink);
                let paper = ink.paper;
                let at = format!("{id} on {paper:?}");
                // Text a tracked change touched is text: 4.5 to 1, and
                // deleted text, faded, 3 to 1.
                assert!(
                    contrast(marks.inserted, paper) >= TEXT_CONTRAST,
                    "inserted, {at}"
                );
                assert!(
                    contrast(marks.deleted, paper) >= EDGE_CONTRAST,
                    "deleted, {at}"
                );
                // Waves and handles are marks: 3 to 1.
                assert!(
                    contrast(marks.misspelt, paper) >= EDGE_CONTRAST,
                    "wave, {at}"
                );
                assert!(
                    contrast(marks.handle, paper) >= EDGE_CONTRAST,
                    "handle, {at}"
                );
                // Washes show on the paper, and the text still reads on
                // them.
                for (name, wash) in [
                    ("selection", marks.selection),
                    ("idle selection", marks.selection_idle),
                    ("comment", marks.comment),
                    ("note", marks.note),
                ] {
                    let ground = over(paper, wash);
                    assert_ne!(ground, paper, "{name} shows, {at}");
                    assert!(
                        contrast(ink.text, ground) >= TEXT_CONTRAST,
                        "text on the {name}, {at}"
                    );
                }
                // The marks that share a page stay apart.
                let selected = over(paper, marks.selection);
                assert!(
                    contrast(selected, paper) > contrast(over(paper, marks.comment), paper),
                    "the selection is stronger than a comment, {at}"
                );
                assert!(
                    contrast(over(paper, marks.comment), paper)
                        > contrast(over(paper, marks.note), paper),
                    "a comment is stronger than a note, {at}"
                );
                assert_ne!(marks.inserted, marks.deleted, "{at}");
            }
        }
    }

    #[test]
    fn a_monochrome_theme_gives_monochrome_marks() {
        // Sue's own themes leave danger, info and warning at the text
        // color: no hue enters the page.
        for (id, theme) in &presets() {
            let ink = Ink::paper();
            let marks = PageMarks::resolve(theme, &ink);
            for color in [
                marks.handle,
                marks.misspelt,
                marks.inserted,
                marks.deleted,
                marks.selection,
                marks.comment,
                marks.note,
            ] {
                assert!(
                    color.r() == color.g() && color.g() == color.b(),
                    "{id}: {color:?} is gray"
                );
            }
            assert_eq!(marks.inserted, ink.text, "{id}");
            assert_eq!(marks.deleted, ink.soft, "{id}");
        }
    }

    #[test]
    fn marks_follow_the_theme() {
        let (_, mut theme) = presets().remove(0);
        let ink = Ink::paper();
        let before = PageMarks::resolve(&theme, &ink);
        // A theme whose focus is a hue gets handles of that hue, darkened
        // only as far as white paper needs.
        theme.border_active = Color32::from_rgb(0x30, 0x60, 0xd0);
        theme.shadow_blur = 0.0;
        let after = PageMarks::resolve(&theme, &ink);
        assert_eq!(after.handle, theme.border_active);
        assert_ne!(after.handle, before.handle);
        // The page's shadow is the theme's: no blur in the rice, none on
        // the page.
        assert_eq!(after.page_shadow, theme.elevation(Elevation::Raised));
        assert_eq!(after.page_shadow.blur, 0);
        // A pale focus on white paper is darkened until it reads, and no
        // further.
        theme.border_active = Color32::from_rgb(0xd0, 0xe0, 0xff);
        let pale = PageMarks::resolve(&theme, &ink);
        assert!(contrast(pale.handle, ink.paper) >= EDGE_CONTRAST);
        assert_ne!(pale.handle, ink.text);
    }

    #[test]
    fn underlines_and_strikes_sit_on_the_text_with_wide_line_spacing() {
        let ctx = egui::Context::default();
        let under = Color32::from_rgb(1, 200, 3);
        let strike = Color32::from_rgb(200, 5, 6);
        let font = FontId::proportional(20.0);
        let mut job = LayoutJob::default();
        job.wrap.max_width = 120.0;
        for (text, underline, strikethrough) in [
            ("kept words that wrap ", Stroke::NONE, Stroke::NONE),
            ("added words ", Stroke::new(1.0, under), Stroke::NONE),
            ("gone words", Stroke::NONE, Stroke::new(1.0, strike)),
        ] {
            let start = job.text.len();
            job.text.push_str(text);
            job.sections.push(LayoutSection {
                leading_space: 0.0,
                byte_range: ByteIndex(start)..ByteIndex(job.text.len()),
                format: TextFormat {
                    font_id: font.clone(),
                    line_height: Some(60.0),
                    underline,
                    strikethrough,
                    ..TextFormat::default()
                },
            });
        }
        let mut galley = None;
        let mut full = ctx.run_ui(egui::RawInput::default(), |ui| {
            galley = Some(lay_out(ui.ctx(), job.clone()));
        });
        full.textures_delta.clear();
        let galley = galley.unwrap();
        assert!(galley.rows.len() > 2, "the text wraps");
        let mut seen = [false, false];
        for placed in &galley.rows {
            let Some(glyph) = placed.row.glyphs.first() else {
                continue;
            };
            let (base, ascent) = (glyph.pos.y, glyph.font_ascent);
            let descent = glyph.font_height - ascent;
            for vertex in &placed.row.visuals.mesh.vertices {
                if vertex.color == under {
                    seen[0] = true;
                    // Below the baseline, within the letters' descent:
                    // not down in the line spacing.
                    assert!(vertex.pos.y > base - 1.0, "{} under {base}", vertex.pos.y);
                    assert!(vertex.pos.y < base + descent + 1.0);
                }
                if vertex.color == strike {
                    seen[1] = true;
                    // Through the small letters, above the baseline.
                    assert!(vertex.pos.y < base - 2.0, "{} over {base}", vertex.pos.y);
                    assert!(vertex.pos.y > base - ascent * 0.6);
                }
            }
        }
        assert_eq!(seen, [true, true], "both lines are drawn");
        // The lines are counted, so the painter makes room for them.
        let counted: usize = galley
            .rows
            .iter()
            .map(|placed| placed.row.visuals.mesh.vertices.len())
            .sum();
        assert_eq!(counted, galley.num_vertices);
    }

    #[test]
    fn tracked_changes_and_comments_are_set_in_the_marks_colors() {
        let text = "kept added gone talked\n";
        let mut marks = TextMarks::new();
        let at = |word: &str| {
            let start = text.find(word).unwrap();
            start..start + word.len()
        };
        let change = sue_model::changes::Change::new("Ann", "2026-11-14").value();
        marks.set(at("added"), kind::INSERTED, Some(change.clone()));
        marks.set(at("gone"), kind::DELETED, Some(change));
        marks.set(
            at("talked"),
            kind::COMMENT,
            Some(Value::Entity(EntityId::new())),
        );
        let ctx = egui::Context::default();
        let ink = Ink::paper();
        let page_marks = PageMarks {
            inserted: Color32::from_rgb(1, 2, 3),
            deleted: Color32::from_rgb(4, 5, 6),
            comment: Color32::from_rgba_unmultiplied(7, 8, 9, 100),
            ..PageMarks::plain(&ink)
        };
        let mut sections = Vec::new();
        let mut full = ctx.run_ui(egui::RawInput::default(), |ui| {
            let sheet = StyleSheet::standard(&Fonts::default());
            let setting = Setting {
                sheet: &sheet,
                scale: 1.0,
                width: 400.0,
                ink,
                marks: page_marks,
            };
            let source = Source {
                text,
                marks: &marks,
                figure_size: &|_| None,
                tables: &|_| None,
                contents: &|_| None,
            };
            let blocks = lay_document(ui.ctx(), 0, &source, &setting);
            let job = &blocks[0].galley.job;
            sections = job
                .sections
                .iter()
                .map(|section| {
                    (
                        job.text[section.byte_range.start.0..section.byte_range.end.0].to_owned(),
                        section.format.color,
                        section.format.background,
                    )
                })
                .collect();
        });
        full.textures_delta.clear();
        let find = |word: &str| {
            sections
                .iter()
                .find(|(piece, _, _)| piece.contains(word))
                .map(|(_, color, background)| (*color, *background))
                .unwrap()
        };
        assert_eq!(find("kept").0, ink.text);
        assert_eq!(find("added").0, page_marks.inserted);
        assert_eq!(find("gone").0, page_marks.deleted);
        assert_eq!(find("talked").1, page_marks.comment);
    }

    fn laid(text: &str, marks: &TextMarks, mode: ViewMode, page: &PageSetup) -> Laid {
        laid_with(text, marks, mode, page, &[])
    }

    fn laid_with(
        text: &str,
        marks: &TextMarks,
        mode: ViewMode,
        page: &PageSetup,
        tables: &[(EntityId, TableSource)],
    ) -> Laid {
        let ctx = egui::Context::default();
        let mut out = Laid::default();
        let mut full = ctx.run_ui(egui::RawInput::default(), |ui| {
            let sheet = StyleSheet::standard(&Fonts::default());
            let setting = Setting {
                sheet: &sheet,
                scale: 1.0,
                width: page.text_width(),
                ink: Ink::paper(),
                marks: PageMarks::plain(&Ink::paper()),
            };
            let no_figures = |_: EntityId| None;
            let source = Source {
                text,
                marks,
                figure_size: &no_figures,
                tables: &|id| {
                    tables
                        .iter()
                        .find(|(known, _)| *known == id)
                        .map(|(_, table)| table.clone())
                },
                contents: &|_| None,
            };
            out.blocks = lay_document(ui.ctx(), 0, &source, &setting);
            out.doc_starts = vec![0];
            paginate(&mut out, mode, page, 1.0, 0.0);
        });
        full.textures_delta.clear();
        out
    }

    #[test]
    fn paragraphs_break_across_pages_between_lines() {
        let line = "word ".repeat(400);
        let text = format!("Heading\n{line}\n{line}\nEnd\n");
        let mut marks = TextMarks::new();
        marks.set_paragraphs(&text, 0..1, kind::STYLE, Some("heading1".into()));
        let page = PageSetup {
            size: [300.0, 300.0],
            margins: [20.0; 4],
            ..PageSetup::default()
        };
        // The footer numbers each page; a title page shows none.
        let footers = page.furniture(1, 3);
        assert_eq!(footers[0], None);
        assert_eq!(
            footers[1],
            Some([String::new(), "2".to_owned(), String::new()])
        );
        let titled = PageSetup {
            different_first: true,
            header: Running::parse("{title}\t\tPage {page} of {pages}"),
            title: "Book".to_owned(),
            ..page.clone()
        };
        assert_eq!(titled.furniture(0, 3), [None, None]);
        assert_eq!(
            titled.furniture(2, 3)[0],
            Some(["Book".to_owned(), String::new(), "Page 3 of 3".to_owned()])
        );
        let paged = laid(&text, &marks, ViewMode::Page, &page);
        assert_eq!(paged.blocks.len(), 4);
        assert!(paged.pages > 2, "{} pages", paged.pages);
        // No piece runs past the bottom of its page.
        for piece in &paged.pieces {
            let block = &paged.blocks[piece.block];
            let rows = &block.rows[piece.rows.clone()];
            let bottom = piece.top + rows.last().unwrap().1 - rows[0].0;
            assert!(
                bottom <= paged.text_height + 0.5,
                "{bottom} > {}",
                paged.text_height
            );
        }
        // A long paragraph is split, its rows in order.
        let split: Vec<&Piece> = paged
            .pieces
            .iter()
            .filter(|piece| piece.block == 1)
            .collect();
        assert!(split.len() > 1);
        assert_eq!(split[0].rows.start, 0);
        assert_eq!(split[1].rows.start, split[0].rows.end);
        let draft = laid(&text, &marks, ViewMode::Draft { width: 260.0 }, &page);
        assert_eq!(draft.pages, 1);
        assert_eq!(draft.pieces.len(), 4);
        // Character indices map back to bytes.
        let block = &paged.blocks[3];
        assert_eq!(byte_of_char(&text, block, 1), block.range.start + 1);
        assert_eq!(char_of_byte(&text, block, block.range.end), 3);
    }

    #[test]
    fn tables_break_between_rows_and_repeat_their_headings() {
        let id = EntityId::new();
        let mut cells = vec![vec!["Part".to_owned(), "Count".to_owned()]];
        for row in 0..60 {
            cells.push(vec![format!("Part {row}"), row.to_string()]);
        }
        let source = TableSource {
            cells,
            header_rows: 1,
            shares: vec![0.75, 0.25],
        };
        let text = format!("Before\n{}\nAfter\n", sue_model::OBJECT);
        let mut marks = TextMarks::new();
        let at = text.find(sue_model::OBJECT).unwrap();
        marks.set(
            at..at + sue_model::OBJECT.len_utf8(),
            kind::EMBED,
            Some(Value::Entity(id)),
        );
        let page = PageSetup {
            size: [300.0, 400.0],
            margins: [20.0; 4],
            ..PageSetup::default()
        };
        let paged = laid_with(&text, &marks, ViewMode::Page, &page, &[(id, source)]);
        let block = &paged.blocks[1];
        let table = block.table.as_ref().expect("a table block");
        assert_eq!(block.rows.len(), 61);
        assert_eq!(table.columns(), 2);
        assert!((table.edges[1] - table.edges[2] * 0.75).abs() < 0.5);
        let pieces: Vec<&Piece> = paged
            .pieces
            .iter()
            .filter(|piece| piece.block == 1)
            .collect();
        assert!(pieces.len() > 1, "the table runs onto more pages");
        // Later pages start below the repeated heading row.
        let heading = table.repeat_height();
        assert!(heading > 0.0);
        for piece in &pieces[1..] {
            assert!(
                (piece.top - heading).abs() < 0.01,
                "{} vs {heading}",
                piece.top
            );
            let rows = &block.rows[piece.rows.clone()];
            let bottom = piece.top + rows.last().unwrap().1 - rows[0].0;
            assert!(bottom <= paged.text_height + 0.5);
        }
        // Every row is placed once, in order.
        let placed: Vec<usize> = pieces.iter().flat_map(|piece| piece.rows.clone()).collect();
        assert_eq!(placed, (0..61).collect::<Vec<_>>());
        // A cell is hit by a point inside it.
        let rect = table.cell_rect(3, 1);
        let (row, column, offset) = table.hit(rect.center().to_vec2()).unwrap();
        assert_eq!((row, column), (3, 1));
        assert!(offset <= table.texts[3][1].len());
    }
}
