//! # sue-type: typography for every studio that sets text
//!
//! Sue owns its text features once, and studios use them: a writing
//! studio, notes, slides, a page layout studio and a font studio all edit
//! and set rich text the same way.
//!
//! - [`style`]: paragraph styles and style sheets (standard, novel
//!   manuscript, screenplay), and how a paragraph and its characters look
//!   from their marks.
//! - [`layout`]: paragraphs shaped into egui galleys with the installed
//!   fonts (`sue_wm::fonts`), stacked into one column or broken across
//!   printed pages.
//! - [`editor`]: the rich text editor, on one document or several in a
//!   row, as pages or a column, with its own caret, selection, clipboard,
//!   input methods and lists.
//! - [`caret`]: how that caret moves: it glides to its new place instead of
//!   jumping, and blinks only at rest.
//! - [`tables`]: tables in running text, on the shared `Table` model:
//!   laid out, broken across pages, painted and typed in.
//! - [`contents`]: tables of contents made from the headings, with pages.
//! - [`case`]: changing text to capitals, small letters or sentence case.
//! - [`spell`]: spelling: which words are wrong, and what was meant.
//! - [`find`]: finding and replacing, by plain text or regular expression.
//! - [`edit`]: the edits it makes, in place in the arena with their undo.
//! - [`typeset`]: the same text set on pages for print, from the font files
//!   themselves, which the PDF writer in `sue-io` draws.
//!
//! The text lives where it always does (backbone rule 1): a `StringBuffer`
//! with [`sue_model::TextMarks`] on an entity of the arena.

/// Text from the language database's `type` namespace (the shared text editor).
macro_rules! tr {
    ($($arg:tt)*) => { sue_lang::tr!("type", $($arg)*) };
}

pub mod caret;
pub mod case;
pub mod contents;
pub mod edit;
pub mod editor;
pub mod find;
pub mod layout;
pub mod raster;
pub mod spell;
pub mod style;
pub mod tables;
pub mod typeset;

pub use editor::{Caret, Editor, EditorOutput, EditorView, OutlineEntry};
pub use layout::{Ink, PX_PER_PT, PageSetup, ViewMode};
pub use style::{
    Align, BODY, CharFormat, Fonts, ListKind, ParagraphLook, ParagraphStyle, SHEETS, StyleSheet,
    sheet_name,
};
