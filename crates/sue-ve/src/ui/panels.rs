//! The audio meters, and the dock's scopes and mixer palettes (smaller
//! forms of the Color and Audio pages' own).

use egui::{Align2, Rect, Ui, pos2};
use sue_wm::ui::{Space, TextRole};
use sue_wm::{AppletContext, Theme};

use super::{SQUARE, font};
use crate::studio::SueVe;

/// The audio meters: the program's left and right peaks.
pub fn meters_ui(studio: &SueVe, ui: &mut Ui, theme: &Theme) {
    let painter = ui.painter_at(ui.max_rect());
    let rect = ui
        .max_rect()
        .shrink(theme.space(Space::S2))
        .shrink2(egui::Vec2::Y * (theme.text_size(TextRole::Micro) / 2.0));
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

/// The scopes as a palette: the picked scope of the program monitor's
/// processed frame (the Color page shows the frame it grades).
pub fn scopes_ui(studio: &mut SueVe, ui: &mut Ui, cx: &mut AppletContext<'_>, theme: &Theme) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, SQUARE, theme.void);
    let scope = studio.color.scope;
    if let Some(frame) = studio.shown.clone() {
        studio.scopes.want(scope, &frame);
    } else {
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        sue_wm::ui::empty_state("sve-scopes-empty", &tr!("scopes-empty"))
            .icon(sue_wm::icons::Icon::Scope)
            .show(&mut child, theme);
        return;
    }
    if let Some((_, _, picture)) = studio.scopes.latest() {
        let key = super::pool::texture_key(("sve-scope-panel", scope.id()));
        if let Some(texture) = cx.textures.texture(
            key,
            picture.serial,
            [picture.width, picture.height],
            4,
            &picture.rgba,
            false,
        ) {
            ui.painter().image(
                texture,
                rect.shrink(theme.space(Space::S3)),
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        }
    }
}

/// The mixer as a palette: each audio track's level, short term
/// loudness and meter.
pub fn mixer_ui(studio: &mut SueVe, ui: &mut Ui, cx: &mut AppletContext<'_>, theme: &Theme) {
    let rect = ui.max_rect().shrink(theme.space(Space::S3));
    let Some(sequence) = studio.current(cx) else {
        return;
    };
    let mut y = rect.top();
    let row = theme.text_size(TextRole::Caption) * 2.4;
    for (index, track) in sequence.tracks.iter().enumerate() {
        if crate::model::is_video(track) {
            continue;
        }
        let meter = studio.clock.mixer.tracks.get(&index);
        let level = meter.map_or(0.0, |m| m.meter.level[0].max(m.meter.level[1]));
        let loudness = meter.map_or(-120.0, |m| m.loudness.short_term());
        let gain = track
            .properties
            .get(crate::model::key::VOLUME)
            .and_then(sue_core::Value::as_f64)
            .unwrap_or(0.0);
        ui.painter().text(
            pos2(rect.left(), y),
            Align2::LEFT_TOP,
            format!("{}  {gain:+.1} dB", track.name),
            crate::ui::font(theme, TextRole::Caption),
            theme.text,
        );
        let shown = if loudness <= -100.0 {
            "-".to_owned()
        } else {
            format!("{loudness:.0} LUFS")
        };
        ui.painter().text(
            pos2(rect.right(), y),
            Align2::RIGHT_TOP,
            shown,
            crate::ui::font(theme, TextRole::Mono),
            theme.text_muted(),
        );
        let bar = Rect::from_min_size(
            pos2(
                rect.left(),
                y + theme.text_size(TextRole::Caption) + theme.space(Space::S1),
            ),
            egui::vec2(rect.width(), theme.space(Space::S2)),
        );
        ui.painter().rect_filled(bar, SQUARE, theme.surface(1));
        let db = if level <= 1e-5 {
            -60.0
        } else {
            20.0 * level.log10()
        };
        let share = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
        ui.painter().rect_filled(
            Rect::from_min_size(bar.min, egui::vec2(bar.width() * share, bar.height())),
            SQUARE,
            if level >= 1.0 {
                theme.danger()
            } else {
                theme.success()
            },
        );
        y += row;
        if y > rect.bottom() {
            break;
        }
    }
}
