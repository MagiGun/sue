//! The story's dialogs: 3D Preview for Binding, Change Page Settings,
//! Change Story Information, the Story Editor and Export Webtoon.

use egui::{RichText, Ui};
use sue_core::EntityId;
use sue_wm::{AppletContext, Theme};

use super::{Outcome, ok_cancel};
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

/// A whole number field over `value`, from `least` to `most`, with
/// `suffix` after it.
fn whole_field(
    ui: &mut Ui,
    salt: &str,
    value: &mut f64,
    (least, most): (f64, f64),
    suffix: &str,
    theme: &Theme,
) {
    sue_wm::ui::number_field(("sue-paint-story", salt), value)
        .range(least..=most)
        .suffix(suffix)
        .show(ui, theme);
    *value = value.round();
}

/// A number field over `value` with `decimals` places.
fn decimal_field(
    ui: &mut Ui,
    salt: &str,
    value: &mut f64,
    (least, most, decimals): (f64, f64, usize),
    suffix: &str,
    theme: &Theme,
) {
    sue_wm::ui::number_field(("sue-paint-story", salt), value)
        .range(least..=most)
        .decimals(decimals)
        .step(0.5)
        .suffix(suffix)
        .show(ui, theme);
}

/// A one-of-two choice: `on` holds the second option.
fn either(ui: &mut Ui, salt: &str, options: [String; 2], on: &mut bool, theme: &Theme) {
    let picked = sue_wm::ui::segmented(("sue-paint-story", salt), &options, Some(usize::from(*on)))
        .show(ui, theme)
        .picked;
    if let Some(picked) = picked {
        *on = picked == 1;
    }
}

/// A line of help under a form.
fn note(ui: &mut Ui, text: &str, theme: &Theme) {
    ui.label(
        RichText::new(text)
            .size(theme.small_size)
            .color(theme.text_muted()),
    );
}

/// Change Page Settings.
pub(super) fn page_settings_ui(ui: &mut Ui, settings: &mut PageSettings, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    let (px, dpi, mm) = (tr!("px"), tr!("dpi"), tr!("mm"));
    sue_wm::ui::form_grid(ui, "sue-paint-page-settings", theme, |form| {
        form.row(&tr!("width"), |ui| {
            let mut width = f64::from(settings.width);
            whole_field(ui, "width", &mut width, (1.0, 10_000.0), &px, theme);
            settings.width = width as u32;
        });
        form.row(&tr!("height"), |ui| {
            let mut height = f64::from(settings.height);
            whole_field(ui, "height", &mut height, (1.0, 10_000.0), &px, theme);
            settings.height = height as u32;
        });
        form.row(&tr!("resolution"), |ui| {
            decimal_field(ui, "dpi", &mut settings.dpi, (1.0, 2400.0, 0), &dpi, theme);
        });
        form.row(&tr!("bleed"), |ui| {
            decimal_field(ui, "bleed", &mut settings.bleed, (0.0, 50.0, 1), &mm, theme);
        });
        form.row(&tr!("inner-border"), |ui| {
            decimal_field(
                ui,
                "inner",
                &mut settings.inner,
                (0.0, 100.0, 1),
                &mm,
                theme,
            );
        });
    });
    note(ui, &tr!("every-page-story-changes-each-keeps"), theme);
    ok_cancel(ui, &mut outcome);
    outcome
}

/// Change Story Information, with the folio.
pub(super) fn story_info_ui(ui: &mut Ui, info: &mut StoryInfo, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    sue_wm::ui::form_grid(ui, "sue-paint-story-info", theme, |form| {
        form.row(&tr!("title"), |ui| {
            sue_wm::ui::text_field("sue-paint-story-title", &mut info.title).show(ui, theme);
        });
        form.row(&tr!("author"), |ui| {
            sue_wm::ui::text_field("sue-paint-story-author", &mut info.author).show(ui, theme);
        });
        form.row(&tr!("first-page-number"), |ui| {
            let mut first = info.first as f64;
            whole_field(ui, "first", &mut first, (0.0, 9_999.0), "", theme);
            info.first = first as i64;
        });
        form.row(&tr!("binding"), |ui| {
            either(
                ui,
                "binding",
                [tr!("left-left-to-right"), tr!("right-manga")],
                &mut info.right_bound,
                theme,
            );
        });
        form.row(&tr!("first-page-place"), |ui| {
            either(
                ui,
                "start",
                [tr!("start-page-alone"), tr!("start-page-paired")],
                &mut info.paired,
                theme,
            );
        });
        // Clip Studio Paint's folio: page numbers on the pages.
        form.row("", |ui| {
            if sue_wm::ui::checkbox("sue-paint-story-folio", &tr!("show-folio"), info.folio.show)
                .show(ui, theme)
                .clicked()
            {
                info.folio.show = !info.folio.show;
            }
        });
        let show = info.folio.show;
        form.row(&tr!("size-2"), |ui| {
            ui.add_enabled_ui(show, |ui| {
                let mut size = f64::from(info.folio.size);
                decimal_field(ui, "folio-size", &mut size, (4.0, 72.0, 1), " pt", theme);
                info.folio.size = size as f32;
            });
        });
        form.row(&tr!("folio-position"), |ui| {
            ui.add_enabled_ui(show, |ui| {
                either(
                    ui,
                    "folio-place",
                    [tr!("folio-outside"), tr!("folio-center")],
                    &mut info.folio.center,
                    theme,
                );
            });
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
    // Room under the pages for OK and Cancel.
    let footer = theme.space(sue_wm::ui::Space::S9) * 2.0;
    egui::ScrollArea::vertical()
        .max_height(size.y - footer)
        .show(ui, |ui| {
            let mut last_page = 0;
            for (number, _, _, words) in texts.iter_mut() {
                if *number != last_page {
                    last_page = *number;
                    ui.add_space(theme.space(sue_wm::ui::Space::S2));
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
    let px = tr!("px");
    sue_wm::ui::form_grid(ui, "sue-paint-webtoon", theme, |form| {
        form.row(&tr!("split-every"), |ui| {
            let mut split = f64::from(*height);
            whole_field(ui, "split", &mut split, (200.0, 20_000.0), &px, theme);
            *height = split as u32;
        });
        form.row(&tr!("output-width"), |ui| {
            let mut wide = f64::from(*width);
            whole_field(ui, "webtoon-width", &mut wide, (100.0, 4000.0), &px, theme);
            *width = wide as u32;
        });
    });
    note(ui, &tr!("tall-canvas-cut-into-numbered-pictures"), theme);
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
