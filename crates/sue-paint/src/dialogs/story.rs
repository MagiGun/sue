//! The story's dialogs: 3D Preview for Binding, Change Page Settings,
//! Change Story Information, the Story Editor and Export Webtoon.

use egui::{RichText, Ui, vec2};
use sue_core::EntityId;
use sue_wm::{AppletContext, Theme};

use super::{Outcome, label, ok_cancel};
use crate::studio::SuePaint;

/// 3D Preview for Binding: the story as a bound book (the Page Manager
/// itself is a view of the canvas area, `pages.rs`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PagesDialog {
    /// The spread open, by its index among the book's spreads.
    pub spread: usize,
    /// How near the book is, 1 fitting it to the window.
    pub zoom: f32,
}

impl PagesDialog {
    /// The book open at spread `spread`, fitted to the window.
    pub(crate) const fn at(spread: usize) -> Self {
        Self { spread, zoom: 1.0 }
    }
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

/// Change Page Settings.
pub(super) fn page_settings_ui(ui: &mut Ui, settings: &mut PageSettings, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    ui.horizontal(|ui| {
        label(ui, &tr!("size"), theme);
        ui.add(
            egui::DragValue::new(&mut settings.width)
                .range(1..=10_000)
                .suffix(tr!("px")),
        );
        ui.label("by");
        ui.add(
            egui::DragValue::new(&mut settings.height)
                .range(1..=10_000)
                .suffix(tr!("px")),
        );
    });
    ui.horizontal(|ui| {
        label(ui, &tr!("resolution"), theme);
        ui.add(
            egui::DragValue::new(&mut settings.dpi)
                .range(1.0..=2400.0)
                .suffix(tr!("dpi")),
        );
    });
    ui.horizontal(|ui| {
        label(ui, &tr!("bleed"), theme);
        ui.add(
            egui::DragValue::new(&mut settings.bleed)
                .range(0.0..=50.0)
                .suffix(tr!("mm")),
        );
    });
    ui.horizontal(|ui| {
        label(ui, &tr!("inner-border"), theme);
        ui.add(
            egui::DragValue::new(&mut settings.inner)
                .range(0.0..=100.0)
                .suffix(tr!("mm")),
        );
    });
    ui.label(
        RichText::new(tr!("every-page-story-changes-each-keeps"))
            .size(theme.small_size)
            .color(theme.muted),
    );
    ok_cancel(ui, &mut outcome);
    outcome
}

/// Change Story Information, with the folio.
pub(super) fn story_info_ui(ui: &mut Ui, info: &mut StoryInfo, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    ui.horizontal(|ui| {
        label(ui, &tr!("title"), theme);
        ui.text_edit_singleline(&mut info.title);
    });
    ui.horizontal(|ui| {
        label(ui, &tr!("author"), theme);
        ui.text_edit_singleline(&mut info.author);
    });
    ui.horizontal(|ui| {
        label(ui, &tr!("first-page-number"), theme);
        ui.add(egui::DragValue::new(&mut info.first).range(0..=9_999));
    });
    ui.horizontal(|ui| {
        label(ui, &tr!("binding"), theme);
        ui.radio_value(&mut info.right_bound, false, tr!("left-left-to-right"));
        ui.radio_value(&mut info.right_bound, true, tr!("right-manga"));
    });
    ui.horizontal(|ui| {
        label(ui, &tr!("first-page-place"), theme);
        ui.radio_value(&mut info.paired, false, tr!("start-page-alone"));
        ui.radio_value(&mut info.paired, true, tr!("start-page-paired"));
    });
    ui.separator();
    // Clip Studio Paint's folio: page numbers on the pages.
    ui.checkbox(&mut info.folio.show, tr!("show-folio"));
    ui.add_enabled_ui(info.folio.show, |ui| {
        ui.horizontal(|ui| {
            label(ui, &tr!("size-2"), theme);
            ui.add(
                egui::DragValue::new(&mut info.folio.size)
                    .range(4.0..=72.0)
                    .suffix(" pt"),
            );
        });
        ui.horizontal(|ui| {
            label(ui, &tr!("folio-position"), theme);
            ui.radio_value(&mut info.folio.center, false, tr!("folio-outside"));
            ui.radio_value(&mut info.folio.center, true, tr!("folio-center"));
        });
    });
    ok_cancel(ui, &mut outcome);
    outcome
}

/// The Story Editor: every text layer's words, page by page.
pub(super) fn story_editor_ui(
    ui: &mut Ui,
    texts: &mut [(usize, EntityId, EntityId, String)],
    size: egui::Vec2,
    theme: &Theme,
) -> Outcome {
    let mut outcome = Outcome::Open;
    egui::ScrollArea::vertical()
        .max_height(size.y - 120.0)
        .show(ui, |ui| {
            let mut last_page = 0;
            for (number, _, _, words) in texts.iter_mut() {
                if *number != last_page {
                    last_page = *number;
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(tr!("page", number = number))
                            .color(theme.text)
                            .strong(),
                    );
                }
                ui.add(
                    egui::TextEdit::multiline(words)
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                );
            }
        });
    ok_cancel(ui, &mut outcome);
    outcome
}

/// Export Webtoon: where it is cut and how wide the pieces are.
pub(super) fn webtoon_ui(ui: &mut Ui, height: &mut u32, width: &mut u32, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    egui::Grid::new("sue-paint-webtoon")
        .num_columns(2)
        .spacing(vec2(12.0, 8.0))
        .show(ui, |ui| {
            label(ui, &tr!("split-every"), theme);
            ui.add(
                egui::DragValue::new(height)
                    .range(200..=20000)
                    .suffix(tr!("px")),
            );
            ui.end_row();
            label(ui, &tr!("output-width"), theme);
            ui.add(
                egui::DragValue::new(width)
                    .range(100..=4000)
                    .suffix(tr!("px")),
            );
            ui.end_row();
        });
    ui.label(
        RichText::new(tr!("tall-canvas-cut-into-numbered-pictures"))
            .size(theme.small_size)
            .color(theme.muted),
    );
    ok_cancel(ui, &mut outcome);
    outcome
}

impl SuePaint {
    /// Changes every page of the story.
    pub(super) fn page_settings_ok(&mut self, cx: &mut AppletContext<'_>, settings: &PageSettings) {
        match crate::story::page_settings(
            &cx.world.arena,
            settings.story,
            [settings.width.max(1), settings.height.max(1)],
            settings.dpi,
            settings.bleed,
            settings.inner,
        ) {
            Ok(edit) => {
                Self::apply(cx, edit);
                for doc in &mut self.docs {
                    doc.view.fitted = false;
                }
            }
            Err(error) => cx.fail(error),
        }
    }

    /// Keeps the story's information as one undo step.
    pub(super) fn story_info_ok(cx: &mut AppletContext<'_>, info: &StoryInfo) {
        let mut edit = crate::doc::Edit::new(tr!("change-story-information"))
            .then(sue_core::txn::SetProperty::new(
                info.story,
                crate::story::AUTHOR,
                info.author.clone(),
            ))
            .then(sue_core::txn::SetProperty::new(
                info.story,
                crate::story::FIRST_NUMBER,
                info.first,
            ))
            .then(sue_core::txn::SetProperty::new(
                info.story,
                crate::story::BINDING,
                if info.right_bound { "right" } else { "left" },
            ))
            .then(sue_core::txn::SetProperty::new(
                info.story,
                crate::story::START,
                if info.paired { "paired" } else { "alone" },
            ))
            .then(sue_core::txn::SetProperty::new(
                info.story,
                crate::story::FOLIO,
                info.folio.show,
            ))
            .then(sue_core::txn::SetProperty::new(
                info.story,
                crate::story::FOLIO_SIZE,
                f64::from(info.folio.size),
            ))
            .then(sue_core::txn::SetProperty::new(
                info.story,
                crate::story::FOLIO_PLACE,
                if info.folio.center {
                    "center"
                } else {
                    "outside"
                },
            ));
        if !info.title.trim().is_empty() {
            edit = edit.then(sue_core::txn::Rename::new(info.story, info.title.trim()));
        }
        Self::apply(cx, edit);
    }

    /// Puts the words that changed back on their layers, as one undo step.
    pub(super) fn story_editor_ok(
        cx: &mut AppletContext<'_>,
        texts: Vec<(usize, EntityId, EntityId, String)>,
    ) {
        let mut edit = crate::doc::Edit::new(tr!("story-editor"));
        let mut changed = false;
        for (_, page, layer, words) in texts {
            let arena = &cx.world.arena;
            let same = arena
                .get_by_id(layer)
                .ok()
                .and_then(crate::lettering::Lettering::of)
                .is_some_and(|lettering| lettering.content == words);
            if same {
                continue;
            }
            let Some([width, height]) = arena
                .get_by_id(page)
                .ok()
                .and_then(sue_model::picture::size)
            else {
                continue;
            };
            if let Some(step) = crate::lettering::retext(arena, layer, &words, width, height) {
                edit = edit.then(step);
                changed = true;
            }
        }
        if changed {
            Self::apply(cx, edit);
        }
    }
}
