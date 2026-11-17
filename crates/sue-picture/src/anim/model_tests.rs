//! The animation model's edits, each with its undo pair (done, undone to
//! the arena as before with every id, redone to the arena as after), and
//! what they show when composited.

use sue_core::{
    Arena, Color, ComponentKind, EntityId, Rate, ReversibleTransaction, Time, UndoStack,
};
use sue_model::{Animation, Sequence};

use super::editing::{self, Selection};
use super::motion::{self, Ease};
use super::setup::{self, OutputFrame, Overflow, Playback, Settings};
use super::*;
use crate::composite::{RenderOptions, flatten_view};
use crate::doc::{NewPicture, create};

/// Everything an edit can change, in an order that does not depend on
/// where entities sit in the arena: each entity (id, name, class,
/// properties, pixels, timeline and keyframes) and each relation.
fn state(arena: &Arena) -> (Vec<String>, Vec<String>) {
    let mut entities: Vec<String> = arena
        .iter()
        .map(|(_, entity)| {
            let mut properties: Vec<String> = entity
                .properties()
                .iter()
                .map(|(key, value)| format!("{key}={value:?}"))
                .collect();
            properties.sort();
            let pixels = entity
                .component(ComponentKind::RasterTile2D)
                .and_then(|component| component.as_raster())
                .map(|raster| {
                    (
                        raster.width(),
                        raster.height(),
                        raster.bytes().map(<[u8]>::to_vec),
                    )
                });
            format!(
                "{:?} {} {} {properties:?} {pixels:?} {:?} {:?}",
                entity.id(),
                entity.name(),
                entity.class(),
                entity.ext::<Sequence>(),
                entity.ext::<Animation>(),
            )
        })
        .collect();
    entities.sort();
    let mut relations: Vec<String> = arena
        .relations()
        .iter()
        .map(|relation| {
            format!(
                "{:?} {} {:?} {:?}",
                relation.from, relation.kind, relation.to, relation.order
            )
        })
        .collect();
    relations.sort();
    (entities, relations)
}

/// Applies `edit`, then checks its undo pair.
fn undo_pair(arena: &mut Arena, history: &mut UndoStack<Arena>, edit: Edit) {
    let before = state(arena);
    history.apply(arena, edit).unwrap();
    let after = state(arena);
    assert_ne!(before, after, "the edit changes something");
    history.undo(arena).unwrap();
    assert_eq!(state(arena), before, "undo is exact");
    history.redo(arena).unwrap();
    assert_eq!(state(arena), after, "redo is exact");
}

/// A white `size` picture with a timeline of `length` frames at 10 a
/// second and one animation folder.
fn animated(size: u32, length: u32) -> (Arena, UndoStack<Arena>, EntityId, EntityId) {
    let mut arena = Arena::new();
    let (mut made, picture) = create(&NewPicture {
        width: size,
        height: size,
        paper: Some(Color::srgb8(255, 255, 255)),
        ..NewPicture::default()
    })
    .unwrap();
    made.apply(&mut arena).unwrap();
    let mut timeline = new_timeline(&arena, picture, 10, length);
    timeline.apply(&mut arena).unwrap();
    let folder = track_folder(&super::timeline(&arena, picture).unwrap().tracks[0]).unwrap();
    (arena, UndoStack::new(), picture, folder)
}

/// Paints all of `layer` in `rgba`.
fn fill(arena: &mut Arena, layer: EntityId, rgba: [u8; 4]) {
    let raster = crate::doc::raster_mut(arena, layer).unwrap();
    for pixel in raster.bytes_mut().unwrap().as_chunks_mut::<4>().0 {
        pixel.copy_from_slice(&rgba);
    }
}

/// Paints the `[x0, y0, x1, y1]` square of `layer` in `rgba`.
fn square(arena: &mut Arena, layer: EntityId, [x0, y0, x1, y1]: [u32; 4], rgba: [u8; 4]) {
    let raster = crate::doc::raster_mut(arena, layer).unwrap();
    let width = raster.width();
    let bytes = raster.bytes_mut().unwrap();
    for y in y0..y1 {
        for x in x0..x1 {
            let at = ((y * width + x) * 4) as usize;
            bytes[at..at + 4].copy_from_slice(&rgba);
        }
    }
}

/// The color at `(x, y)` of `picture` as frame `frame` shows it.
fn seen(arena: &Arena, picture: EntityId, frame: u32, [x, y]: [u32; 2]) -> [u8; 3] {
    let view = frame_view(arena, picture, frame, None, None).unwrap();
    let options = RenderOptions {
        paper: true,
        drafts: false,
    };
    let flat = flatten_view(arena, picture, options, Some(&view)).unwrap();
    let at = ((y * flat.width() + x) * 4) as usize;
    let bytes = flat.bytes().unwrap();
    [bytes[at], bytes[at + 1], bytes[at + 2]]
}

#[test]
fn a_folder_cel_shows_on_its_frames_only_and_as_a_whole() {
    let (mut arena, mut history, picture, folder) = animated(8, 6);
    // A plain cel on frame 0, a folder cel of two layers on frame 3.
    let (edit, plain) = new_cel(&arena, picture, folder, 0).unwrap();
    history.apply(&mut arena, edit).unwrap();
    fill(&mut arena, plain, [255, 0, 0, 255]);
    let (made, cel) = crate::doc::new_folder(&arena, picture, Some(plain), false);
    history.apply(&mut arena, made).unwrap();
    assert_eq!(
        cel_folder(&arena, cel),
        Some(folder),
        "a folder in an animation folder is a cel"
    );
    let (spawn, line) =
        crate::doc::add_raster_layer(&arena, picture, Some(cel), "Line".into(), None).unwrap();
    history
        .apply(&mut arena, Edit::new("Line").then(spawn))
        .unwrap();
    let (spawn, color) =
        crate::doc::add_raster_layer(&arena, picture, Some(cel), "Color".into(), None).unwrap();
    history
        .apply(&mut arena, Edit::new("Color").then(spawn))
        .unwrap();
    assert_eq!(crate::doc::folder_of(&arena, line), Some(cel));
    square(&mut arena, line, [0, 0, 4, 8], [0, 0, 255, 255]);
    square(&mut arena, color, [4, 0, 8, 8], [0, 255, 0, 255]);
    assert_eq!(
        cels(&arena, picture, folder),
        vec![plain, cel],
        "listed with the cels"
    );
    let sequence = super::timeline(&arena, picture).unwrap().clone();
    let shown = assign(&sequence, 0, 3, Some(cel), picture, 6);
    history
        .apply(&mut arena, set(picture, shown, "Specify cels"))
        .unwrap();
    for frame in 0..3 {
        assert_eq!(
            seen(&arena, picture, frame, [1, 1]),
            [255, 0, 0],
            "frame {frame}: the plain cel"
        );
        assert_eq!(seen(&arena, picture, frame, [6, 1]), [255, 0, 0]);
    }
    for frame in 3..6 {
        assert_eq!(
            seen(&arena, picture, frame, [1, 1]),
            [0, 0, 255],
            "frame {frame}: the folder cel"
        );
        assert_eq!(seen(&arena, picture, frame, [6, 1]), [0, 255, 0]);
    }
    // Onion skin shows the folder cel faintly as a whole on frame 0.
    let onion = Onion {
        before: 0,
        after: 1,
        mode: 0,
        opacity: Some((0.5, 0.0)),
        ..Onion::default()
    };
    let view = frame_view(&arena, picture, 0, Some(&onion), None).unwrap();
    for layer in [line, color] {
        assert!(view.hidden.contains(&layer) && view.onion.iter().any(|skin| skin.cel == layer));
    }
}

#[test]
fn onion_skin_shows_a_folder_inside_a_folder_cel() {
    let (mut arena, mut history, picture, folder) = animated(8, 6);
    // An empty cel on frame 0; on frame 3 a folder cel holding a Normal
    // folder that holds a blue layer.
    let (edit, plain) = new_cel(&arena, picture, folder, 0).unwrap();
    history.apply(&mut arena, edit).unwrap();
    let (made, cel) = crate::doc::new_folder(&arena, picture, Some(plain), false);
    history.apply(&mut arena, made).unwrap();
    let (spawn, inside) =
        crate::doc::add_raster_layer(&arena, picture, Some(cel), "Inner".into(), None).unwrap();
    history
        .apply(&mut arena, Edit::new("Inner").then(spawn))
        .unwrap();
    let (made, inner) = crate::doc::new_folder(&arena, picture, Some(inside), true);
    history.apply(&mut arena, made).unwrap();
    history
        .apply(
            &mut arena,
            Edit::new("Normal").then(sue_core::txn::SetProperty::new(
                inner,
                sue_model::picture::BLEND,
                "normal",
            )),
        )
        .unwrap();
    assert_eq!(crate::doc::folder_of(&arena, inside), Some(inner));
    assert_eq!(crate::doc::folder_of(&arena, inner), Some(cel));
    fill(&mut arena, inside, [0, 0, 255, 255]);
    let sequence = super::timeline(&arena, picture).unwrap().clone();
    let shown = assign(&sequence, 0, 3, Some(cel), picture, 6);
    history
        .apply(&mut arena, set(picture, shown, "Specify cels"))
        .unwrap();
    assert_eq!(seen(&arena, picture, 0, [1, 1]), [255, 255, 255]);
    assert_eq!(seen(&arena, picture, 3, [1, 1]), [0, 0, 255]);
    let onion = Onion {
        before: 0,
        after: 3,
        mode: 0,
        opacity: Some((0.5, 0.0)),
        ..Onion::default()
    };
    // The frame view, and the cels' own view as the canvas takes it.
    let sequence = super::timeline(&arena, picture).unwrap().clone();
    for view in [
        frame_view(&arena, picture, 0, Some(&onion), None).unwrap(),
        view_in(&arena, picture, &sequence, 0, Some(&onion)),
    ] {
        assert!(!view.hidden.contains(&inner), "the inner folder stays");
        let flat = flatten_view(
            &arena,
            picture,
            RenderOptions {
                paper: true,
                drafts: false,
            },
            Some(&view),
        )
        .unwrap();
        let pixel = &flat.bytes().unwrap()[(8 + 1) * 4..(8 + 1) * 4 + 3];
        assert_ne!(
            pixel,
            [255, 255, 255],
            "the folder cel's inner folder shows faintly"
        );
    }
}

#[test]
fn a_new_cel_after_a_folder_cel_copies_its_folders_and_layers_empty() {
    let (mut arena, mut history, picture, folder) = animated(8, 6);
    let (edit, first) = new_cel(&arena, picture, folder, 0).unwrap();
    history.apply(&mut arena, edit).unwrap();
    // Make the first cel a folder cel: a folder holding "Line" and an
    // inner folder holding "Shade".
    let (made, cel) = crate::doc::new_folder(&arena, picture, Some(first), true);
    history.apply(&mut arena, made).unwrap();
    history
        .apply(
            &mut arena,
            Edit::new("Name").then(sue_core::txn::Rename::new(first, "Line")),
        )
        .unwrap();
    let (made, inner) = crate::doc::new_folder(&arena, picture, Some(first), false);
    history.apply(&mut arena, made).unwrap();
    let (spawn, shade) =
        crate::doc::add_raster_layer(&arena, picture, Some(inner), "Shade".into(), None).unwrap();
    history
        .apply(&mut arena, Edit::new("Shade").then(spawn))
        .unwrap();
    fill(&mut arena, first, [9, 9, 9, 255]);
    let sequence = super::timeline(&arena, picture).unwrap().clone();
    let keyed = assign(&sequence, 0, 0, Some(cel), picture, 6);
    history
        .apply(&mut arena, set(picture, keyed, "Specify cels"))
        .unwrap();
    assert_eq!(crate::doc::folder_of(&arena, shade), Some(inner));

    let (edit, copy) = new_cel(&arena, picture, folder, 2).unwrap();
    undo_pair(&mut arena, &mut history, edit);
    assert!(crate::doc::is_folder(&arena, copy), "a folder cel too");
    assert_eq!(cel_folder(&arena, copy), Some(folder));
    assert_eq!(arena.get_by_id(copy).unwrap().name(), "2");
    let names: Vec<(String, bool)> = crate::doc::members(&arena, picture, copy)
        .into_iter()
        .map(|id| {
            let empty = crate::doc::raster(&arena, id)
                .is_none_or(|raster| raster.bytes().unwrap().iter().all(|byte| *byte == 0));
            (arena.get_by_id(id).unwrap().name().to_owned(), empty)
        })
        .collect();
    let mut original: Vec<String> = crate::doc::members(&arena, picture, cel)
        .into_iter()
        .map(|id| arena.get_by_id(id).unwrap().name().to_owned())
        .collect();
    let mut copied: Vec<String> = names.iter().map(|(name, _)| name.clone()).collect();
    original.sort();
    copied.sort();
    assert_eq!(copied, original, "the same structure");
    assert!(names.iter().all(|(_, empty)| *empty), "nothing drawn");
    let sequence = super::timeline(&arena, picture).unwrap();
    assert_eq!(cel_at(&sequence.tracks[0], sequence.rate, 2), Some(copy));
}

#[test]
fn a_cel_made_between_two_is_named_after_the_one_before_and_sits_above_it() {
    let (mut arena, mut history, picture, folder) = animated(4, 8);
    let (edit, one) = new_cel(&arena, picture, folder, 0).unwrap();
    history.apply(&mut arena, edit).unwrap();
    let (edit, two) = new_cel(&arena, picture, folder, 4).unwrap();
    history.apply(&mut arena, edit).unwrap();
    // At frame 2, between 1 and 2.
    let (edit, between) = new_cel(&arena, picture, folder, 2).unwrap();
    undo_pair(&mut arena, &mut history, edit);
    assert_eq!(arena.get_by_id(between).unwrap().name(), "1a");
    assert_eq!(
        cels(&arena, picture, folder),
        vec![one, between, two],
        "right above 1"
    );
    // Again between 1a and 2: 1b.
    let (edit, next) = new_cel(&arena, picture, folder, 3).unwrap();
    history.apply(&mut arena, edit).unwrap();
    assert_eq!(arena.get_by_id(next).unwrap().name(), "1b");
    let taken: std::collections::HashSet<String> = ["1a".to_owned()].into();
    assert_eq!(cels::in_between_name("1", &taken), "1b");
    assert_eq!(cels::in_between_name("Walk", &taken), "Walka");

    // Rename in timeline order: 1, 1a, 1b, 2 become 1, 2, 3, 4.
    let edit = rename_in_timeline_order(&arena, picture, folder).unwrap();
    undo_pair(&mut arena, &mut history, edit);
    let name = |id: EntityId| arena.get_by_id(id).unwrap().name().to_owned();
    assert_eq!(
        [name(one), name(between), name(next), name(two)],
        ["1", "2", "3", "4"]
    );
    assert!(
        rename_in_timeline_order(&arena, picture, folder).is_none(),
        "in order already"
    );
    // In layer order (bottom up): 1, 1a, 2, 1b... the stack is one,
    // between, next, two, so the names are the same here.
    assert!(rename_in_layer_order(&arena, picture, folder).is_none());
    let first = cels::first_frame_of(super::timeline(&arena, picture).unwrap(), 0, next);
    assert_eq!(first, Some(3));
}

#[test]
fn a_tween_renders_the_expected_place_at_the_middle_frame_for_each_ease() {
    for (ease, expected) in [(Ease::Hold, 0), (Ease::Linear, 8), (Ease::Smooth, 8)] {
        let mut arena = Arena::new();
        let (mut made, picture) = create(&NewPicture {
            width: 32,
            height: 8,
            paper: Some(Color::srgb8(255, 255, 255)),
            ..NewPicture::default()
        })
        .unwrap();
        made.apply(&mut arena).unwrap();
        let mut timeline =
            setup::create_timeline(&arena, picture, 10, 9, &Settings::named("Timeline 1"));
        timeline.apply(&mut arena).unwrap();
        let layer = crate::doc::layers(&arena, picture).last().unwrap().id;
        // A black bar two pixels wide at the left.
        square(&mut arena, layer, [0, 0, 2, 8], [0, 0, 0, 255]);
        let rate = super::timeline(&arena, picture).unwrap().rate;
        let at = |frame: i64| Time::from_frames(frame, rate);
        let mut history = UndoStack::new();
        let first = motion::key(&arena, layer, at(0), &[(motion::POSITION_X, 0.0)], ease);
        undo_pair(&mut arena, &mut history, first);
        let second = motion::key(&arena, layer, at(8), &[(motion::POSITION_X, 16.0)], ease);
        undo_pair(&mut arena, &mut history, second);
        // At frame 4 the bar is at `expected`.
        assert_eq!(
            seen(&arena, picture, 4, [expected, 4]),
            [0, 0, 0],
            "{ease:?}"
        );
        assert_eq!(
            seen(&arena, picture, 4, [expected + 1, 4]),
            [0, 0, 0],
            "{ease:?}"
        );
        let gone = if expected == 0 { 8 } else { 0 };
        assert_eq!(
            seen(&arena, picture, 4, [gone, 4]),
            [255, 255, 255],
            "{ease:?}"
        );
        // The ends are where the keys put them, whatever the ease.
        assert_eq!(seen(&arena, picture, 8, [16, 4]), [0, 0, 0], "{ease:?}");
    }
}

#[test]
fn keyed_opacity_fades_a_layer_through_its_folder() {
    let mut arena = Arena::new();
    let (mut made, picture) = create(&NewPicture {
        width: 4,
        height: 4,
        paper: Some(Color::srgb8(255, 255, 255)),
        ..NewPicture::default()
    })
    .unwrap();
    made.apply(&mut arena).unwrap();
    let mut timeline =
        setup::create_timeline(&arena, picture, 10, 11, &Settings::named("Timeline 1"));
    timeline.apply(&mut arena).unwrap();
    let layer = crate::doc::layers(&arena, picture).last().unwrap().id;
    fill(&mut arena, layer, [0, 0, 0, 255]);
    let (mut folder_made, folder) = crate::doc::new_folder(&arena, picture, Some(layer), true);
    folder_made.apply(&mut arena).unwrap();
    let rate = super::timeline(&arena, picture).unwrap().rate;
    let at = |frame: i64| Time::from_frames(frame, rate);
    for (frame, opacity) in [(0, 1.0), (10, 0.0)] {
        let mut edit = motion::key(
            &arena,
            folder,
            at(frame),
            &[(motion::OPACITY, opacity)],
            Ease::Linear,
        );
        edit.apply(&mut arena).unwrap();
    }
    let gray = seen(&arena, picture, 5, [1, 1])[0];
    assert!((126..=129).contains(&gray), "half way: {gray}");
    assert_eq!(seen(&arena, picture, 0, [1, 1]), [0, 0, 0]);
}

#[test]
fn layers_hide_outside_their_clips_and_clips_trim_split_and_merge() {
    let mut arena = Arena::new();
    let (mut made, picture) = create(&NewPicture {
        width: 4,
        height: 4,
        paper: Some(Color::srgb8(255, 255, 255)),
        ..NewPicture::default()
    })
    .unwrap();
    made.apply(&mut arena).unwrap();
    let layer = crate::doc::layers(&arena, picture).last().unwrap().id;
    fill(&mut arena, layer, [0, 0, 0, 255]);
    let mut history = UndoStack::new();
    let edit = setup::create_timeline(&arena, picture, 12, 8, &Settings::named("Timeline 1"));
    undo_pair(&mut arena, &mut history, edit);
    let sequence = super::timeline(&arena, picture).unwrap().clone();
    assert!(
        folders(&sequence).is_empty(),
        "New timeline makes no animation folder"
    );
    assert_eq!(
        setup::layer_tracks(&sequence).len(),
        2,
        "the layer and the paper"
    );
    let track = setup::track_of_layer(&sequence, layer).unwrap();
    // The clip from frame 2 on: hidden before.
    let trimmed = editing::set_first_displayed(&sequence, track, 2, 8, picture).unwrap();
    undo_pair(
        &mut arena,
        &mut history,
        set(picture, trimmed, "First displayed frame"),
    );
    assert_eq!(seen(&arena, picture, 1, [0, 0]), [255, 255, 255]);
    assert_eq!(seen(&arena, picture, 2, [0, 0]), [0, 0, 0]);
    let sequence = super::timeline(&arena, picture).unwrap().clone();
    let ended = editing::set_last_displayed(&sequence, track, 5, 8, picture).unwrap();
    assert_eq!(setup::off_clip(&ended, 6), vec![layer]);
    let split = editing::split_clip(&ended, track, 4, 8).unwrap();
    assert_eq!(split.tracks[track].clips().len(), 2);
    assert_eq!(setup::off_clip(&split, 4), Vec::<EntityId>::new());
    let merged = editing::merge_clips(&split, track, 2, 6, 8, picture).unwrap();
    assert_eq!(merged.tracks[track].clips(), ended.tracks[track].clips());
    // A second timeline, then back to the first; undo each.
    let mut settings = Settings::named("Timeline 2");
    settings.scene = 3;
    settings.shot = 7;
    let edit = setup::create_timeline(&arena, picture, 24, 4, &settings);
    undo_pair(&mut arena, &mut history, edit);
    let names: Vec<String> = setup::timelines(&arena, picture)
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    assert_eq!(names, ["Timeline 1", "Timeline 2"]);
    assert_eq!(Settings::of(&arena, picture).scene, 3);
    assert_eq!(length(&arena, picture), 4);
    let aside = setup::timelines(&arena, picture)[0].aside.unwrap();
    let edit = setup::switch_timeline(&arena, picture, aside).unwrap();
    undo_pair(&mut arena, &mut history, edit);
    assert_eq!(length(&arena, picture), 8);
    assert_eq!(Settings::of(&arena, picture).name, "Timeline 1");
    let names: Vec<String> = setup::timelines(&arena, picture)
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    assert_eq!(
        names,
        ["Timeline 1", "Timeline 2"],
        "the picker keeps its order"
    );
    assert_eq!(
        seen(&arena, picture, 1, [0, 0]),
        [255, 255, 255],
        "its clip came back"
    );
}

#[test]
fn timeline_settings_labels_playback_and_output_frame_keep_and_undo() {
    let (mut arena, mut history, picture, _) = animated(4, 24);
    let mut settings = Settings::of(&arena, picture);
    assert_eq!(settings.range(24), (0, 23));
    settings.name = "Walk".into();
    settings.division = 8;
    settings.start = 2;
    settings.end = Some(20);
    undo_pair(&mut arena, &mut history, settings.apply(picture));
    assert_eq!(Settings::of(&arena, picture), settings);
    assert_eq!(
        Settings::of(&arena, picture).range(12),
        (2, 11),
        "kept inside the timeline"
    );
    // Labels.
    let edit = setup::set_label(&arena, picture, 6, Some("Jump")).unwrap();
    undo_pair(&mut arena, &mut history, edit);
    let edit = setup::set_label(&arena, picture, 1, Some("Start: run")).unwrap();
    history.apply(&mut arena, edit).unwrap();
    let names: Vec<(u32, String)> = setup::labels(&arena, picture)
        .into_iter()
        .map(|label| (label.frame, label.name))
        .collect();
    assert_eq!(
        names,
        [(1, "Start: run".to_owned()), (6, "Jump".to_owned())]
    );
    let edit = setup::set_label(&arena, picture, 6, None).unwrap();
    undo_pair(&mut arena, &mut history, edit);
    assert!(
        setup::set_label(&arena, picture, 6, None).is_none(),
        "no label there"
    );
    // Playback settings.
    assert_eq!(Playback::of(&arena, picture), Playback::default());
    let playback = Playback {
        real_time: false,
        render_first: true,
        prefer_quality: true,
        render_camera: false,
    };
    undo_pair(&mut arena, &mut history, playback.apply(picture));
    assert_eq!(Playback::of(&arena, picture), playback);
    assert_eq!(
        Playback::from_text("camera=0,later=1"),
        Playback {
            render_camera: false,
            ..Playback::default()
        }
    );
    // The output frame.
    let frame = OutputFrame {
        size: [1920, 1080],
        blank: [10, 20, 30, 40],
        title_safe: true,
        overflow: None,
    };
    assert_eq!(frame.canvas_size(), [1990, 1110]);
    assert_eq!(frame.rect(), [30, 10, 1920, 1080]);
    let wide = OutputFrame {
        overflow: Some(Overflow {
            size: [2334, 1658],
            anchor: 4,
            offset: [0, 8],
        }),
        ..frame
    };
    assert_eq!(wide.canvas_size(), [2334, 1658]);
    assert_eq!(wide.rect(), [207, 297, 1920, 1080]);
    for frame in [frame, wide, OutputFrame::filling([640, 480])] {
        assert_eq!(OutputFrame::from_text(&frame.to_text()), Some(frame));
    }
    undo_pair(&mut arena, &mut history, wide.apply(picture));
    assert_eq!(OutputFrame::of(&arena, picture), Some(wide));
    // All of it saves with the picture.
    let mut file = sue_core::SueFile::new();
    file.set_arena(&arena);
    let loaded = sue_core::SueFile::from_bytes(&file.to_bytes())
        .unwrap()
        .arena(&sue_model::registry())
        .unwrap();
    assert_eq!(Settings::of(&loaded, picture), settings);
    assert_eq!(Playback::of(&loaded, picture), playback);
    assert_eq!(OutputFrame::of(&loaded, picture), Some(wide));
    assert_eq!(
        setup::labels(&loaded, picture),
        setup::labels(&arena, picture)
    );
}

#[test]
fn timeline_edits_undo_exactly() {
    let (mut arena, mut history, picture, folder) = animated(4, 8);
    let mut made = Vec::new();
    for frame in [0, 2, 4] {
        let (edit, cel) = new_cel(&arena, picture, folder, frame).unwrap();
        history.apply(&mut arena, edit).unwrap();
        made.push(cel);
    }
    let sequence = super::timeline(&arena, picture).unwrap().clone();
    let picked = Selection::dragged(vec![0], 2, 3);
    // Cut, paste, move: each a timeline the picture is set to.
    let (cut, clipboard) = editing::cut(&sequence, &picked, 8, picture);
    undo_pair(&mut arena, &mut history, set(picture, cut, "Cut"));
    let sequence = super::timeline(&arena, picture).unwrap().clone();
    let (pasted, _) = editing::paste(&sequence, &clipboard, &[0], 6, 8, picture, |track, cel| {
        may_show(&arena, &sequence, track, cel)
    });
    undo_pair(&mut arena, &mut history, set(picture, pasted, "Paste"));
    let sequence = super::timeline(&arena, picture).unwrap();
    assert_eq!(
        cel_at(&sequence.tracks[0], sequence.rate, 6),
        Some(made[1]),
        "the same cel again"
    );
    assert_eq!(cels(&arena, picture, folder).len(), 3, "no new layer");
    // Insert two frames on the folder's track only: the timeline keeps
    // its length; then on every track, with a layer's keyframes.
    let layer = crate::doc::layers(&arena, picture)[1].id;
    let rate = super::timeline(&arena, picture).unwrap().rate;
    let at = |frame: i64| Time::from_frames(frame, rate);
    let key = motion::key(
        &arena,
        layer,
        at(5),
        &[(motion::ROTATION, 10.0)],
        Ease::Linear,
    );
    history.apply(&mut arena, key).unwrap();
    let edit = editing::insert_frames(&arena, picture, 1, 2, Some(&[0])).unwrap();
    undo_pair(&mut arena, &mut history, edit);
    assert_eq!(length(&arena, picture), 8);
    assert_eq!(
        motion::key_times(&arena, layer),
        vec![at(5)],
        "not on a picked track"
    );
    let edit = editing::insert_frames(&arena, picture, 1, 2, None).unwrap();
    undo_pair(&mut arena, &mut history, edit);
    assert_eq!(length(&arena, picture), 10);
    assert_eq!(motion::key_times(&arena, layer), vec![at(7)]);
    let edit = editing::insert_frames(&arena, picture, 1, -2, None).unwrap();
    undo_pair(&mut arena, &mut history, edit);
    assert_eq!(length(&arena, picture), 8);
    // A track label and a volume, each a timeline set.
    let sequence = super::timeline(&arena, picture).unwrap().clone();
    let labelled = editing::with_track_label(&sequence, 0, 3, Some(editing::TrackLabel::InBetween));
    undo_pair(
        &mut arena,
        &mut history,
        set(picture, labelled, "Track label"),
    );
}

#[test]
fn audio_clips_play_at_their_volume() {
    let (mut arena, mut history, picture, _) = animated(4, 24);
    let sequence = super::timeline(&arena, picture).unwrap().clone();
    let tone = sue_core::PCMFloat1D::new(MIX_RATE, 1, vec![0.4; MIX_RATE as usize]).unwrap();
    let (edit, _) = add_audio(picture, &sequence, "Tone", tone, 0);
    history.apply(&mut arena, edit).unwrap();
    let sequence = super::timeline(&arena, picture).unwrap().clone();
    let track = audio_tracks(&sequence)[0];
    let quiet = with_volume(&sequence, track, 0.5);
    undo_pair(
        &mut arena,
        &mut history,
        set(picture, quiet.clone(), "Volume"),
    );
    let mixed = mix(&arena, &quiet).unwrap();
    assert!((mixed.samples()[1000] - 0.2).abs() < 1e-4, "half as loud");
    assert_eq!(
        with_volume(&quiet, track, 1.0),
        sequence,
        "1 leaves no property"
    );
    // Scrubbing at frame 2 hears one frame of it.
    let heard = scrub(&arena, &quiet, 2).unwrap();
    assert_eq!(heard.frames(), (MIX_RATE / 10) as usize);
    let from = (MIX_RATE / 5) as usize * 2;
    assert_eq!(
        heard.samples(),
        &mixed.samples()[from..from + heard.samples().len()],
        "the same samples the whole mix has there"
    );
    assert!(scrub(&arena, &quiet, 20).is_none(), "the tone is over");
}

#[test]
fn old_onion_settings_load_unchanged() {
    // As 0.0.75 wrote them (fixture (f) holds these), and the defaults.
    let old = "2,3,1,ff0080,0080ff,1,0.6,0.2";
    let onion = Onion::from_text(old).unwrap();
    assert_eq!(
        onion,
        Onion {
            before: 2,
            after: 3,
            mode: 1,
            before_color: [255, 0, 128],
            after_color: [0, 128, 255],
            opacity: Some((0.6, 0.2)),
        }
    );
    assert_eq!(onion.to_text(), old, "written back the same");
    assert_eq!(Onion::MODES, ["Color", "Half color", "Monochrome"]);
    let default = Onion::default();
    assert_eq!((default.before, default.after, default.mode), (1, 1, 2));
    assert_eq!(
        (default.before_color, default.after_color),
        ([0, 0, 255], [0, 255, 0])
    );
}

#[test]
fn every_layer_gets_a_track_once_and_new_folders_are_named_folder_n() {
    let (mut arena, mut history, picture, folder) = animated(4, 6);
    assert_eq!(arena.get_by_id(folder).unwrap().name(), "Folder 1");
    let (edit, second, _) = add_folder(&arena, picture, None);
    history.apply(&mut arena, edit).unwrap();
    assert_eq!(arena.get_by_id(second).unwrap().name(), "Folder 2");
    let edit = setup::give_every_layer_a_track(&arena, picture).unwrap();
    undo_pair(&mut arena, &mut history, edit);
    assert!(
        setup::give_every_layer_a_track(&arena, picture).is_none(),
        "once is enough"
    );
    let sequence = super::timeline(&arena, picture).unwrap();
    for info in crate::doc::layers(&arena, picture) {
        let has = setup::track_of_layer(sequence, info.id).is_some()
            || track_of(sequence, info.id).is_some();
        assert!(has, "{} has a track", info.name);
    }
    let rate = Rate::new(10, 1);
    assert_eq!(sequence.rate, rate);
}

#[test]
fn the_animation_of_0_0_75_plays_the_same_cels_and_camera_path_with_a_camera_track() {
    // Fixture (f) of P0-4: an animation folder of three cels, a blank, a
    // camera folder keyed twice, written by Sue 0.0.75.
    let bytes = include_bytes!("../../../sue-paint/tests/golden/animation.sue");
    let mut arena = sue_core::SueFile::from_bytes(bytes)
        .unwrap()
        .arena(&sue_model::registry())
        .unwrap();
    let picture = arena
        .iter()
        .map(|(_, entity)| entity)
        .find(|entity| sue_model::picture::is_picture(entity))
        .unwrap()
        .id();
    let size = arena
        .get_by_id(picture)
        .ok()
        .and_then(sue_model::picture::size)
        .unwrap();
    let length = length(&arena, picture);
    let camera = crate::doc::layers(&arena, picture)
        .into_iter()
        .find(|info| crate::camera::is_camera(&arena, info.id))
        .unwrap()
        .id;
    let played = |arena: &Arena| {
        let sequence = super::timeline(arena, picture).unwrap();
        (0..length)
            .map(|frame| {
                let view = frame_view(arena, picture, frame, None, None).unwrap();
                let mut hidden: Vec<EntityId> = view.hidden.into_iter().collect();
                hidden.sort();
                let time = crate::camera::time_of(frame, sequence.rate);
                (
                    hidden,
                    view.cameras,
                    crate::camera::views(arena, picture, Some(time), size),
                )
            })
            .collect::<Vec<_>>()
    };
    // What 0.0.75 played, written down (not read back from this tree):
    // cels 1, 2, 3 on frames 0, 3, 6, cel 1 again on 8, nothing from 10;
    // the camera keyed at frame 0 and frame 11, a straight line between.
    let folder = crate::doc::layers(&arena, picture)
        .into_iter()
        .find(|info| is_animation_folder(&arena, info.id))
        .unwrap()
        .id;
    assert_eq!(crate::doc::folder_of(&arena, folder), Some(camera));
    let names: Vec<String> = (0..length)
        .map(|frame| {
            let view = frame_view(&arena, picture, frame, None, None).unwrap();
            cels(&arena, picture, folder)
                .into_iter()
                .filter(|cel| !view.hidden.contains(cel))
                .map(|cel| arena.get_by_id(cel).unwrap().name().to_owned())
                .collect::<Vec<_>>()
                .join(",")
        })
        .collect();
    assert_eq!(
        names,
        ["1", "1", "1", "2", "2", "2", "3", "3", "1", "1", "", ""]
    );
    let rate = super::timeline(&arena, picture).unwrap().rate;
    let stands = |frame: u32| {
        crate::camera::Camera::of(
            &arena,
            camera,
            Some(crate::camera::time_of(frame, rate)),
            size,
        )
    };
    let close = |a: f32, b: f32| (a - b).abs() < 1e-3;
    for (frame, center, scale, angle) in [
        (0, [16.0, 16.0], 1.0, 0.0),
        (11, [20.0, 12.0], 1.5, 0.25),
        (
            4,
            [16.0 + 16.0 / 11.0, 16.0 - 16.0 / 11.0],
            1.0 + 2.0 / 11.0,
            1.0 / 11.0,
        ),
    ] {
        let camera = stands(frame);
        assert!(
            close(camera.center[0], center[0])
                && close(camera.center[1], center[1])
                && close(camera.scale, scale)
                && close(camera.angle, angle),
            "frame {frame}: {camera:?}"
        );
    }
    let before = played(&arena);
    for (frame, (hidden, cameras, path)) in before.iter().enumerate() {
        let sequence = super::timeline(&arena, picture).unwrap();
        let mut old: Vec<EntityId> = view_in(&arena, picture, sequence, frame as u32, None)
            .hidden
            .into_iter()
            .collect();
        old.sort();
        assert_eq!(hidden, &old, "frame {frame}: the same cels");
        assert_eq!(cameras, path, "frame {frame}: the same camera path");
    }
    // The camera becomes a track; nothing plays differently.
    let mut tracks = setup::give_every_layer_a_track(&arena, picture).unwrap();
    tracks.apply(&mut arena).unwrap();
    let sequence = super::timeline(&arena, picture).unwrap();
    let track = setup::track_of_layer(sequence, camera).expect("the camera has a track");
    assert_eq!(sequence.tracks[track].kind, setup::CAMERA);
    assert_eq!(played(&arena), before);
}

/// A sheet of animation frames for a person to look at: a row each, a
/// frame each column, every pixel shown as a 3 by 3 block.
#[test]
#[ignore = "writes a picture for a person to look at"]
fn animation_sheet() {
    const SIDE: u32 = 40;
    const FRAMES: u32 = 9;
    const ZOOM: u32 = 3;
    let fresh = || {
        let mut arena = Arena::new();
        let (mut made, picture) = create(&NewPicture {
            width: SIDE,
            height: SIDE,
            paper: Some(Color::srgb8(255, 255, 255)),
            ..NewPicture::default()
        })
        .unwrap();
        made.apply(&mut arena).unwrap();
        let mut timeline =
            setup::create_timeline(&arena, picture, 8, FRAMES, &Settings::named("Timeline 1"));
        timeline.apply(&mut arena).unwrap();
        let layer = crate::doc::layers(&arena, picture).last().unwrap().id;
        (arena, picture, layer)
    };
    let mut rows: Vec<Vec<Vec<u8>>> = Vec::new();
    let frames = |arena: &Arena, picture: EntityId, onion: Option<&Onion>| {
        (0..FRAMES)
            .map(|frame| {
                let view = frame_view(arena, picture, frame, onion, None).unwrap();
                let options = RenderOptions {
                    paper: true,
                    drafts: false,
                };
                flatten_view(arena, picture, options, Some(&view))
                    .unwrap()
                    .bytes()
                    .unwrap()
                    .to_vec()
            })
            .collect::<Vec<_>>()
    };
    // Rows 1 to 3: a square moved across and turned by two keys, Hold,
    // Linear and Smooth.
    for ease in [Ease::Hold, Ease::Linear, Ease::Smooth] {
        let (mut arena, picture, layer) = fresh();
        square(&mut arena, layer, [4, 14, 16, 26], [20, 20, 20, 255]);
        let rate = super::timeline(&arena, picture).unwrap().rate;
        for (frame, x, turn) in [(0, 0.0, 0.0), (8, 20.0, 90.0)] {
            let mut key = motion::key(
                &arena,
                layer,
                Time::from_frames(frame, rate),
                &[
                    (motion::POSITION_X, x),
                    (motion::ROTATION, turn),
                    (motion::CENTER_X, 10.0),
                    (motion::CENTER_Y, 20.0),
                ],
                ease,
            );
            key.apply(&mut arena).unwrap();
        }
        rows.push(frames(&arena, picture, None));
    }
    // Row 4: an animation folder; a red cel on frames 0 to 2, a folder cel
    // (blue left half, green right half) from frame 3, a blank from 6.
    // Row 5: the same with onion skin (one before, one after).
    {
        let mut arena = Arena::new();
        let (mut made, picture) = create(&NewPicture {
            width: SIDE,
            height: SIDE,
            paper: Some(Color::srgb8(255, 255, 255)),
            ..NewPicture::default()
        })
        .unwrap();
        made.apply(&mut arena).unwrap();
        let mut timeline = new_timeline(&arena, picture, 8, FRAMES);
        timeline.apply(&mut arena).unwrap();
        let folder = track_folder(&super::timeline(&arena, picture).unwrap().tracks[0]).unwrap();
        let (mut edit, plain) = new_cel(&arena, picture, folder, 0).unwrap();
        edit.apply(&mut arena).unwrap();
        square(&mut arena, plain, [8, 8, 32, 32], [220, 40, 40, 255]);
        let (mut made, cel) = crate::doc::new_folder(&arena, picture, Some(plain), false);
        made.apply(&mut arena).unwrap();
        for (name, rect, rgba) in [
            ("Line", [6, 12, 20, 28], [40, 60, 220, 255]),
            ("Color", [20, 12, 34, 28], [40, 180, 60, 255]),
        ] {
            let (spawn, layer) =
                crate::doc::add_raster_layer(&arena, picture, Some(cel), name.into(), None)
                    .unwrap();
            let mut spawn = Edit::new(name).then(spawn);
            spawn.apply(&mut arena).unwrap();
            square(&mut arena, layer, rect, rgba);
        }
        let sequence = super::timeline(&arena, picture).unwrap().clone();
        let sequence = assign(&sequence, 0, 3, Some(cel), picture, FRAMES);
        let sequence = assign(&sequence, 0, 6, None, picture, FRAMES);
        let mut keyed = set(picture, sequence, "Specify cels");
        keyed.apply(&mut arena).unwrap();
        rows.push(frames(&arena, picture, None));
        let onion = Onion {
            mode: 0,
            opacity: Some((0.4, 0.1)),
            ..Onion::default()
        };
        rows.push(frames(&arena, picture, Some(&onion)));
    }
    let gap = 6;
    let cell = SIDE * ZOOM;
    let width = FRAMES * (cell + gap) + gap;
    let height = rows.len() as u32 * (cell + gap) + gap;
    let mut sheet = vec![200_u8; (width * height * 4) as usize];
    for (row, frames) in rows.iter().enumerate() {
        for (column, pixels) in frames.iter().enumerate() {
            let (left, top) = (
                gap + column as u32 * (cell + gap),
                gap + row as u32 * (cell + gap),
            );
            for y in 0..cell {
                for x in 0..cell {
                    let from = (((y / ZOOM) * SIDE + x / ZOOM) * 4) as usize;
                    let to = (((top + y) * width + left + x) * 4) as usize;
                    sheet[to..to + 4].copy_from_slice(&pixels[from..from + 4]);
                }
            }
        }
    }
    let png = sue_io::encode_png_rgba(width, height, &sheet).unwrap();
    let folder = std::env::var("SUE_SHOTS").expect("SUE_SHOTS names the folder to write in");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(std::path::Path::new(&folder).join("animation.png"), png).unwrap();
}

#[test]
fn a_light_table_entry_keeps_its_own_eye_opacity_and_mode_on_its_link() {
    let (mut arena, mut history, picture, folder) = animated(8, 6);
    let (edit, cel) = new_cel(&arena, picture, folder, 0).unwrap();
    history.apply(&mut arena, edit).unwrap();
    let (edit, other) = new_cel(&arena, picture, folder, 3).unwrap();
    history.apply(&mut arena, edit).unwrap();
    history.apply(&mut arena, register(cel, other)).unwrap();
    history
        .apply(&mut arena, register_general(picture, other))
        .unwrap();
    // A new entry shows whole, in the table's mode.
    assert_eq!(
        entry(&arena, cel, LIGHT_TABLE, other),
        LightEntry::default()
    );
    let order = |arena: &Arena| {
        arena
            .relations()
            .get(cel, LIGHT_TABLE, other)
            .map(|link| link.order)
    };
    let placed = order(&arena);
    let changed = LightEntry {
        shown: false,
        opacity: 0.25,
        mode: EntryMode::Tinted,
    };
    let edit = set_entry(&arena, cel, LIGHT_TABLE, other, changed).unwrap();
    history.apply(&mut arena, edit).unwrap();
    assert_eq!(entry(&arena, cel, LIGHT_TABLE, other), changed);
    assert_eq!(order(&arena), placed, "it keeps its place on the table");
    // The general table's entry of the same layer is its own.
    assert_eq!(
        entry(&arena, picture, GENERAL_LIGHT_TABLE, other),
        LightEntry::default()
    );
    // Hidden, it is left out of what the table shows.
    assert!(traced_on(&arena, cel, LIGHT_TABLE, [8, 8]).is_empty());
    assert_eq!(
        traced_on(&arena, picture, GENERAL_LIGHT_TABLE, [8, 8]).len(),
        1
    );
    // Undo and redo are exact.
    history.undo(&mut arena).unwrap();
    assert_eq!(
        entry(&arena, cel, LIGHT_TABLE, other),
        LightEntry::default()
    );
    assert_eq!(order(&arena), placed);
    history.redo(&mut arena).unwrap();
    assert_eq!(entry(&arena, cel, LIGHT_TABLE, other), changed);
    // The same settings again change nothing; a layer not on the table
    // has none to change.
    assert!(set_entry(&arena, cel, LIGHT_TABLE, other, changed).is_none());
    assert!(set_entry(&arena, other, LIGHT_TABLE, cel, changed).is_none());
    // Shown at a quarter and tinted, the skin says so.
    let shown = LightEntry {
        shown: true,
        ..changed
    };
    let edit = set_entry(&arena, cel, LIGHT_TABLE, other, shown).unwrap();
    history.apply(&mut arena, edit).unwrap();
    let traced = traced_on(&arena, cel, LIGHT_TABLE, [8, 8]);
    let view = FrameView::default().with_light_table(&traced, 0.5, None);
    let skin = view.onion.iter().find(|skin| skin.cel == other).unwrap();
    assert!((skin.opacity - 0.125).abs() < 1e-6);
    assert_eq!(skin.color, LIGHT_TABLE_TINT);
    assert!((skin.tint - 1.0).abs() < 1e-6);
    // In its own colors it is never tinted, whatever the table says.
    assert_eq!(EntryMode::Own.tint(Some([1.0, 0.0, 0.0])), None);
    assert_eq!(EntryMode::Table.tint(None), None);
    for mode in EntryMode::ALL {
        assert_eq!(EntryMode::from_name(mode.name()), mode);
    }
}

#[test]
fn reversing_a_light_table_keeps_each_entrys_eye_opacity_and_mode() {
    let (mut arena, mut history, picture, folder) = animated(8, 6);
    let (edit, cel) = new_cel(&arena, picture, folder, 0).unwrap();
    history.apply(&mut arena, edit).unwrap();
    let (edit, first) = new_cel(&arena, picture, folder, 2).unwrap();
    history.apply(&mut arena, edit).unwrap();
    let (edit, second) = new_cel(&arena, picture, folder, 4).unwrap();
    history.apply(&mut arena, edit).unwrap();
    history.apply(&mut arena, register(cel, first)).unwrap();
    history.apply(&mut arena, register(cel, second)).unwrap();
    let faint = LightEntry {
        shown: false,
        opacity: 0.3,
        mode: EntryMode::Own,
    };
    let edit = set_entry(&arena, cel, LIGHT_TABLE, first, faint).unwrap();
    history.apply(&mut arena, edit).unwrap();
    let before = light_table(&arena, cel);
    let edit = reversed(&arena, cel);
    undo_pair(&mut arena, &mut history, edit);
    let mut turned = before.clone();
    turned.reverse();
    assert_eq!(light_table(&arena, cel), turned, "turned round");
    assert_eq!(
        entry(&arena, cel, LIGHT_TABLE, first),
        faint,
        "settings kept"
    );
    assert_eq!(
        entry(&arena, cel, LIGHT_TABLE, second),
        LightEntry::default()
    );
    // Twice is the table as it was.
    let edit = reversed(&arena, cel);
    history.apply(&mut arena, edit).unwrap();
    assert_eq!(light_table(&arena, cel), before);
    assert_eq!(entry(&arena, cel, LIGHT_TABLE, first), faint);
}

#[test]
fn a_cel_wrapped_in_a_folder_hands_the_folder_its_keys_and_keeps_the_track() {
    let (mut arena, mut history, picture, folder) = animated(8, 6);
    let (edit, first) = new_cel(&arena, picture, folder, 0).unwrap();
    history.apply(&mut arena, edit).unwrap();
    let (edit, second) = new_cel(&arena, picture, folder, 3).unwrap();
    history.apply(&mut arena, edit).unwrap();
    // The track hidden and locked: wrapping a cel keeps both.
    let mut sequence = super::timeline(&arena, picture).unwrap().clone();
    sequence.tracks[0].muted = true;
    sequence.tracks[0].locked = true;
    history
        .apply(&mut arena, set(picture, sequence, "Hide"))
        .unwrap();
    let (mut edit, wrapper) = crate::doc::new_folder(&arena, picture, Some(first), true);
    // Reads the arena before the folder exists, as Create Folder and
    // Insert Layer does.
    edit = edit.then(cels::wrap_cel(&arena, picture, first, wrapper).unwrap());
    undo_pair(&mut arena, &mut history, edit);
    let sequence = super::timeline(&arena, picture).unwrap();
    let track = &sequence.tracks[0];
    assert!(track.muted && track.locked, "the track as it was");
    assert_eq!(cel_at(track, sequence.rate, 0), Some(wrapper));
    assert_eq!(cel_at(track, sequence.rate, 3), Some(second));
    assert_eq!(arena.get_by_id(wrapper).unwrap().name(), "1");
    assert_eq!(crate::doc::folder_of(&arena, first), Some(wrapper));
    // A layer of no animation folder is no cel.
    assert!(cels::wrap_cel(&arena, picture, folder, wrapper).is_none());
}

#[test]
fn assign_multiple_cels_names_cels_to_come_and_create_all_supported_cels_makes_them() {
    let (mut arena, mut history, picture, folder) = animated(8, 12);
    let (edit, one) = new_cel(&arena, picture, folder, 0).unwrap();
    history.apply(&mut arena, edit).unwrap();
    // "1-3" on twos from frame 0: cel 1 is the folder's, 2 and 3 are not
    // drawn yet.
    let names = editing::cel_list("1-3", &[]);
    assert_eq!(names, ["1", "2", "3"]);
    assert_eq!(
        editing::cel_list("b, a 4-2,x", &[]),
        ["b", "a", "4", "3", "2", "x"]
    );
    // A cel whose name holds a space stays one name.
    let known = ["Walk 2".to_owned()];
    assert_eq!(
        editing::cel_list("1, Walk 2, a b", &known),
        ["1", "Walk 2", "a", "b"]
    );
    let picks: Vec<editing::CelPick> = names
        .iter()
        .map(|name| {
            if name == "1" {
                editing::CelPick::Cel(one)
            } else {
                editing::CelPick::Wanted(name.clone())
            }
        })
        .collect();
    let sequence = super::timeline(&arena, picture).unwrap().clone();
    let assigned = editing::assign_many(&sequence, 0, (0, 2, 2), &picks, picture, 12);
    let rate = assigned.rate;
    assert_eq!(
        editing::wanted_cels(&assigned.tracks[0], rate),
        vec![
            (2, "2".to_owned()),
            (4, "3".to_owned()),
            (8, "2".to_owned()),
            (10, "3".to_owned())
        ]
    );
    assert_eq!(
        cel_at(&assigned.tracks[0], rate, 7),
        Some(one),
        "twice over"
    );
    assert_eq!(cel_at(&assigned.tracks[0], rate, 3), None, "not drawn yet");
    undo_pair(
        &mut arena,
        &mut history,
        Edit::new("Assign").then(sue_core::txn::SetExtension::set(picture, assigned)),
    );
    // Create All Supported Cels: cels 2 and 3, once each, on top of the
    // folder's cels, shown on every frame that named them.
    let edit = cels::create_supported_cels(&arena, picture)
        .unwrap()
        .unwrap();
    undo_pair(&mut arena, &mut history, edit);
    let made = cels(&arena, picture, folder);
    let names: Vec<String> = made
        .iter()
        .map(|cel| arena.get_by_id(*cel).unwrap().name().to_owned())
        .collect();
    assert_eq!(made.len(), 3, "{names:?}");
    let sequence = super::timeline(&arena, picture).unwrap();
    let track = &sequence.tracks[0];
    let shown = |frame| {
        cel_at(track, rate, frame).map(|cel| arena.get_by_id(cel).unwrap().name().to_owned())
    };
    assert_eq!(
        (0..12).map(shown).collect::<Vec<_>>(),
        ["1", "1", "2", "2", "3", "3", "1", "1", "2", "2", "3", "3"]
            .map(|name| Some(name.to_owned()))
            .to_vec()
    );
    assert!(editing::wanted_cels(track, rate).is_empty());
    assert!(
        cels::create_supported_cels(&arena, picture)
            .unwrap()
            .is_none(),
        "nothing left to make"
    );
}
