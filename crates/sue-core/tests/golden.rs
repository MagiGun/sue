//! Old files still load: the kernel's golden file.
//!
//! `tests/golden/kernel.sue` was written once by Sue 0.0.75 (commit
//! `823b060`) and is never written again. It holds every built-in
//! component kind, every kind of property value, an extension component,
//! relations of every sort, document properties, a preset library, a
//! thumbnail and a chunk no build knows. `kernel.txt` beside it says what
//! the file holds, in words that do not depend on how it is encoded.
//!
//! A test that fails here means the new code no longer reads a 0.0.75
//! file the way 0.0.75 wrote it. That is a bug in the new code: fix the
//! reader, never the files. The generator at the end is kept to show how
//! the files were made; it writes to a temporary folder only.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;

use sue_core::codec::{CodecError, Reader, Writer};
use sue_core::file::{ARENA, META, PIXELS, PRESETS, THUMBNAIL};
use sue_core::{
    ASTNode, Arena, CHILD, Chunk, Color, ColorSpace, ComponentData, ComponentKind,
    ComponentRegistry, ComponentType, Entity, EntityId, MeshBuffer3D, OpaqueComponent, OrderKey,
    PCMFloat1D, PixelFormat, Preset, PresetLibrary, Properties, Quantity, REF, RasterTile2D,
    Relation, StringBuffer, SueFile, Time, TimeSpan, Value, VectorSpline2D,
};

/// Every committed fixture of this crate and its SHA-256. A fixture is
/// read, never rewritten (rule 12 of the 0.0.8 plan): a change of one byte
/// fails here before anything else is looked at.
const LEDGER: &[(&str, &str)] = &[
    (
        "kernel.sue",
        "1c9e209b94a7fd896635584fcceba37ba00154b7c7a95a6c756c27fee016fd0b",
    ),
    (
        "kernel.txt",
        "c728b766261a55c0777567f6bf410f9c0b563aefdb7d481079f5dfc3fe130652",
    ),
];

/// The chunk tag no build of Sue knows.
const UNKNOWN: [u8; 4] = *b"GOLD";

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn golden(name: &str) -> Vec<u8> {
    let path = golden_dir().join(name);
    std::fs::read(&path).unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

fn golden_text(name: &str) -> String {
    String::from_utf8(golden(name)).expect("the description is UTF-8")
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

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

fn floats(values: &[f32]) -> String {
    let parts: Vec<String> = values.iter().map(f32::to_string).collect();
    parts.join(" ")
}

/// The extension component the golden file carries: a note with a pin.
/// Registered, it reads as itself; not registered, the kernel keeps its
/// bytes and writes them back.
#[derive(Clone, Debug, PartialEq)]
struct Note {
    text: String,
    pin: EntityId,
    weight: f32,
}

impl ComponentType for Note {
    const NAME: &'static str = "golden.note";
    const VERSION: u32 = 2;

    fn encode(&self, out: &mut Writer) {
        out.str(&self.text);
        out.raw(&self.pin.to_bytes());
        out.f32(self.weight);
    }

    fn decode(_version: u32, input: &mut Reader<'_>) -> Result<Self, CodecError> {
        let text = input.str()?;
        let mut pin = [0; 16];
        pin.copy_from_slice(input.raw(16)?);
        Ok(Self {
            text,
            pin: EntityId::from_bytes(pin),
            weight: input.f32()?,
        })
    }
}

fn registry() -> ComponentRegistry {
    let mut registry = ComponentRegistry::new();
    registry.register::<Note>();
    registry
}

/// A value in words: its kind, then what it holds.
fn value_text(value: &Value) -> String {
    match value {
        Value::Bool(on) => format!("bool {on}"),
        Value::Int(number) => format!("int {number}"),
        Value::Float(number) => format!("float {number}"),
        Value::Text(text) => format!("text {text:?}"),
        Value::Vec2(v) => format!("vec2 {} {}", v[0], v[1]),
        Value::Vec3(v) => format!("vec3 {} {} {}", v[0], v[1], v[2]),
        Value::Vec4(v) => format!("vec4 {} {} {} {}", v[0], v[1], v[2], v[3]),
        Value::Color(color) => format!(
            "color {} {} {} alpha {} in {}",
            color.rgb[0],
            color.rgb[1],
            color.rgb[2],
            color.alpha,
            color.space.name()
        ),
        Value::Quantity(quantity) => {
            format!("quantity {} {}", quantity.value, quantity.unit.symbol)
        }
        Value::Time(time) => format!("time {} flicks", time.flicks()),
        Value::Entity(id) => format!("entity {id}"),
        Value::List(items) => {
            let items: Vec<String> = items.iter().map(value_text).collect();
            format!("list [{}]", items.join(", "))
        }
        Value::Map(map) => {
            let items: Vec<String> = map
                .iter()
                .map(|(key, value)| format!("{key:?}: {}", value_text(value)))
                .collect();
            format!("map {{{}}}", items.join(", "))
        }
        Value::Bytes(bytes) => format!("bytes {}", hex(bytes)),
        Value::Opaque { tag, bytes } => format!("opaque tag {tag} {}", hex(bytes)),
        other => panic!("a kind of value the description does not know yet: {other:?}"),
    }
}

fn properties_text(out: &mut String, indent: &str, properties: &Properties) {
    let sorted: BTreeMap<&str, &Value> = properties.iter().collect();
    for (key, value) in sorted {
        let _ = writeln!(out, "{indent}property {key} = {}", value_text(value));
    }
}

fn component_text(data: &ComponentData) -> String {
    match data {
        ComponentData::StringBuffer(text) => format!("text {:?}", text.as_str()),
        ComponentData::PCMFloat1D(pcm) => format!(
            "sound {} Hz, {} channels, {} frames, samples {}",
            pcm.sample_rate(),
            pcm.channels(),
            pcm.frames(),
            floats(pcm.samples())
        ),
        ComponentData::VectorSpline2D(spline) => {
            let knots: Vec<String> = spline
                .knots()
                .iter()
                .map(|knot| format!("({} {})", knot[0], knot[1]))
                .collect();
            format!(
                "spline closed {}, knots {}",
                spline.closed(),
                knots.join(" ")
            )
        }
        ComponentData::RasterTile2D(raster) => {
            let pixels = match (raster.bytes(), raster.f32_samples()) {
                (Some(bytes), _) => hex(bytes),
                (None, Some(samples)) => floats(samples),
                (None, None) => String::new(),
            };
            format!(
                "pixels {} by {} {:?}: {pixels}",
                raster.width(),
                raster.height(),
                raster.format()
            )
        }
        ComponentData::MeshBuffer3D(mesh) => {
            let positions: Vec<String> = mesh
                .positions()
                .iter()
                .map(|p| format!("({} {} {})", p[0], p[1], p[2]))
                .collect();
            format!(
                "mesh positions {}, indices {:?}",
                positions.join(" "),
                mesh.indices()
            )
        }
        ComponentData::TimeSpan(span) => {
            format!("span from {} for {} flicks", span.start(), span.duration())
        }
        ComponentData::ASTNode(node) => {
            let children: Vec<String> = node.children().iter().map(EntityId::to_string).collect();
            format!(
                "node {:?} over {}..{}, children [{}]",
                node.kind(),
                node.span().start,
                node.span().end,
                children.join(", ")
            )
        }
    }
}

/// Everything in `arena` in words, entities by id, then relations: what
/// must come back the same from the golden file whatever the encoding
/// becomes.
fn describe_arena(arena: &Arena) -> String {
    let mut out = String::new();
    let mut entities: Vec<&Entity> = arena.iter().map(|(_, entity)| entity).collect();
    entities.sort_by_key(|entity| entity.id());
    let _ = writeln!(out, "entities {}", entities.len());
    for entity in entities {
        let _ = writeln!(
            out,
            "entity {} class {:?} name {:?}",
            entity.id(),
            entity.class(),
            entity.name()
        );
        let metadata: BTreeMap<&str, &str> = entity.metadata().collect();
        for (key, value) in metadata {
            let _ = writeln!(out, "  meta {key} = {value:?}");
        }
        properties_text(&mut out, "  ", entity.properties());
        for kind in ComponentKind::ALL {
            if let Some(data) = entity.component(kind) {
                let role = if entity.base_kind() == Some(kind) {
                    "base"
                } else {
                    "also"
                };
                let _ = writeln!(out, "  {role} {}", component_text(data));
            }
        }
        let mut extensions: Vec<String> = entity
            .exts()
            .map(|component| {
                if let Some(note) = component.as_any().downcast_ref::<Note>() {
                    format!(
                        "  extension golden.note: text {:?}, pin {}, weight {}",
                        note.text, note.pin, note.weight
                    )
                } else if let Some(kept) = component.as_any().downcast_ref::<OpaqueComponent>() {
                    format!(
                        "  extension {} kept as bytes, version {}: {}",
                        kept.type_name,
                        kept.version,
                        hex(&kept.bytes)
                    )
                } else {
                    format!("  extension {}", component.type_name())
                }
            })
            .collect();
        extensions.sort();
        for line in extensions {
            let _ = writeln!(out, "{line}");
        }
    }
    let mut kinds: Vec<&str> = arena.relations().tree_kinds().collect();
    kinds.sort_unstable();
    let _ = writeln!(out, "tree kinds {}", kinds.join(" "));
    let mut relations: Vec<Relation> = arena.relations().iter().collect();
    relations
        .sort_by(|a, b| (a.from, &a.kind, &a.order, a.to).cmp(&(b.from, &b.kind, &b.order, b.to)));
    let _ = writeln!(out, "relations {}", relations.len());
    for relation in relations {
        let _ = writeln!(
            out,
            "relation {} {} {} order {:?}",
            relation.from,
            relation.kind,
            relation.to,
            relation.order.as_str()
        );
        properties_text(&mut out, "  ", &relation.properties);
    }
    out
}

/// The whole file in words: the arena, the document properties, the
/// presets, the thumbnail and the chunk nobody knows.
fn describe(file: &SueFile, registry: &ComponentRegistry) -> String {
    let mut out = String::from("# kernel.sue, as Sue 0.0.75 wrote it\n");
    let arena = file.arena(registry).expect("the arena reads");
    out.push_str(&describe_arena(&arena));
    out.push_str("document\n");
    properties_text(&mut out, "  ", &file.meta().expect("the properties read"));
    let presets = file.presets().expect("the presets read");
    for preset in presets.iter() {
        let _ = writeln!(out, "preset {:?} for {:?}", preset.name, preset.target);
        properties_text(&mut out, "  ", &preset.values);
    }
    if let Some(thumbnail) = file.chunk(THUMBNAIL) {
        let _ = writeln!(out, "thumbnail {}", hex(&thumbnail.payload));
    }
    // The pixel store is the arena's own, read above with it.
    for chunk in file.chunks() {
        if ![ARENA, META, PRESETS, THUMBNAIL, PIXELS].contains(&chunk.tag) {
            let _ = writeln!(
                out,
                "unknown chunk {} version {}: {}",
                chunk.tag_name(),
                chunk.version,
                hex(&chunk.payload)
            );
        }
    }
    out
}

/// Fails with the first line that differs, which says more than two long
/// texts side by side.
fn assert_same_text(found: &str, expected: &str, what: &str) {
    if found == expected {
        return;
    }
    let (mut left, mut right) = (found.lines(), expected.lines());
    let mut line = 1;
    loop {
        match (left.next(), right.next()) {
            (Some(a), Some(b)) if a == b => line += 1,
            (a, b) => panic!(
                "{what}: line {line} differs\n  found:    {}\n  expected: {}",
                a.unwrap_or("(nothing)"),
                b.unwrap_or("(nothing)")
            ),
        }
    }
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
    // More than one block, and a length that needs a block of padding.
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
fn the_golden_file_loads_to_the_same_ids_counts_and_pixels() {
    let file = SueFile::from_bytes(&golden("kernel.sue")).expect("a 0.0.75 file reads");
    assert_same_text(
        &describe(&file, &registry()),
        &golden_text("kernel.txt"),
        "kernel.sue",
    );
}

#[test]
fn the_golden_file_saved_again_holds_the_same() {
    let expected = golden_text("kernel.txt");
    let file = SueFile::from_bytes(&golden("kernel.sue")).expect("a 0.0.75 file reads");
    // Saved as it is.
    let again = SueFile::from_bytes(&file.to_bytes()).expect("what was saved reads");
    assert_same_text(&describe(&again, &registry()), &expected, "saved again");
    // And with the arena read and written by this build.
    let arena = file.arena(&registry()).expect("the arena reads");
    let mut rewritten = file.clone();
    rewritten.set_arena(&arena);
    let rewritten = SueFile::from_bytes(&rewritten.to_bytes()).expect("what was saved reads");
    assert_same_text(
        &describe(&rewritten, &registry()),
        &expected,
        "arena written again",
    );
}

#[test]
fn what_this_build_does_not_know_is_kept() {
    let file = SueFile::from_bytes(&golden("kernel.sue")).expect("a 0.0.75 file reads");
    let unknown = file.chunk(UNKNOWN).expect("the unknown chunk is kept");
    assert_eq!(
        (unknown.version, unknown.payload.as_slice()),
        (3, &b"kept as it is"[..])
    );
    // A build without the note's type keeps the note as bytes, and a build
    // with it reads the very same note from what that build saved.
    let stranger = file
        .arena(&ComponentRegistry::new())
        .expect("the arena reads without the type");
    let kept = stranger
        .iter()
        .filter_map(|(_, entity)| entity.ext_named(Note::NAME))
        .filter(|component| component.as_any().is::<OpaqueComponent>())
        .count();
    assert_eq!(kept, 1, "one note, kept as bytes");
    let mut resaved = file.clone();
    resaved.set_arena(&stranger);
    let resaved = SueFile::from_bytes(&resaved.to_bytes()).expect("what was saved reads");
    assert_same_text(
        &describe(&resaved, &registry()),
        &golden_text("kernel.txt"),
        "saved by a build without the note's type",
    );
}

/// Where a generator writes: a folder under the system's temporary one,
/// never `tests/golden`.
fn out_dir() -> PathBuf {
    let dir = std::env::temp_dir().join("sue-golden").join("sue-core");
    std::fs::create_dir_all(&dir).expect("the temporary folder can be made");
    dir
}

/// Writes `kernel.sue` and `kernel.txt`. Run once at `823b060`:
/// `cargo test -p sue-core --test golden -- --ignored generate`.
/// The files it made are the ones committed; they are never made again.
#[test]
#[ignore = "generator: the golden files are committed and never rewritten"]
fn generate_the_kernel_file() {
    let mut arena = Arena::new();
    arena.add_tree_kind("golden.part");
    let pin = Entity::empty("Pin", "golden.pin");
    let pin_id = pin.id();

    let mut map = BTreeMap::new();
    map.insert("depth".to_owned(), Value::Int(2));
    map.insert("label".to_owned(), Value::Text("inner".to_owned()));
    let folder = Entity::empty("Folder", "golden.folder")
        .with_meta("source", "made by the generator")
        .with_meta("note", "two lines\nof text")
        .with_property("a.bool", true)
        .with_property("a.int", -42_i64)
        .with_property("a.float", 0.1_f64)
        .with_property("a.text", "Text with \"quotes\" and a tab\there")
        .with_property("a.vec2", Value::Vec2([1.5, -2.0]))
        .with_property("a.vec3", Value::Vec3([0.0, 1.0, 2.5]))
        .with_property("a.vec4", Value::Vec4([0.25, 0.5, 0.75, 1.0]))
        .with_property("a.color", Color::srgb8(255, 128, 0))
        .with_property(
            "a.wide_color",
            Value::Color(Color {
                rgb: [0.5, 0.25, 0.125],
                alpha: 0.5,
                space: ColorSpace::DisplayP3,
            }),
        )
        .with_property("a.quantity", Value::Quantity(Quantity::new(12.5, "mm")))
        .with_property("a.time", Value::Time(Time::from_seconds(1.5)))
        .with_property("a.entity", Value::Entity(pin_id))
        .with_property(
            "a.list",
            Value::List(vec![
                Value::Int(1),
                Value::Text("two".to_owned()),
                Value::Bool(false),
            ]),
        )
        .with_property("a.map", Value::Map(map))
        .with_property("a.bytes", Value::Bytes(vec![0, 1, 2, 254, 255]))
        .with_property(
            "a.newer",
            Value::Opaque {
                tag: 200,
                bytes: vec![9, 8, 7],
            },
        );
    let folder_id = folder.id();

    let text = Entity::new("Text", StringBuffer::from("It was a dark night."))
        .with_ext(Note {
            text: "Check the weather".to_owned(),
            pin: pin_id,
            weight: 0.75,
        })
        .with_component(
            ASTNode::new("paragraph", 0..20)
                .expect("a span in order")
                .with_children([pin_id]),
        )
        .expect("a text may carry a node");
    let sound = Entity::new(
        "Sound",
        PCMFloat1D::new(8000, 2, vec![0.0, 0.5, -0.5, 1.0, -1.0, 0.25]).expect("three frames"),
    );
    let spline = Entity::new(
        "Spline",
        VectorSpline2D::new(vec![[0.0, 0.0], [1.0, 2.0], [3.0, 2.0], [4.0, 0.0]], true)
            .expect("four knots"),
    );
    let picture = Entity::new(
        "Picture",
        RasterTile2D::from_bytes(
            2,
            2,
            PixelFormat::Rgba8,
            vec![255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 64, 0, 0, 0, 0],
        )
        .expect("two by two"),
    )
    .with_property("layer.opacity", 0.5_f64);
    let gray = Entity::new(
        "Gray",
        RasterTile2D::from_bytes(3, 1, PixelFormat::Gray8, vec![0, 127, 255]).expect("three"),
    );
    let light = Entity::new(
        "Light",
        RasterTile2D::from_f32(1, 1, vec![1.5, 0.25, 0.0, 1.0]).expect("one pixel"),
    );
    let mesh = Entity::new("Mesh", MeshBuffer3D::quad(2.0, 1.0));
    let span = Entity::new("Span", TimeSpan::new(705_600, 1_411_200).expect("a span"));
    let node = Entity::new("Node", ASTNode::new("root", 0..0).expect("an empty span"));

    let ids: Vec<EntityId> = [
        &text, &sound, &spline, &picture, &gray, &light, &mesh, &span, &node,
    ]
    .iter()
    .map(|entity| entity.id())
    .collect();
    for entity in [
        pin, folder, text, sound, spline, picture, gray, light, mesh, span, node,
    ] {
        arena.insert_keep_id(entity).expect("a new id");
    }
    let keys = OrderKey::sequence(None, ids.len());
    for (id, key) in ids.iter().zip(keys) {
        arena
            .relate(Relation::new(folder_id, CHILD, *id).ordered(key))
            .expect("a child");
    }
    arena
        .relate(Relation::new(ids[0], REF, ids[3]))
        .expect("a reference");
    let mut cable = Relation::new(ids[1], "golden.cable", ids[2]);
    cable.properties.set("port", "left");
    cable.properties.set("gain", 0.5_f64);
    arena.relate(cable).expect("a relation with data");
    arena
        .relate(Relation::new(ids[6], "golden.part", ids[7]))
        .expect("a tree of its own");

    let mut file = SueFile::new();
    file.set_arena(&arena);
    file.set_meta(
        &Properties::new()
            .with("doc.title", "Golden")
            .with("doc.author", "Sue 0.0.75")
            .with("doc.created", Value::Time(Time::from_seconds(60.0))),
    );
    let mut presets = PresetLibrary::new();
    presets.add(Preset::new(
        "Print",
        "export.pdf",
        Properties::new().with("dpi", 300_i64).with("bleed", true),
    ));
    presets.add(Preset::new(
        "Voice",
        "audio.compress",
        Properties::new().with("ratio", 4.0_f64),
    ));
    file.set_presets(&presets);
    file.set(Chunk {
        tag: THUMBNAIL,
        version: 1,
        payload: vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A],
    });
    file.set(Chunk {
        tag: UNKNOWN,
        version: 3,
        payload: b"kept as it is".to_vec(),
    });

    let bytes = file.to_bytes();
    let read = SueFile::from_bytes(&bytes).expect("what was written reads");
    let words = describe(&read, &registry());
    let dir = out_dir();
    std::fs::write(dir.join("kernel.sue"), &bytes).expect("the file can be written");
    std::fs::write(dir.join("kernel.txt"), &words).expect("the file can be written");
    println!("wrote kernel.sue and kernel.txt to {}", dir.display());
    println!("kernel.sue {}", sha256(&bytes));
    println!("kernel.txt {}", sha256(words.as_bytes()));
}
