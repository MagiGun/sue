//! Old files still load: Sue Paint's golden files, and the ledgers of the
//! names files and settings are written with.
//!
//! Everything in `tests/golden` was written once by Sue 0.0.75 (commit
//! `823b060`) and is never written again. Beside each `.sue` file a `.txt`
//! says what the file holds, in words that do not depend on how it is
//! encoded: entity ids, names, classes, properties, counts and pixels.
//!
//! A test that fails here means the new code no longer reads a 0.0.75
//! file the way 0.0.75 wrote it. That is a bug in the new code: fix the
//! reader or write the migration, never touch the files. When the way the
//! data is held changes (a tip gets new fields, a ruler a new shape), the
//! functions here that put it into words change with it, so that the same
//! words come out; the `.txt` files stay as they are.
//!
//! What is here:
//!
//! - `picture.sue`: paper, a raster layer with a mask, a vector layer (a
//!   round pen line, a decoration line, a dot, a flicked line, a symmetry
//!   stroke), layer and picture properties.
//! - `animation.sue`: an animation folder of three cels, a timeline, a 2D
//!   camera with two keys, a sound, onion skin settings, light tables.
//! - `comic.sue`: a story of two pages with nested folders, a correction
//!   layer, words, frame borders, balloons, rulers, layer effects, layer
//!   comps and a 3D layer.
//! - `corrections.sue`: a Level Correction layer and a Gradient Map
//!   layer, each over a ramp of grays, with the colors they drew.
//! - `writer.sue`: a Sue Writer project.
//! - `clip-picture.txt`: what Sue Paint reads from the `.clip` in
//!   `crates/sue-io/tests/golden`.
//! - `tips.txt`: the brush of every built-in sub tool, as text.
//! - `paint/`: Sue Paint's data folder (Item Bank, palette layout,
//!   workspaces, command bar, auto actions, canvas presets, color sets,
//!   materials library).
//! - `work/`: the recent files, labels and unsaved work Sue keeps.
//! - `settings.suerice`: a settings file of format 6 with changed values.
//! - `ledger/`: every command name and every settings key of 0.0.75.
//!
//! The generators at the end show how the files were made; they write to
//! a temporary folder only and are never run again. When code a generator
//! calls changes, delete the generator instead of porting it: its files
//! are made, and the tests above it are what must keep working.
//!
//! This file is a module of the crate (`src/lib.rs` names it by path), so
//! it reaches the private readers: the palette layout, the workspaces, the
//! command bar, the Item Bank, the auto actions and the color sets.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use sue_core::txn::{SetProperty, SpawnEntity};
use sue_core::{
    Arena, Chunk, Color, ComponentData, ComponentKind, Entity, EntityId, Properties, Relation,
    ReversibleTransaction, SueFile, Value,
};
use sue_model::picture::{self, LayerKind};
use sue_model::{Animation, AttributeData, Domain, Geometry, Interpolation, PrimKind, Sequence};

use crate::SuePaint;
use crate::brush::{Copy2D, Finish, Stroke, Tip};
use crate::doc::{self, NewPicture};
use crate::tools::Tool;
use crate::vector;

/// The chunk tag no build of Sue knows; the files carry one, as a file
/// from another build might.
const UNKNOWN: [u8; 4] = *b"GOLD";

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn golden(name: &str) -> Vec<u8> {
    let path = golden_dir().join(name);
    std::fs::read(&path).unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

fn golden_text(name: &str) -> String {
    String::from_utf8(golden(name)).expect("the file is UTF-8")
}

/// FNV-1a of `bytes`: short enough to read in a description, and enough
/// to tell one layer's pixels from another's.
fn fnv(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn fnv_f32(values: &[f32]) -> u64 {
    let bytes: Vec<u8> = values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect();
    fnv(&bytes)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

fn numbers<T: ToString>(values: &[T]) -> String {
    let parts: Vec<String> = values.iter().map(ToString::to_string).collect();
    parts.join(" ")
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

// ----- A document in words ---------------------------------------------------------

/// A value in words: its kind, then what it holds. Text is quoted the way
/// Rust quotes it (`{:?}`), which for what the golden files hold (plain
/// letters, line ends, tabs and quotes) never changes.
fn value_text(value: &Value) -> String {
    match value {
        Value::Bool(on) => format!("bool {on}"),
        Value::Int(number) => format!("int {number}"),
        Value::Float(number) => format!("float {number}"),
        Value::Text(text) => format!("text {text:?}"),
        Value::Vec2(v) => format!("vec2 {}", numbers(v)),
        Value::Vec3(v) => format!("vec3 {}", numbers(v)),
        Value::Vec4(v) => format!("vec4 {}", numbers(v)),
        Value::Color(color) => format!(
            "color {} alpha {} in {}",
            numbers(&color.rgb),
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
            "sound {} Hz, {} channels, {} frames, samples fnv {:016x}",
            pcm.sample_rate(),
            pcm.channels(),
            pcm.frames(),
            fnv_f32(pcm.samples())
        ),
        ComponentData::RasterTile2D(raster) => {
            let bytes = raster.bytes().expect("8-bit pixels");
            let painted = match raster.format() {
                sue_core::PixelFormat::Rgba8 => bytes
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .filter(|pixel| pixel[3] != 0)
                    .count(),
                _ => bytes.iter().filter(|value| **value != 0).count(),
            };
            let format = match raster.format() {
                sue_core::PixelFormat::Gray8 => "Gray8",
                sue_core::PixelFormat::Rgba8 => "Rgba8",
                sue_core::PixelFormat::RgbaF32 => "RgbaF32",
            };
            format!(
                "pixels {} by {} {format}, {painted} not clear, fnv {:016x}",
                raster.width(),
                raster.height(),
                fnv(bytes)
            )
        }
        ComponentData::MeshBuffer3D(mesh) => format!(
            "mesh {} points, {} triangles",
            mesh.vertex_count(),
            mesh.triangle_count()
        ),
        other => panic!("a component the description does not know yet: {other:?}"),
    }
}

fn geometry_text(out: &mut String, geometry: &Geometry) {
    let points: Vec<u8> = geometry
        .points()
        .iter()
        .flatten()
        .flat_map(|value| value.to_le_bytes())
        .collect();
    let _ = writeln!(
        out,
        "  geometry: {} points fnv {:016x}, {} primitives",
        geometry.points().len(),
        fnv(&points),
        geometry.prims().len()
    );
    let (mut polygons, mut polylines, mut curves) = (0, 0, 0);
    for prim in geometry.prims() {
        match prim.kind {
            PrimKind::Polygon => polygons += 1,
            PrimKind::Polyline => polylines += 1,
            PrimKind::Bezier { .. } => curves += 1,
        }
    }
    let _ = writeln!(
        out,
        "    {polygons} polygons, {polylines} polylines, {curves} curves"
    );
    for (domain, name, data) in geometry.attributes() {
        let domain = match domain {
            Domain::Point => "point",
            Domain::Vertex => "vertex",
            Domain::Primitive => "primitive",
            Domain::Detail => "detail",
        };
        let what = match data {
            AttributeData::Float { width, values } => format!(
                "{} floats, {width} each, fnv {:016x}",
                values.len(),
                fnv_f32(values)
            ),
            AttributeData::Int { width, values } if values.len() <= 16 => {
                format!(
                    "{} whole numbers, {width} each: {}",
                    values.len(),
                    numbers(values)
                )
            }
            AttributeData::Int { width, values } => {
                let bytes: Vec<u8> = values
                    .iter()
                    .flat_map(|value| value.to_le_bytes())
                    .collect();
                format!(
                    "{} whole numbers, {width} each, fnv {:016x}",
                    values.len(),
                    fnv(&bytes)
                )
            }
            AttributeData::Text(values) => format!("{} texts: {values:?}", values.len()),
        };
        let _ = writeln!(out, "    attribute {domain} {name}: {what}");
    }
}

fn sequence_text(out: &mut String, sequence: &Sequence) {
    let _ = writeln!(
        out,
        "  sequence: {}/{} frames a second, {} Hz, {} tracks",
        sequence.rate.numerator(),
        sequence.rate.denominator(),
        sequence.sample_rate,
        sequence.tracks.len()
    );
    for track in &sequence.tracks {
        let _ = writeln!(
            out,
            "    track {:?} kind {:?} muted {} locked {}, {} clips",
            track.name,
            track.kind,
            track.muted,
            track.locked,
            track.clips().len()
        );
        properties_text(out, "      ", &track.properties);
        for clip in track.clips() {
            let _ = writeln!(
                out,
                "      clip of {} at {} for {} flicks, from {} at speed {}",
                clip.source,
                clip.start.flicks(),
                clip.duration.flicks(),
                clip.source_in.flicks(),
                clip.speed
            );
            properties_text(out, "        ", &clip.properties);
        }
    }
}

fn animation_text(out: &mut String, animation: &Animation) {
    let channels: BTreeMap<&str, &sue_model::Curve> = animation.channels().collect();
    let _ = writeln!(out, "  animation: {} channels", channels.len());
    for (name, curve) in channels {
        let keys: Vec<String> = curve
            .keys()
            .iter()
            .map(|key| {
                let how = match key.interpolation {
                    Interpolation::Constant => "held".to_owned(),
                    Interpolation::Linear => "straight".to_owned(),
                    Interpolation::Bezier { out, incoming } => {
                        format!("curved {} {}", numbers(&out), numbers(&incoming))
                    }
                };
                format!("{} at {} flicks {how}", key.value, key.time.flicks())
            })
            .collect();
        let _ = writeln!(out, "    channel {name}: {}", keys.join("; "));
    }
}

fn marks_text(out: &mut String, marks: &sue_model::TextMarks) {
    let _ = writeln!(out, "  marks: {}", marks.marks().len());
    for mark in marks.marks() {
        let _ = writeln!(
            out,
            "    {}..{} {} = {}{}",
            mark.range.start,
            mark.range.end,
            mark.kind,
            value_text(&mark.value),
            if mark.inclusive { ", grows" } else { "" }
        );
    }
}

fn table_text(out: &mut String, table: &sue_model::Table) {
    let (rows, columns) = table.size();
    let _ = writeln!(out, "  table: {rows} rows, {columns} columns");
    for row in table.texts() {
        let _ = writeln!(out, "    {row:?}");
    }
}

/// Everything in `arena` in words, entities by id, then relations: what
/// must come back the same from a golden file whatever the encoding
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
        let mut names: Vec<&str> = entity
            .exts()
            .map(|component| component.type_name())
            .collect();
        names.sort_unstable();
        for name in names {
            let component = entity.ext_named(name).expect("it was just listed").as_any();
            if let Some(geometry) = component.downcast_ref::<Geometry>() {
                geometry_text(&mut out, geometry);
            } else if let Some(sequence) = component.downcast_ref::<Sequence>() {
                sequence_text(&mut out, sequence);
            } else if let Some(animation) = component.downcast_ref::<Animation>() {
                animation_text(&mut out, animation);
            } else if let Some(marks) = component.downcast_ref::<sue_model::TextMarks>() {
                marks_text(&mut out, marks);
            } else if let Some(table) = component.downcast_ref::<sue_model::Table>() {
                table_text(&mut out, table);
            } else {
                panic!("an extension the description does not know yet: {name}");
            }
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

/// A brush tip in the words Sue 0.0.75 kept it in (`Tip::to_text` at
/// `823b060`, copied here so it stays as it was): `key=value` pairs joined
/// by semicolons. Reading an old tip and saying it again in these words
/// gives the old text back, whatever the tip is kept as today.
fn tip_words(tip: &Tip) -> String {
    let flag = |on: bool| if on { "1" } else { "0" }.to_owned();
    let mut out = vec![
        ("size".to_owned(), tip.size.to_string()),
        ("opacity".to_owned(), tip.opacity.to_string()),
        ("hardness".to_owned(), tip.hardness.to_string()),
        ("aa".to_owned(), (tip.aa as u8).to_string()),
        ("spacing".to_owned(), tip.spacing.to_string()),
        ("build_up".to_owned(), flag(tip.build_up)),
        ("flow".to_owned(), tip.flow.to_string()),
        ("grain".to_owned(), tip.grain.to_string()),
        ("screen_size".to_owned(), flag(tip.screen_size)),
        ("at_least_one".to_owned(), flag(tip.at_least_one)),
        ("eraser".to_owned(), flag(tip.eraser)),
        ("taper_start".to_owned(), tip.taper_start.to_string()),
        ("taper_end".to_owned(), tip.taper_end.to_string()),
        ("taper_mode".to_owned(), (tip.taper_mode as u8).to_string()),
        ("taper_size".to_owned(), flag(tip.taper_size)),
        ("taper_opacity".to_owned(), flag(tip.taper_opacity)),
        ("taper_by_speed".to_owned(), flag(tip.taper_by_speed)),
        ("mix".to_owned(), tip.mix.to_string()),
        ("follow".to_owned(), flag(tip.follow)),
        ("follow_tilt".to_owned(), flag(tip.follow_tilt)),
        ("angle".to_owned(), tip.angle.to_string()),
        ("scatter".to_owned(), tip.scatter.to_string()),
        ("size_jitter".to_owned(), tip.size_jitter.to_string()),
        ("turn_jitter".to_owned(), tip.turn_jitter.to_string()),
        ("ribbon".to_owned(), flag(tip.ribbon)),
        ("keep_colors".to_owned(), flag(tip.keep_colors)),
        ("shape_count".to_owned(), tip.shape_count.to_string()),
        ("jitter.tip".to_owned(), flag(tip.jitter.tip)),
        ("jitter.hue".to_owned(), tip.jitter.hue.to_string()),
        (
            "jitter.saturation".to_owned(),
            tip.jitter.saturation.to_string(),
        ),
        ("jitter.value".to_owned(), tip.jitter.value.to_string()),
        ("jitter.sub".to_owned(), tip.jitter.sub.to_string()),
        ("jitter.stroke".to_owned(), flag(tip.jitter.stroke)),
        (
            "jitter.stroke_hue".to_owned(),
            tip.jitter.stroke_hue.to_string(),
        ),
        (
            "jitter.stroke_saturation".to_owned(),
            tip.jitter.stroke_saturation.to_string(),
        ),
        (
            "jitter.stroke_value".to_owned(),
            tip.jitter.stroke_value.to_string(),
        ),
    ];
    for (name, by) in [("size_by", &tip.size_by), ("opacity_by", &tip.opacity_by)] {
        out.extend([
            (format!("{name}.pressure"), flag(by.pressure)),
            (format!("{name}.pressure_min"), by.pressure_min.to_string()),
            (
                format!("{name}.pressure_curve"),
                by.pressure_curve.to_string(),
            ),
            (format!("{name}.velocity"), flag(by.velocity)),
            (format!("{name}.velocity_min"), by.velocity_min.to_string()),
            (
                format!("{name}.velocity_curve"),
                by.velocity_curve.to_string(),
            ),
            (format!("{name}.tilt"), flag(by.tilt)),
            (format!("{name}.tilt_min"), by.tilt_min.to_string()),
            (format!("{name}.tilt_curve"), by.tilt_curve.to_string()),
            (format!("{name}.random"), flag(by.random)),
            (format!("{name}.random_min"), by.random_min.to_string()),
        ]);
    }
    if let Some(shape) = crate::brush::shape(tip.shape) {
        out.push(("shape".to_owned(), shape.name.replace([';', '='], " ")));
    }
    out.iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join(";")
}

/// One vector line in words.
fn line_text(index: usize, line: &vector::Line) -> String {
    let first = line.samples.first().map_or([0.0; 2], |sample| sample.at);
    let last = line.samples.last().map_or([0.0; 2], |sample| sample.at);
    let pressure: Vec<f32> = line.samples.iter().map(|sample| sample.pressure).collect();
    let flick = line
        .finish
        .flick
        .map_or_else(|| "none".to_owned(), |flick| numbers(&flick));
    let copies: Vec<String> = line.copies.iter().map(|copy| numbers(copy)).collect();
    format!(
        "    line {index}: {} samples from {} to {}, pressure fnv {:016x}, colors {} and {}, \
         correction {}, flick {flick}, {} marks\n      copies {}\n      tip {}\n",
        line.samples.len(),
        numbers(&first),
        numbers(&last),
        fnv_f32(&pressure),
        numbers(&line.colors[0]),
        numbers(&line.colors[1]),
        line.finish.correction,
        line.marks.len(),
        copies.join(" | "),
        tip_words(&line.tip)
    )
}

/// The pictures of `arena` as Sue Paint reads them: every layer from the
/// bottom, the folders, masks and vector lines, and what the picture
/// keeps beside its layers.
fn describe_pictures(arena: &Arena) -> String {
    let mut out = String::new();
    let mut pictures: Vec<&Entity> = arena
        .iter()
        .map(|(_, entity)| entity)
        .filter(|entity| picture::is_picture(entity) && picture::size(entity).is_some())
        .filter(|entity| picture::picture_of(arena, entity.id()).is_none())
        .collect();
    pictures.sort_by_key(|entity| entity.id());
    for entity in pictures {
        let id = entity.id();
        let [width, height] = picture::size(entity).expect("checked above");
        let _ = writeln!(out, "picture {id} {:?}: {width} by {height}", entity.name());
        for info in doc::layers(arena, id) {
            let kind = match info.kind {
                LayerKind::Raster => "raster",
                LayerKind::Paper => "paper",
                LayerKind::Other => "other",
            };
            let _ = writeln!(
                out,
                "  layer {} {:?} {kind}: visible {}, opacity {}, blend {}, clip {}, offset {}, \
                 draft {}, locked {}, alpha locked {}",
                info.id,
                info.name,
                info.style.visible,
                info.style.opacity,
                info.style.blend.name(),
                info.style.clip,
                numbers(&info.style.offset),
                info.style.draft,
                info.locked,
                info.alpha_locked
            );
            if let Some(folder) = doc::folder_of(arena, info.id) {
                let _ = writeln!(out, "    in folder {folder}");
            }
            if doc::is_folder(arena, info.id) {
                let _ = writeln!(out, "    a folder, open {}", doc::is_open(arena, info.id));
            }
            if let Some(mask) = doc::mask_of(arena, info.id) {
                let _ = writeln!(
                    out,
                    "    mask {mask}, in use {}",
                    doc::mask_on(arena, info.id)
                );
            }
            if vector::is_vector(arena, info.id) {
                let lines = vector::lines(&vector::geometry(arena, info.id));
                let _ = writeln!(out, "    vector lines {}", lines.len());
                for (index, line) in lines.iter().enumerate() {
                    out.push_str(&line_text(index, line));
                }
            }
            layer_extras(&mut out, arena, id, info.id, [width, height]);
        }
        rulers_text(&mut out, &crate::rulers::Rulers::of(arena, id));
        picture_extras(&mut out, arena, id);
    }
    let mut stories: Vec<&Entity> = arena
        .iter()
        .map(|(_, entity)| entity)
        .filter(|entity| entity.class() == crate::story::STORY)
        .collect();
    stories.sort_by_key(|entity| entity.id());
    for story in stories {
        let id = story.id();
        let pages: Vec<String> = crate::story::pages(arena, id)
            .iter()
            .map(EntityId::to_string)
            .collect();
        let folio = crate::story::Folio::of(arena, id);
        let _ = writeln!(
            out,
            "story {id} {:?}: bound right {}, starts paired {}, folio shown {} at {} points, \
             centered {}, first number {}\n  pages {}",
            story.name(),
            crate::story::bound_right(arena, id),
            crate::story::starts_paired(arena, id),
            folio.show,
            folio.size,
            folio.center,
            folio.first,
            pages.join(" ")
        );
    }
    out
}

/// A picture's rulers in words.
fn rulers_text(out: &mut String, rulers: &crate::rulers::Rulers) {
    use crate::rulers::Special;
    if let Some([from, to]) = rulers.linear {
        let _ = writeln!(
            out,
            "  linear ruler from {} to {}",
            numbers(&from),
            numbers(&to)
        );
    }
    if let Some(special) = rulers.special {
        let what = match special {
            Special::Parallel(angle) => format!("parallel lines at {angle}"),
            Special::Radial(center) => format!("radial lines from {}", numbers(&center)),
            Special::Concentric(center) => {
                format!("concentric circles around {}", numbers(&center))
            }
        };
        let _ = writeln!(out, "  special ruler: {what}");
    }
    if let Some(perspective) = &rulers.perspective {
        let points: Vec<String> = perspective
            .points
            .iter()
            .map(|point| numbers(point))
            .collect();
        let _ = writeln!(
            out,
            "  perspective ruler, vanishing points {}",
            points.join(" | ")
        );
    }
    if let Some((center, angle)) = rulers.symmetry {
        let _ = writeln!(
            out,
            "  symmetrical ruler through {} at {angle}",
            numbers(&center)
        );
    }
    for path in &rulers.paths {
        let first = path.points.first().copied().unwrap_or_default();
        let last = path.points.last().copied().unwrap_or_default();
        let _ = writeln!(
            out,
            "  ruler path of {} points, closed {}, from {} to {}, fnv {:016x}",
            path.points.len(),
            path.closed,
            numbers(&first),
            numbers(&last),
            fnv_f32(path.points.as_flattened())
        );
    }
}

/// The Sue Writer projects of `arena` as the shared manuscript model reads
/// them: the binder from the top, each item with its depth, role and
/// words.
fn describe_manuscripts(arena: &Arena) -> String {
    use sue_model::manuscript;
    let mut out = String::new();
    let mut roots: Vec<&Entity> = arena
        .iter()
        .map(|(_, entity)| entity)
        .filter(|entity| manuscript::is_manuscript(entity))
        .collect();
    roots.sort_by_key(|entity| entity.id());
    for root in roots {
        let id = root.id();
        let _ = writeln!(
            out,
            "manuscript {id} {:?}: {} words to publish",
            root.name(),
            manuscript::total_words(arena, id)
        );
        for role in [
            manuscript::role::DRAFT,
            manuscript::role::RESEARCH,
            manuscript::role::TRASH,
        ] {
            let folder = manuscript::folder(arena, id, role)
                .map_or_else(|| "none".to_owned(), |folder| folder.to_string());
            let _ = writeln!(out, "  {role} folder {folder}");
        }
        for (item, depth) in manuscript::walk(arena, id) {
            let entity = arena.get_by_id(item).expect("an item of the binder");
            let kind = if manuscript::is_folder(entity) {
                "folder"
            } else if manuscript::is_document(entity) {
                "document"
            } else {
                "other"
            };
            let _ = writeln!(
                out,
                "  {}{kind} {item} {:?}: {} words, included {}",
                "  ".repeat(depth),
                entity.name(),
                manuscript::words(arena, item),
                manuscript::included(entity)
            );
        }
        let publishable: Vec<String> = manuscript::publishable(arena, id)
            .iter()
            .map(|(item, _)| item.to_string())
            .collect();
        let _ = writeln!(out, "  publishes {}", publishable.join(" "));
    }
    out
}

/// What a layer is beyond its pixels: a cel, a camera, a correction, words,
/// panels, balloons, a 3D view, effects, a place on a light table.
fn layer_extras(
    out: &mut String,
    arena: &Arena,
    picture: EntityId,
    layer: EntityId,
    size: [u32; 2],
) {
    let entity = arena.get_by_id(layer).expect("a layer of the picture");
    if crate::anim::is_animation_folder(arena, layer) {
        let cels: Vec<String> = crate::anim::cels(arena, picture, layer)
            .iter()
            .map(EntityId::to_string)
            .collect();
        let _ = writeln!(out, "    an animation folder, cels {}", cels.join(" "));
    }
    if crate::camera::is_camera(arena, layer) {
        let _ = writeln!(out, "    a camera folder");
        for time in crate::camera::keys(arena, layer) {
            let camera = crate::camera::Camera::of(arena, layer, Some(time), size);
            let _ = writeln!(
                out,
                "      key at {} flicks: center {}, scale {}, angle {}",
                time.flicks(),
                numbers(&camera.center),
                camera.scale,
                camera.angle
            );
        }
    }
    let table: Vec<String> = crate::anim::light_table(arena, layer)
        .iter()
        .map(EntityId::to_string)
        .collect();
    if !table.is_empty() {
        let _ = writeln!(out, "    light table {}", table.join(" "));
    }
    if entity.property(crate::anim::PLACE).is_some() {
        let _ = writeln!(
            out,
            "    placed on light tables at {}",
            numbers(&crate::anim::place_of(entity))
        );
    }
    if entity.class() == doc::CORRECTION {
        let text = entity
            .property(doc::CORRECTION_FILTER)
            .and_then(Value::as_str)
            .unwrap_or_default();
        let filter = crate::filters::Filter::from_text(text).expect("the correction reads");
        let what = match filter {
            crate::filters::Filter::BrightnessContrast {
                brightness,
                contrast,
            } => format!("brightness {brightness}, contrast {contrast}"),
            crate::filters::Filter::Levels {
                black,
                white,
                gamma,
                out_black,
                out_white,
            } => format!(
                "levels, input black {black}, white {white}, gamma {gamma}, output black \
                 {out_black}, white {out_white}"
            ),
            crate::filters::Filter::GradientMap { from, to } => {
                format!("gradient map from {} to {}", numbers(&from), numbers(&to))
            }
            other => panic!("a correction the description does not know yet: {other:?}"),
        };
        let _ = writeln!(out, "    a correction layer: {what}");
    }
    if let Some(words) = crate::lettering::Lettering::of(entity) {
        let ruby: Vec<String> = words
            .ruby
            .iter()
            .map(|ruby| format!("{}..{} {:?}", ruby.range.start, ruby.range.end, ruby.text))
            .collect();
        let justify = match words.justify {
            sue_type::raster::Justify::Start => "start",
            sue_type::raster::Justify::Middle => "middle",
            sue_type::raster::Justify::End => "end",
        };
        let area = crate::lettering::area_of(entity)
            .map_or_else(|| "none".to_owned(), |area| numbers(&area));
        let _ = writeln!(
            out,
            "    words {:?} in font {:?} at {} pixels, color {}, at {}, centered {}, \
             vertical {}, tracking {}, leading {}, justified {justify}, on a path of {} \
             points, ruby [{}], box {area}",
            words.content,
            words.font,
            words.size,
            numbers(&words.rgb),
            numbers(&words.at),
            words.center,
            words.vertical,
            words.tracking,
            words.leading,
            words.path.len(),
            ruby.join(", ")
        );
    }
    if entity.class() == crate::manga::FRAME {
        let panels = entity
            .property(crate::manga::PANELS)
            .and_then(Value::as_str)
            .map(crate::manga::decode_panels)
            .unwrap_or_default();
        let _ = writeln!(out, "    frame borders: {} panels", panels.len());
        for panel in panels {
            let corners: Vec<String> = panel.iter().map(|corner| numbers(corner)).collect();
            let _ = writeln!(out, "      panel {}", corners.join(" | "));
        }
    }
    if entity.class() == crate::manga::BALLOON {
        let shapes = entity
            .property(crate::manga::SHAPES)
            .and_then(Value::as_str)
            .map(crate::manga::decode_shapes)
            .unwrap_or_default();
        let _ = writeln!(out, "    balloons: {} shapes", shapes.len());
        for shape in shapes {
            use crate::manga::Shape;
            let text = match shape {
                Shape::Ellipse(rect) => format!("ellipse {}", numbers(&rect)),
                Shape::Rounded(rect) => format!("rounded {}", numbers(&rect)),
                Shape::Cloud(rect) => format!("cloud {}", numbers(&rect)),
                Shape::Flash(rect) => format!("flash {}", numbers(&rect)),
                Shape::Tail(points) => format!("tail {}", numbers(points.as_flattened())),
                Shape::Drawn(points) => {
                    format!("drawn {}", numbers(points.as_flattened()))
                }
            };
            let _ = writeln!(out, "      {text}");
        }
    }
    if crate::three::is_3d(arena, layer) {
        let view = crate::three::View3D::of(entity);
        let _ = writeln!(
            out,
            "    a 3D layer of model {}: yaw {}, pitch {}, distance {}, style {}, lines {} wide",
            crate::three::model_of(arena, layer)
                .map_or_else(|| "none".to_owned(), |model| model.to_string()),
            view.yaw,
            view.pitch,
            view.distance,
            view.style,
            view.line_width
        );
    }
    let dpi = arena
        .get_by_id(picture)
        .ok()
        .and_then(|entity| entity.property(picture::DPI))
        .and_then(Value::as_f64)
        .unwrap_or(72.0);
    let effects = crate::effects::Effects::of(entity, dpi);
    if !effects.is_plain() {
        let pair = |pair: Option<(f32, f32)>| {
            pair.map_or_else(|| "off".to_owned(), |(a, b)| format!("{a} {b}"))
        };
        let _ = writeln!(
            out,
            "    effects: border {}, watercolor edge {}, extract line {}, tone {}, layer \
             color {}, expression {}, threshold {}",
            effects.border.map_or_else(
                || "off".to_owned(),
                |(width, color)| format!("{width} wide in {}", numbers(&color))
            ),
            pair(effects.watercolor),
            pair(effects.extract),
            pair(effects.tone),
            effects
                .color
                .map_or_else(|| "off".to_owned(), |color| numbers(&color)),
            effects.expression,
            effects.threshold
        );
    }
}

/// What a picture keeps beside its layers: onion skin, layer comps, the
/// general light table and the timeline.
fn picture_extras(out: &mut String, arena: &Arena, picture: EntityId) {
    let entity = arena.get_by_id(picture).expect("the picture");
    if entity.property(crate::anim::ONION).is_some() {
        let onion = crate::anim::Onion::of(arena, picture);
        let _ = writeln!(
            out,
            "  onion skin: {} before in {}, {} after in {}, mode {}, opacity {}",
            onion.before,
            numbers(&onion.before_color),
            onion.after,
            numbers(&onion.after_color),
            onion.mode,
            onion.opacity.map_or_else(
                || "full".to_owned(),
                |(start, step)| format!("{start} less {step} each")
            )
        );
    }
    for comp in crate::comps::read(arena, picture) {
        let _ = writeln!(out, "  layer comp {:?}", comp.name);
        for state in comp.states {
            let _ = writeln!(
                out,
                "    {} visible {}, opacity {}, blend {}",
                state.layer, state.visible, state.opacity, state.blend
            );
        }
    }
    let general: Vec<String> = crate::anim::general_light_table(arena, picture)
        .iter()
        .map(EntityId::to_string)
        .collect();
    if !general.is_empty() {
        let _ = writeln!(out, "  general light table {}", general.join(" "));
    }
    if let Some(sequence) = crate::anim::timeline(arena, picture) {
        let length = crate::anim::length(arena, picture);
        let _ = writeln!(
            out,
            "  timeline: {length} frames at {} a second",
            crate::anim::fps(sequence)
        );
        for index in crate::anim::folders(sequence) {
            let track = &sequence.tracks[index];
            let folder = crate::anim::track_folder(track)
                .map_or_else(|| "none".to_owned(), |folder| folder.to_string());
            let _ = writeln!(out, "    cels of {:?}, folder {folder}", track.name);
            for (frame, cel) in crate::anim::keys(track, sequence.rate) {
                let cel = cel.map_or_else(|| "blank".to_owned(), |cel| cel.to_string());
                let _ = writeln!(out, "      frame {frame}: {cel}");
            }
            let shown: Vec<String> = (0..length)
                .map(|frame| {
                    crate::anim::cel_at(track, sequence.rate, frame).map_or_else(
                        || "-".to_owned(),
                        |cel| {
                            arena
                                .get_by_id(cel)
                                .map_or_else(|_| "?".to_owned(), |cel| cel.name().to_owned())
                        },
                    )
                })
                .collect();
            let _ = writeln!(out, "      shows {}", shown.join(" "));
        }
        for index in crate::anim::audio_tracks(sequence) {
            let track = &sequence.tracks[index];
            for clip in track.clips() {
                let _ = writeln!(
                    out,
                    "    sound {:?} of {} from frame {}",
                    track.name,
                    clip.source,
                    clip.start.frame(sequence.rate)
                );
            }
        }
    }
}

/// A `.sue` file in words: its chunks, the arena, and the pictures as Sue
/// Paint reads them.
fn describe(title: &str, file: &SueFile) -> String {
    // A vector line names its brush's shape, and the name is looked up
    // among the shapes known to this run. The decoration shapes are known
    // from the moment the tool palette exists, as they are in the studio
    // before any file opens.
    let _ = Tool::Decoration.sub_tools();
    let mut out = format!("# {title}, as Sue 0.0.75 wrote it\n");
    for chunk in file.chunks() {
        if chunk.tag == sue_core::file::ARENA {
            let _ = writeln!(out, "chunk {} version {}", chunk.tag_name(), chunk.version);
        } else {
            let _ = writeln!(
                out,
                "chunk {} version {}: {}",
                chunk.tag_name(),
                chunk.version,
                hex(&chunk.payload)
            );
        }
    }
    let arena = file
        .arena(&sue_model::registry())
        .expect("the arena of a 0.0.75 file reads");
    out.push_str(&describe_arena(&arena));
    out.push_str(&describe_pictures(&arena));
    out.push_str(&describe_manuscripts(&arena));
    out
}

/// Loads golden file `name`, compares it with its description, then saves
/// it with this build and compares what comes back with the same words.
fn check_sue(name: &str) -> Arena {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let expected = golden_text(&format!("{name}.txt"));
    let file = SueFile::from_bytes(&golden(name)).expect("a 0.0.75 file reads");
    assert_same_text(&describe(name, &file), &expected, name);
    let arena = file
        .arena(&sue_model::registry())
        .expect("the arena of a 0.0.75 file reads");
    // Saved by this build, the file may be laid out in other chunks; what
    // it holds is the same, and the chunk no build knows is still there.
    let mut again = file.clone();
    again.set_arena(&arena);
    let again = SueFile::from_bytes(&again.to_bytes()).expect("what this build saved reads");
    assert_same_text(
        &content_of(&describe(name, &again)),
        &content_of(&expected),
        &format!("{name}, saved again by this build"),
    );
    assert_eq!(
        again.chunk(UNKNOWN).map(|chunk| chunk.payload.as_slice()),
        file.chunk(UNKNOWN).map(|chunk| chunk.payload.as_slice()),
        "the unknown chunk is kept"
    );
    arena
}

/// A description without its title and its list of chunks: what the file
/// holds, however it is laid out.
fn content_of(description: &str) -> String {
    description
        .lines()
        .filter(|line| !line.starts_with("# ") && !line.starts_with("chunk "))
        .fold(String::new(), |mut out, line| {
            out.push_str(line);
            out.push('\n');
            out
        })
}

/// What Sue Paint reads from `.clip` `bytes`: the picture inside, in words.
fn describe_clip_picture(bytes: &[u8]) -> String {
    let _ = Tool::Decoration.sub_tools();
    let (arena, picture) =
        crate::clip_io::picture_arena(bytes).expect("a .clip Sue 0.0.75 wrote opens");
    let mut out = format!("# picture.clip, as Sue Paint reads it: picture {picture}\n");
    out.push_str(&describe_arena(&arena));
    out.push_str(&describe_pictures(&arena));
    out
}

/// The work folder at `dir` in words: the recent files with their labels,
/// and the work kept unsaved, each with what its project file holds.
fn describe_work(dir: &Path) -> String {
    let list = sue_wm::WorkList::open(dir.to_path_buf());
    let mut out = String::from("# work, as Sue 0.0.75 kept it\n");
    let _ = writeln!(out, "labels {:?}", list.labels());
    let _ = writeln!(out, "recent files {}", list.recent().count());
    for file in list.recent() {
        let labels: Vec<&String> = list
            .labels()
            .iter()
            .filter(|label| list.has_label(&file.path, label))
            .collect();
        let _ = writeln!(
            out,
            "recent {:?}: opened {}, saved {}, page {}, a project {}, favorite {}, labels {labels:?}",
            file.path.to_string_lossy(),
            file.opened,
            file.saved,
            file.page.as_ref().map_or("none", |page| page.as_str()),
            file.is_project(),
            list.is_favorite(&file.path)
        );
    }
    let kept: Vec<&sue_wm::Autosave> = list.unsaved().collect();
    let _ = writeln!(out, "kept work {}", kept.len());
    for autosave in kept {
        let name = autosave
            .file
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "kept {:?} in {name}: at {}, of project {:?}, holding {:?}, page {}",
            autosave.id,
            autosave.at,
            autosave
                .project
                .as_ref()
                .map(|project| project.to_string_lossy().into_owned())
                .unwrap_or_default(),
            autosave.name,
            autosave.page.as_ref().map_or("none", |page| page.as_str())
        );
        let bytes = std::fs::read(&autosave.file).expect("the kept project is there");
        let file = SueFile::from_bytes(&bytes).expect("a kept 0.0.75 project reads");
        out.push_str(&describe(&name, &file));
    }
    out
}

/// Runs `work` with an applet context whose data folder for Sue Paint is
/// `folder`, as the studio sees it inside Sue. Fails if the studio
/// reported an error.
fn with_context<R>(folder: &Path, work: impl FnOnce(&mut sue_wm::AppletContext<'_>) -> R) -> R {
    let mut rice =
        sue_rice::RiceSchema::parse(sue_rice::DEFAULT_RICE).expect("the shipped settings read");
    sue_wm::register_rice(&mut rice).expect("the shell's settings register");
    let theme = sue_wm::Theme::from_rice(&rice);
    let mut world = sue_wm::World {
        arena: Arena::new(),
        rice,
    };
    let mut history = sue_core::UndoStack::new();
    let mut typing = sue_wm::Typing::default();
    let mut requests = Vec::new();
    let mut catalog = sue_wm::Catalog::default();
    catalog
        .folders
        .insert("paint".to_owned(), folder.to_path_buf());
    let mut textures = sue_platform::NoTextures;
    let mut transport = sue_wm::Transport::default();
    let work_list = sue_wm::WorkList::default();
    let mut transfers = sue_wm::Transfers::default();
    let result = {
        let mut cx = sue_wm::AppletContext {
            world: &mut world,
            history: &mut history,
            sue_history: None,
            typing: &mut typing,
            requests: &mut requests,
            catalog: &catalog,
            theme: &theme,
            textures: &mut textures,
            audio: None,
            transport: &mut transport,
            work: &work_list,
            transfers: &mut transfers,
            view: sue_wm::ShellView {
                form_factor: sue_platform::FormFactor::Desktop,
                profile: sue_rice::HardwareProfile::Balanced,
                now: std::time::Instant::now(),
                dt: 0.0,
                page: None,
                reveal: 1.0,
                interactive: false,
            },
        };
        work(&mut cx)
    };
    for request in &requests {
        assert!(
            !matches!(request, sue_wm::ShellRequest::Fail(_)),
            "the studio reported {request:?}"
        );
    }
    result
}

/// A setting's value in words: a switch, a number, a color as `#rrggbb`,
/// chords joined by ` or `, or the text itself.
fn setting_words(value: &sue_rice::RiceValue) -> String {
    use sue_rice::RiceValue;
    match value {
        RiceValue::Bool(on) => on.to_string(),
        RiceValue::Int(number) => number.to_string(),
        RiceValue::Float(number) => number.to_string(),
        RiceValue::Color(color) => color.to_hex(),
        RiceValue::Keybind(chords) => chords.to_string(),
        RiceValue::Text(text) => text.clone(),
    }
}

/// Every string literal of Rust source `text` (good enough for names:
/// none of them holds a quote).
fn literals_of(text: &str) -> Vec<String> {
    text.lines()
        .flat_map(|line| line.split('"').skip(1).step_by(2))
        .map(str::to_owned)
        .collect()
}

/// `true` for text shaped like a command name: `edit.undo`.
fn command_like(name: &str) -> bool {
    name.contains('.')
        && name.split('.').all(|part| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        })
        && name.starts_with(|c: char| c.is_ascii_lowercase())
}

fn lines_of(names: &std::collections::BTreeSet<String>) -> String {
    names.iter().fold(String::new(), |mut out, name| {
        out.push_str(name);
        out.push('\n');
        out
    })
}

/// Every settings key Sue registers: the shipped file's, the shell's, Sue
/// Paint's and Sue Writer's.
fn every_setting() -> std::collections::BTreeSet<String> {
    use sue_wm::SueApplet as _;
    let mut rice =
        sue_rice::RiceSchema::parse(sue_rice::DEFAULT_RICE).expect("the shipped settings read");
    sue_wm::register_rice(&mut rice).expect("the shell's settings register");
    rice.register_all(&SuePaint::default().rice_declarations())
        .expect("Sue Paint's settings register");
    rice.register_all(&sue_writer::SueWriter::default().rice_declarations())
        .expect("Sue Writer's settings register");
    rice.entries()
        .iter()
        .map(|entry| entry.path().to_owned())
        .collect()
}

// ----- The tests -------------------------------------------------------------------

#[test]
fn a_picture_from_0_0_75_loads_to_the_same_ids_counts_and_pixels() {
    let arena = check_sue("picture.sue");
    // What the description cannot say: the picture still draws.
    let picture = arena
        .iter()
        .map(|(_, entity)| entity)
        .find(|entity| picture::is_picture(entity) && entity.name() == "Golden picture")
        .expect("the picture is there")
        .id();
    let flat = crate::composite::render(
        &arena,
        picture,
        [0, 0, 64, 64],
        crate::composite::RenderOptions::default(),
    );
    assert_eq!(flat.len(), 64 * 64 * 4);
    assert!(
        flat.as_chunks::<4>().0.iter().all(|pixel| pixel[3] == 255),
        "the paper covers the picture"
    );
}

#[test]
fn an_animation_from_0_0_75_loads_to_the_same_ids_counts_and_pixels() {
    let arena = check_sue("animation.sue");
    let picture = arena
        .iter()
        .map(|(_, entity)| entity)
        .find(|entity| picture::is_picture(entity))
        .expect("the picture is there")
        .id();
    // The sound still plays: the timeline reaches its samples.
    let sequence = crate::anim::timeline(&arena, picture).expect("the timeline is there");
    let mixed = crate::anim::mix(&arena, sequence).expect("the sound mixes");
    assert!(mixed.samples().iter().any(|sample| sample.abs() > 0.1));
}

#[test]
fn a_comic_from_0_0_75_loads_to_the_same_ids_counts_and_pixels() {
    let arena = check_sue("comic.sue");
    // Every page still draws, with its folio, corrections and effects.
    let story = arena
        .iter()
        .map(|(_, entity)| entity)
        .find(|entity| entity.class() == crate::story::STORY)
        .expect("the story is there")
        .id();
    let pages = crate::story::pages(&arena, story);
    assert_eq!(pages.len(), 2);
    for page in pages {
        let flat = crate::composite::render(
            &arena,
            page,
            [0, 0, 64, 64],
            crate::composite::RenderOptions::default(),
        );
        assert_eq!(flat.len(), 64 * 64 * 4);
    }
}

/// What the two pictures of `corrections.sue` drew in 0.0.75, first row,
/// left to right: the ramp of grays under Level Correction (every channel
/// the same), and under Gradient Map.
const LEVELS_ROW: [u8; 16] = [
    13, 13, 41, 72, 96, 117, 136, 154, 170, 186, 201, 216, 230, 230, 230, 230,
];
const GRADIENT_ROW: [[u8; 3]; 16] = [
    [32, 30, 80],
    [46, 43, 88],
    [60, 57, 96],
    [74, 70, 104],
    [87, 83, 112],
    [101, 97, 120],
    [115, 110, 128],
    [129, 123, 136],
    [143, 137, 144],
    [157, 150, 152],
    [171, 163, 160],
    [185, 177, 168],
    [198, 190, 176],
    [212, 203, 184],
    [226, 217, 192],
    [240, 230, 200],
];

#[test]
fn corrections_from_0_0_75_load_and_correct_as_they_did() {
    let arena = check_sue("corrections.sue");
    let picture = |name: &str| {
        arena
            .iter()
            .map(|(_, entity)| entity)
            .find(|entity| picture::is_picture(entity) && entity.name() == name)
            .unwrap_or_else(|| panic!("the picture {name} is there"))
            .id()
    };
    // A correction kept in the short text of 0.0.75 means what it meant:
    // the same colors come out, give or take one step of rounding.
    let near = |found: &[[u8; 3]], expected: &[[u8; 3]], what: &str| {
        assert_eq!(found.len(), expected.len(), "{what}");
        for (at, (found, expected)) in found.iter().zip(expected).enumerate() {
            assert!(
                found.iter().zip(expected).all(|(a, b)| a.abs_diff(*b) <= 1),
                "{what}: pixel {at} draws as {found:?}, 0.0.75 drew {expected:?}"
            );
        }
    };
    let levels: Vec<[u8; 3]> = LEVELS_ROW.iter().map(|gray| [*gray; 3]).collect();
    near(
        &first_row(&arena, picture("Golden levels")),
        &levels,
        "Level Correction",
    );
    near(
        &first_row(&arena, picture("Golden gradient map")),
        &GRADIENT_ROW,
        "Gradient Map",
    );
}

#[test]
fn a_writer_project_from_0_0_75_loads_to_the_same_ids_counts_and_text() {
    let arena = check_sue("writer.sue");
    let root = arena
        .iter()
        .map(|(_, entity)| entity)
        .find(|entity| sue_model::manuscript::is_manuscript(entity))
        .expect("the project is there");
    assert_eq!(root.name(), "The Lighthouse Keeper");
}

#[test]
fn a_clip_from_0_0_75_opens_as_the_picture_it_carries() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../sue-io/tests/golden/picture.clip");
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()));
    assert_same_text(
        &describe_clip_picture(&bytes),
        &golden_text("clip-picture.txt"),
        "picture.clip",
    );
}

#[test]
fn every_built_in_tip_of_0_0_75_reads_back() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    // A tip names its shape; the decoration shapes are known from the
    // moment the tool palette exists, as they are in the studio.
    let _ = Tool::Decoration.sub_tools();
    let mut count = 0;
    for line in golden_text("tips.txt").lines() {
        let mut parts = line.splitn(3, '\t');
        let (Some(tool), Some(name), Some(tip)) = (parts.next(), parts.next(), parts.next()) else {
            panic!("not a line of tips.txt: {line}");
        };
        assert_eq!(
            tip_words(&Tip::from_text(tip)),
            tip,
            "the brush of {name} ({tool})"
        );
        count += 1;
    }
    assert_eq!(count, TIPS, "every built-in sub tool of 0.0.75");
}

/// How many built-in sub tools 0.0.75 had.
const TIPS: usize = 123;

/// A copy of golden folder `name` in the temporary folder. What reads a
/// whole folder reads the copy, so nothing can write into the golden files.
fn copy_of(name: &str) -> PathBuf {
    static COPIES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    fn copy(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).expect("the folder can be made");
        for entry in std::fs::read_dir(from).expect("the golden folder is there") {
            let entry = entry.expect("a readable entry");
            let target = to.join(entry.file_name());
            if entry.path().is_dir() {
                copy(&entry.path(), &target);
            } else {
                std::fs::copy(entry.path(), &target).expect("the file can be copied");
            }
        }
    }
    let number = COPIES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let to = std::env::temp_dir()
        .join("sue-golden-read")
        .join(format!("{}-{number}-{name}", sue_platform::process_id()));
    let _ = std::fs::remove_dir_all(&to);
    copy(&golden_dir().join(name), &to);
    to
}

#[test]
fn the_data_folder_of_0_0_75_loads_into_the_studio() {
    use crate::dock::Palette;
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let folder = copy_of("paint");
    let mut paint = SuePaint::default();
    with_context(&folder, |cx| paint.load_extras(cx));
    paint.color_sets.load(Some(&folder));

    // The auto actions: Sue's two and the one recorded.
    let names: Vec<&str> = paint
        .auto
        .sets
        .iter()
        .map(|set| set.name.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "Flip Canvas and Fit",
            "Rotate Canvas Right",
            "Clean line art"
        ]
    );
    assert_eq!(
        paint.auto.sets[2].steps,
        [
            "select.all",
            "filter.invert",
            "layer.merge_down",
            "view.fit"
        ]
    );

    // The Item Bank: each sub tool with its tool, name, stabilization and
    // the very brush the file holds.
    let bank = golden_text("paint/item_bank.txt");
    let brushes: Vec<&str> = bank
        .lines()
        .filter_map(|line| line.splitn(4, '\t').nth(3))
        .collect();
    assert_eq!(paint.item_bank.len(), 2);
    let pen = &paint.item_bank[0];
    assert_eq!(
        (pen.tool, pen.name.as_str(), pen.stabilization),
        (Tool::Pen, "My inking pen", 12)
    );
    assert_eq!(tip_words(&pen.tip), brushes[0]);
    assert_eq!(pen.tip.size, 7.5);
    let chain = &paint.item_bank[1];
    assert_eq!(
        (chain.tool, chain.name.as_str(), chain.stabilization),
        (Tool::Decoration, "Chain", 5)
    );
    assert_eq!(tip_words(&chain.tip), brushes[1]);
    assert!(
        crate::brush::shape(chain.tip.shape).is_some(),
        "the chain kept its shape"
    );

    // The canvas presets.
    let presets: Vec<(&str, u32, u32, f64, bool)> = paint
        .canvas_presets
        .iter()
        .map(|preset| {
            (
                preset.name.as_str(),
                preset.width,
                preset.height,
                preset.dpi,
                preset.paper,
            )
        })
        .collect();
    assert_eq!(
        presets,
        [
            ("Webtoon strip", 800, 12_800, 144.0, true),
            ("Sticker", 370, 320, 72.0, false)
        ]
    );

    // The command bar. 0.0.75 wrote its four lines without line ends, so
    // 0.0.75 itself reads them back as one hidden button that is none
    // (`command_bar.txt` shows it). Reading the file as 0.0.75 does is
    // right; so is a reader that finds the four entries again.
    let mut hidden: Vec<&str> = paint.bar_hidden.iter().map(String::as_str).collect();
    hidden.sort_unstable();
    let as_written = hidden == ["edit.redohide file.saveadd view.flip_hadd layer.new"]
        && paint.bar_extra.is_empty();
    let recovered =
        hidden == ["edit.redo", "file.save"] && paint.bar_extra == ["view.flip_h", "layer.new"];
    assert!(
        as_written || recovered,
        "the command bar reads as {hidden:?} and {:?}",
        paint.bar_extra
    );

    // The workspaces, and the default one.
    let spaces: Vec<&str> = paint
        .workspaces
        .iter()
        .map(|space| space.name.as_str())
        .collect();
    assert_eq!(spaces, ["As it started", "Inking"]);
    assert_eq!(
        paint
            .default_workspace
            .as_ref()
            .map(|space| space.name.as_str()),
        Some("Workspace")
    );

    // The palettes where `layout.txt` left them.
    let widths = |paint: &SuePaint| -> (Vec<f32>, Vec<f32>) {
        (
            paint.dock.left.iter().map(|column| column.width).collect(),
            paint.dock.right.iter().map(|column| column.width).collect(),
        )
    };
    let floating = |paint: &SuePaint, palette: Palette| {
        paint
            .dock
            .floating
            .iter()
            .find(|(floating, _)| *floating == palette)
            .map(|(_, rect)| [rect.min.x, rect.min.y, rect.width(), rect.height()])
    };
    assert_eq!(widths(&paint), (vec![46.0, 284.0, 86.0], vec![]));
    assert_eq!(
        floating(&paint, Palette::Navigator),
        Some([420.0, 180.0, 260.0, 320.0])
    );
    assert_eq!(
        floating(&paint, Palette::Layer),
        Some([96.5, 64.25, 300.0, 420.0])
    );
    assert!(paint.dock.iconized.contains(&Palette::ColorSet));
    assert!(paint.dock.iconized.contains(&Palette::Material));
    let left: Vec<Vec<Palette>> = paint.dock.left[1]
        .groups
        .iter()
        .map(|group| group.tabs.clone())
        .collect();
    // Tool Property, Brush Size and the color palettes were tabs of Sub
    // Tool and Color Wheel then; they come back as those palettes' tabs.
    assert_eq!(
        left,
        [
            vec![Palette::SubTool, Palette::ToolProperty, Palette::BrushGrid],
            vec![
                Palette::Color,
                Palette::ColorSlider,
                Palette::Intermediate,
                Palette::Approximate
            ]
        ]
    );
    assert_eq!(
        paint.dock.top.first().map(|group| group.tabs.clone()),
        Some(vec![Palette::CommandBar])
    );
    assert_eq!(
        paint.dock.bottom.first().map(|group| group.tabs.clone()),
        Some(vec![Palette::Timeline])
    );
    assert_eq!(
        paint.panels,
        crate::studio::Panels {
            brush_size: false,
            timeline: false,
            ..crate::studio::Panels::default()
        }
    );
    assert!(paint.menus_hidden);

    // Window, Workspace: each registered one lays the palettes out again.
    paint.use_workspace(0);
    assert!(paint.dock.floating.is_empty());
    assert_eq!(widths(&paint), (vec![46.0, 236.0, 86.0], vec![300.0]));
    let right: Vec<(Vec<Palette>, f32)> = paint.dock.right[0]
        .groups
        .iter()
        .map(|group| (group.tabs.clone(), group.weight))
        .collect();
    assert_eq!(
        right,
        [
            (vec![Palette::Navigator], 0.42),
            (vec![Palette::Layer], 0.58)
        ]
    );
    assert!(!paint.dock.iconized.contains(&Palette::ColorSet));
    assert!(!paint.menus_hidden);
    paint.use_workspace(1);
    assert_eq!(
        floating(&paint, Palette::Navigator),
        Some([420.0, 180.0, 260.0, 320.0])
    );
    assert!(paint.dock.iconized.contains(&Palette::ColorSet));
    assert_eq!(widths(&paint), (vec![46.0, 236.0, 86.0], vec![300.0]));

    // The color sets: Sue's four and one of the artist's own.
    let sets: Vec<(&str, usize)> = paint
        .color_sets
        .sets
        .iter()
        .map(|(name, colors)| (name.as_str(), colors.len()))
        .collect();
    assert_eq!(
        sets[1..],
        [
            ("Grays", 17),
            ("Skin Tones", 12),
            ("Pastels", 10),
            ("My inks", 3)
        ]
    );
    assert_eq!(sets[0].0, "Default");
    assert_eq!(
        paint.color_sets.sets[0].1[..3],
        [[0, 0, 0], [255, 255, 255], [240, 60, 90]]
    );
    assert_eq!(
        paint.color_sets.sets[4].1,
        [[20, 20, 28], [120, 30, 40], [240, 236, 220]]
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn the_materials_library_of_0_0_75_keeps_its_ids_and_favorites() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let file = SueFile::from_bytes(&golden("paint/library.sue")).expect("the library reads");
    assert_same_text(
        &describe("paint/library.sue", &file),
        &golden_text("library.sue.txt"),
        "paint/library.sue",
    );
    // Sue Paint finds its materials again under the ids the file gave
    // them, with their favorites and when they were used.
    let folder = copy_of("paint");
    let kinds = crate::materials::MaterialKinds::builtin();
    let library = crate::materials::Library::scan(&kinds, Some(&folder));
    let entries = library.entries();
    let kept = file
        .arena(&sue_model::registry())
        .expect("the library's arena reads");
    assert_eq!(kept.len(), 12);
    for (_, entity) in kept.iter() {
        let entry = entries
            .iter()
            .find(|entry| entry.id == entity.id())
            .unwrap_or_else(|| panic!("{} lost its id", entity.name()));
        assert_eq!(entry.material.name, entity.name());
        assert_eq!(
            Some(entry.material.kind.as_str()),
            entity
                .property(crate::materials::KIND_KEY)
                .and_then(Value::as_str)
        );
        assert_eq!(
            entry.favorite,
            entity
                .property(crate::materials::FAVORITE_KEY)
                .and_then(Value::as_bool)
                .unwrap_or(false)
        );
        assert_eq!(
            entry.used,
            entity
                .property(crate::materials::USED_KEY)
                .and_then(Value::as_i64)
                .unwrap_or(0)
        );
    }
    assert_eq!(entries.iter().filter(|entry| entry.favorite).count(), 2);
    assert_eq!(
        entries.iter().map(|entry| entry.used).max(),
        Some(1_760_000_000)
    );
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn the_work_folder_of_0_0_75_reads_the_same() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let folder = copy_of("work");
    assert_same_text(&describe_work(&folder), &golden_text("work.txt"), "work");
    let _ = std::fs::remove_dir_all(&folder);
}

#[test]
fn settings_of_format_6_keep_what_was_changed() {
    use sue_wm::SueApplet as _;
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let text = golden_text("settings.suerice");
    assert_eq!(sue_rice::format_of(&text), 6);
    // As Sue starts: a file of an older format is migrated, then every
    // setting registers over what the file holds.
    let current = match sue_rice::migrate(&text).expect("a 0.0.75 settings file reads") {
        Some(migration) => migration.text,
        None => text,
    };
    let mut rice = sue_rice::RiceSchema::parse(&current).expect("the settings read");
    sue_wm::register_rice(&mut rice).expect("the shell's settings register");
    rice.register_all(&SuePaint::default().rice_declarations())
        .expect("Sue Paint's settings register");
    rice.register_all(&sue_writer::SueWriter::default().rice_declarations())
        .expect("Sue Writer's settings register");
    let mut changed = 0;
    for line in golden_text("settings.txt").lines() {
        let (path, words) = line.split_once('\t').expect("a path, a tab, a value");
        let value = rice
            .get(path)
            .unwrap_or_else(|| panic!("the setting {path} is gone"));
        assert_eq!(setting_words(&value), words, "{path}");
        changed += 1;
    }
    assert_eq!(changed, 24, "every value 0.0.75 had changed");

    // And what they mean to the studio: the pen pressure curve, Studio
    // Mode's first key, and the pen tool's two chords.
    let curve: sue_model::Response = rice
        .get_str(crate::dialogs::PEN_PRESSURE)
        .expect("the curve is there")
        .parse()
        .expect("the curve reads");
    assert_eq!(
        curve.points(),
        [[0.0, 0.0], [0.3, 0.6], [0.7, 0.9], [1.0, 1.0]]
    );
    assert_eq!(
        rice.get_str(&crate::studio_mode::custom_key(0)),
        Some("view.fit")
    );
    let pen = crate::studio::shortcut_table()
        .iter()
        .find(|shortcut| shortcut.command == "tool.pen")
        .expect("the pen tool has a key");
    let chords = sue_wm::shortcuts::chords(&rice, crate::studio::KEYS_SECTION, pen);
    assert_eq!(chords.to_string(), "P or N");
}

/// Names that left on purpose, each with the package that retired it and
/// what stands in its place. A name leaves a ledger only by being listed
/// here: the ledger files themselves are never edited.
const RETIRED: &[(&str, &str)] = &[
    // P2-4: the Image Viewer and the Converter left Sue. Pictures open in
    // Sue Paint, whose own language and keys cover what the two had.
    ("image_viewer.language", "P2-4: sue_paint.language"),
    ("convert.language", "P2-4: sue_paint.language"),
    (
        "image_viewer_keys.viewer_next",
        "P2-4: none, Sue Paint's canvas tabs",
    ),
    (
        "image_viewer_keys.viewer_previous",
        "P2-4: none, Sue Paint's canvas tabs",
    ),
    ("image_viewer_keys.viewer_fit", "P2-4: Sue Paint's view.fit"),
    (
        "image_viewer_keys.viewer_actual",
        "P2-4: Sue Paint's view.actual",
    ),
    (
        "image_viewer_keys.viewer_zoom_in",
        "P2-4: Sue Paint's view.zoom_in",
    ),
    (
        "image_viewer_keys.viewer_zoom_out",
        "P2-4: Sue Paint's view.zoom_out",
    ),
    (
        "image_viewer_keys.viewer_rotate_right",
        "P2-4: Sue Paint's Rotate/Flip Canvas",
    ),
    (
        "image_viewer_keys.viewer_rotate_left",
        "P2-4: Sue Paint's Rotate/Flip Canvas",
    ),
    (
        "image_viewer_keys.viewer_crop",
        "P2-4: Sue Paint's edit.crop",
    ),
    (
        "image_viewer_keys.viewer_save_as",
        "P2-4: Sue Paint's file.save_as",
    ),
];

/// The lines of ledger `name` that have not been retired.
fn ledger(name: &str) -> Vec<String> {
    golden_text(name)
        .lines()
        .filter(|line| !RETIRED.iter().any(|(retired, _)| retired == line))
        .map(str::to_owned)
        .collect()
}

/// Every string literal in the Rust sources of crate `krate`.
fn source_literals(krate: &str) -> std::collections::BTreeSet<String> {
    fn walk(dir: &Path, out: &mut std::collections::BTreeSet<String>) {
        for entry in std::fs::read_dir(dir).expect("the sources are there") {
            let path = entry.expect("a readable entry").path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                let text = std::fs::read_to_string(&path).expect("a source file reads");
                out.extend(literals_of(&text));
            }
        }
    }
    let mut out = std::collections::BTreeSet::new();
    walk(&source_dir(krate), &mut out);
    out
}

/// Fails unless every name of `names` is in `known`; a name ending in `*`
/// stands for a family and needs one known name that starts like it.
fn assert_all_known(names: &[String], known: &std::collections::BTreeSet<String>, what: &str) {
    let lost: Vec<&String> = names
        .iter()
        .filter(|name| match name.strip_suffix('*') {
            Some(family) => !known.iter().any(|known| known.starts_with(family)),
            None => !known.contains(*name),
        })
        .collect();
    assert!(
        lost.is_empty(),
        "{what} lost names that 0.0.75 files and settings may hold: {lost:?}"
    );
}

#[test]
fn no_command_name_of_sue_paint_is_lost() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    // A command is known while the studio's sources name it, in a menu, a
    // shortcut or wherever commands are kept.
    let mut known = source_literals("sue-paint");
    let mut menus = crate::menus::menus();
    crate::menus::set_workspaces(&mut menus, &[]);
    known.extend(
        crate::menus::commands(&menus)
            .into_iter()
            .map(|(_, command)| command),
    );
    known.extend(
        crate::studio::shortcut_table()
            .iter()
            .map(|shortcut| shortcut.command.to_owned()),
    );
    let names = ledger("ledger/paint-commands.txt");
    assert_eq!(
        golden_text("ledger/paint-commands.txt").lines().count(),
        398,
        "the names 0.0.75 had"
    );
    assert_all_known(&names, &known, "Sue Paint");
}

#[test]
fn no_command_name_of_sue_writer_is_lost() {
    let known = source_literals("sue-writer");
    let names = ledger("ledger/writer-commands.txt");
    assert_eq!(
        golden_text("ledger/writer-commands.txt").lines().count(),
        122,
        "the names 0.0.75 had"
    );
    assert_all_known(&names, &known, "Sue Writer");
}

#[test]
fn no_settings_key_is_lost() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let known = every_setting();
    let keys = ledger("ledger/shell-keys.txt");
    assert_eq!(golden_text("ledger/shell-keys.txt").lines().count(), 9);
    assert!(keys.iter().all(|key| key.starts_with("keys.")));
    assert_all_known(&keys, &known, "The shell's keys");
    let all = ledger("ledger/rice-keys.txt");
    assert_eq!(golden_text("ledger/rice-keys.txt").lines().count(), 234);
    assert_all_known(&all, &known, "The settings");
}

/// Every committed fixture of this crate and its SHA-256. A fixture is
/// read, never rewritten (rule 12 of the 0.0.8 plan): a change of one byte
/// fails here before anything else is looked at.
const LEDGER: &[(&str, &str)] = &[
    (
        "animation.sue",
        "a307f76e07e78b837f2748fa72da1c721c840bada7cfb404f37f7b6921573495",
    ),
    (
        "animation.sue.txt",
        "6dc1e219aeb21f15c1addb8fa8d6a07302edc4142a8c3910b2c24110dee870bc",
    ),
    (
        "clip-picture.txt",
        "0bfd6bfbd4d5da424d638f212606b67b7ca91e3335a38b347d783a5349f8ae21",
    ),
    (
        "comic.sue",
        "617f27166e8cc06c99cade3d8c0e6e1c0702c2697e8439a92aba064c3484e528",
    ),
    (
        "comic.sue.txt",
        "fa1e23ce74ed1b6b18557a10203cbc17ebdf9c3d91e3ebc13f9f79cb5564b469",
    ),
    (
        "corrections.sue",
        "15a38ea1c178b1d29079a6b0f79d1ffda3b74524d584d6ddd82a1c9ee75898c0",
    ),
    (
        "corrections.sue.txt",
        "009e0740f9cb87222b99bfe768d9cb161f0ee4ea30d5e6043b5ae11877258309",
    ),
    (
        "ledger/paint-commands.txt",
        "9c31f905a2b6e6de20e0c8646c3e52e992c1f11f809fae5242e5994aac91df1f",
    ),
    (
        "ledger/rice-keys.txt",
        "c75a7cd06a7e2a6c26f84ada27affe8eae93f0d5ef2e791b218a9a86b0079e1d",
    ),
    (
        "ledger/shell-keys.txt",
        "38d7f1b032cc6a0032a8aafd41003072a029b3a60a717c01dadee3c3ba1a7334",
    ),
    (
        "ledger/writer-commands.txt",
        "337a5b1f122d831f00554e31a39baf63846613c92e07f0725a3d09ef95ce98e7",
    ),
    (
        "library.sue.txt",
        "44eb65f5dd601787b63c9ef66c39cefab73c740495af4867fa7f08256d950ad4",
    ),
    (
        "paint/auto_actions.txt",
        "356408eac2104990c825c2c57d1177d72aef7b47a918039373167ba162fc0630",
    ),
    (
        "paint/canvas_presets.txt",
        "d8bc75c99bddd5d9ede516a374259353cfef16f69f86a8c82e539bca52edca5a",
    ),
    (
        "paint/color_sets.txt",
        "e102c86a89138424c903bf75fa112960c85a2d8e9f6b5f6fde99821a05f3f5b1",
    ),
    (
        "paint/command_bar.txt",
        "a212eba3291870afc9db08f3e2db65bd9d564b8f25125ffd6987e0706eaa3cdc",
    ),
    (
        "paint/default_workspace.txt",
        "af86f06f0e92a0b7e78f2f63448824afe5d78c6a514acf8e6dc3f0e9ce2bedc8",
    ),
    (
        "paint/item_bank.txt",
        "84eb97b4dfadb81db0db3c7257e77204f23b00063f01c4813d93fd369c859be3",
    ),
    (
        "paint/layout.txt",
        "041973c2834dc3925d85cea4281ffbea7db0a9346b74feda22fee7f33340ad59",
    ),
    (
        "paint/library.sue",
        "5e9dd9c24f5fac8ef209f0a3e72176d47a2c6972f5ceb7165674fc2442c5eb29",
    ),
    (
        "paint/workspaces.txt",
        "e50ce2a8928dfa1d462fccfa2004f914783076dbaf41449748f35aa430507edd",
    ),
    (
        "picture.sue",
        "0097b176822a5d784a021f40c110ed185da7df99ccbe5d5b63a0cdd35e663093",
    ),
    (
        "picture.sue.txt",
        "7e2d141cc761961ac6308b86a189ac6f59bbc12a328b1fae1d7656cf31afe009",
    ),
    (
        "settings.suerice",
        "d10db1112be8b819a615701f7b24a541258845133fc2378bc20c68dc5094a8d1",
    ),
    (
        "settings.txt",
        "f05a0285a81d0b018c2e9c730763ba28c7003479210352b93f041c46baa8f023",
    ),
    (
        "tips.txt",
        "7b5dcc86786c5f06977bb6327cebeaf1ac5d625a679e921a8a9fb4876a7b0178",
    ),
    (
        "work.txt",
        "64d06034639c6d8c64afccc677222373526f8e05313dbb241213c5b5d94dd48f",
    ),
    (
        "work/autosave/1760000400-20276-1.sue",
        "2977b992cc10da8fab78135aed78b719e8fec0e9eb38ece0af9a0829209a5958",
    ),
    (
        "work/autosave/1760000400-20276-1.txt",
        "cd67b696fd287ddd20b63c51a2f9b23506cfe00e9d081354a7ea0ffafb48dd13",
    ),
    (
        "work/autosave/1760000500-20276-1.sue",
        "545f56552ba6a68cccf23ad03333334de5076c3933be03dff5128bfe162ec2b8",
    ),
    (
        "work/autosave/1760000500-20276-1.txt",
        "653560162b748ee5c2bc167d5fac4fae6000067309464c8f1cb81a358dc46753",
    ),
    (
        "work/labels.txt",
        "db634889817ffebe82d7b1c5b3fd9cccbf2e5d790d516ea8e7406570452bb6c0",
    ),
    (
        "work/recent.txt",
        "967ac1d627be3805ae837bb7f323d35465090e84cc2c7412a9188a68c97cb271",
    ),
    (
        "writer.sue",
        "37dadc3c431acf8ea9e2ecd2c2ac1f0c770c898a2d0d9705e4aea02149e7b8c4",
    ),
    (
        "writer.sue.txt",
        "f4f9db36ee76d64c2991b33e98efd68dac43f1874042b961698323b00eddfda2",
    ),
];

#[test]
fn every_golden_file_is_the_one_committed() {
    fn walk(dir: &Path, under: &str, found: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).expect("tests/golden is there") {
            let entry = entry.expect("a readable entry");
            let name = format!("{under}{}", entry.file_name().to_string_lossy());
            if entry.path().is_dir() {
                walk(&entry.path(), &format!("{name}/"), found);
            } else {
                found.push(name);
            }
        }
    }
    let mut found = Vec::new();
    walk(&golden_dir(), "", &mut found);
    found.sort();
    let mut listed: Vec<&str> = LEDGER.iter().map(|(name, _)| *name).collect();
    listed.sort_unstable();
    assert_eq!(found, listed, "tests/golden and the ledger hold the same");
    for (name, sum) in LEDGER {
        let found = sue_platform::update::sha256_of(&golden_dir().join(name))
            .unwrap_or_else(|error| panic!("cannot read {name}: {error}"));
        assert_eq!(
            found, *sum,
            "{name} changed: golden files are read, never rewritten"
        );
    }
}

// ----- The generators --------------------------------------------------------------
//
// Run once at 823b060, each with
// `cargo test -p sue-paint --lib -- --ignored golden::generate`.
// What they wrote to the temporary folder is what is committed. They are
// never run again: a golden file that no longer loads is a bug in the
// reader.

/// Where a generator writes: a folder under the system's temporary one,
/// never `tests/golden`.
fn out_dir() -> PathBuf {
    let dir = std::env::temp_dir().join("sue-golden").join("sue-paint");
    std::fs::create_dir_all(&dir).expect("the temporary folder can be made");
    dir
}

fn write_out(name: &str, bytes: &[u8]) {
    let path = out_dir().join(name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("the folder can be made");
    }
    std::fs::write(&path, bytes).expect("the file can be written");
    println!("wrote {} ({} bytes)", path.display(), bytes.len());
}

/// Writes `arena` as a project file with a chunk no build knows, and its
/// description beside it.
fn write_sue(name: &str, arena: &Arena) {
    let mut file = SueFile::new();
    file.set(Chunk {
        tag: UNKNOWN,
        version: 3,
        payload: b"kept as it is".to_vec(),
    });
    file.set_arena(arena);
    let bytes = file.to_bytes();
    let read = SueFile::from_bytes(&bytes).expect("what was written reads");
    write_out(name, &bytes);
    write_out(&format!("{name}.txt"), describe(name, &read).as_bytes());
}

fn apply(arena: &mut Arena, mut edit: impl ReversibleTransaction<Arena>) {
    edit.apply(arena).expect("the edit applies");
}

/// The built-in sub tool `name` of `tool`.
fn sub_tool(tool: Tool, name: &str) -> crate::tools::SubTool {
    tool.sub_tools()
        .into_iter()
        .find(|sub| sub.name == name)
        .unwrap_or_else(|| panic!("no sub tool {name}"))
}

/// One stroke of a pen, as the canvas makes it.
struct Pen {
    tip: Tip,
    color: [f32; 3],
    copies: Vec<Copy2D>,
    finish: Finish,
    /// How long the pen was down, in seconds.
    held: f32,
}

impl Pen {
    fn new(tip: Tip, color: [f32; 3]) -> Self {
        Self {
            tip,
            color,
            copies: Vec::new(),
            finish: Finish::default(),
            held: 0.5,
        }
    }
}

/// Draws `points` (place and pressure) on `layer` with `pen`, the way the
/// canvas does when the pen lifts: on a vector layer the line is kept
/// with its pixels.
fn draw(arena: &mut Arena, layer: EntityId, pen: &Pen, points: &[([f32; 2], f32)]) {
    let is_vector = vector::is_vector(arena, layer);
    let mut stroke = Stroke::new(layer, pen.tip, pen.color, false);
    stroke.set_sub_color([1.0, 1.0, 1.0]);
    if !pen.copies.is_empty() {
        stroke.set_copies(pen.copies.clone());
    }
    let raster = doc::raster_mut(arena, layer).expect("a layer with pixels");
    for (index, (at, pressure)) in points.iter().enumerate() {
        stroke.set_speed((index as f32 / points.len() as f32).min(1.0) * 0.5);
        stroke.to(raster, *at, *pressure, None);
    }
    let samples = stroke.samples().to_vec();
    stroke.set_held(pen.held);
    stroke.end(raster, pen.finish, None);
    let kept = is_vector.then(|| vector::Line::of(&stroke, samples, pen.finish));
    let raster = doc::raster(arena, layer).expect("a layer with pixels");
    let pixels = stroke.finish(raster, "Pen");
    assert!(!pixels.is_empty(), "the stroke painted something");
    if let Some(kept) = kept {
        let old = vector::geometry(arena, layer);
        let mut lines = vector::lines(&old);
        lines.push(kept);
        let _undo = vector::keep(arena, layer, old, &lines, Some(pixels), "Pen");
    }
}

/// A wave across a `side` wide canvas at height `y`, pressed harder in
/// the middle.
fn wave(side: f32, y: f32, swing: f32) -> Vec<([f32; 2], f32)> {
    (0..=32)
        .map(|step| {
            let t = step as f32 / 32.0;
            (
                [
                    side * (0.1 + 0.8 * t),
                    y + swing * (t * std::f32::consts::TAU).sin(),
                ],
                0.2 + 0.8 * (t * std::f32::consts::PI).sin(),
            )
        })
        .collect()
}

/// (a) A tiny picture: paper, a raster layer with a mask, a vector layer
/// with a round pen line, a decoration line, a dot, a flicked line and a
/// symmetry stroke, layer and picture properties, and an unknown chunk.
#[test]
#[ignore = "generator: the golden files are committed and never rewritten"]
fn generate_the_picture() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    const SIDE: u32 = 64;
    let side = SIDE as f32;
    let mut arena = Arena::new();
    let (edit, picture) = doc::create(&NewPicture {
        name: "Golden picture".to_owned(),
        width: SIDE,
        height: SIDE,
        dpi: 350.0,
        paper: Some(Color::srgb8(250, 248, 240)),
        timelapse: false,
    })
    .expect("a picture of this size");
    apply(&mut arena, edit);
    let raster = doc::layers(&arena, picture).last().expect("one layer").id;

    // The raster layer: an airbrush cloud under a pencil line, half shown
    // by its mask, multiplied at three quarters.
    let soft = Pen::new(sub_tool(Tool::Airbrush, "Soft").tip, [0.2, 0.5, 0.9]);
    draw(&mut arena, raster, &soft, &wave(side, 24.0, 6.0));
    let pencil = Pen::new(sub_tool(Tool::Pencil, "Pencil").tip, [0.1, 0.1, 0.1]);
    draw(&mut arena, raster, &pencil, &wave(side, 40.0, -5.0));
    let (mask, _) = doc::new_mask(&arena, raster, |x, _| if x < 32 { 1.0 } else { 0.25 })
        .expect("a raster layer takes a mask");
    apply(&mut arena, mask);
    apply(&mut arena, doc::set(raster, picture::OPACITY, 0.75));
    apply(
        &mut arena,
        doc::set(
            raster,
            picture::BLEND,
            sue_model::BlendMode::Multiply.name(),
        ),
    );
    apply(&mut arena, doc::set(raster, picture::ALPHA_LOCKED, true));

    // The vector layer and its five lines.
    let (spawn, lines) =
        doc::add_vector_layer(&arena, picture, Some(raster), "Vector lines".to_owned())
            .expect("a vector layer");
    apply(&mut arena, spawn);
    let g_pen = sub_tool(Tool::Pen, "G-pen").tip;
    draw(
        &mut arena,
        lines,
        &Pen::new(g_pen, [0.05, 0.05, 0.08]),
        &wave(side, 12.0, 4.0),
    );
    let stars = sub_tool(Tool::Decoration, "Stars").tip;
    draw(
        &mut arena,
        lines,
        &Pen::new(stars, [0.9, 0.7, 0.1]),
        &wave(side, 52.0, 3.0),
    );
    // A dot: the pen down and up in one place.
    let dot = Pen {
        held: 0.1,
        ..Pen::new(g_pen, [0.8, 0.1, 0.1])
    };
    draw(&mut arena, lines, &dot, &[([50.0, 20.0], 0.8)]);
    // A flicked line: the pen left the tablet still moving.
    let flicked = Pen {
        finish: Finish {
            correction: 3,
            flick: Some([420.0, -260.0]),
        },
        ..Pen::new(g_pen, [0.1, 0.4, 0.1])
    };
    let rise: Vec<([f32; 2], f32)> = (0..=12)
        .map(|step| {
            let t = step as f32 / 12.0;
            ([8.0 + 20.0 * t, 60.0 - 14.0 * t], 0.9 - 0.5 * t)
        })
        .collect();
    draw(&mut arena, lines, &flicked, &rise);
    // A symmetry stroke: the ruler is on the picture, the stroke paints
    // through its copies.
    let center = [32.0, 32.0];
    let angle = std::f32::consts::FRAC_PI_2;
    apply(
        &mut arena,
        crate::rulers::edit(
            picture,
            crate::rulers::SYMMETRY,
            Some(format!("{},{},{angle}", center[0], center[1])),
            "Symmetrical ruler",
        ),
    );
    let mirrored = Pen {
        copies: crate::brush::symmetry(center, angle, 2, true),
        ..Pen::new(g_pen, [0.3, 0.1, 0.5])
    };
    let arc: Vec<([f32; 2], f32)> = (0..=10)
        .map(|step| {
            let t = step as f32 / 10.0;
            ([36.0 + 10.0 * t, 28.0 - 6.0 * t * t], 0.6)
        })
        .collect();
    draw(&mut arena, lines, &mirrored, &arc);
    apply(&mut arena, doc::set(lines, picture::LOCKED, true));

    // A hidden draft layer, moved, and a property from another feature.
    let (spawn, draft) =
        doc::add_raster_layer(&arena, picture, Some(lines), "Draft".to_owned(), None)
            .expect("a raster layer");
    apply(&mut arena, spawn);
    draw(
        &mut arena,
        draft,
        &Pen::new(g_pen, [0.0, 0.0, 1.0]),
        &wave(side, 32.0, 10.0),
    );
    apply(&mut arena, doc::set(draft, picture::VISIBLE, false));
    apply(&mut arena, doc::set(draft, picture::DRAFT, true));
    apply(&mut arena, doc::set(draft, picture::CLIP, true));
    apply(&mut arena, doc::set(draft, picture::X, 3_i64));
    apply(&mut arena, doc::set(draft, picture::Y, -2_i64));
    apply(
        &mut arena,
        SetProperty::new(picture, "golden.note", "A property Sue Paint does not know"),
    );
    // Something that is no layer at all, beside the picture.
    apply(
        &mut arena,
        SpawnEntity::new(Entity::empty("Loose note", sue_model::class::NOTE)),
    );
    write_sue("picture.sue", &arena);
}

/// A layer drawn by a tool of its own (panels, balloons), on top, as the
/// studio spawns it.
fn spawn_drawn(
    arena: &mut Arena,
    picture: EntityId,
    name: &str,
    class: &str,
    pixels: Vec<u8>,
    properties: Vec<(&str, Value)>,
) -> EntityId {
    let [width, height] = picture::size(arena.get_by_id(picture).expect("the picture"))
        .expect("a picture with a size");
    let raster =
        sue_core::RasterTile2D::from_bytes(width, height, sue_core::PixelFormat::Rgba8, pixels)
            .expect("pixels of the picture's size");
    let mut layer = Entity::new(name, raster).with_class(class);
    for (key, value) in properties {
        layer = layer.with_property(key, value);
    }
    let id = layer.id();
    let order = arena.relations().next_order(picture, sue_core::CHILD);
    apply(
        arena,
        SpawnEntity::new(layer)
            .with_relation(Relation::new(picture, sue_core::CHILD, id).ordered(order)),
    );
    id
}

/// (f) An animation: an animation folder of three cels, a timeline with a
/// cel shown twice and a blank, a 2D camera with two keys, a sound, onion
/// skin settings, and both light tables.
#[test]
#[ignore = "generator: the golden files are committed and never rewritten"]
fn generate_the_animation() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    const SIDE: u32 = 32;
    let side = SIDE as f32;
    let mut arena = Arena::new();
    let (edit, picture) = doc::create(&NewPicture {
        name: "Golden animation".to_owned(),
        width: SIDE,
        height: SIDE,
        dpi: 144.0,
        paper: Some(Color::srgb8(255, 255, 255)),
        timelapse: true,
    })
    .expect("a picture of this size");
    apply(&mut arena, edit);
    let background = doc::layers(&arena, picture).last().expect("one layer").id;
    let pen = Tip {
        size: 3.0,
        ..sub_tool(Tool::Pen, "G-pen").tip
    };
    draw(
        &mut arena,
        background,
        &Pen::new(pen, [0.6, 0.6, 0.6]),
        &wave(side, 26.0, 2.0),
    );

    // Animation, New Animation Layer, Timeline: 12 frames at 8 a second.
    let edit = crate::anim::new_timeline(&arena, picture, 8, 12);
    apply(&mut arena, edit);
    let folder = doc::layers(&arena, picture)
        .into_iter()
        .find(|info| crate::anim::is_animation_folder(&arena, info.id))
        .expect("the animation folder")
        .id;
    let mut cels = Vec::new();
    for (index, frame) in [0_u32, 3, 6].into_iter().enumerate() {
        let (edit, cel) = crate::anim::new_cel(&arena, picture, folder, frame)
            .expect("a cel in the animation folder");
        apply(&mut arena, edit);
        let y = 8.0 + 6.0 * index as f32;
        draw(
            &mut arena,
            cel,
            &Pen::new(pen, [0.1, 0.1, 0.1]),
            &wave(side, y, 3.0),
        );
        cels.push(cel);
    }
    // The first cel again on frame 8, and nothing from frame 10.
    let sequence = crate::anim::timeline(&arena, picture)
        .expect("the timeline")
        .clone();
    let track = crate::anim::track_of(&sequence, folder).expect("the folder's track");
    let sequence = crate::anim::assign(&sequence, track, 8, Some(cels[0]), picture, 12);
    let sequence = crate::anim::assign(&sequence, track, 10, None, picture, 12);
    apply(
        &mut arena,
        crate::anim::set(picture, sequence, "Specify cels"),
    );

    // A 2D camera folder around the animation folder, keyed twice.
    let (edit, camera) = crate::camera::new_camera(&arena, picture, Some(folder));
    apply(&mut arena, edit);
    let rate = crate::anim::timeline(&arena, picture)
        .expect("the timeline")
        .rate;
    for (frame, stand) in [
        (
            0,
            crate::camera::Camera {
                center: [16.0, 16.0],
                scale: 1.0,
                angle: 0.0,
            },
        ),
        (
            11,
            crate::camera::Camera {
                center: [20.0, 12.0],
                scale: 1.5,
                angle: 0.25,
            },
        ),
    ] {
        let edit = crate::camera::key(&arena, camera, crate::camera::time_of(frame, rate), stand);
        apply(&mut arena, edit);
    }

    // File, Import, Audio: a short tone from frame 2.
    let tone: Vec<f32> = (0..480)
        .map(|sample| (sample as f32 * 0.2).sin() * 0.5)
        .collect();
    let pcm = sue_core::PCMFloat1D::new(8000, 1, tone).expect("a mono sound");
    let sequence = crate::anim::timeline(&arena, picture)
        .expect("the timeline")
        .clone();
    let (edit, _) = crate::anim::add_audio(picture, &sequence, "Tone", pcm, 2);
    apply(&mut arena, edit);

    // Onion Skin Settings, changed from the defaults.
    let onion = crate::anim::Onion {
        before: 2,
        after: 3,
        mode: 1,
        before_color: [255, 0, 128],
        after_color: [0, 128, 255],
        opacity: Some((0.6, 0.2)),
    };
    apply(
        &mut arena,
        SetProperty::new(picture, crate::anim::ONION, onion.to_text()),
    );

    // A reference on the second cel's light table, moved and turned, and
    // the background on the general one.
    let (spawn, reference) = doc::add_raster_layer(
        &arena,
        picture,
        Some(background),
        "Reference".to_owned(),
        None,
    )
    .expect("a raster layer");
    apply(&mut arena, spawn);
    draw(
        &mut arena,
        reference,
        &Pen::new(pen, [0.9, 0.2, 0.2]),
        &wave(side, 16.0, 8.0),
    );
    apply(&mut arena, crate::anim::register(cels[1], reference));
    apply(
        &mut arena,
        SetProperty::new(
            reference,
            crate::anim::PLACE,
            crate::anim::place_text([4.0, -2.0, 15.0]),
        ),
    );
    apply(
        &mut arena,
        crate::anim::register_general(picture, background),
    );
    write_sue("animation.sue", &arena);
}

/// (g) A comic: a story of two pages; on the first, nested folders (one
/// closed), a correction layer, a text layer with ruby, frame borders,
/// balloons with a tail, every kind of ruler, layer effects (border, tone,
/// layer color), two layer comps and a 3D layer.
#[test]
#[ignore = "generator: the golden files are committed and never rewritten"]
fn generate_the_comic() {
    use crate::{effects, manga, rulers, story};
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    const SIDE: u32 = 64;
    let side = SIDE as f32;
    let mut arena = Arena::new();
    let spec = NewPicture {
        name: "Page".to_owned(),
        width: SIDE,
        height: SIDE,
        dpi: 600.0,
        paper: Some(Color::srgb8(255, 255, 255)),
        timelapse: false,
    };
    let (edit, story_id, pages) =
        story::create(&spec, 2, "Golden story", true).expect("pages of this size");
    apply(&mut arena, edit);
    let page = pages[0];
    // Story, Change Story Information and Change Page Settings.
    for (key, value) in [
        (story::AUTHOR, Value::from("A. Artist")),
        (story::FIRST_NUMBER, Value::from(3_i64)),
        (story::START, Value::from("paired")),
        (story::FOLIO, Value::from(true)),
        (story::FOLIO_SIZE, Value::from(7.0_f64)),
        (story::FOLIO_PLACE, Value::from("center")),
    ] {
        apply(&mut arena, SetProperty::new(story_id, key, value));
    }
    let edit = story::page_settings(&arena, story_id, [SIDE, SIDE], 600.0, 3.0, 5.0)
        .expect("the pages keep their size");
    apply(&mut arena, edit);

    let ink = doc::layers(&arena, page).last().expect("one layer").id;
    let pen = Tip {
        size: 4.0,
        ..sub_tool(Tool::Pen, "G-pen").tip
    };
    draw(
        &mut arena,
        ink,
        &Pen::new(pen, [0.0, 0.0, 0.0]),
        &wave(side, 30.0, 8.0),
    );
    // Layer Property effects on the ink: border, tone and layer color.
    for (key, value) in [
        (effects::BORDER, Value::from(true)),
        (effects::BORDER_WIDTH, Value::from(2.0_f64)),
        (
            effects::BORDER_COLOR,
            Value::from(Color::srgb8(255, 255, 0)),
        ),
        (effects::TONE, Value::from(true)),
        (effects::TONE_LINES, Value::from(30.0_f64)),
        (effects::TONE_ANGLE, Value::from(15.0_f64)),
        (effects::COLOR, Value::from(true)),
        (effects::COLOR_VALUE, Value::from(Color::srgb8(200, 30, 30))),
        (effects::EXPRESSION, Value::from(1_i64)),
        (effects::THRESHOLD, Value::from(0.4_f64)),
    ] {
        apply(&mut arena, doc::set(ink, key, value));
    }

    // Folders: the ink in an inner folder, closed, inside an outer one.
    let (edit, outer) = doc::new_folder(&arena, page, Some(ink), true);
    apply(&mut arena, edit);
    let (edit, inner) = doc::new_folder(&arena, page, Some(ink), true);
    apply(&mut arena, edit);
    apply(&mut arena, doc::set(inner, doc::OPEN, false));
    apply(&mut arena, doc::set(outer, picture::OPACITY, 0.5));
    let (spawn, color) = doc::add_raster_layer(&arena, page, Some(ink), "Color".to_owned(), None)
        .expect("a raster layer");
    apply(&mut arena, spawn);
    draw(
        &mut arena,
        color,
        &Pen::new(sub_tool(Tool::Airbrush, "Soft").tip, [0.9, 0.5, 0.2]),
        &wave(side, 36.0, 4.0),
    );
    let edit = doc::set_folder(&arena, color, Some(outer)).expect("the layer moves in");
    apply(&mut arena, edit);

    // Layer, New Correction Layer, Brightness/Contrast.
    let filter = crate::filters::Filter::BrightnessContrast {
        brightness: 0.2,
        contrast: -0.1,
    };
    let (edit, _) = doc::new_correction(
        &arena,
        page,
        Some(outer),
        &filter.label(),
        &filter.to_text().expect("a correction that can be kept"),
    );
    apply(&mut arena, edit);

    // Frame borders: two panels in the first row, one in the second.
    let panels = manga::layout(&[2, 1], side, side, 4.0, 3.0, 3.0);
    let pixels = manga::render_frames(&panels, SIDE, SIDE, 1.5, [255, 255, 255]);
    spawn_drawn(
        &mut arena,
        page,
        "Frame",
        manga::FRAME,
        pixels,
        vec![
            (manga::PANELS, Value::from(manga::encode_panels(&panels))),
            (manga::BORDER, Value::from(1.5_f64)),
            (manga::GUTTER, Value::from(3.0_f64)),
        ],
    );

    // Balloons: an ellipse with a tail, a cloud with thought bubbles, and
    // one drawn by hand.
    let cloud = [36.0, 6.0, 58.0, 22.0];
    let mut shapes = vec![
        manga::Shape::Ellipse([8.0, 6.0, 30.0, 22.0]),
        manga::tail([19.0, 14.0], [26.0, 30.0], 6.0),
        manga::Shape::Cloud(cloud),
        manga::Shape::Drawn(vec![[40.0, 40.0], [56.0, 42.0], [50.0, 56.0], [38.0, 52.0]]),
    ];
    shapes.extend(manga::bubbles(cloud, [44.0, 34.0]));
    spawn_drawn(
        &mut arena,
        page,
        "Balloon",
        manga::BALLOON,
        manga::render_balloons(&shapes, SIDE, SIDE, 1.0),
        vec![
            (manga::SHAPES, Value::from(manga::encode_shapes(&shapes))),
            (manga::LINE, Value::from(1.0_f64)),
        ],
    );

    // Words with a reading over them, as the Text tool keeps them.
    let mut words = crate::lettering::Lettering::new([10.0, 9.0], [20, 20, 60]);
    words.content = "Hi\nyou".to_owned();
    words.size = 7.0;
    words.center = true;
    words.tracking = 0.05;
    words.leading = 1.1;
    words.justify = sue_type::raster::Justify::Middle;
    words.set_ruby(0..2, "hai");
    let text = crate::lettering::text_layer(&words, SIDE, SIDE).expect("the words draw");
    let text_id = text.id();
    let order = arena.relations().next_order(page, sue_core::CHILD);
    apply(
        &mut arena,
        SpawnEntity::new(text)
            .with_relation(Relation::new(page, sue_core::CHILD, text_id).ordered(order)),
    );

    // Every kind of ruler, on the page.
    let all = rulers::Rulers {
        linear: Some([[4.0, 60.0], [60.0, 50.0]]),
        special: Some(rulers::Special::Radial([32.0, 20.0])),
        perspective: Some(rulers::Perspective::new(2, [SIDE, SIDE])),
        symmetry: Some(([32.0, 32.0], 0.5)),
        paths: vec![
            rulers::Path::curve(&[[6.0, 6.0], [20.0, 12.0], [30.0, 6.0]]),
            rulers::Path::figure([40.0, 40.0], [56.0, 52.0], true),
        ],
    };
    for key in rulers::ALL {
        apply(&mut arena, rulers::edit(page, key, all.text(key), "Ruler"));
    }

    // Layer, New Layer, 3D: the first of Sue Paint's shapes, turned.
    let shape = crate::three::SHAPES[0];
    let model = Entity::empty(shape, sue_model::class::MODEL).with_ext(crate::three::shape(0));
    let (edit, solid) =
        crate::three::new_layer(&arena, page, None, shape, model).expect("a 3D layer");
    apply(&mut arena, edit);
    let view = crate::three::View3D {
        yaw: 40.0,
        pitch: 25.0,
        style: 2,
        line_width: 1.5,
        ..crate::three::View3D::of(arena.get_by_id(solid).expect("the 3D layer"))
    };
    let edit = crate::three::set_view(&arena, solid, &view, "Camera").expect("the view draws");
    apply(&mut arena, edit);

    // Two layer comps: everything, and the page with its balloons faint.
    let everything = crate::comps::capture(&arena, page, "Everything");
    let mut faint = crate::comps::capture(&arena, page, "Faint balloons");
    for state in &mut faint.states {
        let balloon = arena
            .get_by_id(state.layer)
            .is_ok_and(|entity| entity.class() == manga::BALLOON);
        if balloon {
            state.visible = false;
            state.opacity = 0.5;
            state.blend = sue_model::BlendMode::Multiply.name().to_owned();
        }
    }
    apply(
        &mut arena,
        crate::comps::store(page, &[everything, faint], "New layer comp"),
    );

    // The second page: a pencil line.
    let second = doc::layers(&arena, pages[1]).last().expect("one layer").id;
    draw(
        &mut arena,
        second,
        &Pen::new(sub_tool(Tool::Pencil, "Pencil").tip, [0.2, 0.2, 0.2]),
        &wave(side, 20.0, 5.0),
    );
    write_sue("comic.sue", &arena);
}

/// The first row of `picture` as it draws: one color for each pixel.
fn first_row(arena: &Arena, picture: EntityId) -> Vec<[u8; 3]> {
    let flat = crate::composite::render(
        arena,
        picture,
        [0, 0, RAMP, 1],
        crate::composite::RenderOptions::default(),
    );
    flat.as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect()
}

/// How wide the pictures of `corrections.sue` are: one step of gray for
/// each pixel across.
const RAMP: u32 = 16;

/// (l) Two pictures, each a ramp from black to white under one correction
/// layer: Level Correction on the first, Gradient Map on the second. The
/// stored text of both grows in 0.0.8; these are the short ones.
#[test]
#[ignore = "generator: the golden files are committed and never rewritten"]
fn generate_the_corrections() {
    use crate::filters::Filter;
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut arena = Arena::new();
    for (name, filter) in [
        (
            "Golden levels",
            Filter::Levels {
                black: 0.1,
                white: 0.8,
                gamma: 1.5,
                out_black: 0.05,
                out_white: 0.9,
            },
        ),
        (
            "Golden gradient map",
            Filter::GradientMap {
                from: [32, 30, 80],
                to: [240, 230, 200],
            },
        ),
    ] {
        let (edit, picture) = doc::create(&NewPicture {
            name: name.to_owned(),
            width: RAMP,
            height: RAMP,
            dpi: 72.0,
            paper: Some(Color::srgb8(255, 255, 255)),
            timelapse: false,
        })
        .expect("a picture of this size");
        apply(&mut arena, edit);
        let ramp = doc::layers(&arena, picture).last().expect("one layer").id;
        let bytes = doc::raster_mut(&mut arena, ramp)
            .and_then(|raster| raster.bytes_mut())
            .expect("a layer with 8-bit pixels");
        for (index, pixel) in bytes.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let gray = (index as u32 % RAMP * 17) as u8;
            *pixel = [gray, gray, gray, 255];
        }
        let plain = first_row(&arena, picture);
        let (edit, _) = doc::new_correction(
            &arena,
            picture,
            Some(ramp),
            &filter.label(),
            &filter.to_text().expect("a correction that can be kept"),
        );
        apply(&mut arena, edit);
        let corrected = first_row(&arena, picture);
        assert_ne!(plain, corrected, "the correction changes the picture");
        println!("{name} draws its first row as {corrected:?}");
    }
    write_sue("corrections.sue", &arena);
}

/// (c) The brush of every built-in sub tool, as the text a vector line
/// and the Item Bank keep it in: tool, sub tool and tip, tab separated.
#[test]
#[ignore = "generator: the golden files are committed and never rewritten"]
fn generate_the_tips() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut out = String::new();
    for tool in Tool::ALL {
        for sub in tool.sub_tools() {
            let text = sub.tip.to_text();
            assert_eq!(tip_words(&sub.tip), text, "the copy of to_text is exact");
            assert_eq!(Tip::from_text(&text), sub.tip, "{} reads back", sub.name);
            let _ = writeln!(out, "{}\t{}\t{text}", tool.label(), sub.name);
        }
    }
    write_out("tips.txt", out.as_bytes());
}

/// (d) and (j) Sue Paint's data folder as the studio writes it, in
/// English: the Item Bank with a sub tool of the artist's own, the palette
/// layout, two workspaces, the default workspace, the command bar, the
/// auto actions, the canvas presets, the color sets and the materials
/// library.
#[test]
#[ignore = "generator: the golden files are committed and never rewritten"]
fn generate_the_paint_folder() {
    use crate::dock::{Palette, Side};
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let folder = out_dir().join("paint");
    let _ = std::fs::remove_dir_all(&folder);
    let mut paint = SuePaint::default();

    // Window, Auto Action: Sue's two and one recorded.
    paint.auto.sets = crate::extras::AutoActions::builtin();
    paint.auto.loaded = true;
    paint.auto.sets.push(crate::extras::AutoAction {
        name: "Clean line art".to_owned(),
        steps: vec![
            "select.all".to_owned(),
            "filter.invert".to_owned(),
            "layer.merge_down".to_owned(),
            "view.fit".to_owned(),
        ],
    });
    // Window, Item Bank: a G-pen made one's own, and a decoration brush.
    let mut pen = sub_tool(Tool::Pen, "G-pen");
    pen.name = "My inking pen".to_owned();
    pen.stabilization = 12;
    pen.tip.size = 7.5;
    pen.tip.taper_end = 35.0;
    pen.tip.size_by = crate::brush::Dynamics::pressure(0.2);
    paint.item_bank = vec![
        crate::extras::Item::of(&pen),
        crate::extras::Item::of(&sub_tool(Tool::Decoration, "Chain")),
    ];
    // File, New: two presets registered.
    paint.canvas_presets = vec![
        crate::dialogs::CanvasPreset {
            name: "Webtoon strip".to_owned(),
            width: 800,
            height: 12_800,
            dpi: 144.0,
            paper: true,
        },
        crate::dialogs::CanvasPreset {
            name: "Sticker".to_owned(),
            width: 370,
            height: 320,
            dpi: 72.0,
            paper: false,
        },
    ];
    // The command bar: two buttons hidden, two added.
    paint.bar_hidden.insert("file.save".to_owned());
    paint.bar_hidden.insert("edit.redo".to_owned());
    paint.bar_extra.push("view.flip_h".to_owned());
    paint.bar_extra.push("layer.new".to_owned());

    // Window, Workspace, Register: the palettes as they start, then with
    // the navigator floating and the color set folded away.
    let start = paint.workspace_now("As it started");
    paint
        .dock
        .float(Palette::Navigator, egui::pos2(420.0, 180.0));
    paint.dock.iconize(Palette::ColorSet);
    let inking = paint.workspace_now("Inking");
    paint.workspaces = vec![start, inking];
    // A color set of one's own beside Sue's.
    paint.color_sets.add_set(
        "My inks",
        vec![[20, 20, 28], [120, 30, 40], [240, 236, 220]],
    );
    with_context(&folder, |cx| {
        paint.save_extras(cx);
        // Window, Workspace, Use Current Workspace as Default.
        paint.set_default_workspace(cx);
        paint.save_color_sets(cx);
    });
    // The palettes moved again since: what `layout.txt` remembers, a line
    // each, as the studio writes it once the layout has settled.
    paint.dock.set_column_width(Side::Left, 1, 284.0);
    paint.dock.float(Palette::Layer, egui::pos2(96.5, 64.25));
    paint.panels.timeline = false;
    paint.panels.brush_size = false;
    paint.menus_hidden = true;
    let layout: String = paint
        .workspace_now("")
        .lines
        .iter()
        .map(|line| format!("{line}\n"))
        .collect();
    std::fs::write(folder.join("layout.txt"), layout).expect("the layout can be written");

    // The materials library: Sue Paint's own materials, two favorites and
    // one used, kept in the materials location (the data folder).
    let kinds = crate::materials::MaterialKinds::builtin();
    let mut library = crate::materials::Library::scan(&kinds, Some(&folder));
    let entries = library.entries();
    assert!(entries.len() >= 4, "Sue Paint ships materials");
    library
        .set_favorite(entries[0].id, true)
        .expect("the library can be written");
    library
        .set_favorite(entries[3].id, true)
        .expect("the library can be written");
    library
        .used(entries[1].id, 1_760_000_000)
        .expect("the library can be written");
    let file = SueFile::from_bytes(
        &std::fs::read(folder.join("library.sue")).expect("the library was written"),
    )
    .expect("the library reads");
    write_out(
        "library.sue.txt",
        describe("paint/library.sue", &file).as_bytes(),
    );
    for entry in std::fs::read_dir(&folder).expect("the folder is there") {
        let entry = entry.expect("a readable entry");
        println!(
            "wrote {} ({} bytes)",
            entry.path().display(),
            entry.metadata().map_or(0, |metadata| metadata.len())
        );
    }
}

/// A tiny picture for the files that only need something to keep.
fn tiny_picture(name: &str) -> Arena {
    let mut arena = Arena::new();
    let (edit, picture) = doc::create(&NewPicture {
        name: name.to_owned(),
        width: 16,
        height: 16,
        dpi: 72.0,
        paper: Some(Color::srgb8(255, 255, 255)),
        timelapse: false,
    })
    .expect("a picture of this size");
    apply(&mut arena, edit);
    let layer = doc::layers(&arena, picture).last().expect("one layer").id;
    let pen = Tip {
        size: 2.0,
        ..sub_tool(Tool::Pen, "G-pen").tip
    };
    draw(
        &mut arena,
        layer,
        &Pen::new(pen, [0.1, 0.1, 0.1]),
        &wave(16.0, 8.0, 3.0),
    );
    arena
}

/// (i) The work Sue remembers and keeps, as `sue-wm` writes it: the
/// recent files, the labels and favorites, and two autosaves (one of a
/// saved project, one never saved) with their notes.
#[test]
#[ignore = "generator: the golden files are committed and never rewritten"]
fn generate_the_work_folder() {
    use sue_wm::{AppletId, LabelEdit, WorkList};
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let dir = out_dir().join("work");
    let _ = std::fs::remove_dir_all(&dir);
    let paint = AppletId::new(crate::ID).expect("Sue Paint's id");
    let writer = AppletId::new(sue_writer::ID).expect("Sue Writer's id");
    // Paths as a Windows build writes them; none of them needs to exist.
    let novel = PathBuf::from(r"C:\Users\artist\Documents\The Lighthouse Keeper.sue");
    let cat = PathBuf::from(r"C:\Users\artist\Pictures\cat.clip");
    let scan = PathBuf::from(r"D:\Scans\page 01.png");

    let mut list = WorkList::open(dir.clone());
    list.remember(&novel, true, Some(writer.clone()), 1_760_000_000)
        .expect("the list can be written");
    list.remember(&cat, false, Some(paint.clone()), 1_760_000_100)
        .expect("the list can be written");
    list.remember(&scan, false, None, 1_760_000_200)
        .expect("the list can be written");
    list.remember(&novel, false, None, 1_760_000_300)
        .expect("the list can be written");
    for edit in [
        LabelEdit::Add("Clients".to_owned()),
        LabelEdit::Mark {
            path: cat.clone(),
            label: list.labels()[0].clone(),
            on: true,
        },
        LabelEdit::Mark {
            path: novel.clone(),
            label: "Clients".to_owned(),
            on: true,
        },
        LabelEdit::Favorite {
            path: cat.clone(),
            on: true,
        },
    ] {
        list.edit_labels(&edit).expect("the labels can be written");
    }
    // Kept by a run that had the novel open, changed since it was saved.
    let mut kept = SueFile::new();
    kept.set_arena(&tiny_picture("Cover sketch"));
    list.keep(
        &kept.to_bytes(),
        Some(&novel),
        "The Lighthouse Keeper",
        Some(writer),
        1_760_000_400,
    )
    .expect("the work can be kept");
    // Kept by another run: an illustration nobody saved yet.
    let mut other = WorkList::open(dir.clone());
    let mut unsaved = SueFile::new();
    unsaved.set_arena(&tiny_picture("Illustration"));
    other
        .keep(
            &unsaved.to_bytes(),
            None,
            "Illustration",
            Some(paint),
            1_760_000_500,
        )
        .expect("the work can be kept");
    write_out("work.txt", describe_work(&dir).as_bytes());
    println!("wrote {}", dir.display());
}

/// (j) A settings file of format 6 as Sue writes it: the shipped file
/// with values changed in Settings, shortcuts changed, Studio Mode's keys
/// given other commands and the pen pressure curve redrawn. `settings.txt`
/// lists what differs from the shipped file.
#[test]
#[ignore = "generator: the golden files are committed and never rewritten"]
fn generate_the_settings() {
    use sue_rice::{RiceSchema, RiceValue};
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut rice = RiceSchema::parse(sue_rice::DEFAULT_RICE).expect("the shipped settings read");
    let shipped = rice.clone();
    assert_eq!(sue_rice::RICE_FORMAT, 6, "made by format 6");
    let text = |value: &str| RiceValue::Text(value.to_owned());
    for (path, value) in [
        ("general.confirm_on_exit", RiceValue::Bool(true)),
        ("general.autosave_seconds", RiceValue::Int(120)),
        ("launchpad.pinned", text("sue-paint")),
        ("display.ui_scale", RiceValue::Float(1.25)),
        ("motion.animation_speed", RiceValue::Float(1.5)),
        ("layout.padding", RiceValue::Int(12)),
        ("colors.void", text("#101010")),
        ("sue_paint.width", RiceValue::Int(2480)),
        ("sue_paint.height", RiceValue::Int(3508)),
        ("sue_paint.dpi", RiceValue::Int(350)),
        ("sue_paint.save_format", text("sue")),
        ("sue_paint.touch_mode", text("studio")),
        ("sue_paint.theme", text("paper")),
        ("sue_paint.remember_layout", RiceValue::Bool(false)),
        ("sue_paint.shift_line", RiceValue::Bool(false)),
        // Studio Mode's keys T1 and T7.
        ("sue_paint.custom_key_1", text("view.fit")),
        ("sue_paint.custom_key_7", text("layer.merge_down")),
        // File, Pen Pressure Settings: a harder hand.
        ("sue_paint.pen_pressure", text("0,0 0.3,0.6 0.7,0.9 1,1")),
        // Shortcuts: Sue's own, Sue Paint's and Sue Writer's.
        ("keys.undo", text("Ctrl+Z or Alt+Backspace")),
        ("keys.command_palette_alt", text("")),
        ("sue_paint_keys.file_save_as", text("Ctrl+Shift+S")),
        ("sue_paint_keys.tool_pen", text("P or N")),
        ("sue_paint_keys.view_fit", text("Ctrl+0 or F")),
        (
            "sue_writer_keys.format_bold",
            text("Ctrl+B or Ctrl+Shift+B"),
        ),
    ] {
        rice.set(path, value)
            .unwrap_or_else(|error| panic!("{path}: {error}"));
    }
    let written = rice.to_suerice();
    assert_eq!(sue_rice::format_of(&written), 6);
    let mut changed = String::new();
    for entry in rice.entries() {
        let value = entry.parameter().value();
        if shipped.get(entry.path()) != Some(value.clone()) {
            let _ = writeln!(changed, "{}\t{}", entry.path(), setting_words(&value));
        }
    }
    write_out("settings.suerice", written.as_bytes());
    write_out("settings.txt", changed.as_bytes());
}

/// (e) and (k) The ledgers: every command name of Sue Paint and of Sue
/// Writer, the shell's key settings, and every settings key.
#[test]
#[ignore = "generator: the golden files are committed and never rewritten"]
fn generate_the_ledgers() {
    use std::collections::BTreeSet;
    let _english = sue_lang::scope(sue_lang::ENGLISH);

    // Sue Paint: the menus (with the items Open Recent and Workspace add),
    // the shortcut table, and what `implemented` names in its source.
    let mut paint: BTreeSet<String> = BTreeSet::new();
    let mut menus = crate::menus::menus();
    crate::menus::set_workspaces(&mut menus, &[]);
    paint.extend(
        crate::menus::commands(&menus)
            .into_iter()
            .map(|(_, command)| command),
    );
    paint.extend(
        crate::menus::recent_items(&[])
            .iter()
            .flat_map(|item| item.commands())
            .map(str::to_owned),
    );
    paint.extend(
        crate::studio::shortcut_table()
            .iter()
            .map(|shortcut| shortcut.command.to_owned()),
    );
    let studio = std::fs::read_to_string(source_dir("sue-paint").join("studio.rs"))
        .expect("studio.rs is where it was at 823b060");
    let body = studio
        .split_once("fn implemented(command: &str) -> bool {")
        .and_then(|(_, rest)| rest.split_once("\n}"))
        .map(|(body, _)| body)
        .expect("implemented is where it was at 823b060");
    let mut pieces = body.split('"');
    let mut before = pieces.next().unwrap_or_default();
    while let (Some(name), Some(after)) = (pieces.next(), pieces.next()) {
        if before.trim_end().ends_with("starts_with(") {
            paint.insert(format!("{name}*"));
        } else {
            paint.insert(name.to_owned());
        }
        before = after;
    }
    println!("{} Sue Paint command names", paint.len());
    write_out("ledger/paint-commands.txt", lines_of(&paint).as_bytes());

    // Sue Writer: the names in its menus, the families its menus build
    // (`status.` and a status), and the names its studio matches on.
    let mut writer: BTreeSet<String> = BTreeSet::new();
    let dir = source_dir("sue-writer");
    let menus = std::fs::read_to_string(dir.join("menus.rs")).expect("menus.rs is there");
    let mut spaces: BTreeSet<String> = BTreeSet::new();
    for name in literals_of(&menus)
        .into_iter()
        .filter(|name| command_like(name))
    {
        spaces.insert(name.split('.').next().unwrap_or_default().to_owned());
        writer.insert(name);
    }
    for file in ["menus.rs", "ui.rs", "studio.rs"] {
        let text = std::fs::read_to_string(dir.join(file)).expect("the source is there");
        // `format!("status.{status}")`: a family of names.
        for built in text.split("format!(\"").skip(1) {
            let literal = built.split('"').next().unwrap_or_default();
            if let Some((prefix, _)) = literal.split_once('{')
                && let Some(family) = prefix.strip_suffix('.')
                && command_like(&format!("{family}.x"))
            {
                writer.insert(format!("{prefix}*"));
            }
        }
    }
    let studio = std::fs::read_to_string(dir.join("studio.rs")).expect("studio.rs is there");
    for line in studio.lines() {
        let line = line.trim_start();
        if line.starts_with('"') || line.starts_with("| \"") || line.starts_with('(') {
            for name in literals_of(line) {
                let space = name.split('.').next().unwrap_or_default();
                if command_like(&name) && spaces.contains(space) {
                    writer.insert(name);
                }
            }
        }
    }
    println!("{} Sue Writer command names", writer.len());
    write_out("ledger/writer-commands.txt", lines_of(&writer).as_bytes());

    // The shell's own key settings, and every settings key there is.
    let all = every_setting();
    let keys: BTreeSet<String> = all
        .iter()
        .filter(|path| path.starts_with("keys."))
        .cloned()
        .collect();
    println!("{} shell keys, {} settings keys", keys.len(), all.len());
    write_out("ledger/shell-keys.txt", lines_of(&keys).as_bytes());
    write_out("ledger/rice-keys.txt", lines_of(&all).as_bytes());
}

/// (h) A Sue Writer project: the novel template with two written scenes
/// (formatting, a footnote, a list, a table in the text, a comment with a
/// reply), synopses, labels, status, a target and a running head.
#[test]
#[ignore = "generator: the golden files are committed and never rewritten"]
fn generate_the_writer_project() {
    use sue_core::txn::{Batch, InsertText, SetExtension};
    use sue_model::manuscript::{self, role};
    use sue_model::mark_kind as kind;
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut arena = Arena::new();
    // The fonts a Windows machine has, named so the file does not depend
    // on the machine that made it.
    let fonts = sue_type::Fonts {
        body: "Georgia".to_owned(),
        heading: "Georgia".to_owned(),
        mono: "Consolas".to_owned(),
        size: 12.0,
    };
    let (batch, root) = sue_writer::create_built_in("novel", "The Lighthouse Keeper", &fonts)
        .expect("the novel template");
    apply(&mut arena, batch);
    for (key, value) in [
        (manuscript::AUTHOR, Value::from("A. Writer")),
        (
            "sue_writer.header",
            Value::from("\t\tWriter / {title} / {page}"),
        ),
        (manuscript::TARGET, Value::from(80_000_i64)),
        ("sue_writer.session_target", Value::from(750_i64)),
    ] {
        apply(&mut arena, SetProperty::new(root, key, value));
    }
    let draft = manuscript::folder(&arena, root, role::DRAFT).expect("the draft folder");
    let scenes: Vec<EntityId> = manuscript::children(&arena, draft)
        .iter()
        .filter_map(|chapter| manuscript::children(&arena, *chapter).first().copied())
        .collect();
    assert!(scenes.len() >= 2, "the novel template has two chapters");

    let log = sue_type::tables::table_of(
        &[
            vec!["Night".to_owned(), "Weather".to_owned()],
            vec!["Monday".to_owned(), "Fog".to_owned()],
        ],
        1,
    );
    let log_id = log.id();
    apply(&mut arena, SpawnEntity::new(log));
    let comment = sue_model::comments::comment(
        "A. Writer",
        "2026-10-29",
        "Plant the bell earlier, in chapter one?",
    );
    let comment_id = comment.id();
    let reply = sue_model::comments::comment("Editor", "2026-10-30", "Yes, in the storm.");
    let reply_id = reply.id();
    apply(
        &mut arena,
        Batch::new("Comment").then(SpawnEntity::new(comment)).then(
            SpawnEntity::new(reply).with_relation(
                Relation::new(comment_id, sue_model::comments::REPLY, reply_id)
                    .ordered(sue_core::OrderKey::sequence(None, 1)[0].clone()),
            ),
        ),
    );

    let object = sue_model::OBJECT.to_string();
    let first = [
        "The storm came in off the water a little after six.",
        "Mara had kept the light for eleven years. Tonight it made a new one.",
        "A lamp with oil for three nights",
        "Her father's logbook",
        object.as_str(),
        "Somewhere below, a bell rang once.",
    ];
    let second = ["By morning the sea had laid a man on the north beach, breathing."];
    for (scene, paragraphs, synopsis, status) in [
        (
            scenes[0],
            &first[..],
            "The storm, and a new sound.",
            "Revised Draft",
        ),
        (
            scenes[1],
            &second[..],
            "A stranger washes up.",
            "First Draft",
        ),
    ] {
        let mut text = String::new();
        for paragraph in paragraphs {
            text.push_str(paragraph);
            text.push('\n');
        }
        let mut marks = sue_model::TextMarks::new();
        let mut mark = |words: &str, kind: &str, value: Value| {
            if let Some(at) = text.find(words) {
                marks.set(at..at + words.len(), kind, Some(value));
            }
        };
        mark("new one", kind::ITALIC, Value::Bool(true));
        mark("breathing", kind::BOLD, Value::Bool(true));
        mark(
            "eleven years",
            kind::FOOTNOTE,
            Value::Text("Since the winter her father died.".to_owned()),
        );
        mark(&object, kind::EMBED, Value::Entity(log_id));
        mark("a bell rang once", kind::COMMENT, Value::Entity(comment_id));
        for item in ["A lamp", "Her father's"] {
            if let Some(at) = text.find(item) {
                marks.set_paragraphs(
                    &text,
                    at..at,
                    kind::LIST,
                    Some(Value::Text("bullet".to_owned())),
                );
            }
        }
        // As the editor records it: the text put in, then its marks.
        let handle = arena.handle_of(scene).expect("the scene is in the arena");
        let old = arena
            .get(handle)
            .ok()
            .and_then(|entity| entity.string_buffer())
            .map_or(0, |buffer| buffer.as_str().len());
        let mut write = Batch::new("Write");
        if old > 0 {
            write = write.then(sue_core::txn::DeleteText::new(handle, 0..old));
        }
        apply(
            &mut arena,
            write
                .then(InsertText::new(handle, 0, text))
                .then(SetExtension::set(scene, marks)),
        );
        for (key, value) in [
            (manuscript::SYNOPSIS, synopsis),
            (manuscript::LABEL, "Scene"),
            (manuscript::STATUS, status),
        ] {
            apply(&mut arena, SetProperty::new(scene, key, value));
        }
    }
    // A new scene left out of what is published, with notes and keywords,
    // as Project, New Document and the Inspector make it.
    let chapter = manuscript::parent(&arena, scenes[1]).expect("the scene's chapter");
    let last = arena
        .relations()
        .get(chapter, sue_core::CHILD, scenes[1])
        .expect("the scene is its chapter's child")
        .order;
    let extra = Entity::new(
        "Scene 2",
        sue_core::StringBuffer::from("An idea to keep.\n"),
    )
    .with_class(sue_model::class::DOCUMENT)
    .with_ext(sue_model::TextMarks::new());
    let extra_id = extra.id();
    apply(
        &mut arena,
        SpawnEntity::new(extra).with_relation(
            Relation::new(chapter, sue_core::CHILD, extra_id)
                .ordered(sue_core::OrderKey::between(Some(&last), None)),
        ),
    );
    for (key, value) in [
        (manuscript::INCLUDE, Value::from(false)),
        (
            manuscript::NOTES,
            Value::from("Cut unless the middle drags."),
        ),
        (manuscript::KEYWORDS, Value::from("storm, bell")),
    ] {
        apply(&mut arena, SetProperty::new(extra_id, key, value));
    }
    write_sue("writer.sue", &arena);
}

/// (b) The `.clip` Sue Paint 0.0.75 saves for a procedural picture, with
/// the picture's own project inside. The file goes to `sue-io`'s golden
/// folder; what Sue Paint reads back from it is described here.
#[test]
#[ignore = "generator: the golden files are committed and never rewritten"]
fn generate_the_clip() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    const SIDE: u32 = 64;
    let side = SIDE as f32;
    let mut arena = Arena::new();
    let (edit, picture) = doc::create(&NewPicture {
        name: "Golden clip".to_owned(),
        width: SIDE,
        height: SIDE,
        dpi: 300.0,
        paper: Some(Color::srgb8(255, 252, 245)),
        timelapse: false,
    })
    .expect("a picture of this size");
    apply(&mut arena, edit);
    let ink = doc::layers(&arena, picture).last().expect("one layer").id;
    let pen = sub_tool(Tool::Pen, "G-pen").tip;
    draw(
        &mut arena,
        ink,
        &Pen::new(pen, [0.05, 0.05, 0.05]),
        &wave(side, 20.0, 6.0),
    );
    // Color under the ink in a folder: multiplied, half shown, clipped.
    let (spawn, color) = doc::add_raster_layer(&arena, picture, None, "Color".to_owned(), None)
        .expect("a raster layer");
    apply(&mut arena, spawn);
    draw(
        &mut arena,
        color,
        &Pen::new(sub_tool(Tool::Airbrush, "Soft").tip, [0.9, 0.4, 0.3]),
        &wave(side, 36.0, 5.0),
    );
    apply(
        &mut arena,
        doc::set(color, picture::BLEND, sue_model::BlendMode::Multiply.name()),
    );
    apply(&mut arena, doc::set(color, picture::OPACITY, 0.5));
    apply(&mut arena, doc::set(color, picture::CLIP, true));
    let (edit, folder) = doc::new_folder(&arena, picture, Some(color), true);
    apply(&mut arena, edit);
    apply(&mut arena, crate::doc::set(folder, doc::OPEN, false));
    // A hidden layer, and vector lines Clip Studio sees as pixels only.
    let (spawn, hidden) = doc::add_raster_layer(&arena, picture, None, "Hidden".to_owned(), None)
        .expect("a raster layer");
    apply(&mut arena, spawn);
    draw(
        &mut arena,
        hidden,
        &Pen::new(pen, [0.0, 0.3, 0.8]),
        &wave(side, 50.0, 4.0),
    );
    apply(&mut arena, doc::set(hidden, picture::VISIBLE, false));
    let (spawn, lines) = doc::add_vector_layer(&arena, picture, None, "Vector lines".to_owned())
        .expect("a vector layer");
    apply(&mut arena, spawn);
    draw(
        &mut arena,
        lines,
        &Pen::new(pen, [0.5, 0.1, 0.1]),
        &wave(side, 8.0, 3.0),
    );
    let bytes = crate::clip_io::encode(&arena, picture).expect("the picture saves as .clip");
    let dir = std::env::temp_dir().join("sue-golden").join("sue-io");
    std::fs::create_dir_all(&dir).expect("the temporary folder can be made");
    std::fs::write(dir.join("picture.clip"), &bytes).expect("the file can be written");
    println!(
        "wrote {} ({} bytes)",
        dir.join("picture.clip").display(),
        bytes.len()
    );
    write_out("clip-picture.txt", describe_clip_picture(&bytes).as_bytes());
}

fn source_dir(krate: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(krate)
        .join("src")
}
