//! Tables of contents in running text ([`sue_model::contents`]): the
//! headings of what is shown, each with its page and a dotted leader,
//! laid out as a block whose rows are its entries (so it breaks across
//! pages like a table), made fresh at every layout. A press on an entry
//! goes to its heading.

use std::sync::Arc;

use egui::text::LayoutJob;
use egui::{Color32, Painter, Pos2, pos2};
use sue_core::EntityId;
use sue_model::contents::Contents;

use crate::layout::{Block, Setting, format};
use crate::style::{CharFormat, ParagraphLook};
use crate::tables::TableBlock;

/// How far each heading level is set in, in points.
const LEVEL_INDENT: f32 = 18.0;

/// Where a table of contents' entries lead, and how far each is set in.
#[derive(Clone, Debug, PartialEq)]
pub struct Listing {
    /// Each row's indent in pixels.
    pub indents: Vec<f32>,
    /// Each row's heading: the block it is (none for the title row).
    pub targets: Vec<Option<usize>>,
}

/// One heading a table of contents lists.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// 1 for the biggest headings.
    pub level: u8,
    /// Its text.
    pub title: String,
    /// Its page, from 1, once the pages are known.
    pub page: Option<usize>,
    /// The heading's block.
    pub block: usize,
}

/// The headings of `blocks` (laid out from `texts`) that `contents` lists,
/// with their pages from `pages` (each block's page, from 0) when known.
#[must_use]
pub fn entries(
    blocks: &[Block],
    texts: &[String],
    contents: &Contents,
    pages: Option<&[usize]>,
) -> Vec<Entry> {
    blocks
        .iter()
        .enumerate()
        .filter_map(|(index, block)| {
            let level = block.look.outline?;
            if !contents.lists(level) {
                return None;
            }
            let title = texts
                .get(block.doc)?
                .get(block.range.clone())?
                .trim()
                .to_owned();
            if title.is_empty() {
                return None;
            }
            Some(Entry {
                level,
                title,
                page: pages
                    .and_then(|pages| pages.get(index))
                    .map(|page| page + 1),
                block: index,
            })
        })
        .collect()
}

/// Lays out a table of contents across `width` pixels: its title, then a
/// row for each entry, set in by its level, its page at the right.
#[must_use]
pub fn lay_contents(
    ctx: &egui::Context,
    entity: EntityId,
    contents: &Contents,
    entries: &[Entry],
    look: &ParagraphLook,
    setting: &Setting<'_>,
    width: f32,
) -> TableBlock {
    let pad = 2.0 * setting.scale;
    let page_column = (40.0 * setting.scale).min(width / 3.0);
    let edges = vec![0.0, width - page_column, width];
    let plain = format(ctx, look, &CharFormat::default(), setting, None);
    let natural = ctx.fonts_mut(|fonts| fonts.row_height(&plain.font_id));
    let mut rows = Vec::new();
    let mut cells = Vec::new();
    let mut texts = Vec::new();
    let mut indents = Vec::new();
    let mut targets = Vec::new();
    let mut y = 0.0;
    let mut push = |cells: &mut Vec<Vec<Arc<egui::Galley>>>,
                    title: &str,
                    page: &str,
                    run: &CharFormat,
                    indent: f32| {
        let line = Some(natural * look.line_spacing.min(crate::tables::CELL_SPACING));
        let mut job =
            LayoutJob::single_section(title.to_owned(), format(ctx, look, run, setting, line));
        job.wrap.max_width = (edges[1] - indent - pad * 2.0).max(8.0);
        let name = ctx.fonts_mut(|fonts| fonts.layout_job(job));
        let number = ctx.fonts_mut(|fonts| {
            fonts.layout_job(LayoutJob::single_section(
                page.to_owned(),
                format(ctx, look, &CharFormat::default(), setting, line),
            ))
        });
        let height = name.size().y.max(number.size().y).max(natural) + pad * 2.0;
        rows.push((y, y + height));
        y += height;
        cells.push(vec![name, number]);
        texts.push(vec![title.to_owned(), page.to_owned()]);
        indents.push(indent);
    };
    if !contents.title.trim().is_empty() {
        let title = CharFormat {
            bold: true,
            size: Some(look.size * 1.3),
            ..CharFormat::default()
        };
        push(&mut cells, contents.title.trim(), "", &title, 0.0);
        targets.push(None);
    }
    for entry in entries {
        let run = CharFormat {
            bold: entry.level == 1,
            ..CharFormat::default()
        };
        let page = entry.page.map(|page| page.to_string()).unwrap_or_default();
        let indent = f32::from(entry.level.saturating_sub(1)) * LEVEL_INDENT * setting.scale;
        push(&mut cells, &entry.title, &page, &run, indent);
        targets.push(Some(entry.block));
    }
    TableBlock {
        entity,
        edges,
        rows,
        cells,
        texts,
        header_rows: 0,
        pad,
        listing: Some(Listing { indents, targets }),
    }
}

/// Fills in every table of contents among `blocks` (laid out from
/// `texts`), with the pages from `pages` when known.
pub fn fill(
    ctx: &egui::Context,
    blocks: &mut [Block],
    texts: &[String],
    setting: &Setting<'_>,
    pages: Option<&[usize]>,
) {
    let listed: Vec<usize> = (0..blocks.len())
        .filter(|index| blocks[*index].contents.is_some())
        .collect();
    for index in listed {
        let Some((entity, contents)) = blocks[index].contents.clone() else {
            continue;
        };
        let found = entries(blocks, texts, &contents, pages);
        let block = &blocks[index];
        let table = lay_contents(
            ctx,
            entity,
            &contents,
            &found,
            &block.look,
            setting,
            block.width,
        );
        let block = &mut blocks[index];
        if table.rows.is_empty() {
            block.rows = vec![(0.0, 1.0)];
        } else {
            block.rows.clone_from(&table.rows);
        }
        block.table = Some(Arc::new(table));
    }
}

/// Paints `rows` of a table of contents with its top left at `origin`:
/// each entry set in by its level, a dotted leader to its page number at
/// the right.
pub fn paint_contents(
    painter: &Painter,
    table: &TableBlock,
    rows: std::ops::Range<usize>,
    origin: Pos2,
    text: Color32,
) {
    let Some(listing) = &table.listing else {
        return;
    };
    let dot = text.gamma_multiply(0.6);
    for row in rows {
        let (top, bottom) = table.rows[row];
        let y = origin.y + top + table.pad;
        let indent = listing.indents.get(row).copied().unwrap_or(0.0);
        let name = &table.cells[row][0];
        let number = &table.cells[row][1];
        let name_at = pos2(origin.x + indent + table.pad, y);
        painter.galley(name_at, name.clone(), text);
        let right = origin.x + table.edges[2] - table.pad;
        let number_at = pos2(right - number.size().x, y);
        painter.galley(number_at, number.clone(), text);
        if number.size().x > 0.0 {
            // The leader, from the end of the title's last line.
            let last = name.rows.last().map_or(0.0, |line| line.rect().max.x);
            let baseline = origin.y + bottom - table.pad - 3.0;
            let step = 5.0_f32.max(table.pad * 2.0);
            let mut x = name_at.x + last + step;
            while x < number_at.x - step * 0.5 {
                painter.circle_filled(pos2(x, baseline), 0.8, dot);
                x += step;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{Ink, Source, lay_document};
    use crate::style::{Fonts, StyleSheet};
    use sue_core::Value;
    use sue_model::{TextMarks, mark_kind as kind};

    #[test]
    fn contents_list_the_headings_with_their_pages() {
        let ctx = egui::Context::default();
        let id = EntityId::new();
        let text = format!("{}\nOne\nBody\nTwo\nThree\n", sue_model::OBJECT);
        let mut marks = TextMarks::new();
        marks.set(
            0..sue_model::OBJECT.len_utf8(),
            kind::EMBED,
            Some(Value::Entity(id)),
        );
        let at = |word: &str| text.find(word).unwrap();
        marks.set_paragraphs(
            &text,
            at("One")..at("One"),
            kind::STYLE,
            Some("heading1".into()),
        );
        marks.set_paragraphs(
            &text,
            at("Two")..at("Two"),
            kind::STYLE,
            Some("heading2".into()),
        );
        marks.set_paragraphs(
            &text,
            at("Three")..at("Three"),
            kind::STYLE,
            Some("heading3".into()),
        );
        let mut blocks = Vec::new();
        let mut full = ctx.run_ui(egui::RawInput::default(), |ui| {
            let sheet = StyleSheet::standard(&Fonts::default());
            let setting = Setting {
                sheet: &sheet,
                scale: 1.0,
                width: 400.0,
                ink: Ink::paper(),
            };
            let contents = Contents {
                title: "Contents".to_owned(),
                levels: 2,
            };
            let source = Source {
                text: &text,
                marks: &marks,
                figure_size: &|_| None,
                tables: &|_| None,
                contents: &|known| (known == id).then(|| contents.clone()),
            };
            blocks = lay_document(ui.ctx(), 0, &source, &setting);
            let pages: Vec<usize> = (0..blocks.len()).collect();
            fill(
                ui.ctx(),
                &mut blocks,
                std::slice::from_ref(&text),
                &setting,
                Some(&pages),
            );
        });
        full.textures_delta.clear();
        let table = blocks[0].table.as_ref().expect("the contents are laid out");
        // Its title, then the two levels it lists (not the third).
        let rows: Vec<Vec<String>> = table.texts.clone();
        assert_eq!(
            rows,
            vec![
                vec!["Contents".to_owned(), String::new()],
                vec!["One".to_owned(), "2".to_owned()],
                vec!["Two".to_owned(), "4".to_owned()],
            ]
        );
        let listing = table.listing.as_ref().unwrap();
        assert_eq!(listing.targets, vec![None, Some(1), Some(3)]);
        assert!(listing.indents[2] > listing.indents[1]);
        assert_eq!(blocks[0].rows.len(), 3);
    }
}
