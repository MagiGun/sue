//! The compositor: effects, adjustment clips, mattes, transitions, blend
//! modes, motion blur, time remapping, nesting, and a preview that is
//! the export.

use super::*;
use crate::export::Renderer;
use crate::media::import;
use sue_core::txn::Batch;
use sue_core::{Entity, PixelFormat, RasterTile2D, Rate, UndoStack};
use sue_model::{Interpolation, Key, Transition};

fn s(seconds: f64) -> Time {
    Time::from_seconds(seconds)
}

struct Project {
    arena: Arena,
    sequence: EntityId,
}

/// A 64 by 36 sequence at 25 fps with two video tracks and one audio.
fn project(sources: Vec<Entity>) -> (Project, Vec<EntityId>) {
    let mut arena = Arena::new();
    let ids: Vec<EntityId> = sources.iter().map(Entity::id).collect();
    let edit = Entity::empty("Edit", sue_model::class::SEQUENCE).with_ext(model::empty_sequence(
        64,
        36,
        Rate::FPS_25,
        2,
        1,
    ));
    let sequence = edit.id();
    let mut batch = Batch::new("make");
    batch.push(Box::new(sue_core::txn::SpawnEntity::new(edit)));
    for source in sources {
        batch.push(Box::new(sue_core::txn::SpawnEntity::new(source)));
    }
    UndoStack::new().apply(&mut arena, batch).unwrap();
    (Project { arena, sequence }, ids)
}

fn edit(project: &mut Project, change: impl FnOnce(&mut Sequence)) {
    let entity = project.arena.get_by_id_mut(project.sequence).unwrap();
    change(entity.ext_mut::<Sequence>().unwrap());
}

fn place(sequence: &mut Sequence, track: usize, source: EntityId, start: f64, length: f64) -> u64 {
    let mut clip = Clip::new(source, s(start), s(length));
    clip.id = sequence.alloc_id();
    sequence.tracks[track].insert(clip.clone());
    clip.id
}

fn clip_mut(sequence: &mut Sequence, id: u64) -> &mut Clip {
    sequence.find_mut(id).unwrap().1
}

/// A still of `width` by `height`: left half `left`, right half `right`.
fn still(width: u32, height: u32, left: [u8; 4], right: [u8; 4]) -> Entity {
    let mut rgba = Vec::new();
    for _ in 0..height {
        for x in 0..width {
            rgba.extend_from_slice(if x < width / 2 { &left } else { &right });
        }
    }
    Entity::new(
        "Still",
        RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, rgba).unwrap(),
    )
}

fn draw(project: &Project, time: Time, size: (u32, u32)) -> Vec<u8> {
    let mut renderer = Renderer::new(project.arena.clone(), project.sequence, size, false).unwrap();
    renderer.frame(time)
}

fn px(frame: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let at = ((y * width + x) * 4) as usize;
    [frame[at], frame[at + 1], frame[at + 2], frame[at + 3]]
}

#[test]
fn a_clip_with_effects_is_drawn_with_them_and_needs_the_processor() {
    let (mut project, ids) = project(vec![import::solid("Gray", [100, 100, 100])]);
    let clip = {
        let mut id = 0;
        edit(&mut project, |sequence| {
            id = place(sequence, 1, ids[0], 0.0, 2.0)
        });
        id
    };
    let kind = |id: EntityId| {
        project
            .arena
            .get_by_id(id)
            .map_or(SourceKind::Unknown, SourceKind::of)
    };
    let plain = model::sequence(&project.arena, project.sequence)
        .unwrap()
        .clone();
    assert!(!needs_processing(&plain, s(1.0), &kind));
    assert_eq!(
        px(&draw(&project, s(1.0), (64, 36)), 64, 10, 10),
        [100, 100, 100, 255]
    );
    edit(&mut project, |sequence| {
        let clip = clip_mut(sequence, clip);
        let id = fx::add(clip, "color.brightness_contrast");
        fx::update(clip, id, |effect| {
            effect.params.set("brightness", 40.0);
        });
    });
    let kind = |id: EntityId| {
        project
            .arena
            .get_by_id(id)
            .map_or(SourceKind::Unknown, SourceKind::of)
    };
    let sequence = model::sequence(&project.arena, project.sequence)
        .unwrap()
        .clone();
    assert!(needs_processing(&sequence, s(1.0), &kind));
    let lit = px(&draw(&project, s(1.0), (64, 36)), 64, 10, 10);
    let want = sue_io::picture_ops::brightness_contrast([100.0 / 255.0; 3], 0.4, 0.0)[0] * 255.0;
    assert!((f32::from(lit[0]) - want).abs() <= 1.5, "{lit:?} {want}");
}

#[test]
fn the_preview_is_the_export_at_a_smaller_size() {
    let (mut project, ids) = project(vec![
        still(64, 36, [200, 40, 40, 255], [40, 200, 40, 255]),
        import::title("Title", "Sue"),
    ]);
    edit(&mut project, |sequence| {
        let a = place(sequence, 1, ids[0], 0.0, 2.0);
        let clip = clip_mut(sequence, a);
        fx::add(clip, "blur.gaussian");
        fx::add(clip, "stylize.vignette");
        let grade = crate::grade::Corrector {
            saturation: 1.4,
            lift: [0.0, 0.0, 0.05, 0.0],
            ..crate::grade::Corrector::default()
        };
        crate::grade::set_grade(clip, &[grade]);
        place(sequence, 0, ids[1], 0.0, 2.0);
    });
    let full = draw(&project, s(1.0), (64, 36));
    let half = draw(&project, s(1.0), (32, 18));
    // The export at full size, shrunk two by two, against the preview.
    let mut worst = 0_i32;
    let mut total = 0_i64;
    for y in 0..18 {
        for x in 0..32 {
            let mut mean = [0_i32; 3];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let p = px(&full, 64, x * 2 + dx, y * 2 + dy);
                for c in 0..3 {
                    mean[c] += i32::from(p[c]);
                }
            }
            let preview = px(&half, 32, x, y);
            for c in 0..3 {
                let d = (mean[c] / 4 - i32::from(preview[c])).abs();
                worst = worst.max(d);
                total += i64::from(d);
            }
        }
    }
    let average = total as f64 / (32.0 * 18.0 * 3.0);
    assert!(average < 6.0, "mean difference {average}, worst {worst}");
    // At the same size the preview and the export are the same frame.
    let mut composer = Composer::default();
    let sequence = model::sequence(&project.arena, project.sequence)
        .unwrap()
        .clone();
    let arena = project.arena.clone();
    let direct = frame(
        &arena,
        &sequence,
        s(1.0),
        (64, 36),
        Options::default(),
        &mut |layer: &Layer| {
            render::generated(
                &arena,
                layer.source,
                &layer.kind,
                layer.source_time,
                (64, 36),
                1.0,
            )
            .map(Arc::new)
        },
        &mut composer,
    );
    assert_eq!(direct, full);
}

#[test]
fn an_adjustment_clip_changes_only_what_is_under_it() {
    let (mut project, ids) = project(vec![
        import::solid("Red", [200, 50, 50]),
        import::adjustment("Adjust"),
    ]);
    edit(&mut project, |sequence| {
        place(sequence, 1, ids[0], 0.0, 4.0);
        let adjust = place(sequence, 0, ids[1], 1.0, 2.0);
        let clip = clip_mut(sequence, adjust);
        let id = fx::add(clip, "color.hue_saturation");
        fx::update(clip, id, |effect| {
            effect.params.set("saturation", -100.0);
        });
        clip.properties.set(model::key::OPACITY, 1.0);
    });
    let before = px(&draw(&project, s(0.5), (64, 36)), 64, 5, 5);
    assert_eq!(before, [200, 50, 50, 255]);
    let during = px(&draw(&project, s(2.0), (64, 36)), 64, 5, 5);
    assert!(
        during[0] == during[1] && during[1] == during[2],
        "{during:?}"
    );
    // Half opacity: half way.
    edit(&mut project, |sequence| {
        let id = sequence.tracks[0].clips()[0].id;
        clip_mut(sequence, id)
            .properties
            .set(model::key::OPACITY, 0.5);
    });
    let half = px(&draw(&project, s(2.0), (64, 36)), 64, 5, 5);
    assert!(half[1] > 50 && half[1] < during[1], "{half:?}");
}

#[test]
fn a_track_matte_cuts_the_layer_and_hides_the_matte() {
    let (mut project, ids) = project(vec![
        import::solid("Blue", [20, 20, 220]),
        still(64, 36, [255, 255, 255, 255], [0, 0, 0, 0]),
    ]);
    edit(&mut project, |sequence| {
        let fill = place(sequence, 1, ids[0], 0.0, 2.0);
        place(sequence, 0, ids[1], 0.0, 2.0);
        let clip = clip_mut(sequence, fill);
        fx::add(clip, "key.track_matte");
    });
    let picture = draw(&project, s(1.0), (64, 36));
    assert_eq!(
        px(&picture, 64, 10, 10),
        [20, 20, 220, 255],
        "inside the matte"
    );
    assert_eq!(
        px(&picture, 64, 50, 10),
        [0, 0, 0, 255],
        "outside it, and the matte is not drawn"
    );
    // Luma, inverted: the white half hides it.
    edit(&mut project, |sequence| {
        let id = sequence.tracks[1].clips()[0].id;
        let clip = clip_mut(sequence, id);
        let effect = fx::effects(clip)[0].id;
        fx::update(clip, effect, |effect| {
            effect.params.set("mode", Value::Int(1));
            effect.params.set("invert", true);
        });
    });
    let picture = draw(&project, s(1.0), (64, 36));
    assert_eq!(px(&picture, 64, 10, 10), [0, 0, 0, 255]);
    assert_eq!(px(&picture, 64, 50, 10), [20, 20, 220, 255]);
}

#[test]
fn wipes_slides_pushes_zooms_and_blurs() {
    let (mut project, ids) = project(vec![
        import::solid("Red", [255, 0, 0]),
        import::solid("Green", [0, 255, 0]),
    ]);
    let mut pair = (0, 0);
    edit(&mut project, |sequence| {
        pair = (
            place(sequence, 1, ids[0], 0.0, 2.0),
            place(sequence, 1, ids[1], 2.0, 2.0),
        );
    });
    let set = |project: &mut Project, kind: &str| {
        edit(project, |sequence| {
            sequence.tracks[1].transitions =
                vec![Transition::new(kind, s(1.5), s(1.0), pair.0, pair.1)];
        });
    };
    set(&mut project, "wipe");
    let half = draw(&project, s(2.0), (64, 36));
    assert_eq!(
        px(&half, 64, 4, 18)[1],
        255,
        "the incoming clip on the left"
    );
    assert_eq!(
        px(&half, 64, 60, 18)[0],
        255,
        "the outgoing one on the right"
    );
    set(&mut project, "slide");
    let slid = draw(&project, s(2.0), (64, 36));
    assert_eq!(px(&slid, 64, 20, 18)[1], 255, "half way in from the left");
    assert_eq!(px(&slid, 64, 50, 18)[0], 255);
    set(&mut project, "push");
    let pushed = draw(&project, s(2.0), (64, 36));
    assert_eq!(px(&pushed, 64, 50, 18)[0], 255, "the outgoing pushed right");
    set(&mut project, "zoom");
    let zoomed = draw(&project, s(2.0), (64, 36));
    let middle = px(&zoomed, 64, 32, 18);
    assert!(middle[0] > 50 && middle[1] > 50, "both show: {middle:?}");
    set(&mut project, "blur");
    let blurred = draw(&project, s(2.0), (64, 36));
    let middle = px(&blurred, 64, 32, 18);
    assert!(middle[0] > 50 && middle[1] > 50, "{middle:?}");
}

#[test]
fn blend_modes_mix_with_what_is_under() {
    let (mut project, ids) = project(vec![
        import::solid("Gray", [128, 128, 128]),
        import::solid("Half", [128, 255, 64]),
    ]);
    edit(&mut project, |sequence| {
        place(sequence, 1, ids[0], 0.0, 2.0);
        let top = place(sequence, 0, ids[1], 0.0, 2.0);
        clip_mut(sequence, top)
            .properties
            .set(fx_key::BLEND, "multiply");
    });
    let at = px(&draw(&project, s(1.0), (64, 36)), 64, 30, 18);
    assert_eq!(at[0], 64, "{at:?}");
    assert_eq!(at[1], 128);
    assert_eq!(at[2], 32);
}

#[test]
fn motion_blur_smears_a_moving_layer_and_anchor_turns_about_its_point() {
    let (mut project, ids) = project(vec![still(
        8,
        8,
        [255, 255, 255, 255],
        [255, 255, 255, 255],
    )]);
    edit(&mut project, |sequence| {
        let id = place(sequence, 1, ids[0], 0.0, 2.0);
        let clip = clip_mut(sequence, id);
        clip.properties.set(model::key::SCALE, 0.25);
        let curve = clip.animation.channel_mut(model::key::POS_X);
        for (seconds, x) in [(0.0, -200.0), (2.0, 200.0)] {
            curve.insert(Key {
                time: s(seconds),
                value: x,
                interpolation: Interpolation::Linear,
            });
        }
    });
    let sharp = draw(&project, s(1.0), (64, 36));
    let lit = |frame: &[u8]| (0..64).filter(|x| px(frame, 64, *x, 18)[0] > 20).count();
    edit(&mut project, |sequence| {
        let id = sequence.tracks[1].clips()[0].id;
        clip_mut(sequence, id)
            .properties
            .set(fx_key::MOTION_BLUR, 360.0);
    });
    let smeared = draw(&project, s(1.0), (64, 36));
    assert!(
        lit(&smeared) > lit(&sharp) + 4,
        "{} {}",
        lit(&smeared),
        lit(&sharp)
    );
    // The anchor at the picture's left edge: a half turn swings the
    // picture to the left of the middle.
    edit(&mut project, |sequence| {
        let id = sequence.tracks[1].clips()[0].id;
        let clip = clip_mut(sequence, id);
        clip.properties.remove(fx_key::MOTION_BLUR);
        clip.animation = sue_model::Animation::new();
        clip.properties.set(model::key::SCALE, 0.5);
        clip.properties.set(fx_key::ANCHOR_X, -4.0);
        clip.properties.set(model::key::ROTATION, 180.0);
    });
    let turned = draw(&project, s(1.0), (64, 36));
    assert!(px(&turned, 64, 26, 18)[0] > 200 && px(&turned, 64, 38, 18)[0] < 20);
}

#[test]
fn time_remapping_picks_the_source_time() {
    let mut clip = Clip::new(EntityId::new(), s(10.0), s(4.0));
    clip.source_in = s(1.0);
    assert_eq!(render::source_time_of(&clip, s(11.0)), s(2.0));
    let curve = clip.animation.channel_mut(fx_key::REMAP);
    // Keyed in the clip's source time: half speed, then a hold.
    curve.insert(Key {
        time: s(1.0),
        value: 1.0,
        interpolation: Interpolation::Linear,
    });
    curve.insert(Key {
        time: s(3.0),
        value: 2.0,
        interpolation: Interpolation::Constant,
    });
    assert_eq!(render::source_time_of(&clip, s(11.0)), s(1.5));
    assert_eq!(render::source_time_of(&clip, s(13.5)), s(2.0));
}

#[test]
fn a_nested_sequence_draws_its_own_layers() {
    let (mut project, ids) = project(vec![import::solid("Teal", [0, 128, 128])]);
    let inner_edit = {
        let mut inner = model::empty_sequence(64, 36, Rate::FPS_25, 1, 0);
        let mut clip = Clip::new(ids[0], Time::ZERO, s(3.0));
        clip.id = inner.alloc_id();
        inner.tracks[0].insert(clip);
        Entity::empty("Inner", sue_model::class::SEQUENCE).with_ext(inner)
    };
    let inner = inner_edit.id();
    project.arena.insert_keep_id(inner_edit).unwrap();
    edit(&mut project, |sequence| {
        place(sequence, 1, inner, 0.0, 2.0);
    });
    assert_eq!(
        px(&draw(&project, s(1.0), (64, 36)), 64, 30, 18),
        [0, 128, 128, 255]
    );
    // A sequence that holds itself stops, and draws nothing past it.
    let outer = project.sequence;
    edit(&mut project, |sequence| {
        place(sequence, 0, outer, 0.0, 2.0);
    });
    let _ = draw(&project, s(1.0), (32, 18));
}

#[test]
fn the_frame_key_follows_what_shows() {
    let (mut project, ids) = project(vec![
        import::solid("A", [1, 2, 3]),
        import::solid("B", [4, 5, 6]),
    ]);
    let (mut a, mut b) = (0, 0);
    edit(&mut project, |sequence| {
        a = place(sequence, 1, ids[0], 0.0, 2.0);
        b = place(sequence, 1, ids[1], 2.0, 2.0);
    });
    let key = |project: &Project, seconds: f64| {
        let sequence = model::sequence(&project.arena, project.sequence).unwrap();
        frame_key(
            &project.arena,
            sequence,
            s(seconds),
            (64, 36),
            Options::default(),
        )
    };
    let (early, late) = (key(&project, 1.0), key(&project, 3.0));
    assert_ne!(early, late);
    assert_eq!(early, key(&project, 1.0));
    edit(&mut project, |sequence| {
        fx::add(clip_mut(sequence, b), "blur.gaussian");
    });
    assert_eq!(key(&project, 1.0), early, "an edit elsewhere keeps it");
    assert_ne!(key(&project, 3.0), late);
    edit(&mut project, |sequence| {
        clip_mut(sequence, a)
            .properties
            .set(model::key::OPACITY, 0.5);
    });
    assert_ne!(key(&project, 1.0), early);
    let sequence = model::sequence(&project.arena, project.sequence).unwrap();
    assert_ne!(
        frame_key(
            &project.arena,
            sequence,
            s(1.0),
            (64, 36),
            Options { grades: false }
        ),
        key(&project, 1.0)
    );
}

#[test]
fn masks_cut_a_layer_in_the_frame() {
    let (mut project, ids) = project(vec![import::solid("White", [255, 255, 255])]);
    edit(&mut project, |sequence| {
        let id = place(sequence, 1, ids[0], 0.0, 2.0);
        mask::set_masks(clip_mut(sequence, id), &[mask::Mask::rect()]);
    });
    let picture = draw(&project, s(1.0), (64, 36));
    assert_eq!(px(&picture, 64, 32, 18)[0], 255);
    assert_eq!(px(&picture, 64, 3, 3)[0], 0);
}

#[test]
fn color_effects_baked_into_a_table_match_them_run_one_by_one() {
    use crate::fx::pixels::{Ctx, Image, apply};
    let (mut project, ids) = project(vec![still(64, 36, [180, 90, 40, 255], [30, 140, 210, 255])]);
    let mut stack = Vec::new();
    edit(&mut project, |sequence| {
        let id = place(sequence, 1, ids[0], 0.0, 2.0);
        let clip = clip_mut(sequence, id);
        for (kind, key, value) in [
            ("color.brightness_contrast", "contrast", 25.0),
            ("color.hue_saturation", "hue", 40.0),
            ("color.levels", "gamma", 1.4),
            ("color.balance", "mid_blue", 30.0),
        ] {
            let effect = fx::add(clip, kind);
            fx::update(clip, effect, |found| {
                found.params.set(key, value);
            });
        }
        stack = fx::effects(clip)
            .iter()
            .map(|effect| (effect.kind.clone(), effect.at(clip, s(1.0))))
            .collect();
    });
    let baked = draw(&project, s(1.0), (64, 36));
    let mut direct = Image::new(64, 36, {
        let mut rgba = Vec::new();
        for _ in 0..36 {
            for x in 0..64 {
                rgba.extend_from_slice(if x < 32 {
                    &[180, 90, 40, 255]
                } else {
                    &[30, 140, 210, 255]
                });
            }
        }
        rgba
    });
    for (kind, params) in &stack {
        apply(kind, params, &mut direct, &Ctx::default());
    }
    let worst = baked
        .iter()
        .zip(&direct.rgba)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap_or(0);
    assert!(
        worst <= 4,
        "the table is the effects to {worst} steps in 255"
    );
}
