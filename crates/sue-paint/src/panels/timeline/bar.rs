//! The Timeline palette's bar: its buttons in Clip Studio Paint's order
//! (the timeline, zoom, playback, cels, onion skin), each with its name
//! and key on hover; the buttons that do not fit go under a More button;
//! at the right, the frame shown and the playback range in the chosen
//! unit, and while it plays, the frames a second it reaches.

use egui::{Align2, Rect, pos2};
use sue_wm::icons::Icon;
use sue_wm::ui::{self as wm_ui, Density, Space, TextRole};
use sue_wm::{AppletContext, Theme};

use super::{TimeUnit, command_keys};
use crate::anim;
use crate::studio::SuePaint;

/// A button of the bar: its icon's stable name, its command, and whether
/// it is lit. `None` starts a new group.
type Button = Option<(&'static str, &'static str, bool)>;

/// An icon of the shared set by its stable name.
pub(crate) fn icon(name: &str) -> Icon {
    Icon::from_name(name).unwrap_or(Icon::More)
}

impl SuePaint {
    /// The name a button shows on hover: the command's label and the keys
    /// that run it.
    pub(crate) fn button_name(&self, cx: &AppletContext<'_>, command: &str) -> String {
        let label = crate::commands::registry()
            .label(command)
            .unwrap_or_else(|| command.to_owned())
            .trim_end_matches("...")
            .to_owned();
        let keys = command_keys(&cx.world.rice, command);
        if keys.is_empty() {
            label
        } else {
            format!("{label} ({keys})")
        }
    }

    /// Whether `command` can run now (the registry's own state).
    pub(super) fn command_enabled(&self, cx: &AppletContext<'_>, command: &str) -> bool {
        crate::commands::registry()
            .state(self, &sue_wm::Facts::of(cx), command)
            .enabled
    }

    /// Whether the current track is an animation folder's.
    fn folder_track_here(&self, cx: &AppletContext<'_>) -> bool {
        let track = self.doc().map_or(0, |doc| doc.track);
        self.timeline_here(&cx.world.arena)
            .is_some_and(|(_, sequence, _)| {
                sequence
                    .tracks
                    .get(track)
                    .and_then(anim::track_folder)
                    .is_some()
            })
    }

    /// The bar of buttons along the top of the Timeline palette.
    pub(crate) fn timeline_bar(
        &mut self,
        ui: &mut egui::Ui,
        bar: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let here = self.timeline_here(&cx.world.arena);
        let playing = self.doc().is_some_and(|doc| doc.playing.is_some());
        let buttons: [Button; 21] = [
            Some(("timeline-new", "anim.timeline.new", false)),
            Some(("settings", "anim.timeline.settings", false)),
            Some(("animation-folder", "anim.layer.folder", false)),
            None,
            Some(("zoom-out", "anim.zoom_out", false)),
            Some(("zoom-in", "anim.zoom_in", false)),
            None,
            Some(("frame-first", "anim.first", false)),
            Some(("frame-prev", "anim.prev", false)),
            // One button, as in Clip Studio Paint: Play, and Stop while
            // it plays.
            Some((if playing { "stop" } else { "play" }, "anim.play", playing)),
            Some(("frame-next", "anim.next", false)),
            Some(("frame-last", "anim.last", false)),
            Some(("loop", "anim.loop", self.looping)),
            None,
            Some(("cel-new", "anim.cel.new", false)),
            Some(("cel-specify", "anim.cel.list", false)),
            Some(("minus", "anim.key.delete", false)),
            Some(("plus", "anim.frame.insert", false)),
            Some(("delete", "anim.frame.delete", false)),
            None,
            Some(("onion-skin", "anim.onion", self.onion)),
        ];
        let density = Density::of(ui);
        let side = density.icon_button();
        let gap = theme.space(Space::S1);
        let group = theme.space(Space::S4);
        // The readout at the right: the frame shown, then the playback
        // range, in the unit chosen.
        let readout = here.as_ref().map(|(picture, sequence, length)| {
            let fps = anim::fps(sequence).round().max(1.0) as u32;
            let unit = self.anim_ui.unit;
            let frame = self.doc().map_or(0, |doc| doc.frame);
            let (start, end) = anim::setup::Settings::of(&cx.world.arena, *picture).range(*length);
            let mut text = format!(
                "{}   {} {}  {}",
                unit.show(frame, fps),
                unit.show(start, fps),
                tr!("to"),
                unit.show(end, fps),
            );
            if playing && self.anim_ui.measured > 0.0 {
                text = format!(
                    "{text}   {}",
                    tr!(
                        "fps-reached",
                        value = format!("{:.1}", self.anim_ui.measured)
                    )
                );
            }
            if unit == TimeUnit::Seconds {
                text = format!("{text}   ({})", tr!("seconds-and-frames"));
            }
            text
        });
        let font = theme.text_font(TextRole::Caption);
        let readout_width = readout.as_ref().map_or(0.0, |text| {
            ui.painter()
                .layout_no_wrap(text.clone(), font.clone(), theme.text)
                .size()
                .x
                + group * 2.0
        });
        let limit = bar.right() - readout_width - side;
        let top = bar.center().y - side / 2.0;
        let mut x = bar.left() + group;
        let mut run = None;
        let mut overflow: Vec<&'static str> = Vec::new();
        for button in buttons {
            let Some((icon_name, command, lit)) = button else {
                if overflow.is_empty() {
                    ui.painter().vline(
                        x + group / 2.0 - gap,
                        (top + gap)..=(top + side - gap),
                        theme.hairline(),
                    );
                    x += group;
                }
                continue;
            };
            if x + side > limit || !overflow.is_empty() {
                overflow.push(command);
                continue;
            }
            let rect = Rect::from_min_size(pos2(x, top), egui::Vec2::splat(side));
            // Specify Cels opens the list of the folder's cels; it is the
            // palette's own, not a command.
            let (name, enabled) = if command == "anim.cel.list" {
                (tr!("specify-cels-list"), self.folder_track_here(cx))
            } else {
                (
                    self.button_name(cx, command),
                    self.command_enabled(cx, command),
                )
            };
            let response = wm_ui::icon_button(("timeline-bar", command), icon(icon_name), &name)
                .selected(lit)
                .enabled(enabled)
                .show_at(ui, rect, theme);
            if response.clicked() {
                run = Some(command.to_owned());
            }
            if command == "anim.cel.list" {
                self.cel_list_popup(&response, cx);
            }
            x += side + gap;
        }
        if !overflow.is_empty() {
            // What did not fit, under More, as Clip Studio Paint keeps
            // nothing out of reach on a narrow palette.
            let rect = Rect::from_min_size(pos2(x, top), egui::Vec2::splat(side));
            let response =
                wm_ui::icon_button("timeline-bar-more", icon("more"), &tr!("more-buttons"))
                    .show_at(ui, rect, theme);
            let names: Vec<(String, &str, bool)> = overflow
                .iter()
                .filter(|command| **command != "anim.cel.list")
                .map(|command| {
                    (
                        self.button_name(cx, command),
                        *command,
                        self.command_enabled(cx, command),
                    )
                })
                .collect();
            egui::Popup::menu(&response).show(|ui| {
                for (name, command, enabled) in &names {
                    if ui.add_enabled(*enabled, egui::Button::new(name)).clicked() {
                        run = Some((*command).to_owned());
                        ui.close();
                    }
                }
            });
        }
        if let Some(text) = readout {
            ui.painter().text(
                pos2(bar.right() - group, bar.center().y),
                Align2::RIGHT_CENTER,
                text,
                font,
                theme.text_muted(),
            );
        }
        if let Some(command) = run
            && command != "anim.cel.list"
        {
            self.anim_command(&command, cx);
        }
    }

    /// The list Specify Cels opens: the cels of the current animation
    /// folder (folder cels included), the one shown marked; a pick shows
    /// it from the frame shown.
    fn cel_list_popup(&mut self, response: &egui::Response, cx: &mut AppletContext<'_>) {
        let Some((picture, sequence, _)) = self.timeline_here(&cx.world.arena) else {
            return;
        };
        let (frame, track) = self.doc().map_or((0, 0), |doc| (doc.frame, doc.track));
        let Some(folder) = sequence.tracks.get(track).and_then(anim::track_folder) else {
            return;
        };
        let arena = &cx.world.arena;
        let shown = anim::cel_at(&sequence.tracks[track], sequence.rate, frame);
        let cels: Vec<(sue_core::EntityId, String)> = anim::cels(arena, picture, folder)
            .into_iter()
            .rev()
            .filter_map(|cel| Some((cel, arena.get_by_id(cel).ok()?.name().to_owned())))
            .collect();
        let mut picked = None;
        egui::Popup::menu(response).show(|ui| {
            if cels.is_empty() {
                ui.label(tr!("this-folder-has-no-cels"));
            }
            for (cel, name) in &cels {
                if ui
                    .add(egui::Button::selectable(shown == Some(*cel), name))
                    .clicked()
                {
                    picked = Some(Some(*cel));
                    ui.close();
                }
            }
            ui.separator();
            if ui.button(tr!("blank-from-here")).clicked() {
                picked = Some(None);
                ui.close();
            }
        });
        if let Some(cel) = picked {
            self.timeline_action(cx, super::Action::Assign { track, frame, cel });
        }
    }
}
