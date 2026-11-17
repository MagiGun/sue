//! Clip Studio Paint sub tools (`.sut`) to and from Sue's brushes
//! ([`BrushPreset`]).
//!
//! A `.sut` is an SQLite database: a `Manager` row, a `Node` row per sub
//! tool (its name, and the `Variant` rows of its settings now and as
//! first installed), the `Variant` rows (one column per setting) and
//! `MaterialFile` rows, each a tar archive of one material (a tip
//! picture or a paper) with its thumbnail. The layout and the meaning of
//! the columns were learnt from files made on this machine (Clip Studio
//! Paint's own tool database and sub tools it exported);
//! `docs/brush-files.md` lists what each column becomes.
//!
//! Import maps every setting the brush model and the paint behaviours
//! have a place for, and keeps the whole `Node` and `Variant` rows, the
//! file's `Manager` row and the material archives in the preset
//! ([`BrushPreset::extra`], keys `sut.*`). Export starts from those kept
//! rows and rewrites only what was changed in Sue, so a sub tool that
//! comes in and goes out again keeps every column Sue does not know,
//! byte for byte. A `.sut` Sue writes also carries the preset itself in
//! a table of its own (`SueBrush`), so a Sue brush comes back whole;
//! a setting changed in Clip Studio meanwhile still comes through.
//!
//! What a `.sut` cannot be made to say exactly:
//!
//! - Material pictures: a material's full picture is kept in a layer
//!   format of Clip Studio's own that Sue does not read, so the tip
//!   picture Sue uses is the largest one it can read, the material's
//!   thumbnail. The archive itself is kept and written back unchanged.
//! - Numbers whose meaning is not known (some blend, texture and dual
//!   brush modes) are kept, not mapped.

use std::collections::BTreeMap;

use sha2::{Digest, Sha256};
use sue_brush::paint::{
    Behaviours, DualBrush, Flip, PaperChoice, ParticleDirection, Spray, TextureChoice, TipBlend,
    TipOrder, WatercolorEdge, key,
};
use sue_brush::texture::{TextureMode as PaperMode, TextureSettings};
use sue_core::codec::{Reader, Writer};
use sue_model::brush::{
    AntiAlias, Binding, Combine, ContentHash, EngineKind, Mixing, Pick, PictureRef, Spacing,
    TaperMode, TextureSource, TipSource, input, param,
};
use sue_model::{BlendMode, BrushPreset, Response};

use crate::sqlite::{self, NewTable, Table, Value};

/// The [`BrushPreset::extra`] keys a `.sut` import keeps.
pub mod kept {
    /// The sub tool's `Variant` row, every column.
    pub const VARIANT: &str = "sut.variant";
    /// The `Variant` row it was first installed with (Reset to default).
    pub const INITIAL: &str = "sut.initial";
    /// Its `Node` row.
    pub const NODE: &str = "sut.node";
    /// The file's `Manager` row.
    pub const MANAGER: &str = "sut.manager";
    /// The material archives, by the hash of the picture each became.
    pub const MATERIALS: &str = "sut.materials";
}

/// Sizes in millimeters, points and the like become pixels at this
/// resolution, Clip Studio's default for print.
pub const DPI: f64 = 350.0;

/// A tip or paper picture of an imported brush: a PNG, named in the
/// preset by the SHA-256 of these bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TipPicture {
    /// The encoded picture.
    pub png: Vec<u8>,
}

impl TipPicture {
    /// The SHA-256 of the encoded picture, which names it.
    #[must_use]
    pub fn hash(&self) -> ContentHash {
        sha256(&self.png)
    }
}

/// One sub tool of a `.sut`: the brush and the pictures it names.
#[derive(Clone, Debug, PartialEq)]
pub struct SutBrush {
    /// The brush.
    pub preset: BrushPreset,
    /// The pictures its tip, paper and second tip name by hash.
    pub pictures: Vec<TipPicture>,
}

/// The SHA-256 of `bytes`.
#[must_use]
pub fn sha256(bytes: &[u8]) -> ContentHash {
    Sha256::digest(bytes).into()
}

// ----- Rows ------------------------------------------------------------------

/// One row: each column's name, declared type and value, in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Row {
    columns: Vec<(String, String, Value)>,
}

impl Row {
    fn of(table: &Table, values: &[Value], types: &BTreeMap<String, String>) -> Self {
        Self {
            columns: table
                .columns
                .iter()
                .zip(values)
                .map(|(name, value)| {
                    let kind = types.get(name).cloned().unwrap_or_default();
                    (name.clone(), kind, value.clone())
                })
                .collect(),
        }
    }

    fn get(&self, name: &str) -> Option<&Value> {
        self.columns
            .iter()
            .find(|(known, _, _)| known.eq_ignore_ascii_case(name))
            .map(|(_, _, value)| value)
    }

    fn int(&self, name: &str) -> Option<i64> {
        self.get(name).and_then(Value::int)
    }

    fn real(&self, name: &str) -> Option<f64> {
        self.get(name).and_then(Value::real)
    }

    fn on(&self, name: &str) -> bool {
        self.int(name).is_some_and(|value| value != 0)
    }

    fn blob(&self, name: &str) -> Option<&[u8]> {
        match self.get(name)? {
            Value::Blob(bytes) => Some(bytes),
            _ => None,
        }
    }

    fn set(&mut self, name: &str, kind: &str, value: Value) {
        if let Some(slot) = self
            .columns
            .iter_mut()
            .find(|(known, _, _)| known.eq_ignore_ascii_case(name))
        {
            slot.2 = value;
        } else {
            self.columns.push((name.to_owned(), kind.to_owned(), value));
        }
    }

    fn set_int(&mut self, name: &str, value: i64) {
        self.set(name, "INTEGER", Value::Integer(value));
    }

    fn set_real(&mut self, name: &str, value: f64) {
        self.set(name, "REAL", Value::Real(value));
    }

    fn set_blob(&mut self, name: &str, value: Vec<u8>) {
        self.set(name, "BLOB", Value::Blob(value));
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut out = Writer::new();
        out.varint(self.columns.len() as u64);
        for (name, kind, value) in &self.columns {
            out.str(name);
            out.str(kind);
            match value {
                Value::Null => out.u8(0),
                Value::Integer(value) => {
                    out.u8(1);
                    out.varint_signed(*value);
                }
                Value::Real(value) => {
                    out.u8(2);
                    out.f64(*value);
                }
                Value::Text(text) => {
                    out.u8(3);
                    out.str(text);
                }
                Value::Blob(bytes) => {
                    out.u8(4);
                    out.bytes(bytes);
                }
            }
        }
        out.into_bytes()
    }

    fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let mut input = Reader::new(bytes);
        let count = input.varint().ok()?;
        let mut columns = Vec::new();
        for _ in 0..count {
            let name = input.str().ok()?;
            let kind = input.str().ok()?;
            let value = match input.u8().ok()? {
                0 => Value::Null,
                1 => Value::Integer(input.varint_signed().ok()?),
                2 => Value::Real(input.f64().ok()?),
                3 => Value::Text(input.str().ok()?),
                4 => Value::Blob(input.bytes().ok()?.to_vec()),
                _ => return None,
            };
            columns.push((name, kind, value));
        }
        Some(Self { columns })
    }

    fn kept(preset: &BrushPreset, key: &str) -> Option<Self> {
        preset.extra(key).and_then(Self::from_bytes)
    }
}

/// Each column's declared type, from a table's `CREATE TABLE`.
fn declared(sql: &str) -> BTreeMap<String, String> {
    let inside = sql
        .split_once('(')
        .and_then(|(_, rest)| rest.rsplit_once(')'))
        .map_or("", |(inside, _)| inside);
    inside
        .split(',')
        .filter_map(|part| {
            let mut words = part.split_whitespace();
            let name = words.next()?.trim_matches(['"', '`', '[', ']']).to_owned();
            let kind = words.next().unwrap_or("").to_owned();
            let kind = if kind.eq_ignore_ascii_case("DEFAULT") {
                String::new()
            } else {
                kind
            };
            Some((name, kind))
        })
        .collect()
}

// ----- Effectors: what drives a setting -------------------------------------

/// A setting's inputs in an effector and their bits: pen pressure, speed,
/// tilt and random. Pressure, speed and tilt carry a curve each.
const EFFECTOR_INPUTS: [(&str, u32, usize, Option<usize>); 4] = [
    (input::PRESSURE, 16, 3, Some(8)),
    (input::SPEED, 32, 4, Some(9)),
    (input::TILT, 64, 5, Some(11)),
    (input::RANDOM, 128, 6, None),
];

/// The order the curves follow the header in.
const CURVE_SLOTS: [usize; 3] = [8, 9, 11];

/// An effector's curves, by the header word that says each one's size.
type Curves = BTreeMap<usize, Vec<[f32; 2]>>;

/// The words of an effector's header and its curves by slot.
fn effector_parts(blob: &[u8]) -> Option<(Vec<i32>, Curves)> {
    let word = |at: usize| {
        blob.get(at..at + 4)
            .map(|b| i32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    };
    let header = usize::try_from(word(0)?).ok()?;
    if !(8..=256).contains(&header) || header % 4 != 0 || header > blob.len() {
        return None;
    }
    let words: Vec<i32> = (0..header / 4).filter_map(|i| word(i * 4)).collect();
    let mut curves = BTreeMap::new();
    let mut at = header;
    for slot in CURVE_SLOTS {
        let size = words.get(slot).copied().unwrap_or(0);
        if size <= 0 {
            continue;
        }
        let head = usize::try_from(word(at)?).ok()?;
        let count = usize::try_from(word(at + 4)?).ok()?;
        let step = usize::try_from(word(at + 8)?).ok()?;
        if step < 16 || count > 64 {
            return None;
        }
        let mut points = Vec::with_capacity(count);
        for i in 0..count {
            let p = at + head + i * step;
            let x = f64::from_be_bytes(blob.get(p..p + 8)?.try_into().ok()?);
            let y = f64::from_be_bytes(blob.get(p + 8..p + 16)?.try_into().ok()?);
            points.push([x as f32, y as f32]);
        }
        curves.insert(slot, points);
        at += head + count * step;
    }
    Some((words, curves))
}

/// The bindings an effector gives `param`: one per input that is on, or
/// that has a minimum or a curve of its own, as the 0.0.75 reader kept
/// them.
fn effector_bindings(param: &str, blob: Option<&[u8]>) -> Vec<Binding> {
    let Some((words, curves)) = blob.and_then(effector_parts) else {
        return Vec::new();
    };
    let flags = words.get(2).copied().unwrap_or(0) as u32;
    let mut out = Vec::new();
    for (input, bit, min_at, slot) in EFFECTOR_INPUTS {
        let on = flags & bit != 0;
        let min = words.get(min_at).copied().unwrap_or(0) as f32 / 100.0;
        let curve = slot
            .and_then(|slot| curves.get(&slot))
            .map_or(Response::LINEAR, |points| Response::new(points));
        if on || min != 0.0 || !curve.is_linear() {
            out.push(
                Binding::new(param, input)
                    .range(min, 1.0)
                    .curve(curve)
                    .switched(on),
            );
        }
    }
    out
}

/// The bindings of `param` an effector can hold: multiplied, ending at 1,
/// following one of its inputs.
fn effector_view(preset: &BrushPreset, param: &str) -> Vec<Binding> {
    preset
        .dynamics
        .iter()
        .filter(|binding| {
            binding.param == param
                && binding.combine == Combine::Multiply
                && binding.max == 1.0
                && EFFECTOR_INPUTS
                    .iter()
                    .any(|(input, ..)| *input == binding.input)
        })
        .cloned()
        .map(|mut binding| {
            binding.extra.clear();
            binding
        })
        .collect()
}

/// An effector holding `bindings`, its other words from `kept` (or as a
/// fresh one: every input offered, speed at 100).
fn effector_blob(bindings: &[Binding], kept: Option<&[u8]>, all_inputs: bool) -> Vec<u8> {
    let mut words: Vec<i32> = kept
        .and_then(effector_parts)
        .map(|(words, _)| words)
        .filter(|words| words.len() == 12)
        .unwrap_or_else(|| {
            let mut words = vec![0; 12];
            words[0] = 48;
            words[1] = if all_inputs { 0x1f0 } else { 0xf0 };
            words[10] = 100;
            words
        });
    let mut flags = 0_u32;
    for (input, bit, min_at, _) in EFFECTOR_INPUTS {
        words[min_at] = 0;
        if let Some(binding) = bindings.iter().find(|binding| binding.input == input) {
            if binding.on {
                flags |= bit;
            }
            words[min_at] = (binding.min * 100.0).round() as i32;
        }
    }
    words[2] = flags as i32;
    let mut tail = Vec::new();
    for (_, _, _, slot) in EFFECTOR_INPUTS {
        let Some(slot) = slot else {
            continue;
        };
        words[slot] = 0;
    }
    for slot in CURVE_SLOTS {
        let Some((input, ..)) = EFFECTOR_INPUTS
            .iter()
            .find(|(_, _, _, at)| *at == Some(slot))
        else {
            continue;
        };
        if let Some(binding) = bindings.iter().find(|binding| binding.input == *input) {
            let points = binding.curve.points();
            let mut block = Vec::new();
            for word in [12_i32, points.len() as i32, 16] {
                block.extend(word.to_be_bytes());
            }
            for [x, y] in points {
                block.extend(f64::from(*x).to_be_bytes());
                block.extend(f64::from(*y).to_be_bytes());
            }
            words[slot] = block.len() as i32;
            tail.extend(block);
        }
    }
    let mut out: Vec<u8> = words.iter().flat_map(|word| word.to_be_bytes()).collect();
    out.extend(tail);
    out
}

// ----- Materials --------------------------------------------------------------

/// The paths a picture array (`BrushPatternImageArray`, `TextureImage`)
/// names, in order.
fn picture_paths(blob: &[u8]) -> Vec<String> {
    let word = |at: usize| {
        blob.get(at..at + 4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize)
    };
    let mut out = Vec::new();
    let (Some(header), Some(count)) = (word(0), word(4)) else {
        return out;
    };
    let mut at = header;
    for _ in 0..count.min(256) {
        let (Some(size), Some(length)) = (word(at), word(at + 4)) else {
            break;
        };
        let Some(raw) = blob.get(at + 8..at + 8 + length) else {
            break;
        };
        let units: Vec<u16> = raw
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect();
        out.push(String::from_utf16_lossy(&units));
        if size == 0 {
            break;
        }
        at += size;
    }
    out
}

/// A picture array naming `paths`, each with its `name`.
fn picture_array(entries: &[(String, String)]) -> Vec<u8> {
    let utf16 =
        |text: &str| -> Vec<u8> { text.encode_utf16().flat_map(u16::to_le_bytes).collect() };
    let mut out = Vec::new();
    out.extend(8_u32.to_be_bytes());
    out.extend((entries.len() as u32).to_be_bytes());
    for (path, name) in entries {
        let (path, name) = (utf16(path), utf16(name));
        let size = 4 + 4 + path.len() + 4 + 4 + name.len() + 12;
        out.extend((size as u32).to_be_bytes());
        out.extend((path.len() as u32).to_be_bytes());
        out.extend(&path);
        out.extend(3_u32.to_be_bytes());
        out.extend((name.len() as u32).to_be_bytes());
        out.extend(&name);
        out.extend(1_u32.to_be_bytes());
        out.extend([0; 8]);
    }
    out
}

/// A material of the file: where Clip Studio keeps it, its archive and
/// the picture Sue reads from it.
#[derive(Clone, Debug, PartialEq)]
struct Material {
    path: String,
    archive: Vec<u8>,
    picture: TipPicture,
}

/// A file inside a tar archive, by its name.
pub(crate) fn untar<'a>(archive: &'a [u8], wanted: &str) -> Option<&'a [u8]> {
    let mut at = 0;
    while at + 512 <= archive.len() {
        let header = &archive[at..at + 512];
        if header.iter().all(|byte| *byte == 0) {
            return None;
        }
        let name = String::from_utf8_lossy(header[..100].split(|b| *b == 0).next()?);
        let size_text = String::from_utf8_lossy(&header[124..136]);
        let size =
            usize::from_str_radix(size_text.trim_matches(|c: char| c == '\0' || c == ' '), 8)
                .ok()?;
        let data = at + 512;
        if name == wanted {
            return archive.get(data..data + size);
        }
        at = data + size.div_ceil(512) * 512;
    }
    None
}

/// A tar archive holding `files`.
fn tar(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    for (name, data) in files {
        let mut header = [0_u8; 512];
        header[..name.len().min(100)].copy_from_slice(&name.as_bytes()[..name.len().min(100)]);
        header[100..108].copy_from_slice(b"0000644\0");
        header[108..116].copy_from_slice(b"0000000\0");
        header[116..124].copy_from_slice(b"0000000\0");
        header[124..136].copy_from_slice(format!("{:011o}\0", data.len()).as_bytes());
        header[136..148].copy_from_slice(b"00000000000\0");
        header[156] = b'0';
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        header[148..156].copy_from_slice(b"        ");
        let sum: u32 = header.iter().map(|byte| u32::from(*byte)).sum();
        header[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
        out.extend(header);
        out.extend(*data);
        out.resize(out.len().div_ceil(512) * 512, 0);
    }
    out.extend([0; 1024]);
    out
}

/// The materials kept in a preset: hash, path and archive of each.
fn kept_materials(preset: &BrushPreset) -> Vec<(ContentHash, String, Vec<u8>)> {
    let Some(bytes) = preset.extra(kept::MATERIALS) else {
        return Vec::new();
    };
    let mut input = Reader::new(bytes);
    let mut out = Vec::new();
    let Ok(count) = input.varint() else {
        return out;
    };
    for _ in 0..count {
        let (Ok(hash), Ok(path), Ok(archive)) = (input.bytes(), input.str(), input.bytes()) else {
            break;
        };
        if let Ok(hash) = <ContentHash>::try_from(hash) {
            out.push((hash, path, archive.to_vec()));
        }
    }
    out
}

fn materials_bytes(materials: &[(ContentHash, String, Vec<u8>)]) -> Vec<u8> {
    let mut out = Writer::new();
    out.varint(materials.len() as u64);
    for (hash, path, archive) in materials {
        out.bytes(hash);
        out.str(path);
        out.bytes(archive);
    }
    out.into_bytes()
}

fn put_extra(preset: &mut BrushPreset, key: &str, bytes: Vec<u8>) {
    if let Some(slot) = preset.extra.iter_mut().find(|(known, _)| known == key) {
        slot.1 = bytes;
    } else {
        preset.extra.push((key.to_owned(), bytes));
    }
}

// ----- Units -----------------------------------------------------------------

/// A length in Clip Studio's unit `unit` (0 pixels, 1 centimeters, 2
/// millimeters, 3 inches, 4 points, 5 Q) as pixels at [`DPI`].
fn pixels(value: f64, unit: Option<i64>) -> f32 {
    let inches = match unit.unwrap_or(0) {
        0 => return value as f32,
        1 => value / 2.54,
        2 => value / 25.4,
        3 => value,
        4 => value / 72.0,
        5 => value * 0.25 / 25.4,
        _ => return value as f32,
    };
    (inches * DPI) as f32
}

// ----- The mapping -------------------------------------------------------------

/// What a picture array becomes: the pictures it names that the file
/// holds.
struct Pictures<'a> {
    materials: &'a [Material],
    used: Vec<TipPicture>,
}

impl Pictures<'_> {
    fn of(&mut self, blob: Option<&[u8]>) -> Vec<ContentHash> {
        let Some(blob) = blob else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for path in picture_paths(blob) {
            if let Some(material) = self.materials.iter().find(|material| {
                // A material without a path names no picture.
                !material.path.is_empty()
                    && (material.path == path || path.starts_with(&material.path))
            }) {
                let hash = material.picture.hash();
                if !self.used.iter().any(|picture| picture.hash() == hash) {
                    self.used.push(material.picture.clone());
                }
                out.push(hash);
            }
        }
        out
    }
}

/// One part of the mapping: what it reads from a row into a preset, what
/// it writes back, and what of a preset it covers (so export rewrites
/// only the parts that changed).
struct Aspect {
    read: fn(&Row, &mut BrushPreset, &mut Pictures<'_>),
    key: fn(&BrushPreset) -> String,
    write: fn(&BrushPreset, &mut Row, &mut Vec<Material>),
}

fn percent(row: &Row, name: &str) -> Option<f32> {
    row.int(name).map(|value| value as f32 / 100.0)
}

fn int_of(value: f32) -> i64 {
    (value * 100.0).round() as i64
}

/// `true` for a binding of `param` an effector holds.
fn in_effector(binding: &Binding, param: &str) -> bool {
    binding.param == param
        && binding.combine == Combine::Multiply
        && binding.max == 1.0
        && EFFECTOR_INPUTS
            .iter()
            .any(|(input, ..)| *input == binding.input)
}

/// Puts the bindings of `param` that `column` holds in place of those the
/// preset had, where they were.
fn read_effector(row: &Row, preset: &mut BrushPreset, column: &str, param: &str) {
    if row.get(column).is_none() {
        return;
    }
    let at = preset
        .dynamics
        .iter()
        .position(|binding| in_effector(binding, param))
        .unwrap_or(preset.dynamics.len());
    preset
        .dynamics
        .retain(|binding| !in_effector(binding, param));
    let at = at.min(preset.dynamics.len());
    let found = effector_bindings(param, row.blob(column));
    preset.dynamics.splice(at..at, found);
}

fn write_effector(preset: &BrushPreset, row: &mut Row, column: &str, param: &str, all: bool) {
    let blob = effector_blob(&effector_view(preset, param), row.blob(column), all);
    row.set_blob(column, blob);
}

fn behaviours(preset: &BrushPreset) -> Behaviours {
    Behaviours::from_preset(preset)
}

fn with_behaviours(preset: &mut BrushPreset, change: impl FnOnce(&mut Behaviours)) {
    let mut behaviours = Behaviours::from_preset(preset);
    change(&mut behaviours);
    behaviours.write_to(preset);
}

fn value(preset: &BrushPreset, key: &str) -> f32 {
    sue_brush::paint::registry()
        .value(preset, key)
        .unwrap_or(0.0)
}

/// Every part of the mapping, in the order import applies them.
#[expect(
    clippy::too_many_lines,
    reason = "one pass from top to bottom; split up, its steps would be read out of order"
)]
fn aspects() -> Vec<Aspect> {
    vec![
        // Size, its unit and what drives it.
        Aspect {
            read: |row, preset, _| {
                if let Some(size) = row.real("BrushSize") {
                    preset.set(param::SIZE, pixels(size, row.int("BrushSizeUnit")));
                }
                preset.size_on_screen = row.on("BrushSizeSyncViewScale");
                preset.min_width = if row.on("BrushAtLeast1Pixel") {
                    1.0
                } else {
                    0.0
                };
                read_effector(row, preset, "BrushSizeEffector", param::SIZE);
            },
            key: |preset| {
                format!(
                    "{:?}",
                    (
                        preset.value(param::SIZE),
                        preset.size_on_screen,
                        preset.min_width >= 1.0,
                        effector_view(preset, param::SIZE)
                    )
                )
            },
            write: |preset, row, _| {
                row.set_real("BrushSize", f64::from(value(preset, param::SIZE)));
                row.set_int("BrushSizeUnit", 0);
                row.set_int("BrushSizeSyncViewScale", i64::from(preset.size_on_screen));
                row.set_int("BrushAtLeast1Pixel", i64::from(preset.min_width >= 1.0));
                write_effector(preset, row, "BrushSizeEffector", param::SIZE, true);
            },
        },
        // The plain numbers and their effectors.
        Aspect {
            read: |row, preset, _| {
                for (column, key) in [
                    ("Opacity", param::OPACITY),
                    ("BrushHardness", param::HARDNESS),
                    ("BrushFlow", param::FLOW),
                    ("BrushThickness", param::ASPECT),
                ] {
                    if let Some(value) = percent(row, column) {
                        let value = if key == param::ASPECT {
                            value.clamp(0.01, 1.0)
                        } else {
                            value
                        };
                        preset.set(key, value);
                    }
                }
                for (column, param) in [
                    ("BrushOpacityEffector", param::OPACITY),
                    ("BrushFlowEffector", param::FLOW),
                    ("BrushThicknessEffector", param::ASPECT),
                ] {
                    read_effector(row, preset, column, param);
                }
            },
            key: |preset| {
                let numbers: Vec<Option<f32>> =
                    [param::OPACITY, param::HARDNESS, param::FLOW, param::ASPECT]
                        .iter()
                        .map(|key| preset.value(key))
                        .collect();
                let effectors: Vec<Vec<Binding>> = [param::OPACITY, param::FLOW, param::ASPECT]
                    .iter()
                    .map(|key| effector_view(preset, key))
                    .collect();
                format!("{numbers:?}{effectors:?}")
            },
            write: |preset, row, _| {
                row.set_int("Opacity", int_of(value(preset, param::OPACITY)));
                row.set_int("BrushHardness", int_of(value(preset, param::HARDNESS)));
                row.set_int("BrushFlow", int_of(value(preset, param::FLOW)));
                row.set_int("BrushThickness", int_of(value(preset, param::ASPECT)));
                write_effector(preset, row, "BrushOpacityEffector", param::OPACITY, false);
                write_effector(preset, row, "BrushFlowEffector", param::FLOW, true);
                write_effector(preset, row, "BrushThicknessEffector", param::ASPECT, true);
            },
        },
        // Spacing: a share of the size when fixed (type 0), else the
        // engine's own.
        Aspect {
            read: |row, preset, _| {
                if let Some(interval) = row.real("BrushInterval") {
                    preset.set(param::SPACING, (interval / 100.0) as f32);
                }
                preset.spacing = match row.int("BrushAutoIntervalType") {
                    Some(0) => Spacing::Ratio,
                    Some(_) => Spacing::Auto,
                    None => preset.spacing,
                };
                read_effector(row, preset, "BrushIntervalEffector", param::SPACING);
                if row.get("BrushAdjustFlowByInterval").is_some() {
                    let on = row.on("BrushAdjustFlowByInterval");
                    with_behaviours(preset, |b| b.gap_density = on);
                }
            },
            key: |preset| {
                format!(
                    "{:?}",
                    (
                        preset.value(param::SPACING),
                        preset.spacing,
                        effector_view(preset, param::SPACING),
                        behaviours(preset).gap_density
                    )
                )
            },
            write: |preset, row, _| {
                row.set_real(
                    "BrushInterval",
                    f64::from(value(preset, param::SPACING)) * 100.0,
                );
                let kept = row.int("BrushAutoIntervalType").filter(|kind| *kind != 0);
                row.set_int(
                    "BrushAutoIntervalType",
                    match preset.spacing {
                        Spacing::Auto => kept.unwrap_or(2),
                        Spacing::Ratio | Spacing::Fixed => 0,
                    },
                );
                write_effector(preset, row, "BrushIntervalEffector", param::SPACING, true);
                row.set_int(
                    "BrushAdjustFlowByInterval",
                    i64::from(behaviours(preset).gap_density),
                );
            },
        },
        // The tip's turn and what turns it.
        Aspect {
            read: |row, preset, _| {
                if let Some(angle) = row.real("BrushRotation") {
                    preset.set(param::ANGLE, angle as f32);
                }
                let Some(effector) = row.int("BrushRotationEffector") else {
                    return;
                };
                preset.dynamics.retain(|binding| {
                    !(binding.param == param::ANGLE && binding.combine == Combine::Add)
                });
                let scale = row.int("BrushRotationRandomScale").unwrap_or(100) as f32 / 100.0;
                for (bit, input, low, high) in [
                    (16, input::PRESSURE, 0.0, 360.0),
                    (32, input::DIRECTION, 0.0, 360.0),
                    (64, input::TILT_DIRECTION, 0.0, 360.0),
                    (128, input::RANDOM, -180.0 * scale, 180.0 * scale),
                ] {
                    if effector & bit != 0 {
                        preset
                            .dynamics
                            .push(Binding::new(param::ANGLE, input).range(low, high).added());
                    }
                }
            },
            key: |preset| {
                let turns: Vec<&Binding> = preset
                    .dynamics
                    .iter()
                    .filter(|binding| {
                        binding.param == param::ANGLE
                            && binding.combine == Combine::Add
                            && binding.on
                    })
                    .collect();
                format!("{:?}{turns:?}", preset.value(param::ANGLE))
            },
            write: |preset, row, _| {
                row.set_real("BrushRotation", f64::from(value(preset, param::ANGLE)));
                // The two lowest bits are set in every file seen.
                let mut effector = row.int("BrushRotationEffector").unwrap_or(3) & 3;
                let mut scale = 100;
                for binding in preset.dynamics.iter().filter(|binding| {
                    binding.param == param::ANGLE && binding.combine == Combine::Add && binding.on
                }) {
                    effector |= match binding.input.as_str() {
                        input::PRESSURE => 16,
                        input::DIRECTION => 32,
                        input::TILT_DIRECTION => 64,
                        input::RANDOM => {
                            scale = (binding.max / 180.0 * 100.0).round() as i64;
                            128
                        }
                        _ => 0,
                    };
                }
                row.set_int("BrushRotationEffector", effector);
                row.set_int("BrushRotationRandomScale", scale);
            },
        },
        // Anti-aliasing.
        Aspect {
            read: |row, preset, _| {
                if let Some(level) = row.int("AntiAlias") {
                    preset.antialias = match level {
                        0 => AntiAlias::None,
                        1 => AntiAlias::Weak,
                        2 => AntiAlias::Medium,
                        _ => AntiAlias::Strong,
                    };
                }
            },
            key: |preset| format!("{:?}", preset.antialias),
            write: |preset, row, _| {
                row.set_int(
                    "AntiAlias",
                    match preset.antialias {
                        AntiAlias::None => 0,
                        AntiAlias::Weak => 1,
                        AntiAlias::Medium => 2,
                        AntiAlias::Strong => 3,
                    },
                );
            },
        },
        // How the stroke meets the layer: 0 normal, 27 erase; other
        // numbers are kept.
        Aspect {
            read: |row, preset, _| match row.int("CompositeMode") {
                Some(0) => {
                    preset.blend = BlendMode::Normal;
                    preset.erase = false;
                }
                Some(27) => preset.erase = true,
                _ => {}
            },
            key: |preset| format!("{:?}", (preset.blend == BlendMode::Normal, preset.erase)),
            write: |preset, row, _| {
                if preset.erase {
                    row.set_int("CompositeMode", 27);
                } else if preset.blend == BlendMode::Normal {
                    row.set_int("CompositeMode", 0);
                }
            },
        },
        // Stabilization and post correction.
        Aspect {
            read: |row, preset, _| {
                if let Some(level) = row.int("FlickerReduction") {
                    preset.input.stabilization = level.clamp(0, 100) as u32;
                }
                if row.get("FlickerReductionBySpeed").is_some() {
                    preset.input.by_speed = row.on("FlickerReductionBySpeed");
                }
                preset.input.post_correction = if row.on("BrushUseRevision") {
                    (row.int("BrushRevision").unwrap_or(10) as f32 / 10.0)
                        .round()
                        .clamp(1.0, 10.0) as u32
                } else {
                    0
                };
            },
            key: |preset| {
                let input = &preset.input;
                format!(
                    "{:?}",
                    (input.stabilization, input.by_speed, input.post_correction)
                )
            },
            write: |preset, row, _| {
                let input = &preset.input;
                row.set_int("FlickerReduction", i64::from(input.stabilization));
                row.set_int("FlickerReductionBySpeed", i64::from(input.by_speed));
                row.set_int("BrushUseRevision", i64::from(input.post_correction > 0));
                if input.post_correction > 0 {
                    row.set_int("BrushRevision", i64::from(input.post_correction) * 10);
                }
            },
        },
        // Starting and ending.
        Aspect {
            read: |row, preset, _| {
                let mode = match row.int("BrushInOutType") {
                    Some(1) => TaperMode::Percent,
                    Some(2) => TaperMode::Fade,
                    _ => TaperMode::Length,
                };
                let side = |used: &str, length: &str, unit: &str, ratio: &str| {
                    if !row.on(used) {
                        return 0.0;
                    }
                    match mode {
                        TaperMode::Percent => row.real(ratio).unwrap_or(0.0) as f32,
                        _ => pixels(row.real(length).unwrap_or(0.0), row.int(unit)),
                    }
                };
                preset.taper.mode = mode;
                preset.taper.start = side(
                    "BrushUseIn",
                    "BrushInLength",
                    "BrushInLengthUnit",
                    "BrushInRatio",
                );
                preset.taper.end = side(
                    "BrushUseOut",
                    "BrushOutLength",
                    "BrushOutLengthUnit",
                    "BrushOutRatio",
                );
                preset.taper.by_speed = row.on("BrushInOutBySpeed");
            },
            key: |preset| {
                let taper = &preset.taper;
                format!("{:?}", (taper.start, taper.end, taper.mode, taper.by_speed))
            },
            write: |preset, row, _| {
                let taper = &preset.taper;
                row.set_int(
                    "BrushInOutType",
                    match taper.mode {
                        TaperMode::Length => 0,
                        TaperMode::Percent => 1,
                        TaperMode::Fade => 2,
                    },
                );
                for (used, length, unit, ratio, value) in [
                    (
                        "BrushUseIn",
                        "BrushInLength",
                        "BrushInLengthUnit",
                        "BrushInRatio",
                        taper.start,
                    ),
                    (
                        "BrushUseOut",
                        "BrushOutLength",
                        "BrushOutLengthUnit",
                        "BrushOutRatio",
                        taper.end,
                    ),
                ] {
                    row.set_int(used, i64::from(value > 0.0));
                    if value > 0.0 {
                        if taper.mode == TaperMode::Percent {
                            row.set_real(ratio, f64::from(value));
                        } else {
                            row.set_real(length, f64::from(value));
                            row.set_int(unit, 0);
                        }
                    }
                }
                row.set_int("BrushInOutBySpeed", i64::from(taper.by_speed));
            },
        },
        // The tip: round, or the file's pictures.
        Aspect {
            read: |row, preset, pictures| {
                let hashes = if row.on("BrushUsePatternImage") {
                    pictures.of(row.blob("BrushPatternImageArray"))
                } else {
                    Vec::new()
                };
                let order = match row.int("BrushPatternOrderType") {
                    Some(1) => TipOrder::BackAndForth,
                    Some(2) => TipOrder::Clamp,
                    Some(3) => TipOrder::Random,
                    _ => TipOrder::Repeat,
                };
                preset.tip = if hashes.is_empty() {
                    TipSource::Round
                } else {
                    TipSource::Images {
                        pictures: hashes.into_iter().map(PictureRef::Hash).collect(),
                        pick: if order == TipOrder::Random {
                            Pick::Random
                        } else {
                            Pick::Sequence
                        },
                        keep_colors: row.get("BrushChangePatternColor").is_some()
                            && !row.on("BrushChangePatternColor"),
                    }
                };
                let flip = |name: &str| {
                    if row.on(name) {
                        Flip::Random
                    } else {
                        Flip::Off
                    }
                };
                let flips = [
                    flip("BrushPatternReverseHorizontal"),
                    flip("BrushPatternReverseVertical"),
                ];
                let darken = row.on("BrushBlendPatternByDarken");
                with_behaviours(preset, |b| {
                    b.tip_order = order;
                    b.flip = flips;
                    b.tip_blend = if darken {
                        TipBlend::Darken
                    } else {
                        TipBlend::Over
                    };
                });
                let ribbon = row.on("BrushRibbon");
                preset.engine = match (&preset.tip, ribbon) {
                    (TipSource::Images { .. }, true) => EngineKind::Ribbon,
                    (TipSource::Images { .. }, false) => EngineKind::Dab,
                    (TipSource::Round, _) if preset.spacing != Spacing::Auto => EngineKind::Dab,
                    (TipSource::Round, _)
                        if row.on("BrushUseWaterColor") || row.on("BrushUseSpray") =>
                    {
                        EngineKind::Dab
                    }
                    (TipSource::Round, _) => EngineKind::Stroke,
                };
            },
            key: |preset| {
                let b = behaviours(preset);
                format!(
                    "{:?}",
                    (
                        &preset.tip,
                        preset.engine == EngineKind::Ribbon,
                        b.tip_order,
                        b.flip,
                        b.tip_blend
                    )
                )
            },
            write: |preset, row, materials| {
                let pictures: Vec<ContentHash> = match &preset.tip {
                    TipSource::Images { pictures, .. } => pictures
                        .iter()
                        .filter_map(|picture| match picture {
                            PictureRef::Hash(hash) => Some(*hash),
                            PictureRef::Named(_) => None,
                        })
                        .collect(),
                    TipSource::Round => Vec::new(),
                };
                row.set_int("BrushUsePatternImage", i64::from(!pictures.is_empty()));
                if !pictures.is_empty() {
                    let entries = material_entries(&pictures, materials);
                    row.set_blob("BrushPatternImageArray", picture_array(&entries));
                }
                let b = behaviours(preset);
                row.set_int(
                    "BrushPatternOrderType",
                    match b.tip_order {
                        TipOrder::Repeat => 0,
                        TipOrder::BackAndForth => 1,
                        TipOrder::Clamp => 2,
                        TipOrder::Random => 3,
                    },
                );
                row.set_int(
                    "BrushPatternReverseHorizontal",
                    i64::from(b.flip[0] != Flip::Off),
                );
                row.set_int(
                    "BrushPatternReverseVertical",
                    i64::from(b.flip[1] != Flip::Off),
                );
                row.set_int(
                    "BrushBlendPatternByDarken",
                    i64::from(b.tip_blend == TipBlend::Darken),
                );
                let keep = matches!(
                    preset.tip,
                    TipSource::Images {
                        keep_colors: true,
                        ..
                    }
                );
                row.set_int("BrushChangePatternColor", i64::from(!keep));
                row.set_int(
                    "BrushRibbon",
                    i64::from(preset.engine == EngineKind::Ribbon),
                );
            },
        },
        // The paper.
        Aspect {
            read: |row, preset, pictures| {
                let hashes = pictures.of(row.blob("TextureImage"));
                let Some(hash) = hashes.first().copied() else {
                    if row.get("TextureImage").is_some() {
                        with_behaviours(preset, |b| b.texture = None);
                    }
                    return;
                };
                let settings = TextureSettings {
                    scale: (row
                        .real("TextureScale")
                        .or_else(|| row.real("TextureScale2"))
                        .unwrap_or(100.0)
                        / 100.0) as f32,
                    rotation: row.real("TextureRotate").unwrap_or(0.0) as f32,
                    brightness: percent(row, "TextureBrightness").unwrap_or(0.0),
                    contrast: percent(row, "TextureContrast").unwrap_or(0.0),
                    invert: row.on("TextureReverseDensity"),
                    strength: percent(row, "TextureDensity").unwrap_or(1.0),
                    mode: match row.int("TextureCompositeMode") {
                        Some(0) | None => PaperMode::Multiply,
                        // The others are kept in the row.
                        Some(_) => PaperMode::Multiply,
                    },
                    emphasize: row.on("TextureStressDensity"),
                    per_dab: row.on("TextureForPlot"),
                };
                with_behaviours(preset, |b| {
                    b.texture = Some(TextureChoice {
                        paper: PaperChoice::Picture(hash),
                        settings,
                    });
                });
                read_effector(
                    row,
                    preset,
                    "TextureDensityEffector",
                    param::TEXTURE_STRENGTH,
                );
            },
            key: |preset| {
                format!(
                    "{:?}{:?}",
                    behaviours(preset).texture,
                    effector_view(preset, param::TEXTURE_STRENGTH)
                )
            },
            write: |preset, row, materials| {
                let Some(texture) = behaviours(preset).texture else {
                    row.set("TextureImage", "", Value::Null);
                    return;
                };
                if let PaperChoice::Picture(hash) = texture.paper {
                    let entries = material_entries(&[hash], materials);
                    row.set_blob("TextureImage", picture_array(&entries));
                }
                let s = texture.settings;
                let scale_column = if row.get("TextureScale2").is_some() {
                    "TextureScale2"
                } else {
                    "TextureScale"
                };
                row.set_real(scale_column, f64::from(s.scale) * 100.0);
                row.set_real("TextureRotate", f64::from(s.rotation));
                row.set_int("TextureBrightness", int_of(s.brightness));
                row.set_int("TextureContrast", int_of(s.contrast));
                row.set_int("TextureReverseDensity", i64::from(s.invert));
                row.set_int("TextureDensity", int_of(s.strength));
                if s.mode == PaperMode::Multiply {
                    if row.int("TextureCompositeMode").is_none() {
                        row.set_int("TextureCompositeMode", 0);
                    }
                } else {
                    row.set_int("TextureCompositeMode", 0);
                }
                row.set_int("TextureStressDensity", i64::from(s.emphasize));
                row.set_int("TextureForPlot", i64::from(s.per_dab));
                write_effector(
                    preset,
                    row,
                    "TextureDensityEffector",
                    param::TEXTURE_STRENGTH,
                    true,
                );
            },
        },
        // Mixing with the paint on the picture.
        Aspect {
            read: |row, preset, _| {
                if row.get("BrushUseWaterColor").is_none() {
                    return;
                }
                preset.mix = row.on("BrushUseWaterColor").then(|| {
                    let load = percent(row, "BrushMixColor").unwrap_or(1.0);
                    Mixing {
                        amount: 1.0 - load,
                        load,
                        pull: percent(row, "BrushMixColorExtension").unwrap_or(0.0),
                        blur: row.real("BrushBlur").unwrap_or(0.0) as f32 / 100.0,
                    }
                });
                if let Some(density) = percent(row, "BrushMixAlpha") {
                    with_behaviours(preset, |b| b.paint_density = density);
                }
                read_effector(row, preset, "BrushMixColorEffector", key::AMOUNT_OF_PAINT);
                read_effector(row, preset, "BrushMixAlphaEffector", key::PAINT_DENSITY);
            },
            key: |preset| {
                format!(
                    "{:?}",
                    (
                        preset.mix,
                        preset.mix.map(|_| behaviours(preset).paint_density),
                        effector_view(preset, key::AMOUNT_OF_PAINT),
                        effector_view(preset, key::PAINT_DENSITY)
                    )
                )
            },
            write: |preset, row, _| {
                row.set_int("BrushUseWaterColor", i64::from(preset.mix.is_some()));
                if let Some(mix) = preset.mix {
                    row.set_int("BrushMixColor", int_of(mix.load));
                    row.set_int("BrushMixColorExtension", int_of(mix.pull));
                    row.set_real("BrushBlur", f64::from(mix.blur) * 100.0);
                    row.set_int("BrushMixAlpha", int_of(behaviours(preset).paint_density));
                }
                write_effector(
                    preset,
                    row,
                    "BrushMixColorEffector",
                    key::AMOUNT_OF_PAINT,
                    true,
                );
                write_effector(
                    preset,
                    row,
                    "BrushMixAlphaEffector",
                    key::PAINT_DENSITY,
                    true,
                );
            },
        },
        // The spraying effect.
        Aspect {
            read: |row, preset, _| {
                if row.get("BrushUseSpray").is_none() {
                    return;
                }
                let spray = row.on("BrushUseSpray").then(|| {
                    let linked = row.on("BrushSpraySizeSyncBrushSize");
                    let size = row.real("BrushSpraySize").unwrap_or(0.0);
                    let rotation = row.int("BrushRotationEffectorInSpray").unwrap_or(0);
                    Spray {
                        size: if linked {
                            (size / 100.0) as f32
                        } else {
                            pixels(size, row.int("BrushSpraySizeUnit"))
                        },
                        linked,
                        density: row.int("BrushSprayDensity").unwrap_or(12).max(1) as f32,
                        deviation: (row.int("BrushSprayBias").unwrap_or(0).clamp(0, 100) as f32)
                            / 100.0,
                        direction: if rotation & 128 != 0 {
                            ParticleDirection::Random
                        } else if rotation & 32 != 0 {
                            ParticleDirection::Line
                        } else {
                            ParticleDirection::Brush
                        },
                    }
                });
                with_behaviours(preset, |b| b.spray = spray);
                read_effector(row, preset, "BrushSpraySizeEffector", key::PARTICLE_SIZE);
                read_effector(
                    row,
                    preset,
                    "BrushSprayDensityEffector",
                    key::PARTICLE_DENSITY,
                );
            },
            key: |preset| {
                format!(
                    "{:?}",
                    (
                        behaviours(preset).spray,
                        effector_view(preset, key::PARTICLE_SIZE),
                        effector_view(preset, key::PARTICLE_DENSITY)
                    )
                )
            },
            write: |preset, row, _| {
                let spray = behaviours(preset).spray;
                row.set_int("BrushUseSpray", i64::from(spray.is_some()));
                if let Some(spray) = spray {
                    row.set_int("BrushSpraySizeSyncBrushSize", i64::from(spray.linked));
                    if spray.linked {
                        row.set_real("BrushSpraySize", f64::from(spray.size) * 100.0);
                    } else {
                        row.set_real("BrushSpraySize", f64::from(spray.size));
                        row.set_int("BrushSpraySizeUnit", 0);
                    }
                    row.set_int("BrushSprayDensity", spray.density.round() as i64);
                    row.set_int("BrushSprayBias", int_of(spray.deviation));
                    let low = row.int("BrushRotationEffectorInSpray").unwrap_or(3) & 3;
                    row.set_int(
                        "BrushRotationEffectorInSpray",
                        low | match spray.direction {
                            ParticleDirection::Brush => 0,
                            ParticleDirection::Line => 32,
                            ParticleDirection::Random => 128,
                        },
                    );
                }
                write_effector(
                    preset,
                    row,
                    "BrushSpraySizeEffector",
                    key::PARTICLE_SIZE,
                    true,
                );
                write_effector(
                    preset,
                    row,
                    "BrushSprayDensityEffector",
                    key::PARTICLE_DENSITY,
                    true,
                );
            },
        },
        // The watercolor edge.
        Aspect {
            read: |row, preset, _| {
                if row.get("BrushUseWaterEdge").is_none() {
                    return;
                }
                let edge = row.on("BrushUseWaterEdge").then(|| WatercolorEdge {
                    range: pixels(
                        row.real("BrushWaterEdgeRadius").unwrap_or(6.0),
                        row.int("BrushWaterEdgeRadiusUnit"),
                    ),
                    opacity: 1.0 - percent(row, "BrushWaterEdgeAlphaPower").unwrap_or(0.45),
                    darkness: percent(row, "BrushWaterEdgeValuePower").unwrap_or(0.35),
                    blur: pixels(
                        row.real("BrushWaterEdgeBlur").unwrap_or(1.0),
                        row.int("BrushWaterEdgeBlurUnit"),
                    ),
                    after_stroke: row.on("BrushWaterEdgeAfterDrag"),
                });
                with_behaviours(preset, |b| b.watercolor = edge);
            },
            key: |preset| format!("{:?}", behaviours(preset).watercolor),
            write: |preset, row, _| {
                let edge = behaviours(preset).watercolor;
                row.set_int("BrushUseWaterEdge", i64::from(edge.is_some()));
                if let Some(edge) = edge {
                    row.set_real("BrushWaterEdgeRadius", f64::from(edge.range));
                    row.set_int("BrushWaterEdgeRadiusUnit", 0);
                    row.set_int("BrushWaterEdgeAlphaPower", int_of(1.0 - edge.opacity));
                    row.set_int("BrushWaterEdgeValuePower", int_of(edge.darkness));
                    row.set_real("BrushWaterEdgeBlur", f64::from(edge.blur));
                    row.set_int("BrushWaterEdgeBlurUnit", 0);
                    row.set_int("BrushWaterEdgeAfterDrag", i64::from(edge.after_stroke));
                }
            },
        },
        // Color jitter.
        Aspect {
            read: |row, preset, _| {
                let color = &mut preset.color;
                color.hue = row.int("BrushHueChange").unwrap_or(0) as f32 / 180.0;
                color.saturation = percent(row, "BrushSaturationChange").unwrap_or(0.0);
                color.luminosity = percent(row, "BrushValueChange").unwrap_or(0.0);
                color.sub = percent(row, "BrushSubColor").unwrap_or(0.0);
                color.tip = color.hue != 0.0
                    || color.saturation != 0.0
                    || color.luminosity != 0.0
                    || color.sub != 0.0;
                color.stroke = row.on("BrushChangeStrokeColor");
                color.stroke_hue = row.int("BrushStrokeHueChange").unwrap_or(0) as f32 / 180.0;
                color.stroke_saturation =
                    percent(row, "BrushStrokeSaturationChange").unwrap_or(0.0);
                color.stroke_luminosity = percent(row, "BrushStrokeValueChange").unwrap_or(0.0);
            },
            key: |preset| format!("{:?}", preset.color),
            write: |preset, row, _| {
                let color = &preset.color;
                let tip = |value: f32| if color.tip { value } else { 0.0 };
                row.set_int("BrushHueChange", (tip(color.hue) * 180.0).round() as i64);
                row.set_int("BrushSaturationChange", int_of(tip(color.saturation)));
                row.set_int("BrushValueChange", int_of(tip(color.luminosity)));
                row.set_int("BrushSubColor", int_of(tip(color.sub)));
                row.set_int("BrushChangeStrokeColor", i64::from(color.stroke));
                row.set_int(
                    "BrushStrokeHueChange",
                    (color.stroke_hue * 180.0).round() as i64,
                );
                row.set_int(
                    "BrushStrokeSaturationChange",
                    int_of(color.stroke_saturation),
                );
                row.set_int("BrushStrokeValueChange", int_of(color.stroke_luminosity));
            },
        },
        // The second tip.
        Aspect {
            read: |row, preset, pictures| {
                if row.get("UseDualBrush").is_none() {
                    return;
                }
                let dual = if row.on("UseDualBrush") {
                    let linked = row.on("SyncDualBrushSize");
                    let size = row.real("DualSize").unwrap_or(50.0);
                    let hashes = if row.on("DualUsePatternImage") {
                        pictures.of(row.blob("DualPatternImageArray"))
                    } else {
                        Vec::new()
                    };
                    Some(DualBrush {
                        mode: sue_brush::paint::DualMode::Multiply,
                        size: if linked {
                            (size / 100.0) as f32
                        } else {
                            pixels(size, row.int("DualSizeUnit"))
                        },
                        linked,
                        spacing: (row.real("DualInterval").unwrap_or(50.0) / 100.0) as f32,
                        scatter: DualBrush::default().scatter,
                        hardness: percent(row, "DualHardness").unwrap_or(1.0),
                        pictures: hashes,
                    })
                } else {
                    None
                };
                with_behaviours(preset, |b| b.dual = dual);
            },
            key: |preset| {
                let dual = behaviours(preset).dual.map(|dual| {
                    (
                        dual.size,
                        dual.linked,
                        dual.spacing,
                        dual.hardness,
                        dual.pictures,
                    )
                });
                format!("{dual:?}")
            },
            write: |preset, row, materials| {
                let dual = behaviours(preset).dual;
                row.set_int("UseDualBrush", i64::from(dual.is_some()));
                if let Some(dual) = dual {
                    row.set_int("SyncDualBrushSize", i64::from(dual.linked));
                    if dual.linked {
                        row.set_real("DualSize", f64::from(dual.size) * 100.0);
                    } else {
                        row.set_real("DualSize", f64::from(dual.size));
                        row.set_int("DualSizeUnit", 0);
                    }
                    row.set_real("DualInterval", f64::from(dual.spacing) * 100.0);
                    row.set_int("DualHardness", int_of(dual.hardness));
                    row.set_int("DualUsePatternImage", i64::from(!dual.pictures.is_empty()));
                    if !dual.pictures.is_empty() {
                        let entries = material_entries(&dual.pictures, materials);
                        row.set_blob("DualPatternImageArray", picture_array(&entries));
                    }
                    if row.int("DualBrushCompositeMode").is_none() {
                        row.set_int("DualBrushCompositeMode", 1);
                    }
                }
            },
        },
        // The rest of the paint behaviours: continuous spraying, sharp
        // corners, erasing on every layer, and how vector lines are
        // erased.
        Aspect {
            read: |row, preset, _| {
                let continuous = row.on("BrushContinuousPlot");
                let sharp = row
                    .get("BrushSharpenCorner")
                    .map(|_| row.on("BrushSharpenCorner"));
                let all = row.on("BrushEraseAllLayer");
                with_behaviours(preset, |b| {
                    b.continuous = continuous.then_some(b.continuous.unwrap_or(60.0));
                    if let Some(sharp) = sharp {
                        b.sharp_angles = sharp;
                    }
                    b.all_layers = all;
                });
                preset
                    .extra
                    .retain(|(key, _)| key != sue_brush::presets::field::VECTOR_ERASE);
                if row.on("BrushUseVectorEraser") {
                    let index = row.int("BrushVectorEraserType").unwrap_or(0).clamp(0, 2) as usize;
                    preset.extra.push((
                        sue_brush::presets::field::VECTOR_ERASE.to_owned(),
                        sue_brush::presets::VECTOR_ERASE[index].as_bytes().to_vec(),
                    ));
                }
            },
            key: |preset| {
                let b = behaviours(preset);
                format!(
                    "{:?}",
                    (
                        b.continuous.is_some(),
                        b.sharp_angles,
                        b.all_layers,
                        preset.extra(sue_brush::presets::field::VECTOR_ERASE)
                    )
                )
            },
            write: |preset, row, _| {
                let b = behaviours(preset);
                row.set_int("BrushContinuousPlot", i64::from(b.continuous.is_some()));
                row.set_int("BrushSharpenCorner", i64::from(b.sharp_angles));
                row.set_int("BrushEraseAllLayer", i64::from(b.all_layers));
                let vector = preset
                    .extra(sue_brush::presets::field::VECTOR_ERASE)
                    .is_some();
                row.set_int("BrushUseVectorEraser", i64::from(vector));
                if vector {
                    row.set_int(
                        "BrushVectorEraserType",
                        i64::from(sue_brush::presets::vector_erase(preset)),
                    );
                }
            },
        },
    ]
}

/// The paths and names of `pictures` for a picture array, each from a
/// material kept or added in `materials`.
fn material_entries(pictures: &[ContentHash], materials: &mut [Material]) -> Vec<(String, String)> {
    pictures
        .iter()
        .filter_map(|hash| {
            materials
                .iter()
                .find(|material| material.picture.hash() == *hash)
                .map(|material| (material.path.clone(), hex(&hash[..4])))
        })
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// A preset as Clip Studio's columns are when a file leaves them out.
fn blank(name: &str) -> BrushPreset {
    let mut preset = BrushPreset::new(name);
    preset.spacing = Spacing::Auto;
    preset.antialias = AntiAlias::Medium;
    preset.input.stabilization = 0;
    preset.input.smart_shape = false;
    preset
}

/// What `row` says, read into `preset`.
fn read_row(row: &Row, preset: &mut BrushPreset, pictures: &mut Pictures<'_>) {
    for aspect in aspects() {
        (aspect.read)(row, preset, pictures);
    }
}

// ----- Import ------------------------------------------------------------------

/// The table named `name`.
fn table<'a>(tables: &'a [Table], name: &str) -> Option<&'a Table> {
    tables
        .iter()
        .find(|table| table.name.eq_ignore_ascii_case(name))
}

/// The materials of the file, each with the picture Sue reads from it.
fn materials_of(tables: &[Table]) -> Vec<Material> {
    let Some(files) = table(tables, "MaterialFile") else {
        return Vec::new();
    };
    let (Some(data), path) = (files.column("FileData"), files.column("OriginalPath")) else {
        return Vec::new();
    };
    files
        .rows
        .iter()
        .filter_map(|(_, row)| {
            let archive = row.get(data).and_then(Value::bytes)?;
            let png = untar(archive, "thumbnail/thumbnail.png")?;
            crate::decode_png(png).ok()?;
            Some(Material {
                path: path
                    .and_then(|at| row.get(at))
                    .and_then(Value::text)
                    .unwrap_or_default()
                    .to_owned(),
                archive: archive.to_vec(),
                picture: TipPicture { png: png.to_vec() },
            })
        })
        .collect()
}

/// The sub tools in a `.sut` file, as Sue brushes. A sub tool without a
/// picture tip (a round pen, an airbrush) comes in like any other.
///
/// # Errors
/// When the bytes are not a Clip Studio sub tool file.
pub fn import(bytes: &[u8]) -> Result<Vec<SutBrush>, String> {
    import_named(bytes, "Sub tool")
}

/// [`import`], naming a sub tool that has no name of its own `fallback`
/// (the file's name, say).
///
/// # Errors
/// When the bytes are not a Clip Studio sub tool file.
pub fn import_named(bytes: &[u8], fallback: &str) -> Result<Vec<SutBrush>, String> {
    let database = sqlite::Database::open(bytes)?;
    let types: BTreeMap<String, BTreeMap<String, String>> = database
        .schema()
        .iter()
        .map(|entry| (entry.name.to_ascii_lowercase(), declared(&entry.sql)))
        .collect();
    let tables = sqlite::read(bytes)?;
    let (Some(nodes), Some(variants)) = (table(&tables, "Node"), table(&tables, "Variant")) else {
        return Err(NOT_A_SUB_TOOL.to_owned());
    };
    let types_of = |name: &str| {
        types
            .get(&name.to_ascii_lowercase())
            .cloned()
            .unwrap_or_default()
    };
    let (node_types, variant_types) = (types_of("Node"), types_of("Variant"));
    let manager = table(&tables, "Manager").and_then(|manager| {
        manager
            .rows
            .first()
            .map(|(_, row)| Row::of(manager, row, &types_of("Manager")))
    });
    let materials = materials_of(&tables);
    let sue = table(&tables, "SueBrush");
    let variant = |id: i64| {
        let at = variants.column("VariantID")?;
        variants
            .rows
            .iter()
            .find(|(_, row)| row.get(at).and_then(Value::int) == Some(id))
            .map(|(_, row)| Row::of(variants, row, &variant_types))
    };
    let mut out = Vec::new();
    for (_, values) in &nodes.rows {
        let node = Row::of(nodes, values, &node_types);
        let Some(id) = node.int("NodeVariantID").filter(|id| *id > 0) else {
            continue;
        };
        let Some(row) = variant(id) else {
            continue;
        };
        let name = node
            .get("NodeName")
            .and_then(Value::text)
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(fallback)
            .to_owned();
        let mut pictures = Pictures {
            materials: &materials,
            used: Vec::new(),
        };
        let embedded = sue.and_then(|sue| {
            let (at, data) = (sue.column("VariantID")?, sue.column("Preset")?);
            sue.rows
                .iter()
                .find(|(_, row)| row.get(at).and_then(Value::int) == Some(id))
                .and_then(|(_, row)| row.get(data).and_then(Value::bytes))
                .and_then(|bytes| BrushPreset::from_bytes(bytes).ok())
        });
        let mut preset = match embedded {
            // A Sue brush: the preset as Sue wrote it, with what was
            // changed in Clip Studio since.
            Some(mut preset) => {
                let mut scratch = materials.clone();
                let mut expected = Row::default();
                write_row(&preset, &mut expected, &mut scratch);
                let mut none = Pictures {
                    materials: &scratch,
                    used: Vec::new(),
                };
                let mut wrote = blank(&name);
                read_row(&expected, &mut wrote, &mut none);
                let mut found = blank(&name);
                read_row(&row, &mut found, &mut pictures);
                for aspect in aspects() {
                    if (aspect.key)(&found) != (aspect.key)(&wrote) {
                        (aspect.read)(&row, &mut preset, &mut pictures);
                    }
                }
                // Every picture it names, found by hash.
                for hash in named_pictures(&preset) {
                    if let Some(material) = materials.iter().find(|m| m.picture.hash() == hash)
                        && !pictures.used.iter().any(|p| p.hash() == hash)
                    {
                        pictures.used.push(material.picture.clone());
                    }
                }
                preset.name.clone_from(&name);
                preset
            }
            None => {
                let mut preset = blank(&name);
                read_row(&row, &mut preset, &mut pictures);
                preset
            }
        };
        put_extra(&mut preset, kept::VARIANT, row.to_bytes());
        if let Some(initial) = node.int("NodeInitVariantID").and_then(variant) {
            put_extra(&mut preset, kept::INITIAL, initial.to_bytes());
        }
        put_extra(&mut preset, kept::NODE, node.to_bytes());
        if let Some(manager) = &manager {
            put_extra(&mut preset, kept::MANAGER, manager.to_bytes());
        }
        let used: Vec<(ContentHash, String, Vec<u8>)> = materials
            .iter()
            .filter(|material| pictures.used.contains(&material.picture))
            .map(|material| {
                (
                    material.picture.hash(),
                    material.path.clone(),
                    material.archive.clone(),
                )
            })
            .collect();
        if !used.is_empty() {
            put_extra(&mut preset, kept::MATERIALS, materials_bytes(&used));
        }
        out.push(SutBrush {
            preset,
            pictures: pictures.used,
        });
    }
    if out.is_empty() {
        return Err(NO_SUB_TOOL.to_owned());
    }
    Ok(out)
}

/// The hashes of every picture `preset` names: tip, paper, second tip.
#[must_use]
pub fn named_pictures(preset: &BrushPreset) -> Vec<ContentHash> {
    let mut out = Vec::new();
    if let TipSource::Images { pictures, .. } = &preset.tip {
        out.extend(pictures.iter().filter_map(|picture| match picture {
            PictureRef::Hash(hash) => Some(*hash),
            PictureRef::Named(_) => None,
        }));
    }
    if let Some(texture) = &preset.texture
        && let TextureSource::Picture(hash) = texture.source
    {
        out.push(hash);
    }
    if let Some(dual) = Behaviours::from_preset(preset).dual {
        out.extend(dual.pictures);
    }
    out.dedup();
    out
}

/// Not a sub tool file.
pub const NOT_A_SUB_TOOL: &str = "not a Clip Studio Paint sub tool";
/// A sub tool file without a sub tool.
pub const NO_SUB_TOOL: &str = "the file holds no sub tool";
/// A picture the brush names is not at hand.
pub const PICTURE_MISSING: &str = "a picture the brush uses is missing";

// ----- Export ------------------------------------------------------------------

/// Writes what `preset` says into `row`: everything for a row of its own,
/// only what changed for a row kept from an import.
fn write_row(preset: &BrushPreset, row: &mut Row, materials: &mut Vec<Material>) {
    let fresh = row.columns.is_empty();
    let base = (!fresh).then(|| {
        let mut base = blank(&preset.name);
        let mut pictures = Pictures {
            materials,
            used: Vec::new(),
        };
        read_row(row, &mut base, &mut pictures);
        base
    });
    for aspect in aspects() {
        let changed = base
            .as_ref()
            .is_none_or(|base| (aspect.key)(base) != (aspect.key)(preset));
        if changed {
            (aspect.write)(preset, row, materials);
        }
    }
}

/// The kinds of column Sue writes, for a row of its own.
fn column_type(value: &Value, kept: &str) -> String {
    if !kept.is_empty() {
        return kept.to_owned();
    }
    match value {
        Value::Integer(_) => "INTEGER",
        Value::Real(_) => "REAL",
        Value::Text(_) => "TEXT",
        Value::Blob(_) | Value::Null => "BLOB",
    }
    .to_owned()
}

/// A `CREATE TABLE` for `name` with the columns of `rows` (the first row's
/// order, then any others').
fn create(name: &str, rows: &[&Row]) -> (String, Vec<String>) {
    let mut columns: Vec<(String, String)> = Vec::new();
    for row in rows {
        for (column, kind, value) in &row.columns {
            if column.eq_ignore_ascii_case("_PW_ID") {
                continue;
            }
            if !columns
                .iter()
                .any(|(known, _)| known.eq_ignore_ascii_case(column))
            {
                columns.push((column.clone(), column_type(value, kind)));
            }
        }
    }
    let list: Vec<String> = columns
        .iter()
        .map(|(column, kind)| format!("{column} {kind} DEFAULT NULL"))
        .collect();
    (
        format!(
            "CREATE TABLE {name}(_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT, {})",
            list.join(", ")
        ),
        columns.into_iter().map(|(column, _)| column).collect(),
    )
}

fn values(row: &Row, columns: &[String]) -> Vec<Value> {
    std::iter::once(Value::Null)
        .chain(
            columns
                .iter()
                .map(|column| row.get(column).cloned().unwrap_or(Value::Null)),
        )
        .collect()
}

/// A stable 16 byte id for `preset`'s node, from what it is.
fn uuid_of(preset: &BrushPreset) -> Vec<u8> {
    sha256(&preset.to_bytes())[..16].to_vec()
}

/// `preset` as a `.sut` file of one sub tool. `picture` gives the encoded
/// PNG of each picture the preset names by hash (from the document or the
/// brush library).
///
/// # Errors
/// When a picture the preset names is not at hand.
pub fn export(
    preset: &BrushPreset,
    picture: &dyn Fn(&ContentHash) -> Option<Vec<u8>>,
) -> Result<Vec<u8>, String> {
    // The materials: those kept from an import, then new ones.
    let mut materials: Vec<Material> = kept_materials(preset)
        .into_iter()
        .filter_map(|(hash, path, archive)| {
            let png = untar(&archive, "thumbnail/thumbnail.png")?.to_vec();
            let picture = TipPicture { png };
            (picture.hash() == hash).then_some(Material {
                path,
                archive,
                picture,
            })
        })
        .collect();
    for hash in named_pictures(preset) {
        if materials
            .iter()
            .any(|material| material.picture.hash() == hash)
        {
            continue;
        }
        let png = picture(&hash).ok_or(PICTURE_MISSING)?;
        let id = hex(&hash[..16]);
        let path = format!(".:sue:{id}:data:material_0.layer");
        let archive = tar(&[("thumbnail/thumbnail.png", &png)]);
        materials.push(Material {
            path,
            archive,
            picture: TipPicture { png },
        });
    }

    // The variant now and as first installed.
    let kept_row = Row::kept(preset, kept::VARIANT);
    let mut row = kept_row.clone().unwrap_or_default();
    write_row(preset, &mut row, &mut materials);
    let (current, initial) = (1_i64, 2_i64);
    row.set_int("VariantID", current);
    let mut first = Row::kept(preset, kept::INITIAL)
        .filter(|_| kept_row.is_some())
        .unwrap_or_else(|| row.clone());
    first.set_int("VariantID", initial);

    // The node.
    let mut node = Row::kept(preset, kept::NODE).unwrap_or_else(|| {
        let mut node = Row::default();
        node.set_blob("NodeUuid", uuid_of(preset));
        node.set_int("NodeShortCutKey", 0);
        node.set_int("NodeLock", 0);
        node.set_int("NodeInputOp", 10);
        node.set_int("NodeOutputOp", 10);
        node.set_int("NodeRangeOp", 0);
        node.set_int("NodeIcon", 111);
        node.set_int("NodeIconColor", 0);
        node.set_int("NodeHidden", 0);
        node.set_int("NodeInstalledState", 0);
        node.set_int("NodeInstalledVersion", 0);
        node.set_int("NodeHideByGrade", 0);
        node.set_int("NodeDefaultIdentifier", 0);
        node.set_blob("NodeNextUuid", vec![0]);
        node.set_blob("NodeFirstChildUuid", vec![0]);
        node.set_blob("NodeSelectedUuid", vec![0]);
        node
    });
    node.set("NodeName", "TEXT", Value::Text(preset.name.clone()));
    node.set_int("NodeVariantID", current);
    node.set_int("NodeInitVariantID", initial);
    let node_uuid = node
        .blob("NodeUuid")
        .map(<[u8]>::to_vec)
        .unwrap_or_else(|| uuid_of(preset));

    // The manager.
    let mut manager = Row::kept(preset, kept::MANAGER).unwrap_or_else(|| {
        let mut manager = Row::default();
        manager.set_int("ToolType", 0);
        manager.set_int("Version", 138);
        manager.set_blob("CurrentNodeUuid", vec![0]);
        manager.set_int("CommonVariantID", 44);
        manager.set_blob("ObjectNodeUuid", node_uuid.clone());
        manager.set_int("SavedCount", 1);
        manager.set_int("PressureGraphInitialized", 0);
        manager.set_int("UsePressureFile", 0);
        manager
    });
    manager.set_blob("RootUuid", node_uuid);
    manager.set_int("MaxVariantID", initial);

    // Sue's own table: the preset whole, without what was kept for Clip
    // Studio.
    let mut own = preset.clone();
    own.extra.retain(|(key, _)| !key.starts_with("sut."));

    let (manager_sql, manager_columns) = create("Manager", &[&manager]);
    let (node_sql, node_columns) = create("Node", &[&node]);
    let (variant_sql, variant_columns) = create("Variant", &[&row, &first]);
    let mut tables = vec![
        NewTable {
            name: "Manager".to_owned(),
            sql: manager_sql,
            rows: vec![(1, values(&manager, &manager_columns))],
        },
        NewTable {
            name: "Node".to_owned(),
            sql: node_sql,
            rows: vec![(1, values(&node, &node_columns))],
        },
        NewTable {
            name: "Variant".to_owned(),
            sql: variant_sql,
            rows: vec![
                (1, values(&row, &variant_columns)),
                (2, values(&first, &variant_columns)),
            ],
        },
    ];
    let used: Vec<&Material> = materials
        .iter()
        .filter(|material| named_pictures(preset).contains(&material.picture.hash()))
        .collect();
    if !used.is_empty() {
        tables.push(NewTable {
            name: "MaterialFile".to_owned(),
            sql: "CREATE TABLE MaterialFile(_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT, InstallFolder INTEGER DEFAULT NULL, OriginalPath TEXT DEFAULT NULL, OldMaterial INTEGER DEFAULT NULL, FileData BLOB DEFAULT NULL, CatalogPath TEXT DEFAULT NULL, MaterialUuid TEXT DEFAULT NULL)".to_owned(),
            rows: used
                .iter()
                .zip(1_i64..)
                .map(|(material, rowid)| {
                    let catalog = material
                        .path
                        .strip_suffix(":data:material_0.layer")
                        .unwrap_or(&material.path)
                        .to_owned();
                    (
                        rowid,
                        vec![
                            Value::Null,
                            Value::Integer(0),
                            Value::Text(material.path.clone()),
                            Value::Integer(0),
                            Value::Blob(material.archive.clone()),
                            Value::Text(catalog),
                            Value::Text(String::new()),
                        ],
                    )
                })
                .collect(),
        });
    }
    tables.push(NewTable {
        name: "SueBrush".to_owned(),
        sql: "CREATE TABLE SueBrush(_PW_ID INTEGER PRIMARY KEY AUTOINCREMENT, VariantID INTEGER DEFAULT NULL, Preset BLOB DEFAULT NULL)".to_owned(),
        rows: vec![(
            1,
            vec![Value::Null, Value::Integer(current), Value::Blob(own.to_bytes())],
        )],
    });
    Ok(sqlite::write(&tables))
}
