//! The effects browser, as Premiere Pro's Effects panel and Resolve's
//! Effects Library are: a search over every registered video effect by
//! category, the transitions, and the effect presets saved here. A
//! double click adds an effect to the selected clips (or puts a
//! transition at the cut nearest the playhead); dragging one onto a clip
//! on the timeline adds it there, and onto a cut puts the transition
//! there.

use egui::{Align2, Rect, Sense, Ui, pos2, vec2};
use sue_wm::icons::{self, Icon};
use sue_wm::ui::{Density, Space, TextRole};
use sue_wm::{AppletContext, FilePick, ShellRequest, Theme};

use super::font;
use crate::fx::{self, Category};
use crate::studio::{SueVe, me};

/// A transition dragged from the browser onto a cut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransitionItem(pub &'static str);

/// An effect dragged from the browser onto a clip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EffectItem(pub &'static str);

/// The name of a transition kind.
#[must_use]
pub fn transition_label(kind: &str) -> String {
    tr!(&format!("transition-{kind}"))
}

/// One row of the browser; returns its response.
fn row(ui: &Ui, place: Rect, salt: &str, icon: Icon, name: &str, theme: &Theme) -> egui::Response {
    let response = ui.interact(
        place,
        egui::Id::unique(("sve-browser", salt)),
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
        icon,
        theme.text_muted(),
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
    response
}

/// The browser.
#[allow(
    clippy::too_many_lines,
    reason = "one list, its sections drawn in order"
)]
pub fn browser_ui(studio: &mut SueVe, ui: &mut Ui, cx: &mut AppletContext<'_>, theme: &Theme) {
    let rect = ui.max_rect().shrink(theme.space(Space::S3));
    let search_height = Density::Compact.icon_button() + theme.space(Space::S2);
    let mut top = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(
        rect.min,
        vec2(rect.width(), search_height),
    )));
    sue_wm::ui::search_field("sve-fx-search", &mut studio.fx_search)
        .hint(&tr!("effects-search"))
        .width(rect.width())
        .show(&mut top, theme);
    let body = Rect::from_min_max(pos2(rect.left(), rect.top() + search_height), rect.max);
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(body)
            .id_salt("sve-browser-list"),
    );
    let mut run: Option<&'static str> = None;
    let mut add: Option<&'static str> = None;
    let mut transition: Option<&'static str> = None;
    let mut preset: Option<sue_core::Preset> = None;
    let needle = studio.fx_search.trim().to_lowercase();
    let matches = |name: &str| needle.is_empty() || name.to_lowercase().contains(&needle);
    egui::ScrollArea::vertical()
        .id_salt("sve-browser-scroll")
        .auto_shrink([false, false])
        .show(&mut child, |ui| {
            let width = ui.available_width();
            let height = Density::Regular.row();
            let heading = |ui: &mut Ui, text: &str| {
                ui.add_space(theme.space(Space::S2));
                ui.label(
                    egui::RichText::new(text)
                        .font(font(theme, TextRole::Caption))
                        .color(theme.text_muted()),
                );
            };
            let found = fx::search(&studio.fx_search);
            for category in Category::ALL {
                let in_it: Vec<&fx::EffectKind> = found
                    .iter()
                    .filter(|kind| kind.category == category)
                    .collect();
                if in_it.is_empty() {
                    continue;
                }
                heading(ui, &category.label());
                for kind in in_it {
                    let (place, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
                    let response = row(ui, place, kind.id, Icon::Effects, &kind.label(), theme);
                    if response.drag_started() {
                        response.dnd_set_drag_payload(EffectItem(kind.id));
                    }
                    if response.double_clicked() {
                        add = Some(kind.id);
                    }
                }
            }
            let transitions: Vec<&'static str> = crate::render::TRANSITIONS
                .iter()
                .copied()
                .filter(|kind| matches(&transition_label(kind)))
                .collect();
            if !transitions.is_empty() {
                heading(ui, &tr!("effects-video-transitions"));
                for kind in transitions {
                    let (place, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
                    let response = row(
                        ui,
                        place,
                        kind,
                        Icon::ToolSlide,
                        &transition_label(kind),
                        theme,
                    );
                    if response.drag_started() {
                        response.dnd_set_drag_payload(TransitionItem(kind));
                    }
                    if response.double_clicked() {
                        transition = Some(kind);
                    }
                }
            }
            if matches(&tr!("transition-crossfade")) {
                heading(ui, &tr!("effects-audio-transitions"));
                let (place, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
                if row(
                    ui,
                    place,
                    "crossfade",
                    Icon::Audio,
                    &tr!("transition-crossfade"),
                    theme,
                )
                .double_clicked()
                {
                    run = Some("sequence.audio_dissolve");
                }
            }
            let saved: Vec<(String, sue_core::Preset)> = fx::preset::saved()
                .into_iter()
                .filter(|(name, _)| matches(name))
                .collect();
            heading(ui, &tr!("effects-presets"));
            for (name, found) in saved {
                let (place, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
                if row(
                    ui,
                    place,
                    &format!("preset-{name}"),
                    Icon::Template,
                    &name,
                    theme,
                )
                .double_clicked()
                {
                    preset = Some(found);
                }
            }
            ui.add_space(theme.space(Space::S2));
            ui.horizontal_wrapped(|ui| {
                if sue_wm::ui::button("sve-preset-save", &tr!("cmd-save-effect-preset"))
                    .icon(Icon::Save)
                    .enabled(!studio.selection.is_empty())
                    .show(ui, theme)
                    .clicked()
                {
                    run = Some("fx.save_preset");
                }
                if sue_wm::ui::button("sve-preset-import", &tr!("cmd-import-effect-preset"))
                    .icon(Icon::Import)
                    .show(ui, theme)
                    .clicked()
                {
                    run = Some("fx.import_preset");
                }
            });
        });
    if let Some(kind) = add {
        add_effect(studio, cx, kind, &studio.selected_with_links(cx));
    }
    if let Some(kind) = transition {
        crate::commands::add_transition(studio, cx, kind);
    }
    if let Some(found) = preset {
        apply_preset(studio, cx, &found);
    }
    if let Some(command) = run {
        studio.run(command, cx);
    }
    let _ = (me, FilePick::open, ShellRequest::Hub);
}

/// Adds an effect of `kind` to the video clips among `ids`, as one undo
/// step.
pub fn add_effect(studio: &mut SueVe, cx: &mut AppletContext<'_>, kind: &str, ids: &[u64]) {
    let label = fx::kind(kind).map_or_else(|| kind.to_owned(), |found| found.label());
    let ids = ids.to_vec();
    let added = studio.edit(
        cx,
        &tr!("cmd-add-effect", name = label.clone()),
        |sequence, _| {
            let mut any = None;
            for id in &ids {
                let video = sequence
                    .find(*id)
                    .is_some_and(|(track, _)| crate::model::is_video(&sequence.tracks[track]));
                if !video {
                    continue;
                }
                if let Some((_, clip)) = sequence.find_mut(*id) {
                    any = Some(fx::add(clip, kind));
                }
            }
            any.ok_or(crate::edit::EditError::Nothing)
        },
    );
    match added {
        Some(effect) => studio.fx_picked = Some(effect),
        None => cx.notify(tr!("effects-pick-a-clip")),
    }
}

/// Applies an effect preset to the selected video clips.
pub fn apply_preset(studio: &mut SueVe, cx: &mut AppletContext<'_>, preset: &sue_core::Preset) {
    let ids = studio.selected_with_links(cx);
    let found = studio.edit(cx, &tr!("cmd-apply-effect-preset"), |sequence, _| {
        let mut any = false;
        for id in &ids {
            let video = sequence
                .find(*id)
                .is_some_and(|(track, _)| crate::model::is_video(&sequence.tracks[track]));
            if let (true, Some((_, clip))) = (video, sequence.find_mut(*id)) {
                any |= fx::preset::apply(preset, clip) > 0;
            }
        }
        if any {
            Ok(())
        } else {
            Err(crate::edit::EditError::Nothing)
        }
    });
    if found.is_none() {
        cx.notify(tr!("effects-pick-a-clip"));
    }
}
