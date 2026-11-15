//! What a timeline is set up with, as Clip Studio Paint's New, New
//! timeline, Change settings, Playback settings and Label menus have it:
//! a track with a clip for every layer and folder ([`LAYER`]), several
//! named timelines a canvas switches between, the scene, shot and
//! division line, the playback range, timeline labels, the playback
//! settings (Render 2D camera among them) and the output frame with its
//! blank space and overflow frame.
//!
//! Each is a property of the picture with a stable name, written as
//! plain text so a person can read it; `docs/animation-data.md` lists
//! them.

use sue_core::txn::{SetExtension, SetProperty, SpawnEntity};
use sue_core::{Arena, Entity, EntityId, Rate, Relation, Time, Value};
use sue_model::{Clip, Sequence, Track};

use super::{CELS, Edit, LENGTH, MIX_RATE, TRACK_FOLDER, length, timeline};
use crate::composite::FrameView;

/// The kind of track that shows a layer or folder over the clips it has:
/// outside them the layer is hidden.
pub const LAYER: &str = "sue.paint.layer";
/// The layer or folder a [`LAYER`] track stands for (a track property).
pub const TRACK_LAYER: &str = "anim.layer";
/// The kind of track of a 2D camera folder (its keys are the folder's
/// [`sue_model::Animation`], see [`crate::camera`]).
pub const CAMERA: &str = "sue.paint.camera";

/// The timeline's name (`Text`).
pub const NAME: &str = "anim.timeline.name";
/// The scene number (`Int`).
pub const SCENE: &str = "anim.scene";
/// The shot number (`Int`).
pub const SHOT: &str = "anim.shot";
/// Every how many frames the timeline draws a division line (`Int`).
pub const DIVISION: &str = "anim.division";
/// The first frame played and exported (`Int`, from 0).
pub const PLAY_START: &str = "anim.play.start";
/// The last frame played and exported (`Int`, from 0).
pub const PLAY_END: &str = "anim.play.end";
/// The timeline labels (`Text`, a `frame:name` line each).
pub const LABELS: &str = "anim.labels";
/// The playback settings (`Text`, see [`Playback::to_text`]).
pub const PLAYBACK: &str = "anim.playback";
/// The output frame (`Text`, see [`OutputFrame::to_text`]).
pub const OUTPUT: &str = "anim.output";
/// The relation from a picture to each timeline not shown now.
pub const OTHER_TIMELINE: &str = "sue.paint.timeline";
/// The class of a timeline kept aside: its sequence and settings.
pub const TIMELINE_CLASS: &str = "sue.paint.timeline";

/// The tracks of `sequence` that stand for a layer or folder, by index.
#[must_use]
pub fn layer_tracks(sequence: &Sequence) -> Vec<usize> {
    (0..sequence.tracks.len())
        .filter(|index| sequence.tracks[*index].kind == LAYER)
        .collect()
}

/// The layer a [`LAYER`] track stands for.
#[must_use]
pub fn track_layer(track: &Track) -> Option<EntityId> {
    (track.kind == LAYER)
        .then(|| track.properties.get(TRACK_LAYER).and_then(Value::as_entity))
        .flatten()
}

/// The track of `sequence` standing for `layer` (a layer track, or the
/// track of the camera folder it is).
#[must_use]
pub fn track_of_layer(sequence: &Sequence, layer: EntityId) -> Option<usize> {
    (0..sequence.tracks.len()).find(|index| {
        let track = &sequence.tracks[*index];
        track_layer(track) == Some(layer)
            || (track.kind == CAMERA && super::track_folder(track) == Some(layer))
    })
}

/// A track for `layer` with one clip over all `length` frames.
#[must_use]
pub fn layer_track(name: &str, layer: EntityId, rate: Rate, length: u32) -> Track {
    let mut track = Track::new(name, LAYER);
    track.properties.set(TRACK_LAYER, Value::Entity(layer));
    track.insert(Clip::new(
        layer,
        Time::ZERO,
        Time::from_frames(i64::from(length.max(1)), rate),
    ));
    track
}

/// A track for the camera folder `folder`, over all `length` frames.
#[must_use]
pub fn camera_track(name: &str, folder: EntityId, rate: Rate, length: u32) -> Track {
    let mut track = Track::new(name, CAMERA);
    track.properties.set(TRACK_FOLDER, Value::Entity(folder));
    track.insert(Clip::new(
        folder,
        Time::ZERO,
        Time::from_frames(i64::from(length.max(1)), rate),
    ));
    track
}

/// The layers and folders hidden at `frame` because their track has no
/// clip there.
#[must_use]
pub fn off_clip(sequence: &Sequence, frame: u32) -> Vec<EntityId> {
    let time = Time::from_frames(i64::from(frame), sequence.rate);
    layer_tracks(sequence)
        .into_iter()
        .filter_map(|index| {
            let track = &sequence.tracks[index];
            let layer = track_layer(track)?;
            (track.at(time).next().is_none()).then_some(layer)
        })
        .collect()
}

/// A timeline's settings: its name, scene and shot, division line and
/// playback range (Clip Studio Paint's New timeline and Change settings).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    /// Its name.
    pub name: String,
    /// The scene number.
    pub scene: u32,
    /// The shot number.
    pub shot: u32,
    /// Every how many frames a division line shows.
    pub division: u32,
    /// The first frame played (from 0).
    pub start: u32,
    /// The last frame played (from 0), `None` for the last frame.
    pub end: Option<u32>,
}

impl Settings {
    /// The settings a new timeline named `name` starts with.
    #[must_use]
    pub fn named(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            scene: 1,
            shot: 1,
            division: 6,
            start: 0,
            end: None,
        }
    }

    /// The settings of `picture`'s timeline.
    #[must_use]
    pub fn of(arena: &Arena, picture: EntityId) -> Self {
        let Ok(entity) = arena.get_by_id(picture) else {
            return Self::named(tr!("timeline-1"));
        };
        let int = |key: &str| entity.property(key).and_then(Value::as_i64);
        let count =
            |key: &str, or: u32| int(key).map_or(or, |value| value.clamp(0, 100_000) as u32);
        Self {
            name: entity
                .property(NAME)
                .and_then(Value::as_str)
                .map_or_else(|| tr!("timeline-1"), str::to_owned),
            scene: count(SCENE, 1),
            shot: count(SHOT, 1),
            division: count(DIVISION, 6).max(1),
            start: count(PLAY_START, 0),
            end: int(PLAY_END).map(|value| value.clamp(0, 100_000) as u32),
        }
    }

    /// The frames played, first and last, on a timeline `length` long.
    #[must_use]
    pub fn range(&self, length: u32) -> (u32, u32) {
        let last = length.saturating_sub(1);
        let end = self.end.unwrap_or(last).min(last);
        (self.start.min(end), end)
    }

    /// The edit that gives `picture` these settings (Change settings).
    #[must_use]
    pub fn apply(&self, picture: EntityId) -> Edit {
        let mut edit = Edit::new(tr!("change-settings"))
            .then(SetProperty::new(picture, NAME, self.name.clone()))
            .then(SetProperty::new(picture, SCENE, i64::from(self.scene)))
            .then(SetProperty::new(picture, SHOT, i64::from(self.shot)))
            .then(SetProperty::new(
                picture,
                DIVISION,
                i64::from(self.division),
            ))
            .then(SetProperty::new(picture, PLAY_START, i64::from(self.start)));
        edit = match self.end {
            Some(end) => edit.then(SetProperty::new(picture, PLAY_END, i64::from(end))),
            None => edit.then(SetProperty::remove(picture, PLAY_END)),
        };
        edit
    }
}

/// The edit that gives `picture` a timeline as Clip Studio Paint's New
/// timeline does: `length` frames at `fps`, no animation folder made,
/// and every layer and folder a track with a clip over all of it (an
/// animation folder its cel track, a camera folder its camera track).
/// When the picture has a timeline already, that one is kept aside and
/// stays one pick away ([`timelines`]).
#[must_use]
pub fn create_timeline(
    arena: &Arena,
    picture: EntityId,
    fps: u32,
    length: u32,
    settings: &Settings,
) -> Edit {
    let length = length.max(1);
    let rate = Rate::new(fps.max(1), 1);
    let mut sequence = Sequence::new(rate, MIX_RATE);
    let mut stack = crate::doc::layers(arena, picture);
    stack.reverse();
    for info in stack {
        if super::cel_folder(arena, info.id).is_some()
            || crate::doc::folders_around(arena, info.id)
                .iter()
                .any(|folder| super::is_animation_folder(arena, *folder))
        {
            continue;
        }
        if super::is_animation_folder(arena, info.id) {
            sequence = super::with_linked_folder(&sequence, &info.name, info.id);
        } else if crate::camera::is_camera(arena, info.id) {
            sequence
                .tracks
                .push(camera_track(&info.name, info.id, rate, length));
        } else {
            sequence
                .tracks
                .push(layer_track(&info.name, info.id, rate, length));
        }
    }
    let mut edit = Edit::new(tr!("new-timeline"));
    if let Some(aside) = kept_aside(arena, picture) {
        // The new one goes last in the picker.
        let count = timelines(arena, picture).len() as i64;
        edit = edit
            .then(aside)
            .then(SetProperty::new(picture, ORDER, count));
    }
    edit.then(SetExtension::set(picture, sequence))
        .then(SetProperty::new(picture, LENGTH, i64::from(length)))
        .then(settings.apply(picture))
}

/// Every key that travels with a timeline when it is kept aside.
const TIMELINE_KEYS: [&str; 8] = [
    LENGTH, NAME, SCENE, SHOT, DIVISION, PLAY_START, PLAY_END, LABELS,
];

/// The spawn of an entity holding `picture`'s timeline as it is now (its
/// sequence and the keys that travel with it), tied to the picture.
fn kept_aside(arena: &Arena, picture: EntityId) -> Option<SpawnEntity> {
    let entity = arena.get_by_id(picture).ok()?;
    let sequence = timeline(arena, picture)?.clone();
    let order = entity
        .property(ORDER)
        .and_then(Value::as_i64)
        .unwrap_or_else(|| timelines(arena, picture).len() as i64 - 1);
    let mut aside = Entity::empty(Settings::of(arena, picture).name, TIMELINE_CLASS)
        .with_ext(sequence)
        .with_property(ORDER, order.max(0));
    for key in TIMELINE_KEYS {
        if let Some(value) = entity.property(key) {
            aside = aside.with_property(key, value.clone());
        }
    }
    let id = aside.id();
    Some(SpawnEntity::new(aside).with_relation(Relation::new(picture, OTHER_TIMELINE, id)))
}

/// Where a timeline sits in the picker (`Int`, on the picture for the
/// one shown).
pub const ORDER: &str = "anim.timeline.order";

/// One of a picture's timelines, as the picker lists them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The entity it is kept aside in; `None` for the one shown.
    pub aside: Option<EntityId>,
    /// Its name.
    pub name: String,
}

/// The timelines of `picture` in the picker's order: the one shown and
/// those kept aside. Empty without a timeline.
#[must_use]
pub fn timelines(arena: &Arena, picture: EntityId) -> Vec<Entry> {
    if timeline(arena, picture).is_none() {
        return Vec::new();
    }
    let order = |entity: &Entity| entity.property(ORDER).and_then(Value::as_i64).unwrap_or(0);
    let mut out: Vec<(i64, Entry)> = arena
        .relations()
        .targets(picture, OTHER_TIMELINE)
        .into_iter()
        .filter_map(|id| {
            let entity = arena.get_by_id(id).ok()?;
            Some((
                order(entity),
                Entry {
                    aside: Some(id),
                    name: entity.name().to_owned(),
                },
            ))
        })
        .collect();
    let shown = arena.get_by_id(picture).map_or(0, |entity| {
        entity
            .property(ORDER)
            .and_then(Value::as_i64)
            .unwrap_or(out.len() as i64)
    });
    out.push((
        shown,
        Entry {
            aside: None,
            name: Settings::of(arena, picture).name,
        },
    ));
    out.sort_by_key(|(order, _)| *order);
    out.into_iter().map(|(_, entry)| entry).collect()
}

/// The edit that shows the timeline kept aside in `aside` and keeps the
/// one shown in its place (the picker, Go to previous or next timeline).
#[must_use]
pub fn switch_timeline(arena: &Arena, picture: EntityId, aside: EntityId) -> Option<Edit> {
    let shown = arena.get_by_id(picture).ok()?;
    let other = arena.get_by_id(aside).ok()?;
    let theirs = other.ext::<Sequence>()?.clone();
    let ours = timeline(arena, picture)?.clone();
    let mut edit = Edit::new(tr!("switch-timeline"))
        .then(SetExtension::set(picture, theirs))
        .then(SetExtension::set(aside, ours));
    for key in TIMELINE_KEYS.into_iter().chain([ORDER]) {
        let mine = shown.property(key).cloned();
        let other_value = other.property(key).cloned();
        edit = match other_value {
            Some(value) => edit.then(SetProperty::new(picture, key, value)),
            None => edit.then(SetProperty::remove(picture, key)),
        };
        edit = match mine {
            Some(value) => edit.then(SetProperty::new(aside, key, value)),
            None => edit.then(SetProperty::remove(aside, key)),
        };
    }
    let (ours_name, theirs_name) = (Settings::of(arena, picture).name, other.name().to_owned());
    edit = edit.then(sue_core::txn::Rename::new(aside, ours_name));
    if other.property(NAME).is_none() {
        edit = edit.then(SetProperty::new(picture, NAME, theirs_name));
    }
    Some(edit)
}

/// A timeline label: a name on a frame (Create timeline label).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Label {
    /// The frame (from 0).
    pub frame: u32,
    /// Its name.
    pub name: String,
}

/// `picture`'s timeline labels, first frame first.
#[must_use]
pub fn labels(arena: &Arena, picture: EntityId) -> Vec<Label> {
    let mut out: Vec<Label> = arena
        .get_by_id(picture)
        .ok()
        .and_then(|entity| entity.property(LABELS))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let (frame, name) = line.split_once(':')?;
            Some(Label {
                frame: frame.trim().parse().ok()?,
                name: name.to_owned(),
            })
        })
        .collect();
    out.sort_by_key(|label| label.frame);
    out
}

/// The edit that puts a label named `name` on `frame` (`None`: takes the
/// label there away). `None` when nothing changes.
#[must_use]
pub fn set_label(arena: &Arena, picture: EntityId, frame: u32, name: Option<&str>) -> Option<Edit> {
    let mut all: Vec<Label> = labels(arena, picture)
        .into_iter()
        .filter(|label| label.frame != frame)
        .collect();
    if let Some(name) = name {
        all.push(Label {
            frame,
            name: name.replace(['\n', '\r'], " "),
        });
    }
    all.sort_by_key(|label| label.frame);
    if all == labels(arena, picture) {
        return None;
    }
    let text: Vec<String> = all
        .iter()
        .map(|label| format!("{}:{}", label.frame, label.name))
        .collect();
    let (label, step) = if name.is_some() {
        (
            tr!("create-timeline-label"),
            SetProperty::new(picture, LABELS, text.join("\n")),
        )
    } else if all.is_empty() {
        (
            tr!("delete-timeline-label"),
            SetProperty::remove(picture, LABELS),
        )
    } else {
        (
            tr!("delete-timeline-label"),
            SetProperty::new(picture, LABELS, text.join("\n")),
        )
    };
    Some(Edit::new(label).then(step))
}

/// How playback goes: Clip Studio Paint's Playback settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Playback {
    /// Play in real time, skipping frames that are late (else play all
    /// frames).
    pub real_time: bool,
    /// Render every frame before starting playback.
    pub render_first: bool,
    /// Prefer quality to speed while playing.
    pub prefer_quality: bool,
    /// Play as the 2D cameras see (Render 2D camera).
    pub render_camera: bool,
}

impl Default for Playback {
    fn default() -> Self {
        Self {
            real_time: true,
            render_first: false,
            prefer_quality: false,
            render_camera: true,
        }
    }
}

impl Playback {
    /// The settings as text: `name=0|1` pairs joined by `,`. Names this
    /// Sue does not know are passed over when read.
    #[must_use]
    pub fn to_text(&self) -> String {
        let flag = |on: bool| u8::from(on);
        format!(
            "real_time={},render_first={},quality={},camera={}",
            flag(self.real_time),
            flag(self.render_first),
            flag(self.prefer_quality),
            flag(self.render_camera)
        )
    }

    /// Settings read back from [`Self::to_text`]; a name missing keeps its
    /// default.
    #[must_use]
    pub fn from_text(text: &str) -> Self {
        let mut out = Self::default();
        for pair in text.split(',') {
            let Some((name, value)) = pair.split_once('=') else {
                continue;
            };
            let on = value.trim() == "1";
            match name.trim() {
                "real_time" => out.real_time = on,
                "render_first" => out.render_first = on,
                "quality" => out.prefer_quality = on,
                "camera" => out.render_camera = on,
                _ => {}
            }
        }
        out
    }

    /// The settings of `picture`.
    #[must_use]
    pub fn of(arena: &Arena, picture: EntityId) -> Self {
        arena
            .get_by_id(picture)
            .ok()
            .and_then(|entity| entity.property(PLAYBACK))
            .and_then(Value::as_str)
            .map(Self::from_text)
            .unwrap_or_default()
    }

    /// The edit that gives `picture` these settings.
    #[must_use]
    pub fn apply(&self, picture: EntityId) -> Edit {
        Edit::new(tr!("change-playback-settings")).then(SetProperty::new(
            picture,
            PLAYBACK,
            self.to_text(),
        ))
    }
}

/// The overflow frame: a larger frame around the output frame that the
/// canvas takes the size of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Overflow {
    /// Its size (pixels).
    pub size: [u32; 2],
    /// Which of its nine points the output frame keeps to: 0 top left,
    /// row by row to 8 bottom right (4 the middle).
    pub anchor: u8,
    /// The output frame moved from there (pixels).
    pub offset: [i32; 2],
}

/// The output frame of an animation: what exports, inside the canvas,
/// with blank space around it and maybe an overflow frame (Clip Studio
/// Paint's New, Animation).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OutputFrame {
    /// Its size (pixels).
    pub size: [u32; 2],
    /// Blank space: top, bottom, left, right (pixels).
    pub blank: [u32; 4],
    /// Show the title-safe area.
    pub title_safe: bool,
    /// The overflow frame, if any.
    pub overflow: Option<Overflow>,
}

impl OutputFrame {
    /// An output frame of `size` filling the canvas.
    #[must_use]
    pub fn filling(size: [u32; 2]) -> Self {
        Self {
            size,
            blank: [0; 4],
            title_safe: false,
            overflow: None,
        }
    }

    /// The size of the canvas it needs: the overflow frame's, or the
    /// output frame with its blank space.
    #[must_use]
    pub fn canvas_size(&self) -> [u32; 2] {
        match self.overflow {
            Some(overflow) => overflow.size,
            None => [
                self.size[0] + self.blank[2] + self.blank[3],
                self.size[1] + self.blank[0] + self.blank[1],
            ],
        }
    }

    /// Where the output frame sits on the canvas: `[x, y, width, height]`.
    #[must_use]
    pub fn rect(&self) -> [i64; 4] {
        let [width, height] = self.size.map(i64::from);
        match self.overflow {
            Some(overflow) => {
                let [outer_w, outer_h] = overflow.size.map(i64::from);
                let (column, row) = (
                    i64::from(overflow.anchor.min(8) % 3),
                    i64::from(overflow.anchor.min(8) / 3),
                );
                let x = (outer_w - width) * column / 2 + i64::from(overflow.offset[0]);
                let y = (outer_h - height) * row / 2 + i64::from(overflow.offset[1]);
                [x, y, width, height]
            }
            None => [
                i64::from(self.blank[2]),
                i64::from(self.blank[0]),
                width,
                height,
            ],
        }
    }

    /// The frame as text: `width,height;top,bottom,left,right;safe` and,
    /// with an overflow frame, `;width,height,anchor,x,y`.
    #[must_use]
    pub fn to_text(&self) -> String {
        let [w, h] = self.size;
        let [top, bottom, left, right] = self.blank;
        let mut out = format!(
            "{w},{h};{top},{bottom},{left},{right};{}",
            u8::from(self.title_safe)
        );
        if let Some(overflow) = self.overflow {
            let [ow, oh] = overflow.size;
            let [ox, oy] = overflow.offset;
            out.push_str(&format!(";{ow},{oh},{},{ox},{oy}", overflow.anchor));
        }
        out
    }

    /// A frame read back from [`Self::to_text`].
    #[must_use]
    pub fn from_text(text: &str) -> Option<Self> {
        fn numbers<T: std::str::FromStr>(part: &str) -> Option<Vec<T>> {
            part.split(',')
                .map(|value| value.trim().parse().ok())
                .collect()
        }
        let parts: Vec<&str> = text.split(';').collect();
        let size: Vec<u32> = numbers(parts.first()?)?;
        let blank: Vec<u32> = numbers(parts.get(1)?)?;
        let overflow = match parts.get(3) {
            Some(part) => {
                let values: Vec<i64> = numbers(part)?;
                let [w, h, anchor, x, y] = values.as_slice() else {
                    return None;
                };
                Some(Overflow {
                    size: [u32::try_from(*w).ok()?, u32::try_from(*h).ok()?],
                    anchor: u8::try_from(*anchor).ok()?.min(8),
                    offset: [i32::try_from(*x).ok()?, i32::try_from(*y).ok()?],
                })
            }
            None => None,
        };
        Some(Self {
            size: [*size.first()?, *size.get(1)?],
            blank: [
                *blank.first()?,
                *blank.get(1)?,
                *blank.get(2)?,
                *blank.get(3)?,
            ],
            title_safe: parts.get(2).is_some_and(|value| value.trim() == "1"),
            overflow,
        })
    }

    /// The output frame of `picture`, if it has one.
    #[must_use]
    pub fn of(arena: &Arena, picture: EntityId) -> Option<Self> {
        arena
            .get_by_id(picture)
            .ok()?
            .property(OUTPUT)
            .and_then(Value::as_str)
            .and_then(Self::from_text)
    }

    /// The edit that gives `picture` this output frame.
    #[must_use]
    pub fn apply(&self, picture: EntityId) -> Edit {
        Edit::new(tr!("set-output-frame")).then(SetProperty::new(picture, OUTPUT, self.to_text()))
    }
}

/// How the layers of `picture` show at `frame` of its timeline, all in
/// one: the cels each animation folder shows (a folder cel as a whole),
/// onion skin with `onion`, the layers outside their clips hidden, the
/// layer keyframes, and the 2D cameras when Render 2D camera is on (or
/// `cameras` asks for them). Fill and Auto Select take this view, so they
/// see the frame as it shows. `None` without a timeline.
#[must_use]
pub fn frame_view(
    arena: &Arena,
    picture: EntityId,
    frame: u32,
    onion: Option<&super::Onion>,
    cameras: Option<bool>,
) -> Option<FrameView> {
    let sequence = timeline(arena, picture)?;
    let size = arena
        .get_by_id(picture)
        .ok()
        .and_then(sue_model::picture::size)
        .unwrap_or([0, 0]);
    let mut view = super::view_in(arena, picture, sequence, frame, onion);
    view.hidden.extend(off_clip(sequence, frame));
    let time = Time::from_frames(i64::from(frame), sequence.rate);
    if cameras.unwrap_or_else(|| Playback::of(arena, picture).render_camera) {
        view.cameras = crate::camera::views(arena, picture, Some(time), size);
        view.opacities
            .extend(crate::camera::opacities(arena, picture, time));
    }
    let (moves, opacities) = super::motion::poses(arena, picture, time, size);
    view.moves = moves;
    view.opacities.extend(opacities);
    Some(super::cels::whole_folders(arena, picture, view))
}

/// The edit that names `picture`'s timeline `name` (Change settings, the
/// timeline name alone).
#[must_use]
pub fn rename_timeline(picture: EntityId, name: &str) -> Edit {
    Edit::new(tr!("change-settings")).then(SetProperty::new(picture, NAME, name.to_owned()))
}

/// The edit that gives every layer and folder of `picture` that has no
/// track yet one with a clip over the whole timeline (what New timeline
/// does, for a timeline made before). `None` when each has one.
#[must_use]
pub fn give_every_layer_a_track(arena: &Arena, picture: EntityId) -> Option<Edit> {
    let sequence = timeline(arena, picture)?;
    let length = length(arena, picture);
    let mut out = sequence.clone();
    let mut stack = crate::doc::layers(arena, picture);
    stack.reverse();
    for info in stack {
        let inside = crate::doc::folders_around(arena, info.id)
            .iter()
            .any(|folder| super::is_animation_folder(arena, *folder));
        if inside
            || super::is_animation_folder(arena, info.id)
            || track_of_layer(&out, info.id).is_some()
        {
            continue;
        }
        let track = if crate::camera::is_camera(arena, info.id) {
            camera_track(&info.name, info.id, out.rate, length)
        } else {
            layer_track(&info.name, info.id, out.rate, length)
        };
        out.tracks.push(track);
    }
    (out != *sequence).then(|| Edit::new(tr!("new-timeline")).then(SetExtension::set(picture, out)))
}

/// `true` if `track` is a 2D camera's.
#[must_use]
pub fn is_camera_track(track: &Track) -> bool {
    track.kind == CAMERA
}

/// `true` if `track` is an animation folder's.
#[must_use]
pub fn is_cel_track(track: &Track) -> bool {
    track.kind == CELS
}
