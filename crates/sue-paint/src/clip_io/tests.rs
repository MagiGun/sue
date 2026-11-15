//! The `.clip` converter's promises, on generated pictures and documents
//! only: no file made by Clip Studio enters the repository.

use sue_core::{Arena, Bundle, Entity, EntityId, PixelFormat, RasterTile2D, ReversibleTransaction};
use sue_io::clip::{ClipBitmap, ClipDocument, ClipLayer, read, testkit, write};
use sue_model::{BlendMode, Foreign, class, picture};

use super::*;
use crate::composite::{self, RenderOptions};
use crate::doc::{self, NewPicture};

fn apply(arena: &mut Arena, mut edit: impl ReversibleTransaction<Arena>) {
    edit.apply(arena).expect("the edit applies");
}

/// A raster layer `width` by `height` at `at`, its pixels from `paint`.
fn raster(
    name: &str,
    width: u32,
    height: u32,
    at: [i64; 2],
    paint: impl Fn(u32, u32) -> [u8; 4],
) -> Entity {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            pixels.extend_from_slice(&paint(x, y));
        }
    }
    let tile = RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, pixels).unwrap();
    let mut entity = Entity::new(name, tile).with_class(class::LAYER);
    if at[0] != 0 {
        entity = entity.with_property(picture::X, at[0]);
    }
    if at[1] != 0 {
        entity = entity.with_property(picture::Y, at[1]);
    }
    entity
}

/// A picture with paper, moved and masked layers, a folder with its own
/// blend mode holding a clipped layer, the two glow modes, a hidden layer,
/// a draft and a reference layer.
fn a_picture(arena: &mut Arena) -> EntityId {
    let (edit, picture) = doc::create(&NewPicture {
        width: 300,
        height: 260,
        paper: Some(sue_core::Color::srgb8(250, 240, 200)),
        ..NewPicture::default()
    })
    .unwrap();
    apply(arena, edit);
    let add = |arena: &mut Arena, entity: Entity| {
        let id = entity.id();
        let spawn = doc::landed(arena, picture, None, entity);
        apply(arena, spawn);
        id
    };
    let back = add(
        arena,
        raster("Back", 300, 260, [0, 0], |x, y| {
            [(x % 256) as u8, (y % 256) as u8, 90, 255]
        }),
    );
    apply(arena, doc::set(back, picture::OPACITY, 0.75));
    let moved = add(
        arena,
        raster("Moved", 120, 90, [-20, 40], |x, y| {
            [
                200,
                (x * 2) as u8,
                (y * 2) as u8,
                ((x + y) % 200) as u8 + 30,
            ]
        }),
    );
    let (mask, _) = doc::new_mask(arena, moved, |x, _| if x < 50 { 1.0 } else { 0.25 }).unwrap();
    apply(arena, mask);
    apply(
        arena,
        doc::set(moved, picture::BLEND, BlendMode::GlowDodge.name()),
    );
    let glow = add(
        arena,
        raster("Glow", 300, 260, [0, 0], |x, y| {
            [40, 80, 160, if (x / 9 + y / 7) % 2 == 0 { 140 } else { 0 }]
        }),
    );
    apply(
        arena,
        doc::set(glow, picture::BLEND, BlendMode::AddGlow.name()),
    );
    let inside = add(
        arena,
        raster("Inside", 300, 260, [0, 0], |x, _| {
            [10, 200, 30, (x % 255) as u8]
        }),
    );
    let clipped = add(
        arena,
        raster("Clipped", 300, 260, [0, 0], |_, y| {
            [250, 10, 10, (y % 255) as u8]
        }),
    );
    apply(arena, doc::set(clipped, picture::CLIP, true));
    let (folder_edit, folder) = doc::new_folder(arena, picture, Some(clipped), true);
    apply(arena, folder_edit);
    if let Some(edit) = doc::set_folder(arena, inside, Some(folder)) {
        apply(arena, edit);
    }
    apply(
        arena,
        doc::set(folder, picture::BLEND, BlendMode::Multiply.name()),
    );
    apply(arena, doc::set(folder, picture::OPACITY, 0.5));
    let hidden = add(
        arena,
        raster("Hidden", 300, 260, [0, 0], |_, _| [0, 0, 0, 255]),
    );
    apply(arena, doc::set(hidden, picture::VISIBLE, false));
    let draft = add(
        arena,
        raster("Draft", 300, 260, [0, 0], |_, _| [9, 9, 9, 255]),
    );
    apply(arena, doc::set(draft, picture::DRAFT, true));
    let reference = add(
        arena,
        raster("Reference", 300, 260, [0, 0], |x, _| {
            [0, 0, 255, (x / 3) as u8]
        }),
    );
    apply(arena, doc::set(reference, doc::REFERENCE, true));
    picture
}

fn flat(arena: &Arena, picture: EntityId) -> Vec<u8> {
    composite::flatten(
        arena,
        picture,
        RenderOptions {
            paper: true,
            drafts: true,
        },
    )
    .unwrap()
    .bytes()
    .unwrap()
    .to_vec()
}

/// The layers in words: kind, name, style, folder, locks, mask switch.
fn described(arena: &Arena, picture: EntityId) -> Vec<String> {
    doc::layers(arena, picture)
        .iter()
        .map(|info| {
            let folder = doc::folder_of(arena, info.id)
                .and_then(|folder| arena.get_by_id(folder).ok())
                .map(|folder| folder.name().to_owned());
            let blend = if doc::is_folder(arena, info.id) {
                doc::folder_blend(arena, info.id)
            } else {
                info.style.blend
            };
            format!(
                "{:?} {} {} {:?} {} {} {} {} {:?} {} {}",
                info.kind,
                info.name,
                info.style.opacity,
                blend,
                info.style.visible,
                info.style.clip,
                info.style.draft,
                doc::is_reference(arena, info.id),
                folder,
                doc::mask_of(arena, info.id).is_some(),
                doc::mask_on(arena, info.id),
            )
        })
        .collect()
}

/// Opens `bytes` into a fresh arena.
fn opened(bytes: &[u8]) -> (Arena, EntityId, Report) {
    let mut arena = Arena::new();
    let (opened, report) = open(&arena, bytes, "Opened", "Open").unwrap();
    let picture = match opened {
        Opened::Layers(edit, picture) => {
            apply(&mut arena, edit);
            picture
        }
        Opened::Copy(..) => panic!("opened from a copy of Sue's project"),
    };
    (arena, picture, report)
}

#[test]
fn every_blend_mode_has_a_clip_number_both_ways() {
    for mode in BlendMode::ALL.iter().chain(&[BlendMode::Through]) {
        assert_eq!(blend_of(composite_of(*mode)), Some(*mode), "{mode:?}");
    }
    assert_eq!(composite_of(BlendMode::GlowDodge), 10);
    assert_eq!(composite_of(BlendMode::AddGlow), 12);
    assert_eq!(blend_of(99), None);
}

#[test]
fn a_picture_through_a_clip_comes_back_with_the_same_composite_and_layers() {
    let mut arena = Arena::new();
    let picture = a_picture(&mut arena);
    let (bytes, report) = encode_with_report(&arena, picture).unwrap();
    assert!(report.exact(), "{}", report.summary());
    let doc = read(&bytes).unwrap();
    assert_eq!(doc.sue, None, "no hidden copy of the project");
    assert_eq!((doc.width, doc.height), (300, 260));
    let (back, again, report) = opened(&bytes);
    assert!(report.exact(), "{}", report.summary());
    assert_eq!(
        flat(&back, again),
        flat(&arena, picture),
        "the same picture"
    );
    assert_eq!(described(&back, again), described(&arena, picture));
    let moved = doc::layers(&back, again)
        .into_iter()
        .find(|info| info.name == "Moved")
        .unwrap()
        .id;
    let mask = doc::mask_of(&back, moved).unwrap();
    assert_eq!(
        doc::raster(&back, mask).unwrap().format(),
        PixelFormat::Rgba8,
        "a mask Sue paints on as ever"
    );
    // Written again unchanged, the file is the same.
    assert_eq!(encode(&back, again).unwrap(), bytes);
}

#[test]
fn an_untouched_imported_document_exports_to_the_same_document() {
    for (name, mut doc) in testkit::documents() {
        let bytes = write(&doc).unwrap();
        let (arena, picture, report) = opened(&bytes);
        let back = read(&encode(&arena, picture).unwrap()).unwrap();
        let mut want = read(&bytes).unwrap();
        // The hidden copy of a Sue project is no longer written.
        want.sue = None;
        want.foreign.sue_columns.clear();
        want.foreign
            .shapes
            .retain(|shape| shape.table != "SueProject");
        assert_eq!(back, want, "{name}");
        assert!(
            report.exact() || doc.sue.take().is_some(),
            "{name}: {}",
            report.summary()
        );
    }
}

/// The document `doc` with the layer named `name` (anywhere) changed.
fn layer_named<'a>(layers: &'a mut [ClipLayer], name: &str) -> Option<&'a mut ClipLayer> {
    for layer in layers {
        if layer.name == name {
            return Some(layer);
        }
        if let Some(found) = layer_named(&mut layer.children, name) {
            return Some(found);
        }
    }
    None
}

#[test]
fn an_edited_layer_loses_its_kept_data_and_only_it_across_a_save_and_reopen() {
    let mut doc = testkit::raster_subset(80, 60, 7);
    testkit::add_foreign(&mut doc, 7);
    doc.sue = None;
    let bytes = write(&doc).unwrap();
    let want = read(&bytes).unwrap();
    let (arena, picture, _) = opened(&bytes);
    // Into a Sue project and out again.
    let mut file = sue_core::SueFile::new();
    file.set_arena(&arena);
    let mut arena = sue_core::SueFile::from_bytes(&file.to_bytes())
        .unwrap()
        .arena(&sue_model::registry())
        .unwrap();
    let back = doc::layers(&arena, picture)
        .into_iter()
        .find(|info| info.name == "Back")
        .unwrap()
        .id;
    assert!(
        arena.get_by_id(back).unwrap().ext::<Foreign>().is_some(),
        "the kept data was saved"
    );
    // One pixel of one layer painted.
    doc::raster_mut(&mut arena, back)
        .unwrap()
        .bytes_mut()
        .unwrap()[0] ^= 0xff;
    let (bytes, report) = encode_with_report(&arena, picture).unwrap();
    assert!(report.exact());
    let mut got = read(&bytes).unwrap();
    let mut want = want;
    let edited = layer_named(&mut got.root.children, "Back").unwrap().clone();
    let kept = layer_named(&mut want.root.children, "Back")
        .unwrap()
        .clone();
    assert!(!kept.columns.is_empty(), "the file had columns of its own");
    assert!(
        kept.columns
            .iter()
            .any(|column| !edited.columns.contains(column)),
        "the edited layer's own columns are gone"
    );
    assert_ne!(edited.pixels, kept.pixels);
    // Everything else is as it was read; what described the edited
    // layer's stored bitmaps went with them.
    let gone: Vec<String> = [&kept.pixels, &edited.pixels]
        .into_iter()
        .flatten()
        .map(|bitmap| bitmap.external.clone())
        .collect();
    for doc in [&mut got, &mut want] {
        doc.foreign.external.retain(|(id, _)| !gone.contains(id));
    }
    *layer_named(&mut got.root.children, "Back").unwrap() = kept;
    assert_eq!(got, want);
}

/// `picture` as Sue 0.0.75 wrote a `.clip`: its layers through the
/// Photoshop records and the project hidden inside.
fn as_0_0_75_wrote_it(arena: &Arena, picture: EntityId) -> Vec<u8> {
    let bundle = Bundle::copy_from(arena, &[picture], true).unwrap();
    let mut out = Arena::new();
    for entity in bundle.entities {
        out.insert_keep_id(entity).unwrap();
    }
    for relation in bundle.relations {
        let _ = out.relate(relation);
    }
    let mut file = sue_core::SueFile::new();
    file.set_arena(&out);
    let psd = crate::psd_io::to_psd(arena, picture).unwrap();
    sue_io::clip::encode_clip(&psd, 72.0, Some(&file.to_bytes())).unwrap()
}

#[test]
fn a_hidden_project_opens_only_while_the_layers_match_it() {
    let mut arena = Arena::new();
    let picture = a_picture(&mut arena);
    // Sue 0.0.75 left masks out of the copy it hid (they are not the
    // picture's children), while the layers it wrote have them laid in:
    // such a file opens from its layers, which show what it showed.
    let bytes = as_0_0_75_wrote_it(&arena, picture);
    let (_, _, report) = opened(&bytes);
    assert!(report.stale_copy);
    let moved = doc::layers(&arena, picture)
        .into_iter()
        .find(|info| info.name == "Moved")
        .unwrap()
        .id;
    let unmask = doc::delete_mask(&arena, moved).unwrap();
    apply(&mut arena, unmask);
    let bytes = as_0_0_75_wrote_it(&arena, picture);
    // As written: the copy, with its ids.
    let fresh = Arena::new();
    let (made, report) = open(&fresh, &bytes, "Picture", "Open").unwrap();
    assert!(matches!(made, Opened::Copy(Some(_), id) if id == picture));
    assert!(report.own_copy);
    // Changed in Clip Studio afterwards: one layer painted over.
    let mut doc = read(&bytes).unwrap();
    let layer = doc
        .root
        .children
        .iter_mut()
        .find(|layer| !layer.is_folder() && !layer.is_paper())
        .unwrap();
    let [w, h] = layer.pixels.as_ref().unwrap().size().unwrap();
    layer.pixels = Some(ClipBitmap::color(
        w,
        h,
        &[1, 2, 3, 255].repeat((w * h) as usize),
    ));
    let changed = write(&doc).unwrap();
    assert!(
        read(&changed).unwrap().sue.is_some(),
        "the copy is still in it"
    );
    let (back, again, report) = opened(&changed);
    assert!(report.stale_copy, "the report says why");
    assert!(report.summary().contains("does not match the layers"));
    assert_ne!(again, picture, "a new picture from the layers");
    assert_eq!(
        doc::layers(&back, again).len(),
        doc.all_layers().len(),
        "every layer of the file"
    );
    // Renamed in Clip Studio: from the layers as well.
    let mut doc = read(&bytes).unwrap();
    doc.root.children[1].name.push_str(" (Clip Studio)");
    let renamed = write(&doc).unwrap();
    let (_, _, report) = opened(&renamed);
    assert!(report.stale_copy);
}

#[test]
fn the_clip_sue_0_0_75_wrote_opens_from_its_copy() {
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../sue-io/tests/golden/picture.clip"),
    )
    .unwrap();
    let (made, report) = open(&Arena::new(), &bytes, "picture", "Open").unwrap();
    assert!(
        matches!(made, Opened::Copy(Some(_), _)),
        "{}",
        report.summary()
    );
    let (arena, picture) = picture_arena(&bytes).unwrap();
    assert!(doc::layers(&arena, picture).len() > 2);
    // Opened again while it is open: nothing to bring in, the same canvas.
    let (again, _) = open(&arena, &bytes, "picture", "Open").unwrap();
    assert!(matches!(again, Opened::Copy(None, id) if id == picture));
}

#[test]
fn opening_is_one_undo_step_that_leaves_nothing_behind() {
    let doc = testkit::raster_subset(40, 30, 3);
    let bytes = write(&ClipDocument { sue: None, ..doc }).unwrap();
    let mut arena = Arena::new();
    let (edit, picture, _) = new_picture(&read(&bytes).unwrap(), "A", "Open").unwrap();
    let mut edit = edit;
    edit.apply(&mut arena).unwrap();
    let after: Vec<_> = arena.iter().map(|(_, entity)| entity.clone()).collect();
    assert!(arena.get_by_id(picture).is_ok());
    edit.revert(&mut arena).unwrap();
    assert_eq!(arena.iter().count(), 0, "undo takes every entity away");
    assert!(arena.relations().iter().next().is_none());
    edit.apply(&mut arena).unwrap();
    let again: Vec<_> = arena.iter().map(|(_, entity)| entity.clone()).collect();
    assert_eq!(again.len(), after.len());
    for entity in &after {
        assert_eq!(
            arena.get_by_id(entity.id()).unwrap(),
            entity,
            "redo, same ids"
        );
    }
}

#[test]
fn layers_imported_into_a_picture_land_in_the_picked_folder() {
    let mut arena = Arena::new();
    let (edit, picture) = doc::create(&NewPicture {
        width: 64,
        height: 48,
        ..NewPicture::default()
    })
    .unwrap();
    apply(&mut arena, edit);
    let (folder_edit, folder) = doc::new_folder(&arena, picture, None, false);
    apply(&mut arena, folder_edit);
    let mut doc = ClipDocument::new(64, 48, 72.0);
    doc.root.children = vec![
        ClipLayer::raster(
            "One",
            0,
            0,
            ClipBitmap::color(64, 48, &[5, 5, 5, 255].repeat(64 * 48)),
        ),
        ClipLayer::raster(
            "Two",
            3,
            4,
            ClipBitmap::color(8, 8, &[7, 7, 7, 255].repeat(64)),
        ),
    ];
    let before = doc::layers(&arena, picture).len();
    let (edit, report) = into_picture(&arena, picture, Some(folder), &doc, "Import").unwrap();
    assert!(report.exact());
    apply(&mut arena, edit);
    let layers = doc::layers(&arena, picture);
    assert_eq!(layers.len(), before + 2);
    for name in ["One", "Two"] {
        let id = layers.iter().find(|info| info.name == name).unwrap().id;
        assert_eq!(doc::folder_of(&arena, id), Some(folder), "{name}");
    }
}

#[test]
fn what_clip_studio_has_no_layer_for_is_left_out_and_said() {
    let mut arena = Arena::new();
    let (edit, picture) = doc::create(&NewPicture {
        width: 32,
        height: 32,
        ..NewPicture::default()
    })
    .unwrap();
    apply(&mut arena, edit);
    let (correction, _) = doc::new_correction(&arena, picture, None, "Posterize", "posterize:4");
    apply(&mut arena, correction);
    let (bytes, report) = encode_with_report(&arena, picture).unwrap();
    assert!(!report.exact());
    assert!(report.summary().contains("left out"));
    let doc = read(&bytes).unwrap();
    assert_eq!(doc.all_layers().len(), 2, "paper and the layer");
}

#[test]
fn a_changed_layer_clip_studio_drew_from_more_than_pixels_is_said_flattened() {
    let mut doc = ClipDocument::new(40, 30, 72.0);
    let layer = |name: &str| {
        ClipLayer::raster(
            name,
            0,
            0,
            ClipBitmap::color(40, 30, &[5_u8, 6, 7, 255].repeat(40 * 30)),
        )
    };
    let mut tone = layer("Tone");
    tone.columns.push((
        "FilterLayerInfo".to_owned(),
        sue_io::sqlite::Value::Blob(vec![1, 2, 3]),
    ));
    let mut text = layer("Words");
    text.layer_type = 7;
    doc.root.children = vec![layer("Ink"), tone, text];
    let (mut arena, picture, opened) = opened(&write(&doc).unwrap());
    assert_eq!(opened.names(Outcome::Kept), ["Tone", "Words"]);
    // Untouched, each goes back as it came: nothing to say.
    let (_, report) = encode_with_report(&arena, picture).unwrap();
    assert!(report.exact(), "{}", report.summary());
    // Painted on in Sue: the plain layer is still exact, the other two
    // lose what Clip Studio drew them from, and the report says so.
    for info in doc::layers(&arena, picture) {
        doc::raster_mut(&mut arena, info.id)
            .unwrap()
            .bytes_mut()
            .unwrap()[0] ^= 0xff;
    }
    let (_, report) = encode_with_report(&arena, picture).unwrap();
    assert_eq!(report.names(Outcome::Exact), ["Ink"]);
    assert_eq!(report.names(Outcome::Flattened), ["Tone", "Words"]);
    assert!(
        report
            .summary()
            .contains("written as their pixels: Tone, Words")
    );
}

#[test]
fn settings_of_0_0_75_save_a_sue_project_unless_clip_was_chosen() {
    let old = include_str!("../../../sue-rice/rice/history/v6.suerice");
    assert!(old.contains("save_format = \"clip\""), "0.0.75's default");
    let migrated = sue_rice::migrate(old).unwrap().expect("an older format");
    let schema = sue_rice::RiceSchema::parse(&migrated.text).unwrap();
    assert_eq!(
        schema.get_str(SAVE_FORMAT_KEY),
        Some("sue"),
        "the old default moves on"
    );
    // Someone who had picked Sue projects keeps them; nobody can have
    // picked .clip in a way that differs from the old default, so the
    // new default reaches everyone who did not choose.
    let chosen = old.replace("save_format = \"clip\"", "save_format = \"sue\"");
    let migrated = sue_rice::migrate(&chosen).unwrap().unwrap();
    let schema = sue_rice::RiceSchema::parse(&migrated.text).unwrap();
    assert_eq!(schema.get_str(SAVE_FORMAT_KEY), Some("sue"));
}

/// Writes `.clip` files for the hands-on check in Clip Studio into the
/// folder `SUE_CLIP_HANDS_ON` names: the test picture, and a generated
/// document with every raster feature taken through Sue and back.
#[test]
#[ignore = "writes files for a person to open in Clip Studio"]
fn write_files_for_the_hands_on_check() {
    let Some(dir) = std::env::var_os("SUE_CLIP_HANDS_ON") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut arena = Arena::new();
    let picture = a_picture(&mut arena);
    std::fs::write(
        dir.join("sue-picture.clip"),
        encode(&arena, picture).unwrap(),
    )
    .unwrap();
    let mut doc = testkit::raster_subset(600, 400, 11);
    doc.sue = None;
    let (arena, picture, _) = opened(&write(&doc).unwrap());
    std::fs::write(
        dir.join("raster-subset-through-sue.clip"),
        encode(&arena, picture).unwrap(),
    )
    .unwrap();
}
