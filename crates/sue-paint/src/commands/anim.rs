//! The Animation menu's commands, and those of the timeline, the cels
//! and the light table palettes: one registry entry each, with its own
//! run and its own state.
//!
//! The studio runs these before the registry (`run` in `mod.rs` and the
//! Timeline palette's buttons call [`SuePaint::anim_command`]), so the
//! entries are found by name here as well as through the registry.

use sue_core::txn::{Relate, SetProperty, Unrelate};
use sue_core::{Arena, EntityId, OrderKey, Relation};
use sue_model::Sequence;
use sue_wm::{
    AppletContext, Call, Command, Facts, FilePick, FileType, Flags, ItemState, ShellRequest,
};

use super::{doc_open, flag};
use crate::anim;
use crate::anim_ui::Edit;
use crate::studio::SuePaint;

/// Where an Animation command acts: the open picture, its timeline with
/// its length (when it has one), and the frame and track shown.
struct Here {
    picture: EntityId,
    timeline: Option<(Sequence, u32)>,
    frame: u32,
    track: usize,
}

impl SuePaint {
    /// Where an Animation command acts now; `None` with no canvas open,
    /// when no Animation command does anything.
    fn here(&self, arena: &Arena) -> Option<Here> {
        let picture = self.picture()?;
        let timeline = self
            .timeline_here(arena)
            .map(|(_, sequence, length)| (sequence, length));
        let (frame, track) = self.doc().map_or((0, 0), |doc| (doc.frame, doc.track));
        Some(Here {
            picture,
            timeline,
            frame,
            track,
        })
    }

    /// Runs an Animation command, or one of the File menu's animation
    /// exports that live beside them; `false` if `command` is neither.
    pub(crate) fn anim_command(&mut self, command: &str, cx: &mut AppletContext<'_>) -> bool {
        if let Some(entry) = COMMANDS.iter().find(|entry| entry.name == command) {
            (entry.run)(
                self,
                cx,
                Call {
                    name: command,
                    arg: "",
                },
            );
            return true;
        }
        self.anim_file_command(command, cx)
    }

    /// Runs one of the File menu's animation commands (the exports and
    /// the exposure sheet); `false` if `command` is not one, or with no
    /// canvas open.
    pub(crate) fn anim_file_command(&mut self, command: &str, cx: &mut AppletContext<'_>) -> bool {
        if self.picture().is_none() {
            return false;
        }
        match command {
            "file.export.exposure_sheet" | "file.import.exposure_sheet" => {
                let Some((picture, _, _)) = self.timeline_here(&cx.world.arena) else {
                    cx.fail(tr!("make-timeline-first-animation-timeline-new"));
                    return true;
                };
                let name = cx
                    .world
                    .arena
                    .get_by_id(picture)
                    .map_or_else(|_| tr!("animation"), |entity| entity.name().to_owned());
                let types = vec![FileType::new(&tr!("exposure-sheet-csv"), &["csv"])];
                let pick = if command == "file.export.exposure_sheet" {
                    FilePick::save(
                        Some(sue_wm::AppletId::new(crate::ID).expect("a valid id")),
                        "exposure-sheet-out",
                        &tr!("exposure-sheet"),
                        &format!("{name}.csv"),
                    )
                } else {
                    FilePick::open(
                        Some(sue_wm::AppletId::new(crate::ID).expect("a valid id")),
                        "exposure-sheet-in",
                        &tr!("exposure-sheet"),
                    )
                };
                cx.request(ShellRequest::PickFile(
                    pick.types(types).starting_in(self.last_dir.clone()),
                ));
            }
            "file.export.cels" => {
                let name = self
                    .picture()
                    .and_then(|picture| cx.world.arena.get_by_id(picture).ok())
                    .map_or_else(|| tr!("cel"), |entity| entity.name().to_owned());
                cx.request(ShellRequest::PickFile(
                    FilePick::save(
                        Some(sue_wm::AppletId::new(crate::ID).expect("a valid id")),
                        "anim-cels",
                        &tr!("export-animation-cels"),
                        &format!("{name}.png"),
                    )
                    .types(
                        sue_io::PICTURE_SAVE_TYPES
                            .iter()
                            .map(|(what, extensions)| FileType::new(what, extensions))
                            .collect(),
                    )
                    .starting_in(self.last_dir.clone()),
                ));
            }
            "file.export.gif"
            | "file.export.apng"
            | "file.export.webp_animation"
            | "file.export.sequence"
            | "file.export.movie" => {
                self.ask_export_animation(cx, command);
            }
            _ => return false,
        }
        true
    }
}

// ----- Run: the timeline ----------------------------------------------------

/// New Timeline: a timeline of 24 frames at 24 per second, opened on its
/// first folder; a canvas that has one says where it is edited.
fn timeline_new(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let Some(here) = paint.here(&cx.world.arena) else {
        return;
    };
    if here.timeline.is_some() {
        cx.notify(tr!("canvas-has-timeline-timeline-palette-edits"));
    } else if SuePaint::apply(
        cx,
        anim::new_timeline(&cx.world.arena, here.picture, 24, 24),
    ) {
        paint.panels.timeline = true;
        if let Some((_, sequence, _)) = paint.timeline_here(&cx.world.arena)
            && let Some(folder) = sequence.tracks.first().and_then(anim::track_folder)
            && let Some(doc) = paint.doc_mut()
        {
            doc.track = 0;
            doc.layer = Some(folder);
        }
    }
}

/// Edit Timeline: its frame rate and length, in a dialog.
fn timeline_edit(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let Some(here) = paint.here(&cx.world.arena) else {
        return;
    };
    if let Some((sequence, length)) = &here.timeline {
        paint.dialog = Some(crate::dialogs::Dialog::Timeline(
            anim::fps(sequence).round() as u32,
            *length,
        ));
    } else {
        cx.fail(tr!("make-timeline-first-animation-timeline-new"));
    }
}

/// The exposure sheet, in a dialog.
fn exposure_sheet(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let Some(here) = paint.here(&cx.world.arena) else {
        return;
    };
    if here.timeline.is_some() {
        paint.dialog = Some(crate::dialogs::Dialog::Exposure);
    } else {
        cx.fail(tr!("make-timeline-first-animation-timeline-new"));
    }
}

/// Enable Timeline: switches the timeline on and off.
fn timeline_enable(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if paint.here(&cx.world.arena).is_some() {
        paint.timeline_on = !paint.timeline_on;
    }
}

// ----- Run: new animation layers ---------------------------------------------

/// New Animation Folder: a layer folder holding its cels, with its
/// timeline track, right above the current layer (Clip Studio Paint's
/// New Animation Folder).
fn layer_folder(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let Some(here) = paint.here(&cx.world.arena) else {
        return;
    };
    let opened = here.timeline.is_none();
    let (edit, folder, track) = anim::add_folder(&cx.world.arena, here.picture, paint.layer());
    if SuePaint::apply(cx, edit) {
        if opened {
            paint.panels.timeline = true;
        }
        if let Some(doc) = paint.doc_mut() {
            doc.track = track;
            doc.layer = Some(folder);
        }
    }
}

/// New Camera Folder, made the current layer.
fn layer_camera(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let Some(here) = paint.here(&cx.world.arena) else {
        return;
    };
    let (edit, folder) = crate::camera::new_camera(&cx.world.arena, here.picture, paint.layer());
    if SuePaint::apply(cx, edit)
        && let Some(doc) = paint.doc_mut()
    {
        doc.layer = Some(folder);
    }
}

// ----- Run: cels and keys ----------------------------------------------------

/// New Animation Cel: into the animation folder picked (or the one the
/// picked cel is in, or the timeline's current one), as its top cel; with
/// no folder, into a new one.
fn cel_new(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let Some(here) = paint.here(&cx.world.arena) else {
        return;
    };
    let folder = here
        .timeline
        .as_ref()
        .and_then(|(sequence, _)| paint.cel_target(&cx.world.arena, sequence, here.track));
    let folder = match folder {
        Some(folder) => folder,
        None => {
            let opened = here.timeline.is_none();
            let (edit, folder, _) = anim::add_folder(&cx.world.arena, here.picture, paint.layer());
            if !SuePaint::apply(cx, edit) {
                return;
            }
            if opened {
                paint.panels.timeline = true;
            }
            folder
        }
    };
    match anim::new_cel(&cx.world.arena, here.picture, folder, here.frame) {
        Ok((edit, cel)) => {
            if SuePaint::apply(cx, edit) {
                let track = paint
                    .timeline_here(&cx.world.arena)
                    .and_then(|(_, sequence, _)| anim::track_of(&sequence, folder));
                if let Some(doc) = paint.doc_mut() {
                    doc.layer = Some(cel);
                    doc.track = track.unwrap_or(doc.track);
                }
            }
        }
        Err(error) => cx.fail(error),
    }
}

/// Assign Animation Cel and Add Keyframe: the current track shows the
/// current layer from the current frame on.
fn cel_assign(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    let Some(here) = paint.here(&cx.world.arena) else {
        return;
    };
    let (Some((sequence, length)), Some(layer)) =
        (here.timeline, paint.doc().and_then(|doc| doc.layer))
    else {
        cx.fail(tr!("make-timeline-pick-layer-first"));
        return;
    };
    if !anim::may_show(&cx.world.arena, &sequence, here.track, layer) {
        cx.notify(tr!("only-cels-of-this-animation-folder"));
        return;
    }
    let sequence = anim::assign(
        &sequence,
        here.track,
        here.frame,
        Some(layer),
        here.picture,
        length,
    );
    // The undo step reads as the menu item that made it.
    let label = if call.name == "anim.key.add" {
        tr!("add-keyframe")
    } else {
        tr!("assign-animation-cel")
    };
    SuePaint::apply(cx, anim::set(here.picture, sequence, &label));
}

/// Delete Keyframe: the current track's key on the current frame goes.
fn key_delete(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let Some(here) = paint.here(&cx.world.arena) else {
        return;
    };
    if let Some((sequence, length)) = here.timeline {
        let sequence = anim::unassign(&sequence, here.track, here.frame, length);
        SuePaint::apply(
            cx,
            anim::set(here.picture, sequence, &tr!("delete-keyframe")),
        );
    }
}

/// Insert Frame and Delete Frame: every track moves by one frame from the
/// current one, and the timeline grows or shrinks with it.
fn frame_insert_delete(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    let Some(here) = paint.here(&cx.world.arena) else {
        return;
    };
    let Some((sequence, length)) = here.timeline else {
        return;
    };
    let (by, new_length) = if call.name == "anim.frame.insert" {
        (1, length + 1)
    } else {
        (-1, length.saturating_sub(1).max(1))
    };
    let sequence = anim::shift(&sequence, here.frame, by, new_length);
    let edit = Edit::new(if by > 0 {
        tr!("insert-frame")
    } else {
        tr!("delete-frame")
    })
    .then(sue_core::txn::SetExtension::set(here.picture, sequence))
    .then(SetProperty::new(
        here.picture,
        anim::LENGTH,
        i64::from(new_length),
    ));
    SuePaint::apply(cx, edit);
}

/// Select Previous Cel and Select Next Cel: to the current track's key
/// before or after the current frame.
fn cel_step(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    let Some(here) = paint.here(&cx.world.arena) else {
        return;
    };
    if let Some((sequence, _)) = &here.timeline
        && let Some(doc) = paint.doc()
        && let Some(track) = sequence.tracks.get(doc.track)
    {
        let keys = anim::keys(track, sequence.rate);
        let at = doc.frame;
        let to = if call.name == "anim.cel.prev" {
            keys.iter().rev().find(|(key, _)| *key < at)
        } else {
            keys.iter().find(|(key, _)| *key > at)
        };
        if let Some((frame, _)) = to {
            paint.cel_lock = None;
            paint.go_to_frame(&cx.world.arena, i64::from(*frame));
        }
    }
}

/// Lock Current Animation Cel as Editing Target, and back.
fn cel_lock(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if paint.here(&cx.world.arena).is_none() {
        return;
    }
    paint.cel_lock = match paint.cel_lock {
        Some(_) => None,
        None => paint.editing_cel(&cx.world.arena),
    };
}

// ----- Run: the 2D camera ----------------------------------------------------

/// A camera key at the current frame, from the camera as it is shown.
fn camera_key(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if paint.here(&cx.world.arena).is_none() {
        return;
    }
    if let Some(folder) = paint.camera_folder(&cx.world.arena) {
        let camera = paint.camera_now(&cx.world.arena, folder);
        let (_, time) = paint.frame_time(&cx.world.arena);
        let edit = crate::camera::key(&cx.world.arena, folder, time, camera);
        SuePaint::apply(cx, edit);
        paint.camera_pending = None;
    }
}

/// The camera key at the current frame goes.
fn camera_unkey(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if paint.here(&cx.world.arena).is_none() {
        return;
    }
    if let Some(folder) = paint.camera_folder(&cx.world.arena) {
        let (_, time) = paint.frame_time(&cx.world.arena);
        if let Some(edit) = crate::camera::delete_key(&cx.world.arena, folder, time) {
            SuePaint::apply(cx, edit);
        }
        paint.camera_pending = None;
    }
}

// ----- Run: onion skin and playback ----------------------------------------

/// Enable Onion Skin, and back.
fn onion(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if paint.here(&cx.world.arena).is_some() {
        paint.onion = !paint.onion;
    }
}

/// Onion Skin Settings, in a dialog.
fn onion_settings(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if let Some(here) = paint.here(&cx.world.arena) {
        paint.dialog = Some(crate::dialogs::Dialog::Onion(anim::Onion::of(
            &cx.world.arena,
            here.picture,
        )));
    }
}

/// Play, and Stop while it plays.
fn play(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if paint.here(&cx.world.arena).is_none() {
        return;
    }
    let now = cx.view.now;
    if let Some(doc) = paint.doc_mut() {
        doc.playing = match doc.playing {
            Some(_) => None,
            None => Some((now, doc.frame)),
        };
    }
}

/// Loop playback, and back.
fn looping(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if paint.here(&cx.world.arena).is_some() {
        paint.looping = !paint.looping;
    }
}

/// Go to Start, Go to End, Previous Frame and Next Frame.
fn go(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    let Some(here) = paint.here(&cx.world.arena) else {
        return;
    };
    let to = match call.name {
        "anim.first" => 0,
        "anim.last" => -1,
        "anim.prev" => i64::from(here.frame) - 1,
        _ => i64::from(here.frame) + 1,
    };
    paint.go_to_frame(&cx.world.arena, to);
}

// ----- Run: the light table --------------------------------------------------

/// Enable Light Table, and back.
fn light_table(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if paint.here(&cx.world.arena).is_some() {
        paint.light_table = !paint.light_table;
    }
}

/// The picked layer goes on the light table of the cel being drawn.
fn light_register(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if paint.here(&cx.world.arena).is_none() {
        return;
    }
    let (Some(cel), Some(layer)) = (paint.editing_cel(&cx.world.arena), paint.layer()) else {
        cx.notify(tr!("go-frame-cel-give-light-table"));
        return;
    };
    if cel == layer {
        cx.notify(tr!("pick-layer-trace-layer-palette-then"));
    } else if anim::light_table(&cx.world.arena, cel).contains(&layer) {
        cx.notify(tr!("layer-cels-light-table-already"));
    } else {
        // Last on the table, after the layers already on it.
        let order = cx
            .world
            .arena
            .relations()
            .next_order(cel, anim::LIGHT_TABLE);
        let edit = Edit::new(tr!("register-to-light-table")).then(Relate::new(
            Relation::new(cel, anim::LIGHT_TABLE, layer).ordered(order),
        ));
        if SuePaint::apply(cx, edit) {
            paint.light_table = true;
        }
    }
}

/// The edit that puts `cel`'s light table in the reverse order. Each
/// layer gets an order key of its own: the table is sorted by its keys,
/// and layers that share one (as each registered with the default key
/// does) sort by their ids, which no relinking turns round.
fn reversed_light_table(arena: &Arena, cel: EntityId) -> Edit {
    let layers = anim::light_table(arena, cel);
    let off = layers.iter().fold(
        Edit::new(tr!("reverse-layers-vertically-light-table")),
        |edit, layer| edit.then(Unrelate::new(cel, anim::LIGHT_TABLE, *layer)),
    );
    let keys = OrderKey::sequence(None, layers.len());
    layers
        .iter()
        .rev()
        .zip(keys)
        .fold(off, |edit, (layer, order)| {
            edit.then(Relate::new(
                Relation::new(cel, anim::LIGHT_TABLE, *layer).ordered(order),
            ))
        })
}

/// The light table's layers in the reverse order.
fn light_reverse(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if paint.here(&cx.world.arena).is_none() {
        return;
    }
    if let Some(cel) = paint.editing_cel(&cx.world.arena)
        && anim::light_table(&cx.world.arena, cel).len() > 1
    {
        SuePaint::apply(cx, reversed_light_table(&cx.world.arena, cel));
    }
}

/// The light table's color mode, and back.
fn light_color(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if paint.here(&cx.world.arena).is_some() {
        paint.light_color = !paint.light_color;
    }
}

/// Show the general light table, and back; showing it turns the light
/// table on.
fn light_general(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if paint.here(&cx.world.arena).is_none() {
        return;
    }
    paint.light_general = !paint.light_general;
    if paint.light_general {
        paint.light_table = true;
    }
}

/// Switch Opacity Target, and back.
fn light_opacity_target(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if paint.here(&cx.world.arena).is_some() {
        paint.opacity_general = !paint.opacity_general;
    }
}

/// The Light Table tool, and back.
fn light_tool(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if paint.here(&cx.world.arena).is_none() {
        return;
    }
    paint.light_tool = !paint.light_tool;
    if paint.light_tool {
        cx.notify(tr!("light-table-tool-on"));
    }
}

/// Every layer on the light tables back where it was placed first.
fn light_reset(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let Some(here) = paint.here(&cx.world.arena) else {
        return;
    };
    let mut layers = paint
        .editing_cel(&cx.world.arena)
        .map(|cel| anim::light_table(&cx.world.arena, cel))
        .unwrap_or_default();
    layers.extend(anim::general_light_table(&cx.world.arena, here.picture));
    let edit = anim::reset_places(&cx.world.arena, &layers);
    if !edit.is_empty() {
        SuePaint::apply(cx, edit);
    }
}

/// The picked layer goes on the general light table.
fn light_register_general(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let Some(here) = paint.here(&cx.world.arena) else {
        return;
    };
    let Some(layer) = paint.layer() else {
        cx.notify(tr!("pick-layer-trace-layer-palette-then"));
        return;
    };
    if anim::general_light_table(&cx.world.arena, here.picture).contains(&layer) {
        cx.notify(tr!("layer-cels-light-table-already"));
    } else if SuePaint::apply(cx, anim::register_general(here.picture, layer)) {
        paint.light_table = true;
        paint.light_general = true;
    }
}

/// Select and Register File: a picture file picked to go on the light
/// table of the cel being drawn.
fn light_file(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if paint.here(&cx.world.arena).is_none() {
        return;
    }
    if paint.editing_cel(&cx.world.arena).is_none() {
        cx.notify(tr!("go-frame-cel-see-light-table"));
        return;
    }
    let start = paint
        .last_dir
        .clone()
        .or_else(|| cx.catalog.folders.get("home").cloned());
    cx.request(ShellRequest::PickFile(
        FilePick::open(
            Some(sue_wm::AppletId::new(crate::ID).expect("a valid id")),
            "light-table-file",
            &tr!("select-and-register-file"),
        )
        .extensions(&["png", "jpg", "jpeg", "webp", "jxl", "bmp", "tif", "tiff"])
        .starting_in(start),
    ));
}

/// Show the cel's light table, and back.
fn light_cel(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if paint.here(&cx.world.arena).is_some() {
        paint.light_cel = !paint.light_cel;
    }
}

/// The picked layer leaves the light table of the cel being drawn.
fn light_deregister(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if paint.here(&cx.world.arena).is_none() {
        return;
    }
    if let (Some(cel), Some(layer)) = (paint.editing_cel(&cx.world.arena), paint.layer())
        && anim::light_table(&cx.world.arena, cel).contains(&layer)
    {
        SuePaint::apply(cx, anim::deregister(cel, &[layer]));
    } else {
        cx.notify(tr!("pick-registered-layer-to-deregister"));
    }
}

/// Every layer leaves the light table of the cel being drawn.
fn light_clear(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if paint.here(&cx.world.arena).is_none() {
        return;
    }
    if let Some(cel) = paint.editing_cel(&cx.world.arena) {
        let layers = anim::light_table(&cx.world.arena, cel);
        if !layers.is_empty() {
            SuePaint::apply(cx, anim::deregister(cel, &layers));
        }
    }
}

// ----- Run: naming cels --------------------------------------------------------

/// The cels of `folder` in timeline order: those its track shows, in the
/// order they first show, then the ones it does not show, bottom first.
fn in_timeline_order(
    arena: &Arena,
    picture: EntityId,
    sequence: &Sequence,
    folder: EntityId,
) -> Vec<EntityId> {
    let cels = anim::cels(arena, picture, folder);
    let mut order: Vec<EntityId> = Vec::with_capacity(cels.len());
    if let Some(track) = anim::track_of(sequence, folder) {
        for (_, cel) in anim::keys(&sequence.tracks[track], sequence.rate) {
            if let Some(cel) = cel
                && cels.contains(&cel)
                && !order.contains(&cel)
            {
                order.push(cel);
            }
        }
    }
    let unshown: Vec<EntityId> = cels
        .into_iter()
        .filter(|cel| !order.contains(cel))
        .collect();
    order.extend(unshown);
    order
}

/// The edit that names `cels` 1, 2, 3 and so on in their order, leaving
/// alone a cel that already has its number; empty when every one has.
fn numbered(arena: &Arena, cels: &[EntityId], label: &str) -> Edit {
    let mut edit = Edit::new(label);
    for (index, cel) in cels.iter().enumerate() {
        let name = (index + 1).to_string();
        if arena
            .get_by_id(*cel)
            .is_ok_and(|entity| entity.name() != name)
        {
            edit = edit.then(sue_core::txn::Rename::new(*cel, name));
        }
    }
    edit
}

/// Rename in Timeline Order and Rename in Layer Order: the cels of the
/// animation folder picked (or of the current track's) are named 1, 2, 3
/// and so on, as one undo step. In timeline order the cels the track
/// shows come first, in the order they first show; in layer order the
/// bottom cel is 1.
fn rename_cels(paint: &mut SuePaint, cx: &mut AppletContext<'_>, call: Call<'_>) {
    let Some(here) = paint.here(&cx.world.arena) else {
        return;
    };
    let Some((sequence, _)) = &here.timeline else {
        cx.fail(tr!("make-timeline-first-animation-timeline-new"));
        return;
    };
    let arena = &cx.world.arena;
    let Some(folder) = paint.cel_target(arena, sequence, here.track) else {
        cx.notify(tr!("no-animation-folder-to-rename-cels"));
        return;
    };
    let (cels, label) = if call.name == "anim.cel.rename_layer" {
        (
            anim::cels(arena, here.picture, folder),
            tr!("rename-in-layer-order"),
        )
    } else {
        (
            in_timeline_order(arena, here.picture, sequence, folder),
            tr!("rename-in-timeline-order"),
        )
    };
    let edit = numbered(arena, &cels, &label);
    if !edit.is_empty() {
        SuePaint::apply(cx, edit);
    }
}

// ----- State -----------------------------------------------------------------

/// Works while the canvas has a timeline.
fn on_timeline(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    super::when(paint.timeline_here(facts.arena).is_some())
}

/// Checked while the timeline is on.
fn timeline_on(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    flag(paint.timeline_on)
}

/// Checked while onion skin is on.
fn onion_on(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    flag(paint.onion)
}

/// Checked while playback loops.
fn loop_on(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    flag(paint.looping)
}

/// Checked while the light table is on.
fn light_table_on(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    flag(paint.light_table)
}

/// Checked in the light table's color mode.
fn light_color_on(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    flag(paint.light_color)
}

/// Checked while the cel's light table shows.
fn light_cel_on(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    flag(paint.light_cel)
}

/// Checked while the general light table shows.
fn light_general_on(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    flag(paint.light_general)
}

/// Checked while the opacity acts on the general light table.
fn opacity_general_on(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    flag(paint.opacity_general)
}

/// Checked while the Light Table tool is on.
fn light_tool_on(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    flag(paint.light_tool)
}

/// Checked while a cel is locked as the editing target.
fn cel_locked(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    flag(paint.cel_lock.is_some())
}

// ----- The table -------------------------------------------------------------

/// A command of this group: it works while a canvas is open, and Auto
/// Actions may record it.
const fn c(
    name: &'static str,
    label: &'static str,
    icon: &'static str,
    run: fn(&mut SuePaint, &mut AppletContext<'_>, Call<'_>),
) -> Command<SuePaint> {
    Command::new(name, label, "anim", run)
        .icon(icon)
        .state(doc_open)
        .with(Flags::RECORDABLE)
}

/// Clip Studio Paint ships no default key for any Animation command, and
/// neither does Sue: each can be given one in Settings. No Animation
/// command leaves the canvas, so none is flagged
/// [`Flags::LEAVES_CANVAS`]; every one is [`Flags::RECORDABLE`], as
/// Auto Actions always recorded them.
pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    c(
        "anim.timeline.new",
        "new-timeline-ellipsis",
        "timeline-new",
        timeline_new,
    ),
    c(
        "anim.timeline.edit",
        "edit-timeline-ellipsis",
        "settings",
        timeline_edit,
    ),
    c(
        "anim.exposure_sheet",
        "exposure-sheet-ellipsis",
        "table",
        exposure_sheet,
    ),
    c(
        "anim.timeline.enable",
        "enable-timeline",
        "palette-timeline",
        timeline_enable,
    )
    .state(timeline_on),
    c(
        "anim.layer.folder",
        "animation-folder-2",
        "animation-folder",
        layer_folder,
    ),
    c("anim.layer.camera", "camera-folder", "film", layer_camera),
    c("anim.cel.new", "new-animation-cel", "cel-new", cel_new),
    c(
        "anim.cel.assign",
        "assign-animation-cel",
        "cel-specify",
        cel_assign,
    ),
    c("anim.key.add", "add-keyframe", "cel-specify", cel_assign),
    c("anim.key.delete", "delete-keyframe", "minus", key_delete),
    c(
        "anim.frame.insert",
        "insert-frame",
        "plus",
        frame_insert_delete,
    ),
    c(
        "anim.frame.delete",
        "delete-frame",
        "delete",
        frame_insert_delete,
    ),
    c("anim.onion", "enable-onion-skin", "onion-skin", onion).state(onion_on),
    c(
        "anim.onion_settings",
        "onion-skin-settings-ellipsis",
        "settings",
        onion_settings,
    ),
    c(
        "anim.light_table",
        "enable-light-table",
        "light-table",
        light_table,
    )
    .state(light_table_on),
    c(
        "anim.light_table.register",
        "register-selected-layer",
        "light-table-add",
        light_register,
    ),
    c(
        "anim.light_table.clear",
        "deregister-all-images-light-table",
        "delete",
        light_clear,
    ),
    c("anim.play", "play-stop", "play", play),
    c("anim.first", "go-to-start", "frame-first", go),
    c("anim.last", "go-to-end", "frame-last", go),
    c("anim.prev", "go-to-previous-frame", "frame-prev", go),
    c("anim.next", "go-to-next-frame", "frame-next", go),
    c("anim.loop", "loop", "loop", looping).state(loop_on),
    c("anim.camera.key", "camera-keyframe", "plus", camera_key),
    c(
        "anim.camera.unkey",
        "delete-camera-keyframe",
        "minus",
        camera_unkey,
    ),
    c(
        "anim.light_table.reverse",
        "reverse-layers-vertically-light-table",
        "reverse",
        light_reverse,
    ),
    c(
        "anim.light_table.color",
        "color-mode",
        "layer-color-mode",
        light_color,
    )
    .state(light_color_on),
    c(
        "anim.light_table.cel",
        "show-cel-light-table",
        "cel",
        light_cel,
    )
    .state(light_cel_on),
    c(
        "anim.light_table.general",
        "show-general-light-table",
        "layers",
        light_general,
    )
    .state(light_general_on),
    c(
        "anim.light_table.opacity_target",
        "switch-opacity-target",
        "layer-opacity-target",
        light_opacity_target,
    )
    .state(opacity_general_on),
    c(
        "anim.light_table.tool",
        "light-table-tool",
        "light-table-tool",
        light_tool,
    )
    .state(light_tool_on),
    c(
        "anim.light_table.reset",
        "reset-position-layers-light-table",
        "reset",
        light_reset,
    ),
    c(
        "anim.light_table.file",
        "select-and-register-file",
        "light-table-file",
        light_file,
    ),
    c(
        "anim.light_table.register_general",
        "register-layer-general-light-table",
        "light-table-add",
        light_register_general,
    ),
    c(
        "anim.light_table.deregister",
        "deregister-selected-light-table",
        "light-table-remove",
        light_deregister,
    ),
    c(
        "anim.cel.lock",
        "lock-cel-editing-target",
        "cel-lock",
        cel_lock,
    )
    .state(cel_locked),
    c(
        "anim.cel.prev",
        "select-previous-cel",
        "chevron-left",
        cel_step,
    ),
    c(
        "anim.cel.next",
        "select-next-cel",
        "chevron-right",
        cel_step,
    ),
    // In the command palette until the Animation menu's Edit Track
    // lists them (P3-2).
    c(
        "anim.cel.rename_timeline",
        "rename-in-timeline-order",
        "rename",
        rename_cels,
    )
    .state(on_timeline)
    .with(Flags::PALETTE),
    c(
        "anim.cel.rename_layer",
        "rename-in-layer-order",
        "rename",
        rename_cels,
    )
    .state(on_timeline)
    .with(Flags::PALETTE),
];

#[cfg(test)]
mod tests {
    use super::COMMANDS;
    use sue_wm::Flags;

    #[test]
    fn every_entry_is_of_the_group_with_a_real_icon() {
        for command in COMMANDS {
            assert_eq!(command.group, "anim", "{}", command.name);
            assert!(command.name.starts_with("anim."), "{}", command.name);
            assert!(
                sue_wm::icons::Icon::from_name(command.icon).is_some(),
                "{}: no icon called {:?}",
                command.name,
                command.icon
            );
        }
    }

    #[test]
    fn auto_actions_record_what_they_recorded() {
        // The flags stand for `extras::recordable`, which goes once every
        // group has them; until then the two must agree.
        for command in COMMANDS {
            assert_eq!(
                command.flags.has(Flags::RECORDABLE),
                crate::extras::recordable(command.name),
                "{}",
                command.name
            );
            assert!(!command.flags.has(Flags::LEAVES_CANVAS), "{}", command.name);
        }
    }

    #[test]
    fn the_studio_finds_each_entry_by_its_name() {
        // `anim_command` runs an entry by its name before the registry
        // does; every name is the registry's too, with the same entry.
        let registry = super::super::registry();
        for command in COMMANDS {
            let (found, _) = registry.find(command.name).expect("registered");
            assert_eq!(found.label, command.label, "{}", command.name);
            assert!(
                std::ptr::fn_addr_eq(found.run, command.run),
                "{}",
                command.name
            );
        }
    }
}
