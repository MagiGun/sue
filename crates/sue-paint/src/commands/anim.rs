//! The Animation menu's commands, and those of the timeline, the cels
//! and the light table palettes.

use sue_core::txn::SetProperty;
use sue_core::{Arena, UndoStack};
use sue_wm::{
    AppletContext, Call, Command, Facts, FilePick, FileType, ItemState, ShellRequest, World,
};

use super::{Basis, flag, when};
use crate::anim;
use crate::anim_ui::Edit;
use crate::studio::SuePaint;

/// How an item for a Animation command looks right now.
pub(super) fn state(paint: &SuePaint, facts: &Facts<'_>, call: Call<'_>) -> ItemState {
    paint.state_anim(call.name, facts.arena, facts.history)
}

impl SuePaint {
    /// How an item for a Animation command looks right now.
    fn state_anim(&self, command: &str, arena: &Arena, _history: &UndoStack<World>) -> ItemState {
        let Basis { has_doc, .. } = self.basis(arena);
        match command {
            "anim.timeline.enable" => flag(self.timeline_on),
            "anim.onion" => flag(self.onion),
            "anim.loop" => flag(self.looping),
            "anim.light_table" => flag(self.light_table),
            "anim.light_table.color" => flag(self.light_color),
            "anim.light_table.cel" => flag(self.light_cel),
            "anim.light_table.general" => flag(self.light_general),
            "anim.light_table.opacity_target" => flag(self.opacity_general),
            "anim.light_table.tool" => flag(self.light_tool),
            "anim.cel.lock" => flag(self.cel_lock.is_some()),
            _ => when(has_doc),
        }
    }
}

impl SuePaint {
    /// Runs an Animation menu command; `false` if `command` is not one.
    #[allow(clippy::too_many_lines, reason = "one arm per command")]
    pub(crate) fn anim_command(&mut self, command: &str, cx: &mut AppletContext<'_>) -> bool {
        let Some(picture) = self.picture() else {
            return matches!(command, "anim.timeline.new");
        };
        let here = self.timeline_here(&cx.world.arena);
        let (frame, track) = self.doc().map_or((0, 0), |doc| (doc.frame, doc.track));
        match command {
            "anim.timeline.new" => {
                if here.is_some() {
                    cx.notify(tr!("canvas-has-timeline-timeline-palette-edits"));
                } else if Self::apply(cx, anim::new_timeline(&cx.world.arena, picture, 24, 24)) {
                    self.panels.timeline = true;
                    if let Some((_, sequence, _)) = self.timeline_here(&cx.world.arena)
                        && let Some(folder) = sequence.tracks.first().and_then(anim::track_folder)
                        && let Some(doc) = self.doc_mut()
                    {
                        doc.track = 0;
                        doc.layer = Some(folder);
                    }
                }
            }
            "anim.layer.camera" => {
                let (edit, folder) =
                    crate::camera::new_camera(&cx.world.arena, picture, self.layer());
                if Self::apply(cx, edit)
                    && let Some(doc) = self.doc_mut()
                {
                    doc.layer = Some(folder);
                }
            }
            "anim.camera.key" => {
                if let Some(folder) = self.camera_folder(&cx.world.arena) {
                    let camera = self.camera_now(&cx.world.arena, folder);
                    let (_, time) = self.frame_time(&cx.world.arena);
                    let edit = crate::camera::key(&cx.world.arena, folder, time, camera);
                    Self::apply(cx, edit);
                    self.camera_pending = None;
                }
            }
            "anim.camera.unkey" => {
                if let Some(folder) = self.camera_folder(&cx.world.arena) {
                    let (_, time) = self.frame_time(&cx.world.arena);
                    if let Some(edit) = crate::camera::delete_key(&cx.world.arena, folder, time) {
                        Self::apply(cx, edit);
                    }
                    self.camera_pending = None;
                }
            }
            "anim.layer.folder" => {
                // A layer folder holding its cels, with its timeline track,
                // right above the current layer (Clip Studio Paint's New
                // Animation Folder).
                let opened = here.is_none();
                let (edit, folder, track) =
                    anim::add_folder(&cx.world.arena, picture, self.layer());
                if Self::apply(cx, edit) {
                    if opened {
                        self.panels.timeline = true;
                    }
                    if let Some(doc) = self.doc_mut() {
                        doc.track = track;
                        doc.layer = Some(folder);
                    }
                }
            }
            "anim.cel.new" => {
                // Into the animation folder picked (or the one the picked
                // cel is in, or the timeline's current one), as its top cel.
                let folder = here
                    .as_ref()
                    .and_then(|(_, sequence, _)| self.cel_target(&cx.world.arena, sequence, track));
                let folder = match folder {
                    Some(folder) => folder,
                    None => {
                        let opened = here.is_none();
                        let (edit, folder, _) =
                            anim::add_folder(&cx.world.arena, picture, self.layer());
                        if !Self::apply(cx, edit) {
                            return true;
                        }
                        if opened {
                            self.panels.timeline = true;
                        }
                        folder
                    }
                };
                match anim::new_cel(&cx.world.arena, picture, folder, frame) {
                    Ok((edit, cel)) => {
                        if Self::apply(cx, edit) {
                            let track = self
                                .timeline_here(&cx.world.arena)
                                .and_then(|(_, sequence, _)| anim::track_of(&sequence, folder));
                            if let Some(doc) = self.doc_mut() {
                                doc.layer = Some(cel);
                                doc.track = track.unwrap_or(doc.track);
                            }
                        }
                    }
                    Err(error) => cx.fail(error),
                }
            }
            "anim.cel.assign" | "anim.key.add" => {
                let (Some((_, sequence, length)), Some(layer)) =
                    (here, self.doc().and_then(|doc| doc.layer))
                else {
                    cx.fail(tr!("make-timeline-pick-layer-first"));
                    return true;
                };
                if !anim::may_show(&cx.world.arena, &sequence, track, layer) {
                    cx.notify(tr!("only-cels-of-this-animation-folder"));
                    return true;
                }
                let sequence = anim::assign(&sequence, track, frame, Some(layer), picture, length);
                Self::apply(cx, anim::set(picture, sequence, "Specify Cel"));
            }
            "anim.key.delete" => {
                if let Some((_, sequence, length)) = here {
                    let sequence = anim::unassign(&sequence, track, frame, length);
                    Self::apply(cx, anim::set(picture, sequence, "Delete Key"));
                }
            }
            "anim.frame.insert" | "anim.frame.delete" => {
                let Some((_, sequence, length)) = here else {
                    return true;
                };
                let (by, new_length) = if command == "anim.frame.insert" {
                    (1, length + 1)
                } else {
                    (-1, length.saturating_sub(1).max(1))
                };
                let sequence = anim::shift(&sequence, frame, by, new_length);
                let edit = Edit::new(if by > 0 {
                    tr!("insert-frame")
                } else {
                    tr!("delete-frame")
                })
                .then(sue_core::txn::SetExtension::set(picture, sequence))
                .then(SetProperty::new(
                    picture,
                    anim::LENGTH,
                    i64::from(new_length),
                ));
                Self::apply(cx, edit);
            }
            "anim.onion" => self.onion = !self.onion,
            "anim.onion_settings" => {
                if let Some(picture) = self.picture() {
                    self.dialog = Some(crate::dialogs::Dialog::Onion(anim::Onion::of(
                        &cx.world.arena,
                        picture,
                    )));
                }
            }
            "anim.light_table" => self.light_table = !self.light_table,
            "anim.light_table.register" => {
                let (Some(cel), Some(layer)) = (self.editing_cel(&cx.world.arena), self.layer())
                else {
                    cx.notify(tr!("go-frame-cel-give-light-table"));
                    return true;
                };
                if cel == layer {
                    cx.notify(tr!("pick-layer-trace-layer-palette-then"));
                } else if anim::light_table(&cx.world.arena, cel).contains(&layer) {
                    cx.notify(tr!("layer-cels-light-table-already"));
                } else if Self::apply(cx, anim::register(cel, layer)) {
                    self.light_table = true;
                }
            }
            "anim.light_table.reverse" => {
                if let Some(cel) = self.editing_cel(&cx.world.arena)
                    && anim::light_table(&cx.world.arena, cel).len() > 1
                {
                    Self::apply(cx, anim::reversed(&cx.world.arena, cel));
                }
            }
            "anim.light_table.color" => self.light_color = !self.light_color,
            "anim.light_table.general" => {
                self.light_general = !self.light_general;
                if self.light_general {
                    self.light_table = true;
                }
            }
            "anim.light_table.opacity_target" => self.opacity_general = !self.opacity_general,
            "anim.light_table.tool" => {
                self.light_tool = !self.light_tool;
                if self.light_tool {
                    cx.notify(tr!("light-table-tool-on"));
                }
            }
            "anim.light_table.reset" => {
                let mut layers = self
                    .editing_cel(&cx.world.arena)
                    .map(|cel| anim::light_table(&cx.world.arena, cel))
                    .unwrap_or_default();
                layers.extend(anim::general_light_table(&cx.world.arena, picture));
                let edit = anim::reset_places(&cx.world.arena, &layers);
                if !edit.is_empty() {
                    Self::apply(cx, edit);
                }
            }
            "anim.light_table.register_general" => {
                let Some(layer) = self.layer() else {
                    cx.notify(tr!("pick-layer-trace-layer-palette-then"));
                    return true;
                };
                if anim::general_light_table(&cx.world.arena, picture).contains(&layer) {
                    cx.notify(tr!("layer-cels-light-table-already"));
                } else if Self::apply(cx, anim::register_general(picture, layer)) {
                    self.light_table = true;
                    self.light_general = true;
                }
            }
            "anim.light_table.file" => {
                if self.editing_cel(&cx.world.arena).is_none() {
                    cx.notify(tr!("go-frame-cel-see-light-table"));
                    return true;
                }
                let start = self
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
            "anim.light_table.cel" => self.light_cel = !self.light_cel,
            "anim.light_table.deregister" => {
                if let (Some(cel), Some(layer)) = (self.editing_cel(&cx.world.arena), self.layer())
                    && anim::light_table(&cx.world.arena, cel).contains(&layer)
                {
                    Self::apply(cx, anim::deregister(cel, &[layer]));
                } else {
                    cx.notify(tr!("pick-registered-layer-to-deregister"));
                }
            }
            "anim.cel.lock" => {
                self.cel_lock = match self.cel_lock {
                    Some(_) => None,
                    None => self.editing_cel(&cx.world.arena),
                };
            }
            "anim.cel.prev" | "anim.cel.next" => {
                if let Some((_, sequence, _)) = &here
                    && let Some(doc) = self.doc()
                    && let Some(track) = sequence.tracks.get(doc.track)
                {
                    let keys = anim::keys(track, sequence.rate);
                    let at = doc.frame;
                    let to = if command == "anim.cel.prev" {
                        keys.iter().rev().find(|(key, _)| *key < at)
                    } else {
                        keys.iter().find(|(key, _)| *key > at)
                    };
                    if let Some((frame, _)) = to {
                        self.cel_lock = None;
                        self.go_to_frame(&cx.world.arena, i64::from(*frame));
                    }
                }
            }
            "anim.light_table.clear" => {
                if let Some(cel) = self.editing_cel(&cx.world.arena) {
                    let layers = anim::light_table(&cx.world.arena, cel);
                    if !layers.is_empty() {
                        Self::apply(cx, anim::deregister(cel, &layers));
                    }
                }
            }
            "anim.loop" => self.looping = !self.looping,
            "anim.play" => {
                let now = cx.view.now;
                if let Some(doc) = self.doc_mut() {
                    doc.playing = match doc.playing {
                        Some(_) => None,
                        None => Some((now, doc.frame)),
                    };
                }
            }
            "anim.next" => self.go_to_frame(&cx.world.arena, i64::from(frame) + 1),
            "anim.prev" => self.go_to_frame(&cx.world.arena, i64::from(frame) - 1),
            "anim.first" => self.go_to_frame(&cx.world.arena, 0),
            "file.export.exposure_sheet" | "file.import.exposure_sheet" => {
                let Some((picture, _, _)) = &here else {
                    cx.fail(tr!("make-timeline-first-animation-timeline-new"));
                    return true;
                };
                let name = cx
                    .world
                    .arena
                    .get_by_id(*picture)
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
            "anim.exposure_sheet" => {
                if here.is_some() {
                    self.dialog = Some(crate::dialogs::Dialog::Exposure);
                } else {
                    cx.fail(tr!("make-timeline-first-animation-timeline-new"));
                }
            }
            "anim.timeline.edit" => {
                if let Some((_, sequence, length)) = &here {
                    self.dialog = Some(crate::dialogs::Dialog::Timeline(
                        anim::fps(sequence).round() as u32,
                        *length,
                    ));
                } else {
                    cx.fail(tr!("make-timeline-first-animation-timeline-new"));
                }
            }
            "anim.timeline.enable" => self.timeline_on = !self.timeline_on,
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
            "anim.last" => self.go_to_frame(&cx.world.arena, -1),
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

/// A command of this group.
const fn c(name: &'static str, label: &'static str) -> Command<SuePaint> {
    Command::new(name, label, "anim", super::nothing).state(state)
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    c("anim.timeline.new", "new-timeline-ellipsis"),
    c("anim.timeline.edit", "edit-timeline-ellipsis"),
    c("anim.exposure_sheet", "exposure-sheet-ellipsis"),
    c("anim.timeline.enable", "enable-timeline"),
    c("anim.layer.folder", "animation-folder-2"),
    c("anim.layer.camera", "camera-folder"),
    c("anim.cel.new", "new-animation-cel"),
    c("anim.cel.assign", "assign-animation-cel"),
    c("anim.key.add", "add-keyframe"),
    c("anim.key.delete", "delete-keyframe"),
    c("anim.frame.insert", "insert-frame"),
    c("anim.frame.delete", "delete-frame"),
    c("anim.onion", "enable-onion-skin"),
    c("anim.onion_settings", "onion-skin-settings-ellipsis"),
    c("anim.light_table", "enable-light-table"),
    c("anim.light_table.register", "register-selected-layer"),
    c(
        "anim.light_table.clear",
        "deregister-all-images-light-table",
    ),
    c("anim.play", "play-stop"),
    c("anim.first", "go-to-start"),
    c("anim.last", "go-to-end"),
    c("anim.prev", "go-to-previous-frame"),
    c("anim.next", "go-to-next-frame"),
    c("anim.loop", "loop"),
    c("anim.camera.key", "camera-keyframe"),
    c("anim.camera.unkey", "delete-camera-keyframe"),
    c(
        "anim.light_table.reverse",
        "reverse-layers-vertically-light-table",
    ),
    c("anim.light_table.color", "color-mode"),
    c("anim.light_table.cel", "show-cel-light-table"),
    c("anim.light_table.general", "show-general-light-table"),
    c("anim.light_table.opacity_target", "switch-opacity-target"),
    c("anim.light_table.tool", "light-table-tool"),
    c(
        "anim.light_table.reset",
        "reset-position-layers-light-table",
    ),
    c("anim.light_table.file", "select-and-register-file"),
    c(
        "anim.light_table.register_general",
        "register-layer-general-light-table",
    ),
    c(
        "anim.light_table.deregister",
        "deregister-selected-light-table",
    ),
    c("anim.cel.lock", "lock-cel-editing-target"),
    c("anim.cel.prev", "select-previous-cel"),
    c("anim.cel.next", "select-next-cel"),
];
