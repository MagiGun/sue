//! The layers that are more than pixels and the picture's own parts
//! (comps, rulers, the timeline), taken from Sue to a `.clip` and back,
//! on pictures made here only.

use sue_core::{Arena, Color, Entity, EntityId, PCMFloat1D, ReversibleTransaction, Time, Value};
use sue_io::clip::{ClipContent, ClipCorrection, read};
use sue_model::{Animation, Sequence, class, picture};

use super::*;
use crate::anim::{self, motion};
use crate::brush::Tip;
use crate::composite::{RenderOptions, flatten};
use crate::doc::{self, NewPicture};
use crate::effects;
use crate::gradient::{Edge, Look, Shape};
use crate::gradient_map::{GradientMap, Stop};
use crate::lettering::Lettering;
use crate::rulers::{self, Rulers};
use crate::{camera, comps, vector};

fn apply(arena: &mut Arena, mut edit: impl ReversibleTransaction<Arena>) {
    edit.apply(arena).expect("the edit applies");
}

/// Where each named layer of `picture` is.
fn named(arena: &Arena, picture: EntityId, name: &str) -> EntityId {
    doc::layers(arena, picture)
        .into_iter()
        .find(|info| info.name == name)
        .unwrap_or_else(|| panic!("a layer named {name}"))
        .id
}

/// A picture with a layer of every kind this package maps, comps,
/// rulers and an animation: an animation folder of three cels and a
/// blank key, a moving layer, a camera and a sound.
#[allow(clippy::too_many_lines)]
fn a_rich_picture(arena: &mut Arena) -> EntityId {
    let (edit, picture) = doc::create(&NewPicture {
        width: 120,
        height: 90,
        paper: Some(Color::srgb8(255, 255, 255)),
        ..NewPicture::default()
    })
    .unwrap();
    apply(arena, edit);
    let (edit, _) = doc::new_fill_layer(arena, picture, None, "Sky", Color::srgb8(90, 160, 230));
    apply(arena, edit);
    apply(
        arena,
        doc::set(named(arena, picture, "Sky"), picture::OPACITY, 0.5),
    );
    let gradient = doc::Gradient {
        line: [[10.0, 5.0], [100.0, 80.0]],
        look: Look {
            map: GradientMap::of(&[
                Stop::new(0.0, [250, 200, 10]),
                Stop {
                    alpha: 128,
                    ..Stop::new(0.35, [20, 120, 60])
                },
                Stop::new(1.0, [10, 10, 80]),
            ])
            .unwrap(),
            shape: Shape::Ellipse,
            edge: Edge::Mirror,
            aspect: 0.5,
        },
    };
    let (edit, _) = doc::new_gradient_layer(arena, picture, None, "Glow", gradient);
    apply(arena, edit);
    let mut lettering = Lettering::new([12.0, 20.0], [200, 30, 40]);
    lettering.content = "Hello\nSue".to_owned();
    lettering.size = 18.0;
    let text = crate::lettering::text_layer(&lettering, 120, 90).unwrap();
    let text_id = text.id();
    let spawn = doc::landed(arena, picture, None, text);
    apply(arena, spawn);
    let mut line = vector::figure_line(
        &[[10.0, 70.0], [40.0, 50.0], [80.0, 75.0]],
        Tip {
            size: 4.0,
            ..Tip::default()
        },
        [[0.0, 0.2, 1.0]; 2],
        true,
    );
    line.samples[1].pressure = 0.5;
    let lines = vec![line];
    let pixels = vector::render(&lines, 120, 90).unwrap();
    let ink = Entity::new("Ink", pixels)
        .with_class(vector::CLASS)
        .with_ext(vector::to_geometry(&lines));
    let ink_id = ink.id();
    let spawn = doc::landed(arena, picture, None, ink);
    apply(arena, spawn);
    let (edit, _) = doc::new_correction(arena, picture, None, "Warm", "hue:12,0.1,-0.05");
    apply(arena, edit);
    // A raster layer with a border, a tone, a layer color and a palette
    // color.
    let mut dots = vec![0_u8; 120 * 90 * 4];
    for at in (0..dots.len()).step_by(4 * 7) {
        dots[at..at + 4].copy_from_slice(&[30, 30, 30, 255]);
    }
    let dots = Entity::new(
        "Dots",
        sue_core::RasterTile2D::from_bytes(120, 90, sue_core::PixelFormat::Rgba8, dots).unwrap(),
    )
    .with_class(class::LAYER)
    .with_property(effects::BORDER, true)
    .with_property(effects::BORDER_WIDTH, 3.0)
    .with_property(effects::BORDER_COLOR, Color::srgb8(255, 250, 0))
    .with_property(effects::TONE, true)
    .with_property(effects::TONE_LINES, 40.0)
    .with_property(effects::TONE_ANGLE, 30.0)
    .with_property(effects::TONE_SHAPE, 1_i64)
    .with_property(effects::COLOR, true)
    .with_property(effects::COLOR_VALUE, Color::srgb8(0, 80, 160))
    .with_property(effects::COLOR_SUB, Color::srgb8(240, 240, 200))
    .with_property(doc::PALETTE_COLOR, Color::srgb8(92, 178, 96));
    let dots_id = dots.id();
    let spawn = doc::landed(arena, picture, None, dots);
    apply(arena, spawn);
    // Rulers on the text layer, shown while it is edited.
    let mut found = Rulers {
        linear: Some([[5.0, 5.0], [110.0, 60.0]]),
        perspective: Some(rulers::Perspective {
            points: vec![[0.0, 45.0], [120.0, 45.0]],
        }),
        symmetry: Some(([60.0, 45.0], 1.5)),
        ..Rulers::default()
    };
    found.paths.push(rulers::Path {
        points: vec![[10.0, 10.0], [30.0, 15.0], [50.0, 10.0]],
        closed: false,
    });
    for key in rulers::ALL {
        if let Some(text) = found.text(key) {
            apply(arena, doc::set(picture, key, text));
        }
    }
    apply(
        arena,
        rulers::ShowRange::EditingLayer.edit(picture, Some(text_id), "Ruler"),
    );
    // Two comps: as it is, and without the ink.
    let all = comps::capture(arena, picture, "All");
    apply(arena, doc::set(ink_id, picture::VISIBLE, false));
    let bare = comps::capture(arena, picture, "Bare");
    apply(arena, doc::set(ink_id, picture::VISIBLE, true));
    apply(arena, comps::store(picture, &[all, bare], "Comps"));
    // An animation: 24 frames at 12 a second, three cels and a blank key.
    let edit = anim::new_timeline(arena, picture, 12, 24);
    apply(arena, edit);
    let sequence = anim::timeline(arena, picture).unwrap().clone();
    let folder = anim::track_folder(&sequence.tracks[0]).unwrap();
    for frame in [0, 4, 8] {
        let (edit, _) = anim::new_cel(arena, picture, folder, frame).unwrap();
        apply(arena, edit);
    }
    let sequence = anim::timeline(arena, picture).unwrap().clone();
    let blank = anim::assign(&sequence, 0, 12, None, folder, 24);
    apply(arena, anim::set(picture, blank, "Blank"));
    // The dots move and fade; the camera zooms.
    let at = |frame| Time::from_frames(frame, sequence.rate);
    apply(
        arena,
        motion::key(
            arena,
            dots_id,
            at(0),
            &[(motion::POSITION_X, 0.0), (motion::OPACITY, 1.0)],
            motion::Ease::Linear,
        ),
    );
    apply(
        arena,
        motion::key(
            arena,
            dots_id,
            at(10),
            &[(motion::POSITION_X, 30.0), (motion::OPACITY, 0.25)],
            motion::Ease::Smooth,
        ),
    );
    apply(
        arena,
        motion::key(
            arena,
            dots_id,
            at(20),
            &[(motion::POSITION_X, 10.0), (motion::OPACITY, 0.5)],
            motion::Ease::Hold,
        ),
    );
    let (edit, camera_id) = camera::new_camera(arena, picture, None);
    apply(arena, edit);
    apply(arena, doc::set(camera_id, camera::X, 60.0));
    apply(arena, doc::set(camera_id, camera::Y, 45.0));
    for (frame, scale) in [(0, 1.0), (16, 1.5)] {
        apply(
            arena,
            camera::key_value(
                arena,
                camera_id,
                camera::SCALE,
                at(frame),
                scale,
                motion::Ease::Smooth,
            ),
        );
    }
    // A sound from frame 6, at half volume.
    let tone: Vec<f32> = (0..4800)
        .map(|at| ((at as f32) * 0.05).sin() * 0.5)
        .collect();
    let pcm = PCMFloat1D::new(48_000, 1, tone).unwrap();
    let sequence = anim::timeline(arena, picture).unwrap().clone();
    let (edit, _) = anim::add_audio(picture, &sequence, "Beep", pcm, 6);
    apply(arena, edit);
    let sequence = anim::timeline(arena, picture).unwrap().clone();
    let track = anim::audio_tracks(&sequence)[0];
    let quiet = anim::with_volume(&sequence, track, 0.5);
    apply(arena, anim::set(picture, quiet, "Volume"));
    picture
}

fn opened(bytes: &[u8]) -> (Arena, EntityId, Report) {
    let mut arena = Arena::new();
    let (opened, report) = open(&arena, bytes, "Opened", "Open").unwrap();
    let Opened::Layers(edit, picture) = opened else {
        panic!("opened from a copy of Sue's project");
    };
    apply(&mut arena, edit);
    (arena, picture, report)
}

fn text(arena: &Arena, id: EntityId, key: &str) -> Option<String> {
    arena
        .get_by_id(id)
        .ok()?
        .property(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
}

#[test]
fn every_kind_goes_out_live_and_comes_back_live() {
    let mut arena = Arena::new();
    let picture = a_rich_picture(&mut arena);
    let (bytes, wrote) = encode_with_report(&arena, picture).unwrap();
    for name in ["Sky", "Glow", "Ink", "Dots"] {
        assert!(
            wrote.names(Outcome::Live).contains(&name),
            "{name}: {}",
            wrote.summary()
        );
    }
    assert!(wrote.names(Outcome::Live).contains(&"Hello"));
    assert_eq!(wrote.names(Outcome::SueOnly), ["Warm"]);
    assert!(wrote.names(Outcome::LeftOut).is_empty());
    // What Clip Studio is given: the fill and the gradient as they look,
    // the layer color as its own columns.
    let doc = read(&bytes).unwrap();
    let layer = |name: &str| {
        doc.all_layers()
            .into_iter()
            .find(|layer| layer.name == name)
            .unwrap()
    };
    let sky = layer("Sky");
    assert!(matches!(sky.content, Some(ClipContent::Fill(_))));
    let shown = sky.pixels.as_ref().unwrap().decode().unwrap();
    assert_eq!(&shown.data[..4], &[90, 160, 230, 255]);
    assert!(matches!(
        layer("Warm").content,
        Some(ClipContent::Correction(
            ClipCorrection::HueSaturationLuminosity { .. }
        ))
    ));
    let dots = layer("Dots");
    assert_eq!(dots.draw_color.color(), [0, 80, 160]);
    assert!(dots.draw_color.enabled());
    assert_eq!(dots.palette_color, Some([92, 178, 96]));
    assert!(doc.preview.is_some());
    assert_eq!(doc.comps.len(), 2);
    let timeline = doc.timeline.as_ref().unwrap();
    assert_eq!((timeline.fps, timeline.frames), (12.0, 24));
    assert_eq!(timeline.audio.len(), 1);

    // Back in Sue: every layer is what it was.
    let (back, again, report) = opened(&bytes);
    assert!(report.exact(), "{}", report.summary());
    let kinds = |arena: &Arena, picture: EntityId| -> Vec<(String, String)> {
        doc::layers(arena, picture)
            .into_iter()
            .map(|info| {
                let entity = arena.get_by_id(info.id).unwrap();
                (info.name, entity.class().to_owned())
            })
            .collect()
    };
    assert_eq!(kinds(&back, again), kinds(&arena, picture));
    for (name, keys) in [
        ("Sky", &[doc::FILL_COLOR][..]),
        (
            "Glow",
            &[
                doc::GRADIENT_STOPS,
                doc::GRADIENT_SHAPE,
                doc::GRADIENT_EDGE,
                doc::GRADIENT_ASPECT,
                doc::GRADIENT_FROM_X,
                doc::GRADIENT_TO_Y,
            ][..],
        ),
        ("Warm", &[doc::CORRECTION_FILTER][..]),
        (
            "Dots",
            &[
                effects::BORDER,
                effects::BORDER_WIDTH,
                effects::BORDER_COLOR,
                effects::TONE,
                effects::TONE_LINES,
                effects::TONE_ANGLE,
                effects::TONE_SHAPE,
                effects::COLOR,
                effects::COLOR_VALUE,
                effects::COLOR_SUB,
                doc::PALETTE_COLOR,
            ][..],
        ),
    ] {
        let (ours, theirs) = (named(&arena, picture, name), named(&back, again, name));
        for key in keys {
            let (a, b) = (
                arena.get_by_id(ours).unwrap().property(key).cloned(),
                back.get_by_id(theirs).unwrap().property(key).cloned(),
            );
            match (&a, &b) {
                (Some(Value::Float(x)), Some(Value::Float(y))) => {
                    assert!((x - y).abs() < 1e-4, "{name} {key}: {a:?} {b:?}");
                }
                (Some(Value::Int(x)), Some(Value::Float(y))) => {
                    assert!((*x as f64 - y).abs() < 1e-4, "{name} {key}: {a:?} {b:?}");
                }
                _ => assert_eq!(a, b, "{name} {key}"),
            }
        }
    }
    // The words and the lines.
    let words = |arena: &Arena, picture| {
        Lettering::of(arena.get_by_id(named(arena, picture, "Hello")).unwrap()).unwrap()
    };
    assert_eq!(words(&back, again), words(&arena, picture));
    let lines = |arena: &Arena, picture| {
        vector::lines(&vector::geometry(arena, named(arena, picture, "Ink")))
    };
    let (ours, theirs) = (lines(&arena, picture), lines(&back, again));
    assert_eq!(ours.len(), theirs.len());
    for (a, b) in ours.iter().zip(&theirs) {
        assert_eq!(a.samples, b.samples);
        assert_eq!(a.marks, b.marks);
        assert_eq!(a.colors, b.colors);
        assert_eq!(a.tip.to_text(), b.tip.to_text());
    }
    // The picture looks the same.
    let look = |arena: &Arena, picture| {
        flatten(
            arena,
            picture,
            RenderOptions {
                paper: true,
                drafts: false,
            },
        )
        .unwrap()
        .bytes()
        .unwrap()
        .to_vec()
    };
    assert_eq!(look(&back, again), look(&arena, picture));
    // Rulers, comps.
    for key in rulers::ALL.into_iter().chain([rulers::RANGE]) {
        if key != rulers::SYMMETRY {
            assert_eq!(text(&back, again, key), text(&arena, picture, key), "{key}");
        }
    }
    // A mirror's angle comes back from two points: to a millionth.
    let ([x, y], angle) = Rulers::of(&back, again).symmetry.unwrap();
    assert_eq!([x, y], [60.0, 45.0]);
    assert!((angle - 1.5).abs() < 1e-6, "{angle}");
    let owner = back
        .get_by_id(again)
        .unwrap()
        .property(rulers::LAYER)
        .and_then(Value::as_entity);
    assert_eq!(owner, Some(named(&back, again, "Hello")));
    let names = |comps: Vec<comps::Comp>| -> Vec<(String, usize)> {
        comps
            .into_iter()
            .map(|comp| (comp.name, comp.states.len()))
            .collect()
    };
    assert_eq!(
        names(comps::read(&back, again)),
        names(comps::read(&arena, picture))
    );
    let bare = comps::read(&back, again).remove(1);
    let ink = named(&back, again, "Ink");
    assert!(
        bare.states
            .iter()
            .any(|state| state.layer == ink && !state.visible)
    );
}

#[test]
fn an_animation_goes_out_and_comes_back_with_its_cels_keys_camera_and_sound() {
    let mut arena = Arena::new();
    let picture = a_rich_picture(&mut arena);
    let bytes = encode(&arena, picture).unwrap();
    let (back, again, _) = opened(&bytes);
    let ours = anim::timeline(&arena, picture).unwrap();
    let theirs = anim::timeline(&back, again).unwrap();
    assert_eq!(theirs.rate, ours.rate);
    assert_eq!(anim::length(&back, again), 24);
    // The cels, by name, on the same frames, and the blank key.
    let cels = |arena: &Arena, sequence: &Sequence| -> Vec<(u32, Option<String>)> {
        let track = anim::folders(sequence)[0];
        anim::keys(&sequence.tracks[track], sequence.rate)
            .into_iter()
            .map(|(frame, cel)| {
                (
                    frame,
                    cel.map(|cel| arena.get_by_id(cel).unwrap().name().to_owned()),
                )
            })
            .collect()
    };
    let want = cels(&arena, ours);
    assert_eq!(want.len(), 4);
    assert_eq!(want[3], (12, None));
    assert_eq!(cels(&back, theirs), want);
    // Every frame shows the same.
    for frame in 0..24 {
        let view = |arena: &Arena, picture, sequence| {
            let mut hidden: Vec<String> = anim::view(sequence, frame, None)
                .hidden
                .iter()
                .map(|id| arena.get_by_id(*id).unwrap().name().to_owned())
                .collect();
            hidden.sort();
            let _ = picture;
            hidden
        };
        assert_eq!(
            view(&back, again, theirs),
            view(&arena, picture, ours),
            "frame {frame}"
        );
    }
    // The keys, as they move.
    let curves = |arena: &Arena, id: EntityId| -> Animation {
        arena
            .get_by_id(id)
            .unwrap()
            .ext::<Animation>()
            .cloned()
            .unwrap()
    };
    for name in ["Dots", "Camera 1"] {
        let (a, b) = (
            curves(&arena, named(&arena, picture, name)),
            curves(&back, named(&back, again, name)),
        );
        assert_eq!(a, b, "{name}");
    }
    let dots = back.get_by_id(named(&back, again, "Dots")).unwrap();
    assert!(motion::keyframes_on(dots));
    let camera = named(&back, again, "Camera 1");
    assert!(camera::is_camera(&back, camera));
    assert_eq!(
        back.get_by_id(camera)
            .unwrap()
            .property(camera::X)
            .and_then(Value::as_f64),
        Some(60.0)
    );
    // The sound, sample for sample, from the same frame at the same
    // volume.
    let sound = |arena: &Arena, sequence: &Sequence| {
        let track = &sequence.tracks[anim::audio_tracks(sequence)[0]];
        let clip = &track.clips()[0];
        let pcm = arena
            .get_by_id(clip.source)
            .unwrap()
            .component(sue_core::ComponentKind::PCMFloat1D)
            .unwrap()
            .as_pcm()
            .unwrap()
            .clone();
        (
            clip.start,
            anim::volume_of(clip),
            pcm.samples().to_vec(),
            pcm.sample_rate(),
        )
    };
    assert_eq!(sound(&back, theirs), sound(&arena, ours));
}

#[test]
fn a_rich_picture_through_sue_twice_writes_the_same_file() {
    let mut arena = Arena::new();
    let picture = a_rich_picture(&mut arena);
    let bytes = encode(&arena, picture).unwrap();
    let (back, again, _) = opened(&bytes);
    let (twice, report) = encode_with_report(&back, again).unwrap();
    assert!(report.exact(), "{}", report.summary());
    assert_eq!(read(&twice).unwrap(), read(&bytes).unwrap());
    // Changing a comp in Sue writes Sue's comps again, and says so.
    let mut back = back;
    let mut comps_now = comps::read(&back, again);
    comps_now.pop();
    apply(&mut back, comps::store(again, &comps_now, "Comps"));
    let (changed, report) = encode_with_report(&back, again).unwrap();
    assert_eq!(read(&changed).unwrap().comps.len(), 1);
    assert!(
        report.summary().contains("the layer comps"),
        "{}",
        report.summary()
    );
}

/// A layer changed in Sue is written from Sue's model, but what the
/// picture's own parts hang on it (the rulers of the layer they belong
/// to, the cels of an animation folder, the camera of a camera folder)
/// still goes with it while those parts are unchanged.
#[test]
fn a_changed_layer_keeps_the_rulers_and_folders_hung_on_it() {
    let mut arena = Arena::new();
    let picture = a_rich_picture(&mut arena);
    let bytes = encode(&arena, picture).unwrap();
    let first = read(&bytes).unwrap();
    let (mut back, again, _) = opened(&bytes);
    let sequence = anim::timeline(&back, again).unwrap().clone();
    let folder = anim::track_folder(&sequence.tracks[anim::folders(&sequence)[0]]).unwrap();
    let camera = named(&back, again, "Camera 1");
    let hello = named(&back, again, "Hello");
    for id in [folder, camera, hello] {
        apply(&mut back, doc::set(id, picture::OPACITY, 0.75));
    }
    let (changed, _) = encode_with_report(&back, again).unwrap();
    let changed = read(&changed).unwrap();
    assert_eq!(changed.timeline, first.timeline);
    assert_eq!(changed.comps, first.comps);
    for (path, layer) in first.paths() {
        let now = changed.layer_at(&path).unwrap();
        assert_eq!(now.name, layer.name);
        assert_eq!(now.rulers, layer.rulers, "{}", layer.name);
        if matches!(
            layer.content,
            Some(ClipContent::AnimationFolder(_) | ClipContent::Camera(_))
        ) {
            assert_eq!(now.content, layer.content, "{}", layer.name);
        }
    }
    // And Sue opens it with its rulers, cels and camera.
    let (reopened, picture_again, report) = opened(&sue_io::clip::write(&changed).unwrap());
    assert!(
        !Rulers::of(&reopened, picture_again).is_empty(),
        "{}",
        report.summary()
    );
    let sequence = anim::timeline(&reopened, picture_again).unwrap();
    assert_eq!(anim::folders(sequence).len(), 1);
    assert!(camera::is_camera(
        &reopened,
        named(&reopened, picture_again, "Camera 1")
    ));
}

#[test]
fn opening_a_rich_picture_is_one_step_undone_and_redone_whole() {
    let mut arena = Arena::new();
    let picture = a_rich_picture(&mut arena);
    let bytes = encode(&arena, picture).unwrap();
    let mut world = Arena::new();
    let (opened, _) = open(&world, &bytes, "Opened", "Open").unwrap();
    let Opened::Layers(mut edit, again) = opened else {
        panic!("from layers");
    };
    edit.apply(&mut world).unwrap();
    let after: Vec<Entity> = world.iter().map(|(_, entity)| entity.clone()).collect();
    let relations = world.relations().iter().count();
    assert!(anim::timeline(&world, again).is_some());
    edit.revert(&mut world).unwrap();
    assert_eq!(world.iter().count(), 0, "undone, nothing is left");
    assert!(world.relations().iter().next().is_none());
    edit.apply(&mut world).unwrap();
    assert_eq!(world.iter().count(), after.len());
    assert_eq!(world.relations().iter().count(), relations);
    for entity in &after {
        assert_eq!(
            world.get_by_id(entity.id()).unwrap(),
            entity,
            "redone, the same ids"
        );
    }
}

/// A disc of `rgba` at `center`, `radius` across, painted on `layer`.
fn disc(arena: &mut Arena, layer: EntityId, center: [f32; 2], radius: f32, rgba: [u8; 4]) {
    let raster = doc::raster_mut(arena, layer).unwrap();
    let (width, height) = (raster.width(), raster.height());
    let bytes = raster.bytes_mut().unwrap();
    for y in 0..height {
        for x in 0..width {
            let (dx, dy) = (x as f32 + 0.5 - center[0], y as f32 + 0.5 - center[1]);
            let cover = (radius - dx.hypot(dy) + 0.5).clamp(0.0, 1.0);
            if cover > 0.0 {
                let at = ((y * width + x) * 4) as usize;
                let alpha = (f32::from(rgba[3]) * cover).round() as u8;
                bytes[at..at + 4].copy_from_slice(&[rgba[0], rgba[1], rgba[2], alpha]);
            }
        }
    }
}

/// A white picture `width` by `height` with one raster layer named
/// `Sketch` holding a dark disc.
fn base(arena: &mut Arena, width: u32, height: u32) -> EntityId {
    let (edit, picture) = doc::create(&NewPicture {
        width,
        height,
        paper: Some(Color::srgb8(255, 255, 255)),
        ..NewPicture::default()
    })
    .unwrap();
    apply(arena, edit);
    let (spawn, sketch) =
        doc::add_raster_layer(arena, picture, None, "Sketch".to_owned(), None).unwrap();
    apply(arena, spawn);
    disc(
        arena,
        sketch,
        [width as f32 * 0.35, height as f32 * 0.5],
        60.0,
        [40, 40, 40, 255],
    );
    picture
}

/// The bouncing ball the beginners' animation tutorial draws, as its
/// steps leave the file: 24 frames at 24 a second, an animation folder
/// of five cels (the ball high, falling, squashed on the ground, rising,
/// high again), the sketch under it, and the camera moving in a little.
fn a_bouncing_ball(arena: &mut Arena) -> EntityId {
    let picture = base(arena, 640, 360);
    apply(
        arena,
        doc::set(named(arena, picture, "Sketch"), picture::OPACITY, 0.3),
    );
    let edit = anim::new_timeline(arena, picture, 24, 24);
    apply(arena, edit);
    let sequence = anim::timeline(arena, picture).unwrap().clone();
    let folder = anim::track_folder(&sequence.tracks[0]).unwrap();
    for (frame, height, squash) in [
        (0, 60.0, 1.0),
        (4, 160.0, 1.0),
        (8, 300.0, 0.6),
        (12, 160.0, 1.0),
        (16, 60.0, 1.0),
    ] {
        let (edit, cel) = anim::new_cel(arena, picture, folder, frame).unwrap();
        apply(arena, edit);
        let radius: f32 = 36.0;
        let raster = doc::raster_mut(arena, cel).unwrap();
        let (width, rows) = (raster.width(), raster.height());
        let bytes = raster.bytes_mut().unwrap();
        for y in 0..rows {
            for x in 0..width {
                let dx = (x as f32 + 0.5 - 320.0) * squash;
                let dy = (y as f32 + 0.5 - height) / squash;
                let cover = (radius - dx.hypot(dy) + 0.5).clamp(0.0, 1.0);
                if cover > 0.0 {
                    let at = ((y * width + x) * 4) as usize;
                    bytes[at..at + 4].copy_from_slice(&[
                        220,
                        60,
                        40,
                        (255.0 * cover).round() as u8,
                    ]);
                }
            }
        }
    }
    let (edit, camera_id) = camera::new_camera(arena, picture, None);
    apply(arena, edit);
    apply(arena, doc::set(camera_id, camera::X, 320.0));
    apply(arena, doc::set(camera_id, camera::Y, 180.0));
    let rate = anim::timeline(arena, picture).unwrap().rate;
    for (frame, scale) in [(0, 1.0), (23, 1.2)] {
        apply(
            arena,
            camera::key_value(
                arena,
                camera_id,
                camera::SCALE,
                Time::from_frames(frame, rate),
                scale,
                motion::Ease::Linear,
            ),
        );
    }
    picture
}

#[test]
fn the_bouncing_ball_of_the_tutorial_converts_both_ways() {
    let mut arena = Arena::new();
    let picture = a_bouncing_ball(&mut arena);
    let (bytes, wrote) = encode_with_report(&arena, picture).unwrap();
    assert!(
        wrote.names(Outcome::LeftOut).is_empty(),
        "{}",
        wrote.summary()
    );
    let (back, again, report) = opened(&bytes);
    assert!(report.exact(), "{}", report.summary());
    let theirs = anim::timeline(&back, again).unwrap();
    let ours = anim::timeline(&arena, picture).unwrap();
    let frames = |arena: &Arena, sequence: &Sequence| -> Vec<(u32, Option<String>)> {
        anim::keys(&sequence.tracks[anim::folders(sequence)[0]], sequence.rate)
            .into_iter()
            .map(|(frame, cel)| {
                (
                    frame,
                    cel.map(|cel| arena.get_by_id(cel).unwrap().name().to_owned()),
                )
            })
            .collect()
    };
    assert_eq!(frames(&back, theirs), frames(&arena, ours));
    assert_eq!(frames(&back, theirs).len(), 5);
}

/// Writes one `.clip` per kind this package maps, and the bouncing ball,
/// into the folder `SUE_CLIP_HANDS_ON` names, for a person (or an agent
/// driving Clip Studio) to open and note what Clip Studio shows.
#[test]
#[ignore = "writes files for a person to open in Clip Studio"]
#[allow(clippy::too_many_lines)]
fn write_kinds_for_the_hands_on_check() {
    let Some(dir) = std::env::var_os("SUE_CLIP_HANDS_ON") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    let write = |name: &str, arena: &Arena, picture: EntityId| {
        let (bytes, report) = encode_with_report(arena, picture).unwrap();
        std::fs::write(dir.join(name), &bytes).unwrap();
        // Sue opens what it wrote; with `SUE_CLIP_HANDS_ON_LOOK` set,
        // how Sue shows it and how its layers look without Sue's own
        // parts (as Clip Studio is given them) are pictures there.
        let (back, again, report_in) = opened(&bytes);
        if let Some(look) = std::env::var_os("SUE_CLIP_HANDS_ON_LOOK") {
            let look = std::path::PathBuf::from(look);
            std::fs::create_dir_all(&look).unwrap();
            let options = RenderOptions {
                paper: true,
                drafts: false,
            };
            let stem = name.trim_end_matches(".clip");
            let png = |arena: &Arena, picture| {
                sue_io::encode_png(&flatten(arena, picture, options).unwrap()).unwrap()
            };
            std::fs::write(look.join(format!("{stem}-in-sue.png")), png(&back, again)).unwrap();
            let mut plain = read(&bytes).unwrap();
            plain.comps.clear();
            plain.timeline = None;
            let paths: Vec<_> = plain.paths().into_iter().map(|(path, _)| path).collect();
            for path in paths {
                let layer = plain.layer_at_mut(&path).unwrap();
                layer.content = None;
                layer.effects = None;
                layer.rulers.clear();
            }
            let (plain, plain_picture, _) = opened(&sue_io::clip::write(&plain).unwrap());
            std::fs::write(
                look.join(format!("{stem}-layers-only.png")),
                png(&plain, plain_picture),
            )
            .unwrap();
        }
        eprintln!(
            "{name}: wrote {}; opens {}",
            report.summary(),
            report_in.summary()
        );
    };
    let (w, h) = (600, 400);
    // A fill layer at half opacity.
    let mut arena = Arena::new();
    let picture = base(&mut arena, w, h);
    let (edit, fill) =
        doc::new_fill_layer(&arena, picture, None, "Fill", Color::srgb8(90, 160, 230));
    apply(&mut arena, edit);
    apply(&mut arena, doc::set(fill, picture::OPACITY, 0.5));
    write("01-fill.clip", &arena, picture);
    // A gradient layer: three stops, an ellipse, mirrored.
    let mut arena = Arena::new();
    let picture = base(&mut arena, w, h);
    let gradient = doc::Gradient {
        line: [[100.0, 200.0], [400.0, 200.0]],
        look: Look {
            map: GradientMap::of(&[
                Stop::new(0.0, [250, 200, 10]),
                Stop::new(0.5, [20, 120, 60]),
                Stop::new(1.0, [10, 10, 80]),
            ])
            .unwrap(),
            shape: Shape::Ellipse,
            edge: Edge::Mirror,
            aspect: 0.5,
        },
    };
    let (edit, _) = doc::new_gradient_layer(&arena, picture, None, "Gradient", gradient);
    apply(&mut arena, edit);
    write("02-gradient.clip", &arena, picture);
    // Corrections over a colored layer.
    let mut arena = Arena::new();
    let picture = base(&mut arena, w, h);
    let (spawn, color) =
        doc::add_raster_layer(&arena, picture, None, "Colors".to_owned(), None).unwrap();
    apply(&mut arena, spawn);
    disc(&mut arena, color, [400.0, 200.0], 90.0, [230, 120, 30, 255]);
    let (edit, _) = doc::new_correction(&arena, picture, None, "Posterize", "posterize:4");
    apply(&mut arena, edit);
    let (edit, _) = doc::new_correction(&arena, picture, None, "Hue", "hue:120,0.2,0");
    apply(&mut arena, edit);
    write("03-corrections.clip", &arena, picture);
    // Text.
    let mut arena = Arena::new();
    let picture = base(&mut arena, w, h);
    let mut lettering = Lettering::new([60.0, 60.0], [20, 20, 120]);
    lettering.content = "Sue Paint\nsecond line".to_owned();
    lettering.size = 40.0;
    let text = crate::lettering::text_layer(&lettering, w, h).unwrap();
    let spawn = doc::landed(&arena, picture, None, text);
    apply(&mut arena, spawn);
    write("04-text.clip", &arena, picture);
    // Vector lines.
    let mut arena = Arena::new();
    let picture = base(&mut arena, w, h);
    let lines: Vec<vector::Line> = [
        (
            vec![[60.0, 300.0], [200.0, 120.0], [340.0, 320.0]],
            [0.0, 0.0, 0.0],
            6.0,
        ),
        (vec![[300.0, 80.0], [540.0, 140.0]], [0.8, 0.1, 0.1], 12.0),
    ]
    .into_iter()
    .map(|(points, rgb, size)| {
        vector::figure_line(
            &points,
            Tip {
                size,
                ..Tip::default()
            },
            [rgb; 2],
            true,
        )
    })
    .collect();
    let pixels = vector::render(&lines, w, h).unwrap();
    let ink = Entity::new("Vector", pixels)
        .with_class(vector::CLASS)
        .with_ext(vector::to_geometry(&lines));
    let spawn = doc::landed(&arena, picture, None, ink);
    apply(&mut arena, spawn);
    write("05-vector.clip", &arena, picture);
    // Effects: a border and a layer color with a palette color on the
    // sketch, a tone on a gray disc.
    let mut arena = Arena::new();
    let picture = base(&mut arena, w, h);
    let sketch = named(&arena, picture, "Sketch");
    for (key, value) in [
        (effects::BORDER, Value::from(true)),
        (effects::BORDER_WIDTH, Value::from(6.0)),
        (
            effects::BORDER_COLOR,
            Value::Color(Color::srgb8(255, 220, 0)),
        ),
        (effects::COLOR, Value::from(true)),
        (effects::COLOR_VALUE, Value::Color(Color::srgb8(0, 80, 160))),
        (doc::PALETTE_COLOR, Value::Color(Color::srgb8(92, 178, 96))),
    ] {
        apply(&mut arena, doc::set(sketch, key, value));
    }
    let (spawn, toned) =
        doc::add_raster_layer(&arena, picture, None, "Tone".to_owned(), None).unwrap();
    apply(&mut arena, spawn);
    disc(
        &mut arena,
        toned,
        [430.0, 200.0],
        100.0,
        [128, 128, 128, 255],
    );
    for (key, value) in [
        (effects::TONE, Value::from(true)),
        (effects::TONE_LINES, Value::from(30.0)),
        (effects::TONE_ANGLE, Value::from(45.0)),
    ] {
        apply(&mut arena, doc::set(toned, key, value));
    }
    write("06-effects.clip", &arena, picture);
    // Layer comps.
    let mut arena = Arena::new();
    let picture = base(&mut arena, w, h);
    let sketch = named(&arena, picture, "Sketch");
    let all = comps::capture(&arena, picture, "With sketch");
    apply(&mut arena, doc::set(sketch, picture::VISIBLE, false));
    let bare = comps::capture(&arena, picture, "Without sketch");
    apply(&mut arena, doc::set(sketch, picture::VISIBLE, true));
    apply(&mut arena, comps::store(picture, &[all, bare], "Comps"));
    write("07-comps.clip", &arena, picture);
    // Rulers.
    let mut arena = Arena::new();
    let picture = base(&mut arena, w, h);
    let found = Rulers {
        linear: Some([[50.0, 50.0], [550.0, 350.0]]),
        perspective: Some(rulers::Perspective {
            points: vec![[0.0, 200.0], [600.0, 200.0]],
        }),
        symmetry: Some(([300.0, 200.0], std::f32::consts::FRAC_PI_2)),
        ..Rulers::default()
    };
    for key in rulers::ALL {
        if let Some(text) = found.text(key) {
            apply(&mut arena, doc::set(picture, key, text));
        }
    }
    write("08-rulers.clip", &arena, picture);
    // Every kind together, with an animation and a sound.
    let mut arena = Arena::new();
    let picture = a_rich_picture(&mut arena);
    write("09-everything.clip", &arena, picture);
    // The tutorial's bouncing ball.
    let mut arena = Arena::new();
    let picture = a_bouncing_ball(&mut arena);
    write("10-bouncing-ball.clip", &arena, picture);
}
