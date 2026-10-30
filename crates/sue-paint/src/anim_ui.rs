//! Sue Paint's side of animation (Clip Studio Paint's Animation menu and
//! Timeline palette): making timelines, folders and cels, stepping and
//! playing frames, onion skin, and exporting an animated GIF or a
//! sequence of pictures.

use std::path::Path;

use egui::{Align2, Color32, Rect, Sense, Stroke, Ui, pos2, vec2};
use sue_core::EntityId;
use sue_core::txn::{Rename, SetProperty};
use sue_model::Sequence;
use sue_platform::write_atomic;
use sue_wm::{AppletContext, FilePick, FileType, ShellRequest, Theme};

use crate::anim;
use crate::composite::{self, RenderOptions};
use crate::doc::{self, NewPicture};
use crate::icons::Icon;
use crate::panels::icon_button;
use crate::studio::SuePaint;

type Edit = sue_core::txn::Batch<sue_core::Arena>;

/// A timeline button: its icon, hint, command, whether it can be used and
/// whether it is on.
type Button<'a> = (Icon, String, &'a str, bool, bool);

impl SuePaint {
    /// The current picture's timeline, cloned, and its length.
    /// The cel being drawn: the one the current folder shows at the
    /// current frame.
    pub(crate) fn editing_cel(&self, arena: &sue_core::Arena) -> Option<EntityId> {
        // A locked cel stays the one being drawn on every frame.
        if let Some(cel) = self.cel_lock.filter(|cel| arena.get_by_id(*cel).is_ok()) {
            return Some(cel);
        }
        let (_, sequence, _) = self.timeline_here(arena)?;
        let doc = self.doc()?;
        anim::folders(&sequence)
            .contains(&doc.track)
            .then(|| anim::cel_at(&sequence.tracks[doc.track], sequence.rate, doc.frame))
            .flatten()
    }

    fn timeline_here(&self, arena: &sue_core::Arena) -> Option<(EntityId, Sequence, u32)> {
        let picture = self.picture()?;
        let sequence = anim::timeline(arena, picture)?.clone();
        Some((picture, sequence, anim::length(arena, picture)))
    }

    /// Makes a new animation of `frames` frames at `fps`: a picture with a
    /// timeline whose first folder shows its first layer, renamed `1`.
    pub(crate) fn make_animation(
        &mut self,
        cx: &mut AppletContext<'_>,
        spec: &NewPicture,
        frames: u32,
        fps: u32,
    ) {
        let (edit, picture) = match doc::create(spec) {
            Ok(made) => made,
            Err(error) => {
                cx.fail(error);
                return;
            }
        };
        if !Self::apply(cx, edit) {
            return;
        }
        let timeline = anim::new_timeline(picture, fps, frames);
        if Self::apply(cx, timeline) {
            self.activate(&cx.world.arena, picture);
            if let Some(layer) = doc::layers(&cx.world.arena, picture)
                .into_iter()
                .rev()
                .find(|info| info.kind == sue_model::picture::LayerKind::Raster)
            {
                Self::apply(
                    cx,
                    Edit::new(tr!("name-cel")).then(Rename::new(layer.id, "1")),
                );
                if let Some((_, sequence, length)) = self.timeline_here(&cx.world.arena) {
                    let sequence = anim::assign(&sequence, 0, 0, Some(layer.id), picture, length);
                    Self::apply(cx, anim::set(picture, sequence, "Specify Cel"));
                }
            }
        }
    }

    /// Moves to `frame` (wrapped into the timeline) and makes the cel it
    /// shows in the current folder the current layer.
    pub(crate) fn go_to_frame(&mut self, arena: &sue_core::Arena, frame: i64) {
        let Some((_, sequence, length)) = self.timeline_here(arena) else {
            return;
        };
        let frame = frame.rem_euclid(i64::from(length)) as u32;
        let Some(doc) = self.doc_mut() else {
            return;
        };
        doc.frame = frame;
        let folders = anim::folders(&sequence);
        doc.track = doc.track.min(sequence.tracks.len().saturating_sub(1));
        if let Some(cel) = folders
            .contains(&doc.track)
            .then(|| anim::cel_at(&sequence.tracks[doc.track], sequence.rate, frame))
            .flatten()
        {
            doc.layer = Some(cel);
        }
    }

    /// Keeps the shown frame up to date: plays, and tells the compositor
    /// which cels this frame shows.
    pub(crate) fn animate(&mut self, cx: &mut AppletContext<'_>, ctx: &egui::Context) {
        let now = cx.view.now;
        let timeline = self
            .timeline_here(&cx.world.arena)
            .filter(|_| self.timeline_on);
        let Some((_, sequence, length)) = timeline else {
            if let Some(doc) = self.doc_mut() {
                doc.playing = None;
                doc.compositor.set_view(None);
            }
            return;
        };
        let looping = self.looping;
        let onion = self.onion;
        let mut moved = None;
        if let Some(doc) = self.doc_mut()
            && let Some((started, from)) = doc.playing
        {
            let elapsed = now.saturating_duration_since(started).as_secs_f64();
            let frame = i64::from(from) + (elapsed * anim::fps(&sequence)) as i64;
            if !looping && frame >= i64::from(length) {
                doc.playing = None;
                moved = Some(i64::from(length) - 1);
            } else {
                moved = Some(frame);
            }
            ctx.request_repaint();
        }
        if let Some(frame) = moved {
            self.go_to_frame(&cx.world.arena, frame);
        }
        self.sync_audio(cx, &sequence, length, moved);
        let settings = self
            .picture()
            .map(|picture| anim::Onion::of(&cx.world.arena, picture))
            .unwrap_or_default();
        // The light table of the cel being drawn.
        let traced = if self.light_table && self.light_cel {
            self.editing_cel(&cx.world.arena)
                .map(|cel| anim::light_table(&cx.world.arena, cel))
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let opacity = self.light_opacity;
        let light_color = self.light_color.then_some([0.25, 0.45, 1.0]);
        // The camera folders at the frame shown.
        let (arena, picture) = (&cx.world.arena, self.picture());
        let size = picture
            .and_then(|picture| arena.get_by_id(picture).ok())
            .and_then(sue_model::picture::size)
            .unwrap_or([0, 0]);
        let rate = sequence.rate;
        // The camera being set up shows its layers as they are, with its
        // frame over them, as in Clip Studio Paint.
        let editing = (self.toolbox.tool() == crate::tools::Tool::Operation)
            .then(|| self.camera_folder(arena))
            .flatten();
        let cameras_at = |frame: u32| {
            picture.map_or_else(Vec::new, |picture| {
                let mut views = crate::camera::views(
                    arena,
                    picture,
                    Some(crate::camera::time_of(frame, rate)),
                    size,
                );
                views.retain(|(folder, _)| Some(*folder) != editing);
                views
            })
        };
        if let Some(doc) = self.doc_mut() {
            let playing = doc.playing.is_some();
            let mut view = anim::view(
                &sequence,
                doc.frame,
                (onion && !playing).then_some(&settings),
            );
            view.cameras = cameras_at(doc.frame);
            if !playing {
                view = view.with_light_table(&traced, opacity, light_color);
            }
            doc.compositor.set_view(Some(view));
        }
    }

    /// Keeps the timeline's sound in step with playback: started from the
    /// frame shown when playback starts and at each loop, stopped with it.
    fn sync_audio(
        &mut self,
        cx: &mut AppletContext<'_>,
        sequence: &sue_model::Sequence,
        length: u32,
        playing_at: Option<i64>,
    ) {
        const OWNER: &str = "sue-paint";
        let Some(at) = playing_at.filter(|_| self.doc().is_some_and(|doc| doc.playing.is_some()))
        else {
            if self.audio_lap.take().is_some() && cx.transport.is_playing_for(OWNER) {
                cx.transport.stop(cx.audio.as_deref_mut());
            }
            return;
        };
        let length = i64::from(length.max(1));
        let lap = at.div_euclid(length);
        if self.audio_lap == Some(lap) {
            return;
        }
        self.audio_lap = Some(lap);
        // Mixed again only when the sound changed.
        let signature = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            for index in anim::audio_tracks(sequence) {
                let track = &sequence.tracks[index];
                track.muted.hash(&mut hasher);
                for clip in track.clips() {
                    clip.source.hash(&mut hasher);
                    clip.start.flicks().hash(&mut hasher);
                }
            }
            hasher.finish()
        };
        if self
            .audio_mix
            .as_ref()
            .is_none_or(|(seen, _)| *seen != signature)
        {
            self.audio_mix = anim::mix(&cx.world.arena, sequence)
                .map(|pcm| (signature, sue_core::Shared::new(pcm)));
        }
        let Some((_, mixed)) = self.audio_mix.clone() else {
            return;
        };
        let seconds = at.rem_euclid(length) as f64 / anim::fps(sequence).max(1.0);
        let start = (seconds * f64::from(anim::MIX_RATE)) as usize;
        let end = mixed.frames();
        if start >= end {
            if cx.transport.is_playing_for(OWNER) {
                cx.transport.stop(cx.audio.as_deref_mut());
            }
            return;
        }
        cx.transport.load(
            sue_wm::Clip {
                source: sue_wm::ClipSource::Stream(mixed),
                start,
                end,
                gain: 1.0,
                owner: OWNER.to_owned(),
            },
            cx.audio.as_deref_mut(),
        );
        cx.transport.set_playing(true, cx.audio.as_deref_mut());
    }

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
                } else {
                    Self::apply(cx, anim::new_timeline(picture, 24, 24));
                    self.panels.timeline = true;
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
                let Some((_, sequence, _)) = here else {
                    self.anim_command("anim.timeline.new", cx);
                    return true;
                };
                let name = tr!(
                    "animation-folder",
                    value = anim::folders(&sequence).len() + 1
                );
                let edit = anim::set(
                    picture,
                    anim::with_folder(&sequence, &name),
                    "New Animation Folder",
                );
                if Self::apply(cx, edit)
                    && let Some(doc) = self.doc_mut()
                {
                    doc.track = sequence.tracks.len();
                }
            }
            "anim.cel.new" => {
                if here.is_none() {
                    Self::apply(cx, anim::new_timeline(picture, 24, 24));
                    self.panels.timeline = true;
                }
                let Some((_, sequence, length)) = self.timeline_here(&cx.world.arena) else {
                    return true;
                };
                let track = if anim::folders(&sequence).contains(&track) {
                    track
                } else {
                    anim::folders(&sequence).first().copied().unwrap_or(0)
                };
                let count = anim::keys(&sequence.tracks[track], sequence.rate)
                    .iter()
                    .filter(|(_, cel)| cel.is_some())
                    .count();
                let below = self.doc().and_then(|doc| doc.layer);
                match doc::add_raster_layer(
                    &cx.world.arena,
                    picture,
                    below,
                    (count + 1).to_string(),
                    None,
                ) {
                    Ok((spawn, cel)) => {
                        let sequence =
                            anim::assign(&sequence, track, frame, Some(cel), picture, length);
                        let edit = Edit::new(tr!("new-animation-cel"))
                            .then(spawn)
                            .then(sue_core::txn::SetExtension::set(picture, sequence));
                        if Self::apply(cx, edit)
                            && let Some(doc) = self.doc_mut()
                        {
                            doc.layer = Some(cel);
                            doc.track = track;
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
            "file.export.gif" | "file.export.sequence" | "file.export.movie" => {
                self.ask_export_animation(cx, command);
            }
            _ => return false,
        }
        true
    }

    /// Asks where to export the animation, as a GIF or as numbered pictures.
    fn ask_export_animation(&mut self, cx: &mut AppletContext<'_>, command: &str) {
        let Some((picture, _, _)) = self.timeline_here(&cx.world.arena) else {
            cx.fail(tr!("make-timeline-first-animation-timeline-new"));
            return;
        };
        let name = cx
            .world
            .arena
            .get_by_id(picture)
            .map_or_else(|_| tr!("animation"), |entity| entity.name().to_owned());
        let (purpose, types, file) = if command == "file.export.gif" {
            (
                "anim-gif",
                vec![FileType::new(&tr!("animated-gif"), &["gif"])],
                format!("{name}.gif"),
            )
        } else if command == "file.export.movie" {
            (
                "anim-movie",
                vec![FileType::new(&tr!("motion-jpeg-avi-video"), &["avi"])],
                format!("{name}.avi"),
            )
        } else {
            (
                "anim-sequence",
                sue_io::PICTURE_SAVE_TYPES
                    .iter()
                    .map(|(what, extensions)| FileType::new(what, extensions))
                    .collect(),
                format!("{name}.png"),
            )
        };
        cx.request(ShellRequest::PickFile(
            FilePick::save(
                Some(sue_wm::AppletId::new(crate::ID).expect("a valid id")),
                purpose,
                &tr!("export-animation"),
                &file,
            )
            .types(types)
            .starting_in(self.last_dir.clone()),
        ));
    }

    /// Writes the animation to `path` as a Motion JPEG AVI video.
    pub(crate) fn export_movie(&mut self, cx: &mut AppletContext<'_>, path: &Path) {
        let Some((frames, fps)) = self.render_frames(&cx.world.arena) else {
            return;
        };
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.to_path_buf());
        }
        let Some(first) = frames.first() else {
            return;
        };
        let mut video =
            sue_io::MjpegAvi::new(first.width(), first.height(), fps.round().max(1.0) as u32);
        let result = frames
            .iter()
            .try_for_each(|frame| video.push_frame(frame, 90))
            .and_then(|()| {
                let count = video.frames();
                let bytes = video.finish();
                write_atomic(path, &bytes)
                    .map_err(|error| error.to_string())
                    .map(|()| (bytes.len(), count))
            });
        match result {
            Ok((size, count)) => cx.saved(
                path,
                tr!("frames-kib", value = size.div_ceil(1024), count = count),
            ),
            Err(error) => cx.fail(tr!("cannot-export-the-movie", error = error)),
        }
    }

    /// Every frame of the timeline, flattened.
    fn render_frames(&self, arena: &sue_core::Arena) -> Option<(Vec<sue_core::RasterTile2D>, f64)> {
        let (picture, sequence, length) = self.timeline_here(arena)?;
        let options = RenderOptions {
            paper: true,
            drafts: false,
        };
        let frames = (0..length)
            .filter_map(|frame| {
                let mut view = anim::view(&sequence, frame, None);
                // Seen through the camera folders, as the animation plays.
                view.cameras = crate::camera::views(
                    arena,
                    picture,
                    Some(crate::camera::time_of(frame, sequence.rate)),
                    arena
                        .get_by_id(picture)
                        .ok()
                        .and_then(sue_model::picture::size)
                        .unwrap_or([0, 0]),
                );
                composite::flatten_view(arena, picture, options, Some(&view))
            })
            .collect();
        Some((frames, anim::fps(&sequence)))
    }

    /// Writes the animation to `path`: one GIF, or `Name-0001.png` and on.
    pub(crate) fn export_animation(&mut self, cx: &mut AppletContext<'_>, path: &Path, gif: bool) {
        let Some((frames, fps)) = self.render_frames(&cx.world.arena) else {
            return;
        };
        if let Some(dir) = path.parent() {
            self.last_dir = Some(dir.to_path_buf());
        }
        if gif {
            let delay = (1000.0 / fps.max(1.0)).round() as u32;
            match sue_io::encode_gif_animation(&frames, delay).and_then(|bytes| {
                write_atomic(path, &bytes)
                    .map_err(|error| error.to_string())
                    .map(|()| bytes.len())
            }) {
                Ok(size) => cx.saved(
                    path,
                    tr!(
                        "frames-kib",
                        count = frames.len(),
                        value = size.div_ceil(1024)
                    ),
                ),
                Err(error) => cx.fail(tr!("cannot-export-the-animation", error = error)),
            }
            return;
        }
        let extension = path.extension().map_or_else(
            || "png".to_owned(),
            |ext| ext.to_string_lossy().to_ascii_lowercase(),
        );
        let stem = path
            .file_stem()
            .map_or_else(|| tr!("frame"), |stem| stem.to_string_lossy().into_owned());
        let dir = path.parent().map(Path::to_path_buf).unwrap_or_default();
        for (index, frame) in frames.iter().enumerate() {
            let target = dir.join(format!("{stem}-{:04}.{extension}", index + 1));
            if let Err(error) = sue_io::encode_as(frame, &extension, 90, 7)
                .and_then(|bytes| write_atomic(&target, &bytes).map_err(|error| error.to_string()))
            {
                cx.fail(tr!("cannot-export-frame", value = index + 1, error = error));
                return;
            }
        }
        cx.notify(tr!(
            "exported-frames-to",
            count = frames.len(),
            dir = dir.display()
        ));
    }

    /// The Timeline palette: its buttons, frame rate and length, and the
    /// folders' keys frame by frame.
    #[allow(clippy::too_many_lines, reason = "the whole palette")]
    /// The picture of `id` in `rect` (kept to its aspect), from the same
    /// thumbnails the layer palette keeps.
    fn cel_thumb(
        &mut self,
        painter: &egui::Painter,
        rect: Rect,
        id: EntityId,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let Ok(entity) = cx.world.arena.get_by_id(id) else {
            return;
        };
        let revision = entity.revision();
        if self
            .thumbs
            .get(&id)
            .is_none_or(|(had, _, _)| *had != revision)
            && let Some((pixels, size)) = composite::thumbnail(&cx.world.arena, id, [96, 72])
        {
            self.thumbs.insert(id, (revision, pixels, size));
        }
        let Some((revision, pixels, size)) = self.thumbs.get(&id) else {
            return;
        };
        let aspect = size[0] as f32 / size[1].max(1) as f32;
        let fit = if aspect > rect.width() / rect.height() {
            vec2(rect.width(), rect.width() / aspect)
        } else {
            vec2(rect.height() * aspect, rect.height())
        };
        let image = Rect::from_min_size(rect.min, fit);
        painter.rect_filled(image, 0.0, Color32::WHITE);
        if let Some(texture) = cx.textures.texture(
            crate::studio::texture_key(id, 0x7448_554d_4253),
            *revision,
            *size,
            4,
            pixels,
            false,
        ) {
            painter.image(
                texture,
                image,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        painter.rect_stroke(image, 0.0, theme.hairline(), egui::StrokeKind::Outside);
    }

    /// Clip Studio Paint's Timeline palette: its buttons, then a column
    /// of tracks (each with its eye, name and thumbnail) beside the frames,
    /// where every second is a band, every key shows its cel's picture and
    /// the current frame is a red bar.
    #[allow(clippy::too_many_lines, reason = "the whole Timeline palette")]
    pub(crate) fn timeline_ui(
        &mut self,
        ui: &mut Ui,
        bar: Rect,
        tracks: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let here = self.timeline_here(&cx.world.arena);
        let has = here.is_some();
        let playing = self.doc().is_some_and(|doc| doc.playing.is_some());
        // Clip Studio Paint's order: the timeline, zoom, playback, cels,
        // onion skin. A group starts after each `None`.
        let buttons: Vec<Option<Button<'_>>> = vec![
            Some((
                Icon::New,
                tr!("new-timeline-2"),
                "anim.timeline.new",
                !has,
                false,
            )),
            Some((
                Icon::Timeline,
                tr!("edit-timeline"),
                "anim.timeline.edit",
                has,
                false,
            )),
            Some((
                Icon::NewFolder,
                tr!("new-animation-folder"),
                "anim.layer.folder",
                has,
                false,
            )),
            None,
            Some((
                Icon::ZoomOut,
                tr!("zoom-out"),
                "timeline.zoom_out",
                has,
                false,
            )),
            Some((Icon::ZoomIn, tr!("zoom-in"), "timeline.zoom_in", has, false)),
            None,
            Some((Icon::First, tr!("first-frame"), "anim.first", has, false)),
            Some((Icon::Prev, tr!("previous-frame"), "anim.prev", has, false)),
            // One button, as in Clip Studio Paint: Play, and Stop while it plays.
            Some(if playing {
                (Icon::Stop, tr!("stop"), "anim.play", has, true)
            } else {
                (Icon::Play, tr!("play"), "anim.play", has, false)
            }),
            Some((Icon::Next, tr!("next-frame"), "anim.next", has, false)),
            Some((Icon::Last, tr!("last-frame"), "anim.last", has, false)),
            Some((Icon::Loop, tr!("loop"), "anim.loop", has, self.looping)),
            None,
            Some((
                Icon::Cel,
                tr!("new-animation-cel-2"),
                "anim.cel.new",
                true,
                false,
            )),
            Some((
                Icon::Edit,
                tr!("specify-cel-show-current-layer-frame"),
                "anim.cel.assign",
                has,
                false,
            )),
            Some((
                Icon::Minus,
                tr!("delete-key-frame"),
                "anim.key.delete",
                has,
                false,
            )),
            Some((
                Icon::Add,
                tr!("insert-a-frame-here"),
                "anim.frame.insert",
                has,
                false,
            )),
            Some((
                Icon::Delete,
                tr!("delete-this-frame"),
                "anim.frame.delete",
                has,
                false,
            )),
            None,
            Some((
                Icon::Onion,
                tr!("onion-skin"),
                "anim.onion",
                has,
                self.onion,
            )),
        ];
        let mut run = None;
        let mut x = bar.left() + 8.0;
        for button in &buttons {
            let Some((icon, tip, command, enabled, active)) = button else {
                ui.painter().vline(
                    x + 1.0,
                    bar.shrink2(vec2(0.0, 6.0)).y_range(),
                    theme.hairline(),
                );
                x += 8.0;
                continue;
            };
            let at = Rect::from_min_size(pos2(x, bar.top() + 3.0), vec2(22.0, 22.0));
            if at.right() > bar.right() - 150.0 {
                break;
            }
            if icon_button(ui, at, *icon, *enabled, *active, tip, theme) {
                run = Some(*command);
            }
            x += 25.0;
        }
        // At the right, as Clip Studio Paint: the current frame, then the
        // first and last frames played.
        if let Some((_, _, length)) = &here {
            let frame = self.doc().map_or(0, |doc| doc.frame);
            ui.painter().text(
                pos2(bar.right() - 12.0, bar.center().y),
                Align2::RIGHT_CENTER,
                format!("{}   /   1   /   {length}", frame + 1),
                theme.font(theme.small_size + 0.5),
                theme.text,
            );
        }
        match run {
            Some("timeline.zoom_out") => {
                self.timeline_step = Some((self.timeline_step.unwrap_or(15.0) / 1.25).max(4.0));
            }
            Some("timeline.zoom_in") => {
                self.timeline_step = Some((self.timeline_step.unwrap_or(15.0) * 1.25).min(60.0));
            }
            Some(command) => {
                self.anim_command(command, cx);
            }
            None => {}
        }

        // The track column, then the frames.
        let names = Rect::from_min_max(tracks.min, pos2(tracks.left() + 190.0, tracks.bottom()));
        ui.painter()
            .vline(names.right(), tracks.y_range(), theme.hairline());
        let Some((picture, sequence, length)) = self.timeline_here(&cx.world.arena) else {
            ui.painter().text(
                names.left_top() + vec2(10.0, 14.0),
                Align2::LEFT_CENTER,
                "No timeline",
                theme.font(theme.small_size + 0.5),
                theme.muted.gamma_multiply(0.7),
            );
            ui.painter().text(
                pos2(names.right() + 16.0, tracks.top() + 30.0),
                Align2::LEFT_CENTER,
                "New timeline (or New animation cel) starts an animation; File, New, Animation makes one",
                theme.font(theme.small_size + 0.5),
                theme.muted,
            );
            return;
        };
        let (frame, track) = self.doc().map_or((0, 0), |doc| (doc.frame, doc.track));
        let fps = anim::fps(&sequence).round().max(1.0) as u32;
        let row_height = 74.0;
        let ruler = 34.0;
        let frames_area = Rect::from_min_max(pos2(names.right(), tracks.top()), tracks.max);
        let step = self.timeline_step.unwrap_or_else(|| {
            ((frames_area.width() - 8.0) / length.max(1) as f32).clamp(10.0, 26.0)
        });
        let shown = ((frames_area.width() / step).ceil() as u32).max(1);
        let first = frame
            .saturating_sub(shown.saturating_sub(4))
            .min(length.saturating_sub(shown.saturating_sub(1)));
        let column = |at: u32| frames_area.left() + (at as f32 - first as f32) * step;
        let painter = ui.painter().with_clip_rect(frames_area);
        let below = Rect::from_min_max(
            pos2(frames_area.left(), frames_area.top() + ruler),
            frames_area.max,
        );
        // The timeline's name above the tracks, as Clip Studio Paint's
        // timeline list.
        let title = Rect::from_min_size(
            names.min + vec2(6.0, 6.0),
            vec2(names.width() - 12.0, ruler - 12.0),
        );
        ui.painter().rect_filled(title, 2.0, theme.pill);
        ui.painter()
            .rect_stroke(title, 2.0, theme.hairline(), egui::StrokeKind::Inside);
        let timeline_name = cx
            .world
            .arena
            .get_by_id(picture)
            .map_or_else(|_| tr!("timeline"), |entity| entity.name().to_owned());
        ui.painter().text(
            title.center(),
            Align2::CENTER_CENTER,
            timeline_name,
            theme.font(theme.small_size + 0.5),
            theme.text,
        );
        // Every other second a lighter band, its number above it.
        let last = (first + shown).min(length);
        for second in (first / fps)..=(last / fps) {
            let (from, to) = (second * fps, ((second + 1) * fps).min(length));
            let band = Rect::from_min_max(
                pos2(column(from), frames_area.top() + 14.0),
                pos2(column(to), frames_area.bottom()),
            );
            if second % 2 == 1 {
                painter.rect_filled(band, 0.0, theme.hover);
            }
            if second > 0 {
                painter.vline(
                    column(from),
                    frames_area.top()..=frames_area.top() + 14.0,
                    Stroke::new(1.0, theme.muted),
                );
                painter.text(
                    pos2(column(from) + 3.0, frames_area.top() + 7.0),
                    Align2::LEFT_CENTER,
                    second.to_string(),
                    theme.font(theme.small_size - 1.0),
                    theme.text,
                );
            }
        }
        painter.hline(
            frames_area.x_range(),
            frames_area.top() + 14.0,
            theme.hairline(),
        );
        // The frames: a line each, numbered every third (or sparser when
        // small), as Clip Studio Paint numbers them.
        let every = ((24.0 / step).ceil() as u32).max(1);
        let every = if every <= 3 { 3 } else { every.div_ceil(3) * 3 };
        for at in first..last {
            let x = column(at);
            painter.vline(
                x,
                below.y_range(),
                Stroke::new(1.0, theme.border.gamma_multiply(0.35)),
            );
            if at % every == 0 {
                painter.vline(
                    x,
                    frames_area.top() + 26.0..=frames_area.top() + ruler,
                    Stroke::new(1.0, theme.muted),
                );
                painter.text(
                    pos2(x + 2.0, frames_area.top() + 22.0),
                    Align2::LEFT_CENTER,
                    (at + 1).to_string(),
                    theme.font(theme.small_size - 1.0),
                    theme.muted,
                );
            }
        }
        painter.hline(frames_area.x_range(), below.top(), theme.hairline());
        // The end of the timeline: what lies past it is darker.
        if length < first + shown {
            painter.rect_filled(
                Rect::from_min_max(pos2(column(length), frames_area.top()), frames_area.max),
                0.0,
                theme.void.gamma_multiply(0.6),
            );
        }
        let ruler_rect = Rect::from_min_size(frames_area.min, vec2(frames_area.width(), ruler));
        let ruler_response = ui.interact(
            ruler_rect,
            ui.scope_id().with("timeline-ruler"),
            Sense::click_and_drag(),
        );
        if let Some(pos) = ruler_response.interact_pointer_pos()
            && (ruler_response.clicked() || ruler_response.dragged())
        {
            let at = first + ((pos.x - frames_area.left()) / step).max(0.0) as u32;
            self.go_to_frame(&cx.world.arena, i64::from(at.min(length - 1)));
        }
        // One tall row per folder: its name and the picture it shows now in
        // the column, and each key's cel as a picture in the frames.
        let mut action: Option<(usize, u32, Option<Option<EntityId>>)> = None;
        let names_painter = ui.painter().with_clip_rect(names);
        for (row, index) in anim::folders(&sequence).into_iter().enumerate() {
            let top = tracks.top() + ruler + row as f32 * row_height;
            let name_rect =
                Rect::from_min_size(pos2(names.left(), top), vec2(names.width(), row_height));
            let chosen = index == track;
            if chosen {
                names_painter.rect_filled(name_rect, 0.0, theme.active);
                painter.rect_filled(
                    Rect::from_min_size(
                        pos2(frames_area.left(), top),
                        vec2(frames_area.width(), row_height),
                    ),
                    0.0,
                    theme.active.gamma_multiply(0.35),
                );
            }
            names_painter.hline(names.x_range(), name_rect.bottom(), theme.hairline());
            painter.hline(frames_area.x_range(), name_rect.bottom(), theme.hairline());
            let line = pos2(name_rect.left() + 8.0, top + 11.0);
            crate::icons::paint(
                &names_painter,
                Rect::from_center_size(line + vec2(6.0, 0.0), vec2(14.0, 14.0)),
                Icon::Eye,
                theme.muted,
            );
            crate::icons::paint(
                &names_painter,
                Rect::from_center_size(line + vec2(26.0, 0.0), vec2(14.0, 14.0)),
                Icon::NewFolder,
                theme.muted,
            );
            names_painter.text(
                line + vec2(40.0, 0.0),
                Align2::LEFT_CENTER,
                format!("{} :", sequence.tracks[index].name),
                theme.font(theme.small_size + 0.5),
                if chosen { theme.text } else { theme.muted },
            );
            let keys = anim::keys(&sequence.tracks[index], sequence.rate);
            let thumb =
                |x: f32| Rect::from_min_size(pos2(x, top + 24.0), vec2(64.0, row_height - 30.0));
            if let Some(cel) = anim::cel_at(&sequence.tracks[index], sequence.rate, frame) {
                self.cel_thumb(
                    &names_painter,
                    thumb(name_rect.left() + 22.0),
                    cel,
                    cx,
                    theme,
                );
            }
            for (key, (at, cel)) in keys.iter().enumerate() {
                if *at >= last || *at >= length {
                    continue;
                }
                let end = keys
                    .get(key + 1)
                    .map_or(length, |(next, _)| *next)
                    .min(last);
                let x = column(*at);
                // The cel's name over its picture, and a line for as long
                // as it shows.
                let name = cel.map_or_else(
                    || "x".to_owned(),
                    |cel| {
                        cx.world
                            .arena
                            .get_by_id(cel)
                            .map_or_else(|_| "?".to_owned(), |entity| entity.name().to_owned())
                    },
                );
                painter.text(
                    pos2(x + 3.0, top + 11.0),
                    Align2::LEFT_CENTER,
                    name,
                    theme.font(theme.small_size),
                    theme.text,
                );
                if let Some(cel) = cel {
                    let width = ((end - at) as f32 * step - 2.0).clamp(8.0, 64.0);
                    let rect = Rect::from_min_size(
                        pos2(x + 1.0, top + 24.0),
                        vec2(width, row_height - 30.0),
                    );
                    self.cel_thumb(&painter, rect, *cel, cx, theme);
                    painter.hline(
                        (x + 1.0)..=column(end) - 1.0,
                        top + 20.0,
                        Stroke::new(1.0, theme.muted),
                    );
                }
            }
            // Cells: click to go there, double-click to show the current
            // layer from there, right-click for more.
            let cells = Rect::from_min_size(
                pos2(frames_area.left(), top),
                vec2(frames_area.width(), row_height),
            );
            let response = ui.interact(
                cells,
                ui.scope_id().with(("timeline-cells", index)),
                Sense::click(),
            );
            let name_click = ui.interact(
                name_rect,
                ui.scope_id().with(("timeline-folder", index)),
                Sense::click(),
            );
            if name_click.clicked()
                && let Some(doc) = self.doc_mut()
            {
                doc.track = index;
            }
            let at_pointer = |pos: egui::Pos2| {
                (first + ((pos.x - frames_area.left()) / step).max(0.0) as u32).min(length - 1)
            };
            if let Some(pos) = response.interact_pointer_pos() {
                if response.double_clicked() {
                    let layer = self.doc().and_then(|doc| doc.layer);
                    action = Some((index, at_pointer(pos), Some(layer)));
                } else if response.clicked() {
                    action = Some((index, at_pointer(pos), None));
                }
            }
            let pointer = response.hover_pos();
            response.context_menu(|ui| {
                let at = pointer.map_or(frame, at_pointer);
                if ui.button(tr!("show-the-current-layer-here")).clicked() {
                    action = Some((index, at, Some(self.doc().and_then(|doc| doc.layer))));
                    ui.close();
                }
                if ui.button(tr!("blank-from-here")).clicked() {
                    action = Some((index, at, Some(None)));
                    ui.close();
                }
            });
        }
        // The current frame: a red bar down the frames, as Clip Studio
        // Paint marks it.
        let red = Color32::from_rgb(196, 64, 64);
        let current = Rect::from_min_size(
            pos2(column(frame), frames_area.top() + 14.0),
            vec2(step.min(14.0), frames_area.height() - 14.0),
        );
        painter.rect_filled(current, 0.0, red.gamma_multiply(0.75));
        painter.text(
            current.center_top() + vec2(0.0, 8.0),
            Align2::CENTER_CENTER,
            (frame + 1).to_string(),
            theme.font(theme.small_size - 1.0),
            Color32::WHITE,
        );
        let row_height = 26.0;
        let ruler = ruler + anim::folders(&sequence).len() as f32 * 74.0
            - anim::folders(&sequence).len() as f32 * row_height;
        // One row per audio track: its clip as a bar to drag, and a menu.
        let rows = anim::folders(&sequence).len();
        let mut audio_edit: Option<(sue_model::Sequence, String)> = None;
        for (row, index) in anim::audio_tracks(&sequence).into_iter().enumerate() {
            let top = tracks.top() + ruler + (rows + row) as f32 * row_height;
            let track_ref = &sequence.tracks[index];
            let name_rect =
                Rect::from_min_size(pos2(names.left(), top), vec2(names.width(), row_height));
            names_painter.text(
                name_rect.left_center() + vec2(10.0, 0.0),
                Align2::LEFT_CENTER,
                format!(
                    "{}{}",
                    track_ref.name,
                    if track_ref.muted {
                        tr!("muted")
                    } else {
                        String::new()
                    }
                ),
                theme.font(theme.small_size + 0.5),
                theme.muted,
            );
            let Some(clip) = track_ref.clips().first() else {
                continue;
            };
            let begin = clip.start.frame(sequence.rate);
            let span = ((clip.duration.seconds() * anim::fps(&sequence)).ceil() as i64).max(1);
            let dragged_by = match self.audio_drag {
                Some((dragging, from, _)) if dragging == index => {
                    ui.ctx().pointer_latest_pos().map_or(0, |pos| {
                        (first as i64 + ((pos.x - frames_area.left()) / step) as i64) - from
                    })
                }
                _ => 0,
            };
            let shown_at = (begin + dragged_by).max(0);
            let x0 = frames_area.left() + (shown_at - first as i64) as f32 * step;
            let bar = Rect::from_min_max(
                pos2(x0, top + 4.0),
                pos2(x0 + span as f32 * step, top + row_height - 4.0),
            );
            painter.rect_filled(
                bar,
                3.0,
                theme
                    .active
                    .gamma_multiply(if track_ref.muted { 0.4 } else { 1.0 }),
            );
            painter.text(
                bar.left_center() + vec2(6.0, 0.0),
                Align2::LEFT_CENTER,
                "audio",
                theme.font(theme.small_size - 1.0),
                theme.text,
            );
            let row_rect = Rect::from_min_size(
                pos2(frames_area.left(), top),
                vec2(frames_area.width(), row_height),
            );
            let response = ui.interact(
                row_rect,
                ui.scope_id().with(("timeline-audio", index)),
                Sense::click_and_drag(),
            );
            let at_pointer =
                |pos: egui::Pos2| first as i64 + ((pos.x - frames_area.left()) / step) as i64;
            if response.drag_started()
                && let Some(pos) = response.interact_pointer_pos()
            {
                self.audio_drag = Some((index, at_pointer(pos), begin));
            }
            if response.drag_stopped()
                && let Some((dragging, from, was)) = self.audio_drag.take()
                && dragging == index
            {
                let to = response
                    .interact_pointer_pos()
                    .map_or(was, |pos| was + at_pointer(pos) - from);
                if to != was {
                    audio_edit = Some((anim::audio_moved(&sequence, index, to), tr!("move-audio")));
                }
            }
            response.context_menu(|ui| {
                let label = if track_ref.muted {
                    tr!("unmute")
                } else {
                    tr!("mute")
                };
                if ui.button(label).clicked() {
                    let mut toggled = sequence.clone();
                    toggled.tracks[index].muted = !track_ref.muted;
                    audio_edit = Some((
                        toggled,
                        if track_ref.muted {
                            tr!("unmute-audio")
                        } else {
                            tr!("mute-audio")
                        },
                    ));
                    ui.close();
                }
                if ui.button(tr!("delete-audio-track")).clicked() {
                    audio_edit = Some((
                        anim::without_track(&sequence, index),
                        tr!("delete-audio-track"),
                    ));
                    ui.close();
                }
            });
        }
        if let Some((changed, label)) = audio_edit {
            Self::apply(cx, anim::set(picture, changed, &label));
        }
        if let Some((index, at, assign)) = action {
            if let Some(doc) = self.doc_mut() {
                doc.track = index;
            }
            self.go_to_frame(&cx.world.arena, i64::from(at));
            if let Some(cel) = assign {
                let sequence = anim::assign(&sequence, index, at, cel, picture, length);
                Self::apply(cx, anim::set(picture, sequence, "Specify Cel"));
                self.go_to_frame(&cx.world.arena, i64::from(at));
            }
        }
    }
}
