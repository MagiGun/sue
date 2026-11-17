//! What belongs to a picture as a whole, both ways between Sue's model
//! and a [`ClipDocument`]: the layer comps, the rulers, the timeline
//! (animation folders and their cels, layer keyframes, the 2D camera and
//! sound) and the small preview of the canvas.
//!
//! Like the layers that are more than pixels, these live in Sue's own
//! tables of the file (`docs/clip-sue-tables.md`) until Clip Studio's
//! own storage of them is observed. Layers are named by their place in
//! the file's tree ([`ClipLayerPath`]), never by a row id.

use std::collections::HashMap;

use sue_core::txn::{SetExtension, SetProperty, SpawnEntity};
use sue_core::{Arena, Entity, EntityId, Rate, Time, Value};
use sue_io::clip::{
    ClipAnimationFolder, ClipAudio, ClipCamera, ClipCel, ClipComp, ClipCompState, ClipContent,
    ClipDocument, ClipInterpolation, ClipKey, ClipLayerPath, ClipPicture, ClipRuler, ClipRulerKind,
    ClipTimeline, ClipTrack,
};
use sue_model::{Animation, Curve, Key};
use sue_model::{BlendMode, Clip, Sequence, Track};

use super::report::{Line, Outcome};
use super::{blend_of, composite_of};
use crate::anim::motion::{self, Ease};
use crate::anim::{self, setup};
use crate::camera;
use crate::composite::{RenderOptions, flatten};
use crate::comps;
use crate::doc::Edit;
use crate::rulers::{self, Path, Perspective, Rulers, ShowRange, Special};

/// The longest side of the canvas preview, in pixels.
const PREVIEW_SIDE: u32 = 256;

/// Layer keyframe curves by the property names the file uses.
const LAYER_CURVES: [(&str, &str); 7] = [
    (motion::POSITION_X, "x"),
    (motion::POSITION_Y, "y"),
    (motion::SCALE, "scale"),
    (motion::ROTATION, "angle"),
    (motion::CENTER_X, "center.x"),
    (motion::CENTER_Y, "center.y"),
    (motion::OPACITY, "opacity"),
];

/// The property of a layer track that says where the layer shows: 1
/// from a key on, 0 from a key on (each key holds).
const SHOWN: &str = "shown";

// ----- Out to the file ---------------------------------------------------------------

/// What Sue makes of a picture's comps, rulers and timeline for a file
/// whose layers lie where `paths` says.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Parts {
    /// The layer comps.
    pub comps: Vec<ClipComp>,
    /// The timeline.
    pub timeline: Option<ClipTimeline>,
    /// What the animation folders and the camera folder hold.
    pub folders: Vec<(ClipLayerPath, ClipContent)>,
    /// The rulers, on the layer they hang on.
    pub rulers: Option<(ClipLayerPath, Vec<ClipRuler>)>,
    /// How each came out.
    pub lines: Vec<Line>,
}

impl Parts {
    /// A fingerprint of what the file gets: two equal ones write the
    /// same comps, rulers and timeline.
    #[must_use]
    pub fn print(&self) -> [u8; 32] {
        let mut print = sue_model::foreign::Fingerprint::new();
        print.text("clip parts");
        print.bytes(&sue_io::clip::parts::encode_comps(&self.comps));
        match &self.timeline {
            Some(timeline) => print.bytes(&sue_io::clip::parts::encode_timeline(timeline)),
            None => print.word(0),
        }
        print.word(self.folders.len() as u64);
        for (path, content) in &self.folders {
            path_print(&mut print, path);
            print.bytes(&sue_io::clip::parts::encode_content(content));
        }
        match &self.rulers {
            Some((path, rulers)) => {
                path_print(&mut print, path);
                print.bytes(&sue_io::clip::parts::encode_rulers(rulers));
            }
            None => print.word(0),
        }
        print.finish()
    }

    /// Puts the parts into `doc`, in place of what it held for them.
    pub fn put(self, doc: &mut ClipDocument) {
        let all: Vec<ClipLayerPath> = doc.paths().into_iter().map(|(path, _)| path).collect();
        for path in &all {
            if let Some(layer) = doc.layer_at_mut(path) {
                layer.rulers.clear();
                if matches!(
                    layer.content,
                    Some(ClipContent::AnimationFolder(_) | ClipContent::Camera(_))
                ) {
                    layer.content = None;
                }
            }
        }
        for (path, content) in self.folders {
            if let Some(layer) = doc.layer_at_mut(&path) {
                layer.content = Some(content);
            }
        }
        if let Some((path, rulers)) = self.rulers
            && let Some(layer) = doc.layer_at_mut(&path)
        {
            layer.rulers = rulers;
        }
        doc.comps = self.comps;
        doc.timeline = self.timeline;
    }

    /// Puts into the layers of `doc` at `remade` (written again from
    /// Sue's model, so holding nothing of the parts) what the parts hang
    /// on them: an animation or camera folder's content, the rulers. The
    /// other layers and the picture's own parts stay as the file held
    /// them; for parts that did not change since the file was opened.
    pub fn put_on(self, doc: &mut ClipDocument, remade: &[ClipLayerPath]) {
        for (path, content) in self.folders {
            if remade.contains(&path)
                && let Some(layer) = doc.layer_at_mut(&path)
            {
                layer.content = Some(content);
            }
        }
        if let Some((path, rulers)) = self.rulers
            && remade.contains(&path)
            && let Some(layer) = doc.layer_at_mut(&path)
        {
            layer.rulers = rulers;
        }
    }
}

fn path_print(print: &mut sue_model::foreign::Fingerprint, path: &ClipLayerPath) {
    print.word(path.0.len() as u64);
    for index in &path.0 {
        print.word(u64::from(*index));
    }
}

/// What Sue makes of `picture`'s comps, rulers and timeline, its layers
/// lying in the file where `paths` says.
pub(super) fn parts(
    arena: &Arena,
    picture: EntityId,
    paths: &[(EntityId, ClipLayerPath)],
) -> Parts {
    let path_of: HashMap<EntityId, &ClipLayerPath> =
        paths.iter().map(|(id, path)| (*id, path)).collect();
    let mut out = Parts::default();
    comps_out(arena, picture, &mut out, &path_of);
    rulers_out(arena, picture, &mut out, paths, &path_of);
    timeline_out(arena, picture, &mut out, &path_of);
    out
}

fn comps_out(
    arena: &Arena,
    picture: EntityId,
    out: &mut Parts,
    path_of: &HashMap<EntityId, &ClipLayerPath>,
) {
    let comps = comps::read(arena, picture);
    let mut lost = false;
    out.comps = comps
        .iter()
        .map(|comp| ClipComp {
            name: comp.name.clone(),
            states: comp
                .states
                .iter()
                .filter_map(|state| {
                    let Some(path) = path_of.get(&state.layer) else {
                        // A layer that is gone, or was not written.
                        lost |= arena.get_by_id(state.layer).is_ok();
                        return None;
                    };
                    Some(ClipCompState {
                        layer: (*path).clone(),
                        visible: state.visible,
                        opacity: state.opacity,
                        composite: composite_of(BlendMode::from_name_or_normal(&state.blend)),
                    })
                })
                .collect(),
        })
        .collect();
    if !out.comps.is_empty() {
        out.lines
            .push(Line::new(tr!("clip-report-comps"), Outcome::Live).lost(lost));
    }
}

/// A direction `angle` (radians) as a second point a long way from
/// `at`, so the angle read back from the two is as near as can be.
fn toward(at: [f32; 2], angle: f32) -> [f32; 2] {
    let (sin, cos) = f64::from(angle).sin_cos();
    [
        (f64::from(at[0]) + cos * 1000.0) as f32,
        (f64::from(at[1]) + sin * 1000.0) as f32,
    ]
}

fn rulers_out(
    arena: &Arena,
    picture: EntityId,
    out: &mut Parts,
    paths: &[(EntityId, ClipLayerPath)],
    path_of: &HashMap<EntityId, &ClipLayerPath>,
) {
    let found = Rulers::of(arena, picture);
    if found.is_empty() {
        return;
    }
    let entity = arena.get_by_id(picture).ok();
    let owner = entity
        .and_then(|entity| entity.property(rulers::LAYER))
        .and_then(Value::as_entity);
    let range = ShowRange::of(arena, picture);
    let everywhere = owner.is_none() || range == ShowRange::AllLayers;
    // The layer they belong to, or (belonging to the whole picture) the
    // first layer written.
    let path = owner
        .and_then(|owner| path_of.get(&owner).map(|path| (*path).clone()))
        .or_else(|| paths.first().map(|(_, path)| path.clone()));
    let mut list = Vec::new();
    let ruler = |kind, points: Vec<[f32; 2]>, lines| ClipRuler {
        kind,
        points,
        lines,
        snap: true,
        everywhere,
    };
    if let Some([a, b]) = found.linear {
        list.push(ruler(ClipRulerKind::Linear, vec![a, b], 0));
    }
    match found.special {
        Some(Special::Parallel(angle)) => {
            list.push(ruler(
                ClipRulerKind::Parallel,
                vec![[0.0, 0.0], toward([0.0, 0.0], angle)],
                0,
            ));
        }
        Some(Special::Radial(at)) => list.push(ruler(ClipRulerKind::Radial, vec![at], 0)),
        Some(Special::Concentric(at)) => {
            list.push(ruler(ClipRulerKind::Concentric, vec![at], 0));
        }
        None => {}
    }
    if let Some(perspective) = &found.perspective {
        list.push(ruler(
            ClipRulerKind::Perspective,
            perspective.points.clone(),
            0,
        ));
    }
    if let Some((at, angle)) = found.symmetry {
        // One mirror line: two lines, as Clip Studio counts them.
        list.push(ruler(
            ClipRulerKind::Symmetry,
            vec![at, toward(at, angle)],
            2,
        ));
    }
    for path in &found.paths {
        let kind = if path.closed {
            ClipRulerKind::Figure
        } else {
            ClipRulerKind::Curve
        };
        list.push(ruler(kind, path.points.clone(), 0));
    }
    // Shown with the layers of the same folder: the file knows only its
    // own layer or every layer.
    let lost = range == ShowRange::SameFolder && owner.is_some();
    match path {
        Some(path) => {
            out.rulers = Some((path, list));
            out.lines
                .push(Line::new(tr!("clip-report-rulers"), Outcome::Live).lost(lost));
        }
        None => out
            .lines
            .push(Line::new(tr!("clip-report-rulers"), Outcome::LeftOut)),
    }
}

/// `curve`'s keys on frames at `rate`, and whether one sat between
/// frames.
fn keys_out(curve: &Curve, rate: Rate) -> (Vec<ClipKey>, bool) {
    let mut lost = false;
    let keys = curve
        .keys()
        .iter()
        .map(|key| {
            let frame = key.time.frame(rate).max(0);
            lost |= Time::from_frames(frame, rate) != key.time;
            ClipKey {
                frame: u32::try_from(frame).unwrap_or(u32::MAX),
                value: key.value,
                interpolation: match Ease::of(key.interpolation) {
                    Ease::Hold => ClipInterpolation::Hold,
                    Ease::Linear => ClipInterpolation::Linear,
                    Ease::Smooth => ClipInterpolation::Smooth,
                },
            }
        })
        .collect();
    (keys, lost)
}

#[expect(
    clippy::too_many_lines,
    reason = "one pass from top to bottom; split up, its steps would be read out of order"
)]
fn timeline_out(
    arena: &Arena,
    picture: EntityId,
    out: &mut Parts,
    path_of: &HashMap<EntityId, &ClipLayerPath>,
) {
    let Some(sequence) = anim::timeline(arena, picture) else {
        return;
    };
    let rate = sequence.rate;
    let length = anim::length(arena, picture);
    let settings = setup::Settings::of(arena, picture);
    let (first, last) = settings.range(length);
    let mut lost = false;
    let mut tracks = Vec::new();
    let mut audio = Vec::new();
    for track in &sequence.tracks {
        match track.kind.as_str() {
            anim::CELS => {
                let Some(folder) = anim::track_folder(track) else {
                    lost = true;
                    continue;
                };
                let Some(folder_path) = path_of.get(&folder).map(|path| (*path).clone()) else {
                    lost = true;
                    continue;
                };
                let mut cels = Vec::new();
                for (frame, cel) in anim::keys(track, rate) {
                    let child = match cel {
                        None => None,
                        Some(cel) => match path_of.get(&cel) {
                            // A cel right in its folder: its place there.
                            Some(path)
                                if path.0.len() == folder_path.0.len() + 1
                                    && path.0.starts_with(&folder_path.0) =>
                            {
                                path.0.last().copied()
                            }
                            _ => {
                                lost = true;
                                continue;
                            }
                        },
                    };
                    cels.push(ClipCel { frame, child });
                }
                out.folders.push((
                    folder_path,
                    ClipContent::AnimationFolder(ClipAnimationFolder { cels }),
                ));
            }
            setup::LAYER => {
                let Some(path) = setup::track_layer(track).and_then(|id| path_of.get(&id)) else {
                    continue;
                };
                let whole = Time::from_frames(i64::from(length), rate);
                let clips = track.clips();
                let always =
                    clips.len() == 1 && clips[0].start <= Time::ZERO && clips[0].end() >= whole;
                if always {
                    continue;
                }
                // Where it shows, as keys that hold: 1 from a clip's
                // start, 0 from its end.
                let mut keys: Vec<ClipKey> = Vec::new();
                let mut put = |frame: i64, value: f64| {
                    let frame = u32::try_from(frame.max(0)).unwrap_or(u32::MAX);
                    keys.retain(|key| key.frame != frame);
                    keys.push(ClipKey {
                        frame,
                        value,
                        interpolation: ClipInterpolation::Hold,
                    });
                };
                put(0, 0.0);
                let mut sorted: Vec<&Clip> = clips.iter().collect();
                sorted.sort_by_key(|clip| clip.start);
                for clip in sorted {
                    put(clip.start.frame(rate), 1.0);
                    if clip.end() < whole {
                        put(clip.end().frame(rate), 0.0);
                    }
                }
                keys.sort_by_key(|key| key.frame);
                tracks.push(ClipTrack {
                    layer: (*path).clone(),
                    property: SHOWN.to_owned(),
                    keys,
                });
            }
            setup::CAMERA => {
                let Some(folder) = anim::track_folder(track) else {
                    lost = true;
                    continue;
                };
                let (Some(path), Ok(entity)) = (path_of.get(&folder), arena.get_by_id(folder))
                else {
                    lost = true;
                    continue;
                };
                let value = |key: &str, or: f64| {
                    entity.property(key).and_then(Value::as_f64).unwrap_or(or) as f32
                };
                out.folders.push((
                    (*path).clone(),
                    ClipContent::Camera(ClipCamera {
                        center: [value(camera::X, 0.0), value(camera::Y, 0.0)],
                        scale: value(camera::SCALE, 1.0),
                        angle: value(camera::ANGLE, 0.0),
                    }),
                ));
                if let Some(animation) = entity.ext::<Animation>() {
                    for (name, curve) in animation.channels() {
                        let (keys, off) = keys_out(curve, rate);
                        lost |= off || !camera::VALUES.contains(&name);
                        if !keys.is_empty() {
                            tracks.push(ClipTrack {
                                layer: (*path).clone(),
                                property: name.to_owned(),
                                keys,
                            });
                        }
                    }
                }
            }
            anim::AUDIO => {
                let mut clips = track.clips().iter();
                for clip in clips.by_ref() {
                    let Some(pcm) = arena
                        .get_by_id(clip.source)
                        .ok()
                        .and_then(|entity| entity.component(sue_core::ComponentKind::PCMFloat1D))
                        .and_then(|component| component.as_pcm())
                    else {
                        lost = true;
                        continue;
                    };
                    // Only the sound from its start, whole, plays again.
                    let whole = Time::from_samples(pcm.frames() as i64, pcm.sample_rate());
                    lost |= clip.source_in != Time::ZERO || clip.duration < whole;
                    audio.push(ClipAudio {
                        name: track.name.clone(),
                        start: u32::try_from(clip.start.frame(rate).max(0)).unwrap_or(0),
                        volume: anim::volume_of(clip),
                        muted: track.muted,
                        file: sue_io::encode_wav_as(pcm, sue_io::SampleDepth::Float32),
                    });
                }
                lost |= track.clips().len() > 1;
            }
            _ => lost = true,
        }
    }
    // Layer keyframes.
    for (id, path) in path_of {
        let Ok(entity) = arena.get_by_id(*id) else {
            continue;
        };
        if camera::is_camera(arena, *id) {
            continue;
        }
        let Some(animation) = entity.ext::<Animation>() else {
            continue;
        };
        if !motion::keyframes_on(entity) {
            // Keys that do not count are not written.
            lost |= animation.channels().next().is_some();
            continue;
        }
        for (name, curve) in animation.channels() {
            let Some((_, property)) = LAYER_CURVES.iter().find(|(curve, _)| *curve == name) else {
                lost = true;
                continue;
            };
            let (keys, off) = keys_out(curve, rate);
            lost |= off;
            if !keys.is_empty() {
                tracks.push(ClipTrack {
                    layer: (*path).clone(),
                    property: (*property).to_owned(),
                    keys,
                });
            }
        }
    }
    // The same tracks in the same order, whatever order the map gave.
    tracks.sort_by(|a, b| (&a.layer, &a.property).cmp(&(&b.layer, &b.property)));
    // Other timelines, labels, the output frame, scene and shot numbers,
    // onion skin and playback settings have no place in the file yet.
    let picture_entity = arena.get_by_id(picture).ok();
    lost |= setup::timelines(arena, picture).len() > 1
        || picture_entity.is_some_and(|entity| {
            [setup::LABELS, setup::OUTPUT, setup::PLAYBACK, anim::ONION]
                .iter()
                .any(|key| entity.property(key).is_some())
        });
    out.timeline = Some(ClipTimeline {
        name: settings.name,
        fps: anim::fps(sequence) as f32,
        frames: length,
        playback: [first, last],
        tracks,
        audio,
    });
    out.lines
        .push(Line::new(tr!("clip-report-timeline"), Outcome::Live).lost(lost));
}

/// A small picture of the whole canvas, at most [`PREVIEW_SIDE`] a side.
pub(super) fn preview(arena: &Arena, picture: EntityId) -> Option<ClipPicture> {
    let full = flatten(
        arena,
        picture,
        RenderOptions {
            paper: true,
            drafts: false,
        },
    )?;
    let bytes = full.bytes()?;
    let (w, h) = (full.width(), full.height());
    let scale = (f64::from(w.max(h)) / f64::from(PREVIEW_SIDE)).max(1.0);
    let (pw, ph) = (
        ((f64::from(w) / scale).round() as u32).max(1),
        ((f64::from(h) / scale).round() as u32).max(1),
    );
    let mut rgba = Vec::with_capacity(pw as usize * ph as usize * 4);
    for y in 0..ph {
        let (y0, y1) = (y * h / ph, ((y + 1) * h / ph).max(y * h / ph + 1));
        for x in 0..pw {
            let (x0, x1) = (x * w / pw, ((x + 1) * w / pw).max(x * w / pw + 1));
            let mut sum = [0_u64; 4];
            for sy in y0..y1.min(h) {
                for sx in x0..x1.min(w) {
                    let at = (sy as usize * w as usize + sx as usize) * 4;
                    for (total, value) in sum.iter_mut().zip(&bytes[at..at + 4]) {
                        *total += u64::from(*value);
                    }
                }
            }
            let count = u64::from((y1.min(h) - y0) * (x1.min(w) - x0)).max(1);
            rgba.extend(sum.map(|total| (total / count) as u8));
        }
    }
    Some(ClipPicture {
        width: pw,
        height: ph,
        rgba,
    })
}

// ----- In from the file --------------------------------------------------------------

/// `entity`, a folder made from a layer of `content`, marked as the
/// animation folder or the 2D camera folder it is.
pub(super) fn folder_in(mut entity: Entity, content: Option<&ClipContent>) -> Entity {
    match content {
        Some(ClipContent::AnimationFolder(_)) => {
            entity = entity.with_property(anim::ANIMATION, true);
        }
        Some(ClipContent::Camera(view)) => {
            entity = entity
                .with_property(camera::CAMERA, true)
                .with_property(camera::X, f64::from(view.center[0]))
                .with_property(camera::Y, f64::from(view.center[1]))
                .with_property(camera::SCALE, f64::from(view.scale))
                .with_property(camera::ANGLE, f64::from(view.angle));
        }
        _ => {}
    }
    entity
}

/// `edit` (which makes `picture` and the layers of `doc`, each at its
/// place in `paths`) going on to give the picture `doc`'s comps, rulers
/// and timeline; `parts` says how each came out.
pub(super) fn into(
    doc: &ClipDocument,
    picture: EntityId,
    paths: &[(ClipLayerPath, EntityId)],
    mut edit: Edit,
    parts: &mut Vec<Line>,
) -> Edit {
    let id_of: HashMap<&ClipLayerPath, EntityId> =
        paths.iter().map(|(path, id)| (path, *id)).collect();
    if !doc.comps.is_empty() {
        let mut lost = false;
        let found: Vec<comps::Comp> = doc
            .comps
            .iter()
            .map(|comp| comps::Comp {
                name: comp.name.clone(),
                states: comp
                    .states
                    .iter()
                    .filter_map(|state| {
                        let Some(layer) = id_of.get(&state.layer) else {
                            lost = true;
                            return None;
                        };
                        let blend = blend_of(state.composite).unwrap_or_else(|| {
                            lost = true;
                            BlendMode::Normal
                        });
                        Some(comps::LayerState {
                            layer: *layer,
                            visible: state.visible,
                            opacity: state.opacity.clamp(0.0, 1.0),
                            blend: blend.name().to_owned(),
                        })
                    })
                    .collect(),
            })
            .collect();
        edit = edit.then(comps::store(picture, &found, ""));
        parts.push(Line::new(tr!("clip-report-comps"), Outcome::Exact).lost(lost));
    }
    edit = rulers_into(doc, picture, &id_of, edit, parts);
    if let Some(timeline) = &doc.timeline {
        edit = timeline_into(doc, timeline, picture, &id_of, edit, parts);
    }
    edit
}

fn rulers_into(
    doc: &ClipDocument,
    picture: EntityId,
    id_of: &HashMap<&ClipLayerPath, EntityId>,
    mut edit: Edit,
    parts: &mut Vec<Line>,
) -> Edit {
    let mut found = Rulers::default();
    let mut lost = false;
    let mut owner = None;
    let mut any = false;
    for (path, layer) in doc.paths() {
        for ruler in &layer.rulers {
            any = true;
            if !ruler.everywhere {
                lost |= owner.as_ref().is_some_and(|owner| *owner != path);
                owner.get_or_insert_with(|| path.clone());
            }
            let points = &ruler.points;
            // A picture holds one ruler of each kind (curves aside).
            let taken = match ruler.kind {
                ClipRulerKind::Linear => match points.as_slice() {
                    &[a, b] => found.linear.replace([a, b]).is_some(),
                    _ => true,
                },
                ClipRulerKind::Parallel => match points.as_slice() {
                    &[a, b] => found
                        .special
                        .replace(Special::Parallel((b[1] - a[1]).atan2(b[0] - a[0])))
                        .is_some(),
                    _ => true,
                },
                ClipRulerKind::Radial | ClipRulerKind::Concentric => match points.first() {
                    Some(at) if points.len() == 1 => {
                        let special = if ruler.kind == ClipRulerKind::Radial {
                            Special::Radial(*at)
                        } else {
                            Special::Concentric(*at)
                        };
                        found.special.replace(special).is_some()
                    }
                    _ => true,
                },
                ClipRulerKind::Perspective => {
                    (points.is_empty() || points.len() > 3)
                        || found
                            .perspective
                            .replace(Perspective {
                                points: points.clone(),
                            })
                            .is_some()
                }
                ClipRulerKind::Symmetry => match points.as_slice() {
                    &[a, b] => {
                        found
                            .symmetry
                            .replace((a, (b[1] - a[1]).atan2(b[0] - a[0])))
                            .is_some()
                            || ruler.lines != 2
                    }
                    _ => true,
                },
                ClipRulerKind::Curve | ClipRulerKind::Figure => {
                    if points.len() < 2 {
                        true
                    } else {
                        found.paths.push(Path {
                            points: points.clone(),
                            closed: ruler.kind == ClipRulerKind::Figure,
                        });
                        false
                    }
                }
            };
            lost |= taken || !ruler.snap;
        }
    }
    if !any {
        return edit;
    }
    for key in rulers::ALL {
        if let Some(text) = found.text(key) {
            edit = edit.then(SetProperty::new(picture, key, text));
        }
    }
    if let Some(owner) = owner.and_then(|path| id_of.get(&path).copied()) {
        edit = edit
            .then(SetProperty::new(
                picture,
                rulers::LAYER,
                Value::Entity(owner),
            ))
            .then(SetProperty::new(
                picture,
                rulers::RANGE,
                ShowRange::EditingLayer.id(),
            ));
    }
    parts.push(Line::new(tr!("clip-report-rulers"), Outcome::Exact).lost(lost));
    edit
}

/// Frames a second as Sue's rate (to a thousandth of a frame).
fn rate_of(fps: f32) -> Rate {
    let fps = if fps.is_finite() {
        fps.clamp(1.0, 1000.0)
    } else {
        24.0
    };
    if (fps - fps.round()).abs() < 1e-4 {
        Rate::new(fps.round() as u32, 1)
    } else {
        Rate::new((fps * 1000.0).round() as u32, 1000)
    }
}

/// `keys` as a curve at `rate`, its Smooth keys' handles set.
fn curve_in(keys: &[ClipKey], rate: Rate) -> Curve {
    let mut curve = Curve::new();
    for key in keys {
        let ease = match key.interpolation {
            ClipInterpolation::Hold => Ease::Hold,
            ClipInterpolation::Linear => Ease::Linear,
            ClipInterpolation::Smooth => Ease::Smooth,
        };
        curve.insert(Key {
            time: Time::from_frames(i64::from(key.frame), rate),
            value: key.value,
            interpolation: ease.interpolation(),
        });
    }
    motion::smoothed(&curve)
}

#[expect(
    clippy::too_many_lines,
    reason = "one pass from top to bottom; split up, its steps would be read out of order"
)]
fn timeline_into(
    doc: &ClipDocument,
    timeline: &ClipTimeline,
    picture: EntityId,
    id_of: &HashMap<&ClipLayerPath, EntityId>,
    mut edit: Edit,
    parts: &mut Vec<Line>,
) -> Edit {
    let rate = rate_of(timeline.fps);
    let mut lost = !near_rate(rate, timeline.fps);
    let length = timeline.frames.clamp(1, 100_000);
    let mut sequence = Sequence::new(rate, anim::MIX_RATE);
    // Tracks as a new timeline gives them, top layer first: an animation
    // folder its cels, a camera folder its camera, every other layer
    // outside an animation folder its own.
    let mut layers = doc.paths();
    layers.reverse();
    let animation_folders: Vec<&ClipLayerPath> = layers
        .iter()
        .filter(|(_, layer)| matches!(layer.content, Some(ClipContent::AnimationFolder(_))))
        .map(|(path, _)| path)
        .collect();
    let mut layer_track = HashMap::new();
    for (path, layer) in &layers {
        let Some(id) = id_of.get(path).copied() else {
            continue;
        };
        let inside = animation_folders
            .iter()
            .any(|folder| path.0.len() > folder.0.len() && path.0.starts_with(&folder.0));
        if inside {
            continue;
        }
        match &layer.content {
            Some(ClipContent::AnimationFolder(cels)) => {
                sequence = anim::with_linked_folder(&sequence, &layer.name, id);
                let track = sequence.tracks.len() - 1;
                for cel in &cels.cels {
                    let shown = match cel.child {
                        None => None,
                        Some(child) => {
                            let mut at = path.0.clone();
                            at.push(child);
                            match id_of.get(&ClipLayerPath(at)) {
                                Some(id) => Some(*id),
                                None => {
                                    lost = true;
                                    continue;
                                }
                            }
                        }
                    };
                    sequence = anim::assign(&sequence, track, cel.frame, shown, id, length);
                }
            }
            Some(ClipContent::Camera(_)) => {
                sequence
                    .tracks
                    .push(setup::camera_track(&layer.name, id, rate, length));
            }
            _ => {
                layer_track.insert((*path).clone(), sequence.tracks.len());
                sequence
                    .tracks
                    .push(setup::layer_track(&layer.name, id, rate, length));
            }
        }
    }
    // Keys: where a layer shows, a layer's motion, the camera's.
    let mut animations: HashMap<EntityId, (Animation, bool)> = HashMap::new();
    for track in &timeline.tracks {
        let Some(id) = id_of.get(&track.layer).copied() else {
            lost = true;
            continue;
        };
        let camera = matches!(
            doc.layer_at(&track.layer)
                .and_then(|layer| layer.content.as_ref()),
            Some(ClipContent::Camera(_))
        );
        if track.property == SHOWN {
            let Some(index) = layer_track.get(&track.layer).copied() else {
                lost = true;
                continue;
            };
            let old = &sequence.tracks[index];
            let mut shown = Track::new(old.name.clone(), old.kind.clone());
            shown.properties = old.properties.clone();
            let mut keys = track.keys.clone();
            keys.sort_by_key(|key| key.frame);
            for (at, key) in keys.iter().enumerate() {
                if key.value < 0.5 || key.frame >= length {
                    continue;
                }
                let end = keys[at + 1..]
                    .iter()
                    .find(|next| next.value < 0.5)
                    .map_or(length, |next| next.frame.min(length));
                if at > 0 && keys[at - 1].value >= 0.5 {
                    continue;
                }
                shown.insert(Clip::new(
                    id,
                    Time::from_frames(i64::from(key.frame), rate),
                    Time::from_frames(i64::from(end.saturating_sub(key.frame).max(1)), rate),
                ));
            }
            sequence.tracks[index] = shown;
            continue;
        }
        let channel = if camera {
            camera::VALUES
                .iter()
                .find(|name| **name == track.property)
                .copied()
        } else {
            LAYER_CURVES
                .iter()
                .find(|(_, name)| *name == track.property)
                .map(|(curve, _)| *curve)
        };
        let Some(channel) = channel else {
            lost = true;
            continue;
        };
        let (animation, _) = animations
            .entry(id)
            .or_insert_with(|| (Animation::new(), camera));
        *animation.channel_mut(channel) = curve_in(&track.keys, rate);
    }
    let mut ids: Vec<EntityId> = animations.keys().copied().collect();
    ids.sort();
    for id in ids {
        let (animation, camera) = animations.remove(&id).unwrap_or_default();
        edit = edit.then(SetExtension::set(id, animation));
        if !camera {
            edit = edit.then(SetProperty::new(id, motion::KEYFRAMES, true));
        }
    }
    // Sound.
    for sound in &timeline.audio {
        let Ok((decoded, _)) = sue_io::decode_audio(sound.file.clone(), None) else {
            lost = true;
            continue;
        };
        let pcm = decoded.pcm;
        let duration = Time::from_samples(pcm.frames() as i64, pcm.sample_rate());
        let entity = Entity::new(sound.name.clone(), pcm).with_class(sue_model::class::AUDIO);
        let mut track = Track::new(sound.name.clone(), anim::AUDIO);
        track.muted = sound.muted;
        let mut clip = Clip::new(
            entity.id(),
            Time::from_frames(i64::from(sound.start), rate),
            duration,
        );
        if (sound.volume - 1.0).abs() > f32::EPSILON {
            clip.properties
                .set(anim::VOLUME, f64::from(sound.volume.clamp(0.0, 2.0)));
            lost |= !(0.0..=2.0).contains(&sound.volume);
        }
        track.insert(clip);
        sequence.tracks.push(track);
        edit = edit.then(SpawnEntity::new(entity));
    }
    let mut settings = setup::Settings::named(timeline.name.clone());
    let last = length - 1;
    settings.start = timeline.playback[0].min(last);
    settings.end = (timeline.playback[1] < last).then_some(timeline.playback[1]);
    edit = edit
        .then(SetExtension::set(picture, sequence))
        .then(SetProperty::new(picture, anim::LENGTH, i64::from(length)))
        .then(settings.apply(picture));
    parts.push(Line::new(tr!("clip-report-timeline"), Outcome::Exact).lost(lost));
    edit
}

/// Whether `rate` is `fps` frames a second.
fn near_rate(rate: Rate, fps: f32) -> bool {
    (anim::fps(&Sequence::new(rate, anim::MIX_RATE)) - f64::from(fps)).abs() < 1e-3
}
