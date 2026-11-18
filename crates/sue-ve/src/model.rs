//! Sue VE's data, on the backbone (rules 1 to 4).
//!
//! A project is an entity of class [`class::PROJECT`] whose `sue.child`
//! tree is the media pool: bins (`sue.folder`), media, generated sources
//! (titles, solids, adjustment clips) and sequences. Nothing is a private
//! copy:
//!
//! - A movie or a sound file is a kept encoded file: an entity of class
//!   `sue.video` or `sue.audio` that links to the file by its path
//!   ([`key::PATH`]), and holds its bytes ([`sue_model::Encoded`]) when
//!   the project embeds it. What decoding found (size, rate, length,
//!   codecs) is in its properties.
//! - A still is a raster entity (`sue.image`), a Sue Paint picture or
//!   animation is the picture entity itself: clips name them by
//!   [`sue_core::EntityId`], so a drawing sent from Sue Paint plays as it
//!   is drawn.
//! - A sequence is an entity of class `sue.sequence` holding the shared
//!   [`Sequence`] (version 2): video tracks first, top track first, then
//!   audio tracks. Its frame size is in its properties.
//!
//! Every change is a [`SequenceEdit`] or a kernel transaction, applied
//! through the studio's undo history.

use sue_core::txn::{Batch, SpawnEntity};
use sue_core::{Arena, CHILD, Entity, EntityId, OrderKey, Rate, Relation, Time, Value};
use sue_model::{Sequence, SequenceEdit, Track, class as model_class};

use crate::edit::EditError;

/// Sue VE's classes.
pub mod class {
    /// A project: the media pool and the sequences under it.
    pub const PROJECT: &str = "ve.project";
    /// A title: its text is the entity's text, set in [`super::key`]'s
    /// title keys.
    pub const TITLE: &str = "ve.title";
    /// A color solid.
    pub const SOLID: &str = "ve.solid";
    /// An adjustment clip: what effects on it do to the tracks below
    /// (effects arrive in stage 2; today it passes the picture through).
    pub const ADJUSTMENT: &str = "ve.adjustment";
    /// A numbered run of still pictures in a folder, played as a movie.
    pub const IMAGE_SEQUENCE: &str = "ve.image-sequence";
}

/// Property keys. `media.*` describe a source, `ve.*` a clip, a track or
/// a sequence.
pub mod key {
    /// A kept file's path (text).
    pub const PATH: &str = "media.path";
    /// The pictures' width in pixels.
    pub const WIDTH: &str = "media.width";
    /// The pictures' height in pixels.
    pub const HEIGHT: &str = "media.height";
    /// Frames a second, numerator.
    pub const RATE_NUM: &str = "media.rate_num";
    /// Frames a second, denominator.
    pub const RATE_DEN: &str = "media.rate_den";
    /// How long the source lasts (a time).
    pub const DURATION: &str = "media.duration";
    /// The pictures' codec, as four letters.
    pub const VIDEO_CODEC: &str = "media.video_codec";
    /// The sound's rate.
    pub const AUDIO_RATE: &str = "media.audio_rate";
    /// The sound's channels.
    pub const AUDIO_CHANNELS: &str = "media.audio_channels";
    /// A smaller copy of the movie made for editing (text path).
    pub const PROXY: &str = "media.proxy";
    /// An image sequence's first frame number.
    pub const FIRST: &str = "media.first";
    /// An image sequence's frame count.
    pub const COUNT: &str = "media.count";
    /// An image sequence's file name pattern, `{n}` for the number.
    pub const PATTERN: &str = "media.pattern";
    /// An image sequence's digits.
    pub const DIGITS: &str = "media.digits";

    /// A sequence's frame width.
    pub const SEQ_WIDTH: &str = "ve.width";
    /// A sequence's frame height.
    pub const SEQ_HEIGHT: &str = "ve.height";
    /// The program's in point (a time).
    pub const MARK_IN: &str = "ve.in";
    /// The program's out point (a time).
    pub const MARK_OUT: &str = "ve.out";

    /// A clip's opacity, 0 to 1 (keyframable).
    pub const OPACITY: &str = "ve.opacity";
    /// A clip's scale, 1 is the size that fits the frame (keyframable).
    pub const SCALE: &str = "ve.scale";
    /// A clip's offset across, in sequence pixels (keyframable).
    pub const POS_X: &str = "ve.x";
    /// A clip's offset down, in sequence pixels (keyframable).
    pub const POS_Y: &str = "ve.y";
    /// A clip's turn in degrees, clockwise (keyframable).
    pub const ROTATION: &str = "ve.rotation";
    /// A clip's (or track's) gain in decibels (keyframable).
    pub const VOLUME: &str = "ve.volume";
    /// A sound clip's fade in, in seconds.
    pub const FADE_IN: &str = "ve.fade_in";
    /// A sound clip's fade out, in seconds.
    pub const FADE_OUT: &str = "ve.fade_out";
    /// A clip that holds one source frame (a freeze frame).
    pub const FREEZE: &str = "ve.freeze";

    /// A track edits take (targeted).
    pub const TARGET: &str = "ve.target";
    /// A track that moves with inserts and ripples (sync lock).
    pub const SYNC_LOCK: &str = "ve.sync_lock";
    /// A track heard alone.
    pub const SOLO: &str = "ve.solo";

    /// A title's or solid's color.
    pub const COLOR: &str = "ve.color";
    /// A title's font family.
    pub const FONT: &str = "ve.font";
    /// A title's letter size, in sequence pixels.
    pub const SIZE: &str = "ve.size";
}

/// The properties a clip has that keyframes may drive, with their
/// defaults.
pub const ANIMATABLE: [(&str, f64); 6] = [
    (key::OPACITY, 1.0),
    (key::SCALE, 1.0),
    (key::POS_X, 0.0),
    (key::POS_Y, 0.0),
    (key::ROTATION, 0.0),
    (key::VOLUME, 0.0),
];

/// The default of keyframable `name`.
#[must_use]
pub fn default_of(name: &str) -> f64 {
    ANIMATABLE
        .iter()
        .find(|(key, _)| *key == name)
        .map_or(0.0, |(_, value)| *value)
}

/// A track's kind.
pub const VIDEO: &str = "video";
/// See [`VIDEO`].
pub const AUDIO: &str = "audio";

/// What a clip's source is.
#[derive(Clone, Debug, PartialEq)]
pub enum SourceKind {
    /// A movie file: pictures, maybe sound.
    Movie,
    /// A sound file.
    Sound,
    /// A still picture (a raster entity).
    Still,
    /// A Sue Paint picture, or an animation when it has a timeline.
    Picture,
    /// A numbered run of pictures.
    ImageSequence,
    /// A title.
    Title,
    /// A color solid.
    Solid,
    /// An adjustment clip.
    Adjustment,
    /// Another sequence (nested, stage 2 plays it).
    Sequence,
    /// Something this build cannot play.
    Unknown,
}

impl SourceKind {
    /// The kind of `entity`.
    #[must_use]
    pub fn of(entity: &Entity) -> Self {
        match entity.class() {
            model_class::VIDEO => Self::Movie,
            model_class::AUDIO => Self::Sound,
            class::TITLE => Self::Title,
            class::SOLID => Self::Solid,
            class::ADJUSTMENT => Self::Adjustment,
            class::IMAGE_SEQUENCE => Self::ImageSequence,
            model_class::SEQUENCE => Self::Sequence,
            _ if sue_model::picture::is_picture(entity) => Self::Picture,
            _ if entity
                .component(sue_core::ComponentKind::RasterTile2D)
                .is_some() =>
            {
                Self::Still
            }
            _ => Self::Unknown,
        }
    }

    /// `true` for sources that show pictures.
    #[must_use]
    pub fn has_pictures(&self) -> bool {
        !matches!(self, Self::Sound | Self::Unknown)
    }

    /// `true` for sources whose length is the file's: they cannot be
    /// trimmed past it. Stills, titles and solids last as long as needed.
    #[must_use]
    pub fn has_length(&self) -> bool {
        matches!(
            self,
            Self::Movie | Self::Sound | Self::ImageSequence | Self::Sequence
        ) || matches!(self, Self::Picture)
    }
}

/// A number property of `entity`.
#[must_use]
pub fn number(entity: &Entity, name: &str) -> Option<f64> {
    entity.property(name).and_then(Value::as_f64)
}

/// A text property of `entity`.
#[must_use]
pub fn text<'a>(entity: &'a Entity, name: &str) -> Option<&'a str> {
    entity.property(name).and_then(Value::as_str)
}

/// A time property of `entity`.
#[must_use]
pub fn time(entity: &Entity, name: &str) -> Option<Time> {
    match entity.property(name) {
        Some(Value::Time(time)) => Some(*time),
        _ => None,
    }
}

/// A source's frame rate, if it has one.
#[must_use]
pub fn rate_of(entity: &Entity) -> Option<Rate> {
    let numerator = number(entity, key::RATE_NUM)? as u32;
    let denominator = number(entity, key::RATE_DEN).unwrap_or(1.0) as u32;
    (numerator > 0 && denominator > 0).then(|| Rate::new(numerator, denominator))
}

/// How long `id` lasts as a source: `None` for one with no length of its
/// own (a still, a title), which any clip may stretch.
#[must_use]
pub fn length_of(arena: &Arena, id: EntityId) -> Option<Time> {
    let entity = arena.get_by_id(id).ok()?;
    match SourceKind::of(entity) {
        SourceKind::Movie | SourceKind::Sound | SourceKind::ImageSequence => {
            time(entity, key::DURATION).filter(|length| length.flicks() > 0)
        }
        SourceKind::Picture => {
            let frames = sue_picture::anim::length(arena, id);
            let sequence = sue_picture::anim::timeline(arena, id)?;
            (frames > 0).then(|| Time::from_frames(i64::from(frames), sequence.rate))
        }
        SourceKind::Sequence => entity.ext::<Sequence>().map(Sequence::duration),
        _ => None,
    }
}

/// A source's picture size, if it has one.
#[must_use]
pub fn size_of(arena: &Arena, id: EntityId) -> Option<(u32, u32)> {
    let entity = arena.get_by_id(id).ok()?;
    match SourceKind::of(entity) {
        SourceKind::Movie | SourceKind::ImageSequence => Some((
            number(entity, key::WIDTH)? as u32,
            number(entity, key::HEIGHT)? as u32,
        )),
        SourceKind::Still => {
            let tile = entity
                .component(sue_core::ComponentKind::RasterTile2D)?
                .as_raster()?;
            Some((tile.width(), tile.height()))
        }
        SourceKind::Picture => {
            sue_model::picture::size(entity).map(|[width, height]| (width, height))
        }
        _ => None,
    }
}

/// `true` when the source carries sound.
#[must_use]
pub fn has_sound(arena: &Arena, id: EntityId) -> bool {
    arena
        .get_by_id(id)
        .is_ok_and(|entity| match SourceKind::of(entity) {
            SourceKind::Sound => true,
            SourceKind::Movie => number(entity, key::AUDIO_RATE).is_some_and(|rate| rate > 0.0),
            _ => false,
        })
}

/// The sequence of entity `id`.
#[must_use]
pub fn sequence(arena: &Arena, id: EntityId) -> Option<&Sequence> {
    arena.get_by_id(id).ok()?.ext::<Sequence>()
}

/// A sequence's frame size.
#[must_use]
pub fn frame_size(sequence: &Sequence) -> (u32, u32) {
    let get = |name: &str, default: f64| {
        sequence
            .properties
            .get(name)
            .and_then(Value::as_f64)
            .unwrap_or(default)
            .clamp(16.0, 16_384.0) as u32
    };
    (get(key::SEQ_WIDTH, 1920.0), get(key::SEQ_HEIGHT, 1080.0))
}

/// A sequence's in and out marks.
#[must_use]
pub fn marks(sequence: &Sequence) -> (Option<Time>, Option<Time>) {
    let get = |name: &str| match sequence.properties.get(name) {
        Some(Value::Time(time)) => Some(*time),
        _ => None,
    };
    (get(key::MARK_IN), get(key::MARK_OUT))
}

/// `true` for a video track.
#[must_use]
pub fn is_video(track: &Track) -> bool {
    track.kind == VIDEO
}

/// A track flag (target, sync lock, solo).
#[must_use]
pub fn flag(track: &Track, name: &str) -> bool {
    track
        .properties
        .get(name)
        .and_then(Value::as_bool)
        .unwrap_or(name == key::SYNC_LOCK)
}

/// A new, empty sequence: `video` video tracks over `audio` audio
/// tracks, `width` by `height` at `rate`.
#[must_use]
pub fn empty_sequence(width: u32, height: u32, rate: Rate, video: usize, audio: usize) -> Sequence {
    let mut sequence = Sequence::new(rate, 48_000);
    sequence.properties.set(key::SEQ_WIDTH, i64::from(width));
    sequence.properties.set(key::SEQ_HEIGHT, i64::from(height));
    for index in (1..=video).rev() {
        let mut track = Track::new(format!("V{index}"), VIDEO);
        track.properties.set(key::TARGET, index == 1);
        sequence.tracks.push(track);
    }
    for index in 1..=audio {
        let mut track = Track::new(format!("A{index}"), AUDIO);
        track.properties.set(key::TARGET, index == 1);
        sequence.tracks.push(track);
    }
    sequence
}

/// The parts of a new project made at once.
#[derive(Debug)]
pub struct NewProject {
    /// The steps that make it, one undo step.
    pub batch: Batch<Arena>,
    /// The project.
    pub project: EntityId,
    /// Its first sequence.
    pub sequence: EntityId,
}

/// A new project named `name` with one sequence, `width` by `height` at
/// `rate`.
#[must_use]
pub fn new_project(
    name: &str,
    sequence_name: &str,
    width: u32,
    height: u32,
    rate: Rate,
) -> NewProject {
    let project = Entity::empty(name, class::PROJECT);
    let project_id = project.id();
    let edit = Entity::empty(sequence_name, model_class::SEQUENCE)
        .with_ext(empty_sequence(width, height, rate, 3, 3));
    let sequence_id = edit.id();
    let order = OrderKey::between(None, None);
    let batch = Batch::new(sequence_name.to_owned())
        .then(SpawnEntity::new(project))
        .then(
            SpawnEntity::new(edit)
                .with_relation(Relation::new(project_id, CHILD, sequence_id).ordered(order)),
        );
    NewProject {
        batch,
        project: project_id,
        sequence: sequence_id,
    }
}

/// Spawns `entity` under `parent` (last among its children) as one step.
#[must_use]
pub fn spawn_under(arena: &Arena, parent: EntityId, entity: Entity) -> SpawnEntity {
    let id = entity.id();
    let order = arena.relations().next_order(parent, CHILD);
    SpawnEntity::new(entity).with_relation(Relation::new(parent, CHILD, id).ordered(order))
}

/// Spawns several entities under `parent`, in order, as steps of one
/// batch.
pub fn spawn_all_under(
    arena: &Arena,
    parent: EntityId,
    entities: Vec<Entity>,
    batch: &mut Batch<Arena>,
) {
    let mut order = arena.relations().next_order(parent, CHILD);
    for entity in entities {
        let id = entity.id();
        batch.push(Box::new(SpawnEntity::new(entity).with_relation(
            Relation::new(parent, CHILD, id).ordered(order.clone()),
        )));
        order = OrderKey::between(Some(&order), None);
    }
}

/// The project `id` belongs to (itself for a project).
#[must_use]
pub fn project_of(arena: &Arena, id: EntityId) -> Option<EntityId> {
    let mut at = id;
    for _ in 0..64 {
        let entity = arena.get_by_id(at).ok()?;
        if entity.class() == class::PROJECT {
            return Some(at);
        }
        at = arena.relations().parent(at, CHILD)?;
    }
    None
}

/// Everything in project `project`'s pool, depth first, with its depth.
#[must_use]
pub fn pool(arena: &Arena, project: EntityId) -> Vec<(EntityId, usize)> {
    let mut out = Vec::new();
    let mut stack: Vec<(EntityId, usize)> = arena
        .relations()
        .targets(project, CHILD)
        .into_iter()
        .rev()
        .map(|id| (id, 0))
        .collect();
    while let Some((id, depth)) = stack.pop() {
        out.push((id, depth));
        if depth < 32 {
            for child in arena.relations().targets(id, CHILD).into_iter().rev() {
                stack.push((child, depth + 1));
            }
        }
        if out.len() > 100_000 {
            break;
        }
    }
    out
}

/// The sequences of `project`.
#[must_use]
pub fn sequences(arena: &Arena, project: EntityId) -> Vec<EntityId> {
    pool(arena, project)
        .into_iter()
        .map(|(id, _)| id)
        .filter(|id| sequence(arena, *id).is_some())
        .collect()
}

/// Runs `change` on a copy of sequence `id` and returns the undo step
/// that makes the change, with what `change` returned. `None` for the
/// step when nothing changed.
///
/// # Errors
/// [`EditError::NoSequence`] when `id` holds no sequence, or `change`'s
/// own error (the arena is untouched either way: nothing is applied
/// here).
pub fn plan<T>(
    arena: &Arena,
    id: EntityId,
    label: &str,
    change: impl FnOnce(&mut Sequence) -> Result<T, EditError>,
) -> Result<(Option<SequenceEdit>, T), EditError> {
    let before = sequence(arena, id).ok_or(EditError::NoSequence)?;
    let mut after = before.clone();
    after.ensure_ids();
    let out = change(&mut after)?;
    Ok((SequenceEdit::between(id, label, before, &after), out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_core::UndoStack;

    #[test]
    fn a_new_project_is_one_undo_step_and_saves() {
        let mut arena = Arena::new();
        let made = new_project("Film", "Sequence 1", 1920, 1080, Rate::FPS_25);
        let mut history = UndoStack::new();
        history.apply(&mut arena, made.batch).unwrap();
        assert_eq!(project_of(&arena, made.sequence), Some(made.project));
        assert_eq!(sequences(&arena, made.project), [made.sequence]);
        let edit = sequence(&arena, made.sequence).unwrap();
        assert_eq!(frame_size(edit), (1920, 1080));
        let names: Vec<&str> = edit.tracks.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["V3", "V2", "V1", "A1", "A2", "A3"]);
        assert!(flag(&edit.tracks[2], key::TARGET) && !flag(&edit.tracks[1], key::TARGET));
        assert!(
            flag(&edit.tracks[0], key::SYNC_LOCK),
            "sync lock is on by default"
        );
        // Saved and loaded: the same project.
        let mut file = sue_core::SueFile::new();
        file.set_arena(&arena);
        let loaded = sue_core::SueFile::from_bytes(&file.to_bytes())
            .unwrap()
            .arena(&sue_model::registry())
            .unwrap();
        assert_eq!(
            sequence(&loaded, made.sequence),
            sequence(&arena, made.sequence)
        );
        assert!(history.undo(&mut arena).unwrap());
        assert!(arena.get_by_id(made.project).is_err());
    }
}
