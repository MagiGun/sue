//! The exposure sheet: its dialog, and CSV out and back.

use std::path::Path;

use egui::RichText;
use sue_core::EntityId;
use sue_core::txn::SetProperty;
use sue_platform::write_atomic;
use sue_wm::AppletContext;

use crate::anim;
use crate::doc;
use crate::studio::SuePaint;

impl SuePaint {
    /// The exposure sheet (Clip Studio Paint EX's timesheet): a row for
    /// each frame and a column for each animation folder, each cell what
    /// that folder shows from that frame: a cel, a blank (x), or a hold
    /// (|). A cell's menu sets it; a frame's number goes to that frame.
    pub(crate) fn exposure_ui(
        &mut self,
        ui: &mut egui::Ui,
        cx: &mut AppletContext<'_>,
        theme: &sue_wm::Theme,
    ) -> crate::dialogs::Outcome {
        use crate::dialogs::Outcome;
        let mut outcome = Outcome::Open;
        let Some((picture, sequence, length)) = self.timeline_here(&cx.world.arena) else {
            return Outcome::Cancel;
        };
        let folders = anim::folders(&sequence);
        let names: Vec<(EntityId, String)> = doc::layers(&cx.world.arena, picture)
            .into_iter()
            .rev()
            .filter(|info| info.kind == sue_model::picture::LayerKind::Raster)
            .filter_map(|info| {
                let entity = cx.world.arena.get_by_id(info.id).ok()?;
                Some((info.id, entity.name().to_owned()))
            })
            .collect();
        let name_of = |cel: EntityId| {
            names
                .iter()
                .find(|(id, _)| *id == cel)
                .map_or_else(|| "?".to_owned(), |(_, name)| name.clone())
        };
        let columns: Vec<Vec<Option<Option<EntityId>>>> = folders
            .iter()
            .map(|track| anim::sheet(&sequence.tracks[*track], sequence.rate, length))
            .collect();
        // The cels each column may show: its folder's (any layer for an
        // older project's track).
        let choices: std::collections::HashMap<usize, Option<Vec<EntityId>>> = folders
            .iter()
            .map(|track| {
                (
                    *track,
                    anim::track_folder(&sequence.tracks[*track])
                        .map(|folder| anim::cels(&cx.world.arena, picture, folder)),
                )
            })
            .collect();
        let now = self.doc().map_or(0, |doc| doc.frame);
        // What a cell's menu asked for: its folder, frame and new content
        // (`None` holds, `Some(None)` is blank).
        let mut wanted: Option<(usize, u32, Option<Option<EntityId>>)> = None;
        let mut go = None;
        egui::ScrollArea::both()
            .max_height(ui.available_height() - 40.0)
            .show(ui, |ui| {
                egui::Grid::new("sue-paint-exposure-sheet")
                    .striped(true)
                    .spacing(egui::vec2(
                        theme.space(sue_wm::ui::Space::S5),
                        theme.space(sue_wm::ui::Space::S1),
                    ))
                    .show(ui, |ui| {
                        ui.label(RichText::new(tr!("frame")).color(theme.muted));
                        for track in &folders {
                            ui.label(
                                RichText::new(anim::track_name(
                                    &cx.world.arena,
                                    &sequence.tracks[*track],
                                ))
                                .strong(),
                            );
                        }
                        ui.end_row();
                        for frame in 0..length {
                            let number = RichText::new((frame + 1).to_string());
                            let number = if frame == now {
                                number.color(theme.border_active).strong()
                            } else {
                                number.color(theme.muted)
                            };
                            if ui.add(egui::Button::new(number).frame(false)).clicked() {
                                go = Some(frame);
                            }
                            for (column, track) in columns.iter().zip(&folders) {
                                let text = match column[frame as usize] {
                                    Some(Some(cel)) => name_of(cel),
                                    Some(None) => "x".to_owned(),
                                    None => "|".to_owned(),
                                };
                                ui.menu_button(text, |ui| {
                                    // A folder's column offers its own cels.
                                    let own = choices[track].as_ref();
                                    for (id, name) in names
                                        .iter()
                                        .filter(|(id, _)| own.is_none_or(|own| own.contains(id)))
                                    {
                                        if ui.button(name).clicked() {
                                            wanted = Some((*track, frame, Some(Some(*id))));
                                            ui.close();
                                        }
                                    }
                                    ui.separator();
                                    if ui.button(tr!("blank-cel")).clicked() {
                                        wanted = Some((*track, frame, Some(None)));
                                        ui.close();
                                    }
                                    if ui.button(tr!("hold-previous")).clicked() {
                                        wanted = Some((*track, frame, None));
                                        ui.close();
                                    }
                                });
                            }
                            ui.end_row();
                        }
                    });
            });
        if let Some((track, frame, content)) = wanted {
            let changed = match content {
                Some(cel) => anim::assign(&sequence, track, frame, cel, picture, length),
                None => anim::unassign(&sequence, track, frame, length),
            };
            Self::apply(cx, anim::set(picture, changed, &tr!("exposure-sheet")));
            self.go_to_frame(&cx.world.arena, i64::from(frame));
        }
        if let Some(frame) = go {
            self.go_to_frame(&cx.world.arena, i64::from(frame));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(tr!("close")).clicked() {
                outcome = Outcome::Cancel;
            }
        });
        outcome
    }

    /// Writes the exposure sheet to `path` as CSV.
    pub(crate) fn export_exposure_sheet(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        let Some((_, sequence, length)) = self.timeline_here(&cx.world.arena) else {
            return;
        };
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.to_path_buf());
        }
        let arena = &cx.world.arena;
        let text = anim::sheet_csv(&anim::named(arena, &sequence), length, |cel| {
            arena
                .get_by_id(cel)
                .map_or_else(|_| "?".to_owned(), |entity| entity.name().to_owned())
        });
        match write_atomic(path, text.as_bytes()) {
            Ok(()) => cx.saved(path, tr!("frames-count", count = length)),
            Err(error) => cx.fail(tr!("cannot-export", error = error.to_string())),
        }
    }

    /// Reads an exposure sheet's CSV from `path` onto the timeline, cels
    /// found by their layer names, as one undo step.
    pub(crate) fn import_exposure_sheet(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        let Some((picture, sequence, length)) = self.timeline_here(&cx.world.arena) else {
            return;
        };
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.to_path_buf());
        }
        let text = match sue_platform::fs::read(path) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(error) => {
                cx.fail(error.to_string());
                return;
            }
        };
        let arena = &cx.world.arena;
        let layers: Vec<EntityId> = doc::layers(arena, picture)
            .into_iter()
            .map(|info| info.id)
            .collect();
        let named = anim::named(arena, &sequence);
        // A column's cels are its folder's (any layer for an older
        // project's track).
        let (read, new_length) =
            anim::from_sheet_csv(&named, length, &text, picture, |track, name| {
                let pool = named
                    .tracks
                    .get(track)
                    .and_then(anim::track_folder)
                    .map_or_else(
                        || layers.clone(),
                        |folder| anim::cels(arena, picture, folder),
                    );
                pool.into_iter().find(|id| {
                    arena
                        .get_by_id(*id)
                        .is_ok_and(|entity| entity.name() == name)
                })
            });
        let mut edit = anim::set(picture, read, &tr!("exposure-sheet"));
        if new_length != length {
            edit = edit.then(SetProperty::new(
                picture,
                anim::LENGTH,
                i64::from(new_length),
            ));
        }
        Self::apply(cx, edit);
    }
}
