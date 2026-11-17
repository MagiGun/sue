//! Register Material, Template, and View, Webtoon Display Settings.

use egui::Ui;
use sue_wm::ui::{dropdown, form_grid, text_field};
use sue_wm::{AppletContext, Theme};

use super::fill::check;
use super::{Outcome, bare, note, tip};
use crate::studio::SuePaint;

/// Register Material, Template: the template's name.
pub(super) fn template_ui(ui: &mut Ui, name: &mut String, theme: &Theme) -> Outcome {
    form_grid(ui, "sue-paint-template", theme, |form| {
        let label = bare(tr!("template-name-label"));
        form.row(&label, |ui| {
            let response = text_field("sue-paint-template-name", name)
                .label(&label)
                .show(ui, theme);
            tip(response, &label, "", theme);
        });
    });
    note(ui, &tr!("canvas-layers-become-a-template"), theme);
    Outcome::Open
}

/// Show the screen guides of a webtoon (`Bool`).
pub(crate) const WEBTOON_GUIDES: &str = "sue_paint.webtoon_guides";
/// The screen a webtoon is read on, which sets how tall one screen is
/// for the canvas's width (`EnumChoice` of [`SCREENS`]' ids).
pub(crate) const WEBTOON_SCREEN: &str = "sue_paint.webtoon_screen";

/// The screens a webtoon is read on: their id, their label's key and
/// their shape (width over height).
pub(crate) const SCREENS: [(&str, &str, f32); 4] = [
    ("tall-phone", "webtoon-screen-tall-phone", 9.0 / 19.5),
    ("phone", "webtoon-screen-phone", 9.0 / 16.0),
    ("tablet", "webtoon-screen-tablet", 3.0 / 4.0),
    ("desktop", "webtoon-screen-desktop", 16.0 / 10.0),
];

/// View, Webtoon Display Settings: whether the canvas shows where each
/// screen of a reader's device ends, which device, and whether the view
/// fits the canvas's width at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WebtoonDisplay {
    pub guides: bool,
    pub screen: usize,
    pub fit_width: bool,
}

impl WebtoonDisplay {
    /// The settings as Settings holds them.
    pub(crate) fn of(rice: &sue_rice::RiceSchema) -> Self {
        let screen = rice
            .get_str(WEBTOON_SCREEN)
            .and_then(|id| SCREENS.iter().position(|(of, ..)| *of == id))
            .unwrap_or(0);
        Self {
            guides: rice.get_bool(WEBTOON_GUIDES).unwrap_or(false),
            screen,
            fit_width: true,
        }
    }
}

/// How tall one screen of `screen` (an id of [`SCREENS`]) is on a canvas
/// `width` pixels wide.
pub(crate) fn screen_height(screen: Option<&str>, width: u32) -> f32 {
    let shape = SCREENS
        .iter()
        .find(|(id, ..)| Some(*id) == screen)
        .map_or(SCREENS[0].2, |(_, _, shape)| *shape);
    width as f32 / shape
}

/// The body of View, Webtoon Display Settings.
pub(super) fn webtoon_display_ui(
    ui: &mut Ui,
    display: &mut WebtoonDisplay,
    theme: &Theme,
) -> Outcome {
    form_grid(ui, "sue-paint-webtoon-display", theme, |form| {
        form.wide(|ui| {
            check(
                ui,
                "sue-paint-webtoon-guides",
                &mut display.guides,
                &tr!("webtoon-show-screen-guides"),
                &tr!("webtoon-show-screen-guides-hint"),
                theme,
            );
        });
        let label = bare(tr!("webtoon-read-on"));
        form.row(&label, |ui| {
            let names: Vec<String> = SCREENS
                .iter()
                .map(|(_, key, _)| sue_lang::text("paint", key))
                .collect();
            let response = dropdown("sue-paint-webtoon-screen", &mut display.screen, &names)
                .label(&label)
                .show(ui, theme);
            tip(response, &label, "", theme);
        });
        form.wide(|ui| {
            check(
                ui,
                "sue-paint-webtoon-fit",
                &mut display.fit_width,
                &tr!("webtoon-fit-width"),
                "",
                theme,
            );
        });
    });
    note(ui, &tr!("webtoon-display-note"), theme);
    Outcome::Open
}

impl SuePaint {
    /// View, Webtoon Display Settings: opens its dialog.
    pub(crate) fn open_webtoon_display(&mut self, cx: &AppletContext<'_>) {
        self.dialog = Some(super::Dialog::WebtoonDisplay(WebtoonDisplay::of(cx.rice())));
    }

    /// OK in Webtoon Display Settings: the settings are kept (and undone
    /// like any setting), and the view fits the canvas's width when
    /// asked.
    pub(super) fn webtoon_display_ok(
        &mut self,
        cx: &mut AppletContext<'_>,
        display: WebtoonDisplay,
    ) {
        let screen = SCREENS[display.screen.min(SCREENS.len() - 1)].0;
        for (key, value) in [
            (WEBTOON_GUIDES, sue_rice::RiceValue::Bool(display.guides)),
            (WEBTOON_SCREEN, sue_rice::RiceValue::Text(screen.to_owned())),
        ] {
            let same = match &value {
                sue_rice::RiceValue::Bool(on) => cx.rice().get_bool(key) == Some(*on),
                sue_rice::RiceValue::Text(text) => cx.rice().get_str(key) == Some(text.as_str()),
                _ => false,
            };
            if !same && let Err(error) = cx.edit_rice(sue_rice::SetRiceValue::new(key, value)) {
                cx.fail(error.to_string());
            }
        }
        if display.fit_width {
            self.fit_width(cx);
        }
    }

    /// The canvas as wide as the window, its top at the window's top, as
    /// a webtoon is read.
    pub(crate) fn fit_width(&mut self, cx: &AppletContext<'_>) {
        self.with_view(cx, |view, rect, size| {
            let range = crate::view::ZOOM_RANGE;
            view.rotation = 0.0;
            view.pan = egui::Vec2::ZERO;
            view.zoom =
                ((rect.width() - 48.0).max(16.0) / size[0].max(1) as f32).clamp(range.0, range.1);
            let top = egui::pos2(size[0] as f32 / 2.0, 0.0);
            let target = egui::pos2(rect.center().x, rect.top() + 24.0);
            view.pan += target - view.to_screen(rect, size, top);
            view.fitted = true;
        });
    }
}
