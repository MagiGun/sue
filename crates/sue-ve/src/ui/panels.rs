//! The effects browser (transitions now; effects arrive in stage 2) and
//! the audio meters.

use egui::{Align2, Rect, Sense, Ui, pos2, vec2};
use sue_wm::icons::{self, Icon};
use sue_wm::ui::{Density, Space, TextRole};
use sue_wm::{AppletContext, Theme};

use super::{SQUARE, font};
use crate::studio::SueVe;

/// A transition dragged from the browser onto a cut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransitionItem(pub &'static str);

/// The effects browser.
pub fn effects_ui(studio: &mut SueVe, ui: &mut Ui, cx: &mut AppletContext<'_>, theme: &Theme) {
    let rect = ui.max_rect().shrink(theme.space(Space::S3));
    let row = Density::Regular.row();
    let mut y = rect.top();
    let heading = |ui: &Ui, y: &mut f32, text: String| {
        ui.painter().text(
            pos2(rect.left(), *y),
            Align2::LEFT_TOP,
            text,
            font(theme, TextRole::Caption),
            theme.text_muted(),
        );
        *y += theme.text_size(TextRole::Caption) + theme.space(Space::S2);
    };
    heading(ui, &mut y, tr!("effects-video-transitions"));
    let items: [(&'static str, String, &'static str); 3] = [
        ("dissolve", tr!("transition-dissolve"), "sequence.dissolve"),
        (
            "dip-black",
            tr!("transition-dip-black"),
            "sequence.dip_black",
        ),
        (
            "dip-white",
            tr!("transition-dip-white"),
            "sequence.dip_white",
        ),
    ];
    let mut run = None;
    for (kind, name, command) in items {
        let place = Rect::from_min_size(pos2(rect.left(), y), vec2(rect.width(), row));
        y += row;
        let response = ui.interact(
            place,
            egui::Id::unique(("sve-effect", kind)),
            Sense::click_and_drag(),
        );
        if response.hovered() {
            ui.painter().rect_filled(
                place,
                theme.corner(sue_wm::ui::Radius::Sm),
                theme.fill_hover(),
            );
        }
        let side = Density::Compact.icon();
        icons::paint(
            ui.painter(),
            Rect::from_min_size(
                pos2(
                    place.left() + theme.space(Space::S2),
                    place.center().y - side / 2.0,
                ),
                vec2(side, side),
            ),
            Icon::Effects,
            theme.text,
        );
        ui.painter().text(
            pos2(
                place.left() + side + theme.space(Space::S4),
                place.center().y,
            ),
            Align2::LEFT_CENTER,
            name,
            font(theme, TextRole::Body),
            theme.text,
        );
        if response.drag_started() {
            response.dnd_set_drag_payload(TransitionItem(kind));
        }
        if response.double_clicked() {
            run = Some(command);
        }
    }
    y += theme.space(Space::S4);
    heading(ui, &mut y, tr!("effects-audio-transitions"));
    let place = Rect::from_min_size(pos2(rect.left(), y), vec2(rect.width(), row));
    y += row;
    let response = ui.interact(
        place,
        egui::Id::unique("sve-effect-crossfade"),
        Sense::click(),
    );
    if response.hovered() {
        ui.painter().rect_filled(
            place,
            theme.corner(sue_wm::ui::Radius::Sm),
            theme.fill_hover(),
        );
    }
    ui.painter().text(
        pos2(place.left() + theme.space(Space::S2), place.center().y),
        Align2::LEFT_CENTER,
        tr!("transition-crossfade"),
        font(theme, TextRole::Body),
        theme.text,
    );
    if response.double_clicked() {
        run = Some("sequence.audio_dissolve");
    }
    // Effects: stage 2.
    let rest = Rect::from_min_max(pos2(rect.left(), y + theme.space(Space::S6)), rect.max);
    if rest.height() > row * 2.0 {
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rest));
        sue_wm::ui::empty_state("sve-effects-later", &tr!("effects-later"))
            .icon(Icon::Effects)
            .detail(&tr!("effects-later-detail"))
            .show(&mut child, theme);
    }
    if let Some(command) = run {
        studio.run(command, cx);
    }
}

/// The audio meters: the program's left and right peaks.
pub fn meters_ui(studio: &SueVe, ui: &mut Ui, theme: &Theme) {
    let painter = ui.painter_at(ui.max_rect());
    let rect = ui
        .max_rect()
        .shrink(theme.space(Space::S2))
        .shrink2(vec2(0.0, theme.text_size(TextRole::Micro) / 2.0));
    let scale = theme.text_size(TextRole::Micro) * 2.0;
    let bars = Rect::from_min_max(rect.min, pos2(rect.right() - scale, rect.bottom()));
    let gap = theme.space(Space::S1);
    let width = (bars.width() - gap) / 2.0;
    // The scale: 0, -6, -12, -24, -48 dB.
    let height_of = |level: f32| {
        let decibels = if level <= 1e-5 {
            -60.0
        } else {
            20.0 * level.log10()
        };
        ((decibels + 60.0) / 60.0).clamp(0.0, 1.0)
    };
    for decibels in [0.0_f32, -6.0, -12.0, -24.0, -48.0] {
        let share = (decibels + 60.0) / 60.0;
        let y = bars.bottom() - bars.height() * share;
        painter.hline(bars.left()..=bars.right(), y, theme.hairline());
        painter.text(
            pos2(rect.right(), y),
            Align2::RIGHT_CENTER,
            format!("{decibels:.0}"),
            font(theme, TextRole::Micro),
            theme.text_muted(),
        );
    }
    for channel in 0..2 {
        let left = bars.left() + channel as f32 * (width + gap);
        let column = Rect::from_min_max(pos2(left, bars.top()), pos2(left + width, bars.bottom()));
        painter.rect_filled(column, SQUARE, theme.surface(1));
        let level = studio.clock.meter.level[channel];
        let filled = Rect::from_min_max(
            pos2(
                column.left(),
                column.bottom() - column.height() * height_of(level),
            ),
            column.max,
        );
        let color = if level >= 1.0 {
            theme.danger()
        } else if level >= 0.5 {
            theme.warning()
        } else {
            theme.success()
        };
        painter.rect_filled(filled, SQUARE, color);
        let hold = column.bottom() - column.height() * height_of(studio.clock.meter.hold[channel]);
        painter.hline(
            column.left()..=column.right(),
            hold,
            theme.hairline_active(),
        );
    }
}
