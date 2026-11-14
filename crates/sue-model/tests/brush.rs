//! The brush model against files: its own version 1 fixtures, and the
//! brushes Sue 0.0.75 wrote as text.
//!
//! `tests/golden/brush-v1.bin` is one preset as [`BrushPreset::to_bytes`]
//! wrote it in format 1, and `tests/golden/brushes-v1.sue` a project with
//! a brush set of two. Both were written once and are never written
//! again: a test that fails here means the new code no longer reads a
//! version 1 brush the way version 1 wrote it, which is a bug in the new
//! code. The generator at the end shows how they were made; it writes to
//! a temporary folder only.
//!
//! The old text is `crates/sue-paint/tests/golden/tips.txt`, the brush of
//! every built-in sub tool of 0.0.75 (package P0-4), which this file only
//! reads.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;

use sue_core::txn::{SetExtension, SpawnEntity};
use sue_core::{Arena, CHILD, Entity, EntityId, Relation, SueFile, UndoStack};
use sue_model::brush::{
    AntiAlias, Binding, ColorJitter, Combine, EngineKind, InputSettings, Mixing, Pick, PictureRef,
    Spacing, Taper, TaperMode, Texture, TextureMode, TextureSource, TipSource, input, param,
};
use sue_model::{BlendMode, BrushPreset, Response, class};

/// Every committed fixture of this crate and its SHA-256. A fixture is
/// read, never rewritten (rule 12 of the 0.0.8 plan).
const LEDGER: &[(&str, &str)] = &[
    (
        "brush-v1.bin",
        "1ca5d0a52de06b4b01bececcb2429a2a5ecebafc0f4bf6309abbf37d312f1473",
    ),
    (
        "brushes-v1.sue",
        "7a4eb8f7ccc5daf0e5c9846006687d7615dbacff35be08318d7dabcabdf796b8",
    ),
];

/// The built-in sub tools of 0.0.75.
const TIPS: usize = 123;

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn golden(name: &str) -> Vec<u8> {
    let path = golden_dir().join(name);
    std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// The brushes of 0.0.75 as text: tool, sub tool and tip of each.
fn old_tips() -> Vec<(String, String, String)> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../sue-paint/tests/golden/tips.txt");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    text.lines()
        .map(|line| {
            let mut parts = line.splitn(3, '\t');
            let (Some(tool), Some(name), Some(tip)) = (parts.next(), parts.next(), parts.next())
            else {
                panic!("not a line of tips.txt: {line}");
            };
            (tool.to_owned(), name.to_owned(), tip.to_owned())
        })
        .collect()
}

/// SHA-256 of `data`, as lowercase hex.
fn sha256(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a_2f98,
        0x7137_4491,
        0xb5c0_fbcf,
        0xe9b5_dba5,
        0x3956_c25b,
        0x59f1_11f1,
        0x923f_82a4,
        0xab1c_5ed5,
        0xd807_aa98,
        0x1283_5b01,
        0x2431_85be,
        0x550c_7dc3,
        0x72be_5d74,
        0x80de_b1fe,
        0x9bdc_06a7,
        0xc19b_f174,
        0xe49b_69c1,
        0xefbe_4786,
        0x0fc1_9dc6,
        0x240c_a1cc,
        0x2de9_2c6f,
        0x4a74_84aa,
        0x5cb0_a9dc,
        0x76f9_88da,
        0x983e_5152,
        0xa831_c66d,
        0xb003_27c8,
        0xbf59_7fc7,
        0xc6e0_0bf3,
        0xd5a7_9147,
        0x06ca_6351,
        0x1429_2967,
        0x27b7_0a85,
        0x2e1b_2138,
        0x4d2c_6dfc,
        0x5338_0d13,
        0x650a_7354,
        0x766a_0abb,
        0x81c2_c92e,
        0x9272_2c85,
        0xa2bf_e8a1,
        0xa81a_664b,
        0xc24b_8b70,
        0xc76c_51a3,
        0xd192_e819,
        0xd699_0624,
        0xf40e_3585,
        0x106a_a070,
        0x19a4_c116,
        0x1e37_6c08,
        0x2748_774c,
        0x34b0_bcb5,
        0x391c_0cb3,
        0x4ed8_aa4a,
        0x5b9c_ca4f,
        0x682e_6ff3,
        0x748f_82ee,
        0x78a5_636f,
        0x84c8_7814,
        0x8cc7_0208,
        0x90be_fffa,
        0xa450_6ceb,
        0xbef9_a3f7,
        0xc671_78f2,
    ];
    let mut state: [u32; 8] = [
        0x6a09_e667,
        0xbb67_ae85,
        0x3c6e_f372,
        0xa54f_f53a,
        0x510e_527f,
        0x9b05_688c,
        0x1f83_d9ab,
        0x5be0_cd19,
    ];
    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for block in message.as_chunks::<64>().0 {
        let mut words = [0_u32; 64];
        for (word, bytes) in words.iter_mut().zip(block.as_chunks::<4>().0) {
            *word = u32::from_be_bytes(*bytes);
        }
        for at in 16..64 {
            let (a, b) = (words[at - 15], words[at - 2]);
            let small = a.rotate_right(7) ^ a.rotate_right(18) ^ (a >> 3);
            let large = b.rotate_right(17) ^ b.rotate_right(19) ^ (b >> 10);
            words[at] = words[at - 16]
                .wrapping_add(small)
                .wrapping_add(words[at - 7])
                .wrapping_add(large);
        }
        let mut v = state;
        for (constant, word) in K.iter().zip(words) {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let choose = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7]
                .wrapping_add(s1)
                .wrapping_add(choose)
                .wrapping_add(*constant)
                .wrapping_add(word);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let majority = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(majority);
            v = [
                t1.wrapping_add(t2),
                v[0],
                v[1],
                v[2],
                v[3].wrapping_add(t1),
                v[4],
                v[5],
                v[6],
            ];
        }
        for (sum, value) in state.iter_mut().zip(v) {
            *sum = sum.wrapping_add(value);
        }
    }
    state.iter().fold(String::new(), |mut out, word| {
        let _ = write!(out, "{word:08x}");
        out
    })
}

/// The preset of `brush-v1.bin`: something in every field, a parameter
/// and an input of a feature no build has, and a brush picture of each
/// kind.
fn fixture() -> BrushPreset {
    let mut preset = BrushPreset::new("Dry bristle");
    preset.id = "fixture.dry_bristle".to_owned();
    preset.group = "Paint".to_owned();
    preset.notes = "Leans, drags and breaks up.".to_owned();
    preset.engine = EngineKind::Dab;
    preset.tip = TipSource::Images {
        pictures: vec![
            PictureRef::Hash([7; 32]),
            PictureRef::Named("Bristle".to_owned()),
        ],
        pick: Pick::Sequence,
        keep_colors: true,
    };
    preset
        .set(param::SIZE, 42.5)
        .set(param::OPACITY, 0.8)
        .set(param::FLOW, 0.5)
        .set(param::DENSITY, 0.9)
        .set(param::HARDNESS, 0.7)
        .set(param::ASPECT, 0.4)
        .set(param::ANGLE, 30.0)
        .set(param::SPACING, 3.0)
        .set(param::SCATTER, 0.25)
        .set(param::TEXTURE_STRENGTH, 0.6)
        .set("vendor.bristles", 12.0);
    preset.spacing = Spacing::Fixed;
    preset.antialias = AntiAlias::Strong;
    preset.min_width = 1.0;
    preset.size_on_screen = true;
    preset.texture = Some(Texture {
        source: TextureSource::Picture([9; 32]),
        scale: 2.0,
        rotation: 30.0,
        mode: TextureMode::Subtract,
    });
    preset.dynamics = vec![
        Binding::new(param::SIZE, input::PRESSURE)
            .range(0.15, 1.0)
            .curve(Response::new(&[[0.0, 0.05], [0.4, 0.2], [1.0, 1.0]])),
        Binding::new(param::ANGLE, input::TILT_DIRECTION)
            .range(0.0, 360.0)
            .added(),
        Binding::new(param::HARDNESS, input::SPEED).range(1.0, 0.5),
        Binding::new("vendor.bristles", "vendor.barrel_button").switched(false),
    ];
    preset.taper = Taper {
        start: 20.0,
        end: 35.0,
        mode: TaperMode::Percent,
        size: false,
        opacity: true,
        by_speed: true,
    };
    preset.color = ColorJitter {
        tip: true,
        hue: 0.1,
        saturation: 0.2,
        luminosity: 0.3,
        sub: 0.4,
        stroke: true,
        stroke_hue: 0.5,
        stroke_saturation: 0.6,
        stroke_luminosity: 0.7,
    };
    preset.mix = Some(Mixing {
        amount: 0.6,
        load: 0.5,
        pull: 0.25,
        blur: 0.125,
    });
    preset.blend = BlendMode::Multiply;
    preset.erase = true;
    preset.build_up = true;
    preset.input = InputSettings {
        stabilization: 15,
        by_speed: true,
        post_correction: 4,
        flick: true,
        smart_shape: true,
    };
    preset
        .extra
        .push(("vendor.fibre".to_owned(), vec![0, 1, 2, 255]));
    preset
}

/// The second brush of `brushes-v1.sue`: a plain pen that leaves every
/// parameter at its default.
fn plain_pen() -> BrushPreset {
    let mut preset = BrushPreset::new("Plain pen");
    preset.id = "fixture.plain_pen".to_owned();
    preset.dynamics = vec![Binding::new(param::SIZE, input::PRESSURE).range(0.15, 1.0)];
    preset
}

const SET: EntityId = EntityId::from_u128(0x0b5e_0000_0000_4000_8000_0000_0000_0001);
const FIRST: EntityId = EntityId::from_u128(0x0b5e_0000_0000_4000_8000_0000_0000_0002);
const SECOND: EntityId = EntityId::from_u128(0x0b5e_0000_0000_4000_8000_0000_0000_0003);

/// The arena of `brushes-v1.sue`: a brush set with two brushes.
fn brush_set() -> Arena {
    let mut arena = Arena::new();
    arena
        .insert_keep_id(Entity::empty("Fixture brushes", class::FOLDER).with_id(SET))
        .unwrap();
    for (id, preset) in [(FIRST, fixture()), (SECOND, plain_pen())] {
        let entity = Entity::empty(preset.name.clone(), class::BRUSH)
            .with_id(id)
            .with_ext(preset);
        arena.insert_keep_id(entity).unwrap();
        let order = arena.relations().next_order(SET, CHILD);
        arena
            .relate(Relation::new(SET, CHILD, id).ordered(order))
            .unwrap();
    }
    arena
}

/// `true` if both arenas hold the same entities under the same ids.
fn same(a: &Arena, b: &Arena) -> bool {
    a.len() == b.len()
        && a.iter()
            .all(|(_, entity)| b.get_by_id(entity.id()).ok() == Some(entity))
}

#[test]
fn sha256_is_the_real_one() {
    assert_eq!(
        sha256(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        sha256(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        sha256(&[b'a'; 119]),
        "31eba51c313a5c08226adf18d4a359cfdfd8d2e816b13f4af952f7ea6584dcfb"
    );
}

#[test]
fn every_golden_file_is_the_one_committed() {
    let mut found: Vec<String> = std::fs::read_dir(golden_dir())
        .expect("tests/golden is there")
        .map(|entry| {
            entry
                .expect("a readable entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    found.sort();
    let mut listed: Vec<&str> = LEDGER.iter().map(|(name, _)| *name).collect();
    listed.sort_unstable();
    assert_eq!(found, listed, "tests/golden and the ledger hold the same");
    for (name, sum) in LEDGER {
        assert_eq!(
            sha256(&golden(name)),
            *sum,
            "{name} changed: golden files are read, never rewritten"
        );
    }
}

#[test]
fn the_version_1_brush_loads() {
    let bytes = golden("brush-v1.bin");
    assert_eq!(bytes[0], 1, "it leads with its format");
    let preset = BrushPreset::from_bytes(&bytes).expect("a version 1 brush reads");
    assert_eq!(preset, fixture());
    // Its unknown field came through, and comes through again.
    assert_eq!(preset.extra("vendor.fibre"), Some(&[0, 1, 2, 255][..]));
    let again = BrushPreset::from_bytes(&preset.to_bytes()).unwrap();
    assert_eq!(again, preset);
}

#[test]
fn the_version_1_brush_set_loads_from_its_project() {
    let file = SueFile::from_bytes(&golden("brushes-v1.sue")).expect("a project reads");
    let arena = file.arena(&sue_model::registry()).expect("its arena reads");
    assert!(same(&arena, &brush_set()));
    assert_eq!(arena.relations().children(SET), vec![FIRST, SECOND]);
    let brushes: Vec<&BrushPreset> = arena
        .relations()
        .children(SET)
        .into_iter()
        .map(|id| {
            let entity = arena.get_by_id(id).unwrap();
            assert_eq!(entity.class(), class::BRUSH);
            entity.ext::<BrushPreset>().expect("a brush, not bytes")
        })
        .collect();
    assert_eq!(brushes, vec![&fixture(), &plain_pen()]);

    // Saved and loaded again it is the same, ids and all.
    let mut saved = SueFile::new();
    saved.set_arena(&arena);
    let again = SueFile::from_bytes(&saved.to_bytes())
        .unwrap()
        .arena(&sue_model::registry())
        .unwrap();
    assert!(same(&again, &arena));

    // A build without the brush model keeps the brushes byte for byte.
    let blind = file.arena(&sue_core::ComponentRegistry::new()).unwrap();
    let mut kept = SueFile::new();
    kept.set_arena(&blind);
    let mut known = SueFile::new();
    known.set_arena(&arena);
    assert_eq!(kept.to_bytes(), known.to_bytes());
}

#[test]
fn brush_edits_undo_and_redo_exactly() {
    let mut arena = Arena::new();
    let mut stack = UndoStack::new();
    let empty = arena.clone();

    // A new brush.
    let entity = Entity::empty("Plain pen", class::BRUSH).with_ext(plain_pen());
    let id = entity.id();
    stack.apply(&mut arena, SpawnEntity::new(entity)).unwrap();
    let made = arena.clone();
    assert_eq!(
        arena.get_by_id(id).unwrap().ext::<BrushPreset>(),
        Some(&plain_pen())
    );

    // Edited: a size, a second binding and a texture.
    let mut edited = plain_pen();
    edited.set(param::SIZE, 30.0);
    edited
        .dynamics
        .push(Binding::new(param::OPACITY, input::TILT).range(0.5, 1.0));
    edited.texture = Some(Texture::new(TextureSource::Grain));
    stack
        .apply(&mut arena, SetExtension::set(id, edited.clone()))
        .unwrap();
    let changed = arena.clone();
    assert_eq!(
        arena.get_by_id(id).unwrap().ext::<BrushPreset>(),
        Some(&edited)
    );
    assert!(!same(&changed, &made));

    // The brush taken off the entity.
    stack
        .apply(&mut arena, SetExtension::remove(id, "sue.brush"))
        .unwrap();
    let bare = arena.clone();
    assert_eq!(arena.get_by_id(id).unwrap().ext::<BrushPreset>(), None);

    for before in [&changed, &made, &empty] {
        assert!(stack.undo(&mut arena).unwrap());
        assert!(same(&arena, before));
    }
    assert!(!stack.undo(&mut arena).unwrap());
    for after in [&made, &changed, &bare] {
        assert!(stack.redo(&mut arena).unwrap());
        assert!(same(&arena, after));
    }
    assert!(!stack.redo(&mut arena).unwrap());
    assert_eq!(arena.get_by_id(id).unwrap().id(), id);
}

/// What 0.0.75 gave a brush whose text says nothing else: a size of 8
/// that pressure drives down to 0.15, dabs every 0.06 of the size.
fn old_default() -> BrushPreset {
    let mut preset = BrushPreset::default();
    preset
        .set(param::SIZE, 8.0)
        .set(param::OPACITY, 1.0)
        .set(param::HARDNESS, 1.0)
        .set(param::SPACING, 0.06)
        .set(param::FLOW, 1.0)
        .set(param::ANGLE, 0.0)
        .set(param::SCATTER, 0.0);
    preset.spacing = Spacing::Ratio;
    preset.dynamics = vec![Binding::new(param::SIZE, input::PRESSURE).range(0.15, 1.0)];
    preset
}

/// Size follows pressure down to `min`, switched `on`.
fn size_by_pressure(min: f32, on: bool) -> Binding {
    Binding::new(param::SIZE, input::PRESSURE)
        .range(min, 1.0)
        .switched(on)
}

fn tip_named(name: &str) -> TipSource {
    TipSource::Images {
        pictures: vec![PictureRef::Named(name.to_owned())],
        pick: Pick::Random,
        keep_colors: false,
    }
}

/// Presets written out by hand for built-in brushes of every sort.
#[test]
fn built_in_brushes_of_0_0_75_map_to_the_presets_expected() {
    let tips = old_tips();
    let read = |tool: &str, name: &str| {
        let (_, _, tip) = tips
            .iter()
            .find(|(known_tool, known, _)| known_tool == tool && known == name)
            .unwrap_or_else(|| panic!("{name} ({tool}) is in tips.txt"));
        BrushPreset::from_legacy_text(tip)
    };
    let fine_taper = Taper {
        start: 20.0,
        end: 20.0,
        by_speed: true,
        ..Taper::default()
    };

    // A tool that does not paint keeps the old default.
    assert_eq!(read("Zoom", "Zoom In"), old_default());

    // A pen: one swept stroke.
    let mut g_pen = old_default();
    g_pen.set(param::SIZE, 10.0);
    g_pen.antialias = AntiAlias::Medium;
    g_pen.taper = fine_taper;
    assert_eq!(read("Pen", "G-pen"), g_pen);

    // A pen that speed drives too.
    let mut calligraphy = old_default();
    calligraphy.set(param::SIZE, 50.0);
    calligraphy.taper = fine_taper;
    calligraphy
        .dynamics
        .push(Binding::new(param::SIZE, input::SPEED).range(0.4, 1.0));
    assert_eq!(read("Pen", "Calligraphy"), calligraphy);

    // A pen of one hard pixel.
    let mut dot_pen = old_default();
    dot_pen.set(param::SIZE, 1.0);
    dot_pen.antialias = AntiAlias::None;
    dot_pen.min_width = 1.0;
    dot_pen.dynamics = vec![size_by_pressure(0.15, false)];
    assert_eq!(read("Pen", "Dot pen"), dot_pen);

    // A pencil: paper grain, and pressure drives the opacity as well.
    let mut pencil = old_default();
    pencil
        .set(param::SIZE, 10.0)
        .set(param::OPACITY, 0.9)
        .set(param::FLOW, 0.85)
        .set(param::TEXTURE_STRENGTH, 0.45);
    pencil.texture = Some(Texture::new(TextureSource::Grain));
    pencil.dynamics = vec![
        size_by_pressure(0.4, true),
        Binding::new(param::OPACITY, input::PRESSURE),
    ];
    assert_eq!(read("Sketch", "Pencil"), pencil);

    // A brush that mixes: dabs.
    let mut oil = old_default();
    oil.set(param::SIZE, 80.0)
        .set(param::TEXTURE_STRENGTH, 0.12);
    oil.engine = EngineKind::Dab;
    oil.texture = Some(Texture::new(TextureSource::Grain));
    oil.mix = Some(Mixing::new(0.61));
    oil.dynamics = vec![size_by_pressure(0.6, true)];
    assert_eq!(read("Brush", "Oil paint"), oil);

    // A watercolor: builds up and mixes.
    let mut watercolor = old_default();
    watercolor
        .set(param::SIZE, 80.0)
        .set(param::HARDNESS, 0.3)
        .set(param::FLOW, 0.6);
    watercolor.engine = EngineKind::Dab;
    watercolor.build_up = true;
    watercolor.mix = Some(Mixing::new(0.3));
    assert_eq!(read("Brush", "Round watercolor brush"), watercolor);

    // A spray: dots thrown wide of the line, each of its own size.
    let mut spray = old_default();
    spray
        .set(param::SIZE, 6.0)
        .set(param::SPACING, 0.08)
        .set(param::SCATTER, 16.0);
    spray.engine = EngineKind::Dab;
    spray.tip = tip_named("Dot");
    spray.antialias = AntiAlias::Medium;
    spray.build_up = true;
    spray.dynamics = vec![
        size_by_pressure(0.15, false),
        Binding::new(param::OPACITY, input::PRESSURE),
        Binding::new(param::SIZE, input::RANDOM).range(1.0, 1.0 - 0.8),
    ];
    assert_eq!(read("Airbrush", "Spray"), spray);

    // A decoration that turns with the line and a little at random.
    let mut leaves = old_default();
    leaves
        .set(param::SIZE, 34.0)
        .set(param::SPACING, 0.8)
        .set(param::SCATTER, 0.35);
    leaves.engine = EngineKind::Dab;
    leaves.tip = tip_named("Leaf");
    leaves.dynamics = vec![
        size_by_pressure(0.15, false),
        Binding::new(param::SIZE, input::RANDOM).range(1.0, 1.0 - 0.35),
        Binding::new(param::ANGLE, input::DIRECTION)
            .range(0.0, 360.0)
            .added(),
        Binding::new(param::ANGLE, input::RANDOM)
            .range(-0.2 * 180.0, 0.2 * 180.0)
            .added(),
    ];
    assert_eq!(read("Decoration", "Leaves"), leaves);

    // A ribbon.
    let mut rope = old_default();
    rope.set(param::SIZE, 24.0).set(param::SPACING, 1.0);
    rope.engine = EngineKind::Ribbon;
    rope.tip = tip_named("Rope");
    rope.dynamics = vec![size_by_pressure(0.15, false)];
    assert_eq!(read("Decoration", "Rope"), rope);

    // A soft eraser.
    let mut eraser = old_default();
    eraser
        .set(param::SIZE, 50.0)
        .set(param::HARDNESS, 0.01)
        .set(param::FLOW, 0.3);
    eraser.engine = EngineKind::Dab;
    eraser.antialias = AntiAlias::Medium;
    eraser.erase = true;
    eraser.build_up = true;
    eraser.dynamics = vec![size_by_pressure(0.15, false)];
    assert_eq!(read("Eraser", "Soft"), eraser);
}

/// Every built-in brush of 0.0.75: each key of its text is found again in
/// the preset, where the model keeps it, and nothing else is in the
/// preset. This reads the preset back into the old words; it shares no
/// code with [`BrushPreset::from_legacy_text`].
#[test]
fn every_tip_string_of_0_0_75_maps_to_its_preset() {
    let tips = old_tips();
    assert_eq!(tips.len(), TIPS, "every built-in sub tool of 0.0.75");
    for (tool, name, tip) in &tips {
        let what = format!("{name} ({tool})");
        let words: BTreeMap<&str, &str> = tip
            .split(';')
            .map(|pair| pair.split_once('=').expect("key=value"))
            .collect();
        let checked = std::cell::RefCell::new(Vec::<String>::new());
        let word = |key: &str| -> &str {
            checked.borrow_mut().push(key.to_owned());
            words
                .get(key)
                .copied()
                .unwrap_or_else(|| panic!("{what} has no {key}"))
        };
        let number = |text: &str| text.parse::<f32>().expect("a number");
        let near = |a: f32, b: f32| (a - b).abs() <= 1e-6;
        let preset = BrushPreset::from_legacy_text(tip);

        // Nothing a person named, and nothing left over.
        assert_eq!(preset.id, "", "{what}");
        assert_eq!(preset.name, "", "{what}");
        assert_eq!(preset.group, "", "{what}");
        assert_eq!(preset.notes, "", "{what}");
        assert!(preset.extra.is_empty(), "{what}: {:?}", preset.extra);
        assert_eq!(preset.blend, BlendMode::Normal, "{what}");
        assert_eq!(preset.input, InputSettings::default(), "{what}");
        assert_eq!(word("shape_count"), "1", "{what}");

        // The numbers a pen can drive.
        let mut params = preset.params.clone();
        for (key, param) in [
            ("size", param::SIZE),
            ("opacity", param::OPACITY),
            ("hardness", param::HARDNESS),
            ("spacing", param::SPACING),
            ("flow", param::FLOW),
            ("angle", param::ANGLE),
            ("scatter", param::SCATTER),
        ] {
            assert_eq!(
                params.remove(param),
                Some(number(word(key))),
                "{what} {key}"
            );
        }
        assert_eq!(preset.spacing, Spacing::Ratio, "{what}");
        let grain = number(word("grain"));
        match &preset.texture {
            None => assert_eq!(grain, 0.0, "{what}"),
            Some(texture) => {
                assert_eq!(texture, &Texture::new(TextureSource::Grain), "{what}");
                assert_eq!(
                    params.remove(param::TEXTURE_STRENGTH),
                    Some(grain),
                    "{what}"
                );
                assert!(grain > 0.0, "{what}");
            }
        }
        assert!(params.is_empty(), "{what}: {params:?}");
        let mix = number(word("mix"));
        match preset.mix {
            None => assert_eq!(mix, 0.0, "{what}"),
            Some(mixing) => {
                assert_eq!(mixing, Mixing::new(mix), "{what}");
                assert!(mix > 0.0, "{what}");
            }
        }

        // The tip and what lays it down.
        let ribbon = word("ribbon") == "1";
        let build_up = word("build_up") == "1";
        let keep = word("keep_colors") == "1";
        let shape = words.get("shape").copied();
        checked.borrow_mut().push("shape".to_owned());
        match (&preset.tip, shape) {
            (TipSource::Round, None) => assert!(!keep, "{what}"),
            (
                TipSource::Images {
                    pictures,
                    pick,
                    keep_colors,
                },
                Some(shape),
            ) => {
                assert_eq!(pictures, &[PictureRef::Named(shape.to_owned())], "{what}");
                assert_eq!(*pick, Pick::Random, "{what}");
                assert_eq!(*keep_colors, keep, "{what}");
            }
            (tip, shape) => panic!("{what}: {tip:?} for {shape:?}"),
        }
        let engine = if ribbon {
            EngineKind::Ribbon
        } else if build_up || mix > 0.0 || shape.is_some() {
            EngineKind::Dab
        } else {
            EngineKind::Stroke
        };
        assert_eq!(preset.engine, engine, "{what}");
        assert_eq!(preset.build_up, build_up, "{what}");
        assert_eq!(preset.erase, word("eraser") == "1", "{what}");
        assert_eq!(preset.size_on_screen, word("screen_size") == "1", "{what}");
        assert_eq!(
            preset.min_width,
            if word("at_least_one") == "1" {
                1.0
            } else {
                0.0
            },
            "{what}"
        );
        let aa = AntiAlias::ALL
            .iter()
            .position(|level| *level == preset.antialias)
            .unwrap();
        assert_eq!(aa.to_string(), word("aa"), "{what}");

        // Starting and ending.
        assert_eq!(preset.taper.start, number(word("taper_start")), "{what}");
        assert_eq!(preset.taper.end, number(word("taper_end")), "{what}");
        let mode = TaperMode::ALL
            .iter()
            .position(|mode| *mode == preset.taper.mode)
            .unwrap();
        assert_eq!(mode.to_string(), word("taper_mode"), "{what}");
        assert_eq!(preset.taper.size, word("taper_size") == "1", "{what}");
        assert_eq!(preset.taper.opacity, word("taper_opacity") == "1", "{what}");
        assert_eq!(
            preset.taper.by_speed,
            word("taper_by_speed") == "1",
            "{what}"
        );

        // Color jitter.
        let color = preset.color;
        assert_eq!(color.tip, word("jitter.tip") == "1", "{what}");
        assert_eq!(color.stroke, word("jitter.stroke") == "1", "{what}");
        for (key, value) in [
            ("jitter.hue", color.hue),
            ("jitter.saturation", color.saturation),
            ("jitter.value", color.luminosity),
            ("jitter.sub", color.sub),
            ("jitter.stroke_hue", color.stroke_hue),
            ("jitter.stroke_saturation", color.stroke_saturation),
            ("jitter.stroke_value", color.stroke_luminosity),
        ] {
            assert_eq!(value, number(word(key)), "{what} {key}");
        }

        // What drives what: every binding is taken once.
        let mut bindings = preset.dynamics.clone();
        let mut take = |found: &dyn Fn(&Binding) -> bool| {
            let at = bindings.iter().position(found)?;
            Some(bindings.remove(at))
        };
        for (legacy, param) in [("size_by", param::SIZE), ("opacity_by", param::OPACITY)] {
            for (old, input) in [
                ("pressure", input::PRESSURE),
                ("velocity", input::SPEED),
                ("tilt", input::TILT),
                ("random", input::RANDOM),
            ] {
                // Multiplied in, from a minimum up to all of it.
                let binding = take(&|binding| {
                    binding.param == param
                        && binding.input == input
                        && binding.combine == Combine::Multiply
                        && binding.max == 1.0
                });
                let (on, min, curve) = binding.map_or((false, 0.0, Response::LINEAR), |binding| {
                    assert!(binding.extra.is_empty(), "{what}");
                    (binding.on, binding.min, binding.curve)
                });
                let key = format!("{legacy}.{old}");
                assert_eq!(on, word(&key) == "1", "{what} {key}");
                assert_eq!(min, number(word(&format!("{key}_min"))), "{what} {key}");
                if old == "random" {
                    assert!(curve.is_linear(), "{what} {key}");
                } else {
                    assert_eq!(
                        curve.to_string(),
                        word(&format!("{key}_curve")),
                        "{what} {key}"
                    );
                }
            }
        }
        // Each dab smaller by chance: from all of the size down.
        let size_jitter = take(&|binding| {
            binding.param == param::SIZE
                && binding.input == input::RANDOM
                && binding.combine == Combine::Multiply
                && binding.min == 1.0
                && binding.max < 1.0
                && binding.on
        })
        .map_or(0.0, |binding| 1.0 - binding.max);
        assert!(
            near(size_jitter, number(word("size_jitter"))),
            "{what}: {size_jitter}"
        );
        // Each dab turned by chance: as far one way as the other.
        let turn_jitter = take(&|binding| {
            binding.param == param::ANGLE
                && binding.input == input::RANDOM
                && binding.combine == Combine::Add
                && binding.min == -binding.max
                && binding.on
        })
        .map_or(0.0, |binding| binding.max / 180.0);
        assert!(
            near(turn_jitter, number(word("turn_jitter"))),
            "{what}: {turn_jitter}"
        );
        // Turned with the line, or with the pen's lean: a full turn.
        for (key, input) in [
            ("follow", input::DIRECTION),
            ("follow_tilt", input::TILT_DIRECTION),
        ] {
            let follows = take(&|binding| {
                binding.param == param::ANGLE
                    && binding.input == input
                    && binding.combine == Combine::Add
                    && binding.min == 0.0
                    && binding.max == 360.0
                    && binding.curve.is_linear()
                    && binding.on
            })
            .is_some();
            assert_eq!(follows, word(key) == "1", "{what} {key}");
        }
        assert!(bindings.is_empty(), "{what}: {bindings:?}");

        // Every key of the text was looked at.
        let mut checked = checked.into_inner();
        checked.sort();
        checked.dedup();
        let mut keys: Vec<String> = words.keys().map(|key| (*key).to_owned()).collect();
        if shape.is_none() {
            keys.push("shape".to_owned());
        }
        keys.sort();
        assert_eq!(checked, keys, "{what}");

        // And the preset saves and loads as it is.
        assert_eq!(
            BrushPreset::from_bytes(&preset.to_bytes()).as_ref(),
            Ok(&preset),
            "{what}"
        );
    }
}

/// How the fixtures were made. It writes to a temporary folder; the files
/// in `tests/golden` were copied from there once. When the encoding moves
/// to a format 2, delete this generator: its files are made.
#[test]
#[ignore = "generator: the golden files are committed and never rewritten"]
fn generate_the_version_1_fixtures() {
    let dir = std::env::temp_dir().join("sue-golden").join("sue-model");
    std::fs::create_dir_all(&dir).expect("the temporary folder can be made");
    let mut file = SueFile::new();
    file.set_arena(&brush_set());
    for (name, bytes) in [
        ("brush-v1.bin", fixture().to_bytes()),
        ("brushes-v1.sue", file.to_bytes()),
    ] {
        let path = dir.join(name);
        std::fs::write(&path, &bytes).expect("the file can be written");
        println!(
            "wrote {} ({} bytes, {})",
            path.display(),
            bytes.len(),
            sha256(&bytes)
        );
    }
}
