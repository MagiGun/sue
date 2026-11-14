//! The story's dialogs: the Page Manager, Change Page Settings and
//! Change Story Information.

/// The Page Manager: the story's pages as spreads, one chosen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PagesDialog {
    pub chosen: usize,
}

/// Change Page Settings: every page's size, resolution and borders.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PageSettings {
    pub story: sue_core::EntityId,
    pub width: u32,
    pub height: u32,
    pub dpi: f64,
    pub bleed: f64,
    pub inner: f64,
}

/// Change Story Information.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StoryInfo {
    pub story: sue_core::EntityId,
    pub title: String,
    pub author: String,
    pub first: i64,
    pub right_bound: bool,
    /// The first page faces the second (Clip Studio Paint's start page).
    pub paired: bool,
    /// The page numbers.
    pub folio: crate::story::Folio,
}
