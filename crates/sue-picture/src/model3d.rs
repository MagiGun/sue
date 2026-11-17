//! 3D models from files, on 3D layers: glTF (`.gltf`, `.glb`) and
//! Wavefront OBJ.
//!
//! The file is kept whole inside the model entity as the shared
//! [`Encoded`] bytes (never copied to hand around), so a picture saved
//! with an imported model opens anywhere. A glTF model with a skeleton
//! becomes a figure of [`Kind::Imported`]: its bones are joints, posed
//! like a drawing figure's, each turn measured from the stance the file
//! was modeled in. A model without one is a plain model, drawn smooth.
//!
//! The glTF reader is Sue's own and small: triangle meshes, node
//! transforms, skins and their weights; buffers inside the file (GLB, or
//! base64 data) or beside it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use sue_core::{Entity, Value};
use sue_model::{Encoded, Geometry, PrimKind};

use crate::figure::human::Shaped;
use crate::figure::{self, Joint, Kind};

/// A plain imported model is drawn smooth, folds kept (`Bool`).
pub const SMOOTH: &str = "model.smooth";
/// The file extensions Sue Paint imports as 3D models.
pub const EXTENSIONS: [&str; 3] = ["glb", "gltf", "obj"];

type Mat4 = [[f32; 4]; 4];

const ONE: Mat4 = [
    [1.0, 0.0, 0.0, 0.0],
    [0.0, 1.0, 0.0, 0.0],
    [0.0, 0.0, 1.0, 0.0],
    [0.0, 0.0, 0.0, 1.0],
];

fn times(a: &Mat4, b: &Mat4) -> Mat4 {
    let mut out = [[0.0; 4]; 4];
    for (row, line) in out.iter_mut().enumerate() {
        for (column, value) in line.iter_mut().enumerate() {
            *value = (0..4).map(|k| a[row][k] * b[k][column]).sum();
        }
    }
    out
}

fn moved(m: &Mat4, p: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|row| m[row][0] * p[0] + m[row][1] * p[1] + m[row][2] * p[2] + m[row][3])
}

/// A glTF file read: its JSON and its buffers.
struct Gltf {
    json: serde_json::Value,
    buffers: Vec<Vec<u8>>,
}

/// Decodes base64 text (padding and line breaks skipped).
fn base64(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let (mut bits, mut held) = (0_u32, 0_u32);
    for byte in text.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => continue,
        };
        held = held << 6 | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((held >> bits) as u8);
        }
    }
    out
}

impl Gltf {
    /// Reads a `.glb` or `.gltf` file; `beside` fetches a buffer kept in
    /// a file next to it, by its name.
    fn read(bytes: &[u8], beside: &dyn Fn(&str) -> Option<Vec<u8>>) -> Result<Self, String> {
        let (json, mut inside) = if bytes.starts_with(b"glTF") {
            let mut at = 12;
            let (mut json, mut bin) = (None, None);
            while at + 8 <= bytes.len() {
                let length =
                    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
                        as usize;
                let kind = &bytes[at + 4..at + 8];
                let end = (at + 8 + length).min(bytes.len());
                let chunk = &bytes[at + 8..end];
                if kind == b"JSON" {
                    json = Some(chunk);
                } else if kind == b"BIN\0" {
                    bin = Some(chunk.to_vec());
                }
                at = end;
            }
            (json.ok_or("the file has no scene")?, bin)
        } else {
            (bytes, None)
        };
        let json: serde_json::Value =
            serde_json::from_slice(json).map_err(|error| error.to_string())?;
        let mut buffers = Vec::new();
        for buffer in json["buffers"].as_array().into_iter().flatten() {
            let data = match buffer["uri"].as_str() {
                None => inside.take().unwrap_or_default(),
                Some(uri) => match uri.strip_prefix("data:") {
                    Some(data) => base64(data.split_once(',').map_or("", |(_, text)| text)),
                    None => beside(uri).ok_or_else(|| format!("{uri} is missing"))?,
                },
            };
            buffers.push(data);
        }
        Ok(Self { json, buffers })
    }

    /// The same file as one self-contained `.glb`, its buffers inside.
    fn to_glb(&self) -> Vec<u8> {
        let mut json = self.json.clone();
        let mut bin: Vec<u8> = Vec::new();
        let mut starts = Vec::new();
        for buffer in &self.buffers {
            while !bin.len().is_multiple_of(4) {
                bin.push(0);
            }
            starts.push(bin.len());
            bin.extend_from_slice(buffer);
        }
        while !bin.len().is_multiple_of(4) {
            bin.push(0);
        }
        if let Some(views) = json["bufferViews"].as_array_mut() {
            for view in views {
                let buffer = view["buffer"].as_u64().unwrap_or(0) as usize;
                let offset = view["byteOffset"].as_u64().unwrap_or(0) as usize;
                view["buffer"] = 0.into();
                view["byteOffset"] = (offset + starts.get(buffer).copied().unwrap_or(0)).into();
            }
        }
        json["buffers"] = serde_json::json!([{ "byteLength": bin.len() }]);
        let mut text = serde_json::to_vec(&json).unwrap_or_default();
        while !text.len().is_multiple_of(4) {
            text.push(b' ');
        }
        let total = 12 + 8 + text.len() + 8 + bin.len();
        let mut out = Vec::with_capacity(total);
        out.extend_from_slice(b"glTF");
        out.extend_from_slice(&2_u32.to_le_bytes());
        out.extend_from_slice(&(total as u32).to_le_bytes());
        out.extend_from_slice(&(text.len() as u32).to_le_bytes());
        out.extend_from_slice(b"JSON");
        out.extend_from_slice(&text);
        out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        out.extend_from_slice(b"BIN\0");
        out.extend_from_slice(&bin);
        out
    }

    /// Accessor `index` as numbers: how many values each element has,
    /// and the elements one after another.
    fn numbers(&self, index: &serde_json::Value) -> Option<(usize, Vec<f32>)> {
        let accessor = &self.json["accessors"][index.as_u64()? as usize];
        let count = accessor["count"].as_u64()? as usize;
        let parts = match accessor["type"].as_str()? {
            "SCALAR" => 1,
            "VEC2" => 2,
            "VEC3" => 3,
            "VEC4" => 4,
            "MAT4" => 16,
            _ => return None,
        };
        let kind = accessor["componentType"].as_u64()?;
        let wide = match kind {
            5120 | 5121 => 1,
            5122 | 5123 => 2,
            5125 | 5126 => 4,
            _ => return None,
        };
        let normalized = accessor["normalized"].as_bool().unwrap_or(false);
        let view = &self.json["bufferViews"][accessor["bufferView"].as_u64()? as usize];
        let buffer = self.buffers.get(view["buffer"].as_u64()? as usize)?;
        let start = view["byteOffset"].as_u64().unwrap_or(0) as usize
            + accessor["byteOffset"].as_u64().unwrap_or(0) as usize;
        let stride = view["byteStride"]
            .as_u64()
            .map_or(parts * wide, |stride| stride as usize);
        let mut out = Vec::with_capacity(count * parts);
        for element in 0..count {
            for part in 0..parts {
                let at = start + element * stride + part * wide;
                let raw = buffer.get(at..at + wide)?;
                out.push(match kind {
                    5120 => {
                        let value = f32::from(raw[0] as i8);
                        if normalized { (value / 127.0).max(-1.0) } else { value }
                    }
                    5121 => {
                        let value = f32::from(raw[0]);
                        if normalized { value / 255.0 } else { value }
                    }
                    5122 => {
                        let value = f32::from(i16::from_le_bytes([raw[0], raw[1]]));
                        if normalized { (value / 32767.0).max(-1.0) } else { value }
                    }
                    5123 => {
                        let value = f32::from(u16::from_le_bytes([raw[0], raw[1]]));
                        if normalized { value / 65535.0 } else { value }
                    }
                    5125 => u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]) as f32,
                    _ => f32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]),
                });
            }
        }
        Some((parts, out))
    }

    /// Each node's own transform.
    fn local(node: &serde_json::Value) -> Mat4 {
        let list = |key: &str, count: usize| -> Option<Vec<f32>> {
            let values: Vec<f32> = node[key]
                .as_array()?
                .iter()
                .filter_map(|value| value.as_f64().map(|value| value as f32))
                .collect();
            (values.len() == count).then_some(values)
        };
        if let Some(m) = list("matrix", 16) {
            // Column after column in the file.
            return [0, 1, 2, 3].map(|row| [0, 1, 2, 3].map(|column| m[column * 4 + row]));
        }
        let t = list("translation", 3).unwrap_or_else(|| vec![0.0; 3]);
        let [x, y, z, w]: [f32; 4] = list("rotation", 4)
            .and_then(|q| q.try_into().ok())
            .unwrap_or([0.0, 0.0, 0.0, 1.0]);
        let s = list("scale", 3).unwrap_or_else(|| vec![1.0; 3]);
        let turn = [
            [1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - z * w), 2.0 * (x * z + y * w)],
            [2.0 * (x * y + z * w), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - x * w)],
            [2.0 * (x * z - y * w), 2.0 * (y * z + x * w), 1.0 - 2.0 * (x * x + y * y)],
        ];
        let mut out = ONE;
        for row in 0..3 {
            for column in 0..3 {
                out[row][column] = turn[row][column] * s[column];
            }
            out[row][3] = t[row];
        }
        out
    }
}

/// A model read from a file: its points and triangles as modeled, and
/// its skeleton when it has one.
struct Parsed {
    points: Vec<[f32; 3]>,
    tris: Vec<u32>,
    /// Up to four bones and weights a point (empty without a skeleton).
    skin: Vec<[(u16, f32); 4]>,
    /// Each bone: its name, its parent and where it is.
    bones: Vec<(String, Option<usize>, [f32; 3])>,
}

fn parse(gltf: &Gltf) -> Result<Parsed, String> {
    let nodes = gltf.json["nodes"].as_array().cloned().unwrap_or_default();
    let mut parent: Vec<Option<usize>> = vec![None; nodes.len()];
    for (index, node) in nodes.iter().enumerate() {
        for child in node["children"].as_array().into_iter().flatten() {
            if let Some(child) = child.as_u64().map(|child| child as usize)
                && child < nodes.len()
            {
                parent[child] = Some(index);
            }
        }
    }
    // Every node's place in the world: its parents' transforms, then its
    // own.
    let mut world: Vec<Option<Mat4>> = vec![None; nodes.len()];
    fn place(
        index: usize,
        nodes: &[serde_json::Value],
        parent: &[Option<usize>],
        world: &mut Vec<Option<Mat4>>,
        depth: usize,
    ) -> Mat4 {
        if let Some(found) = world[index] {
            return found;
        }
        let own = Gltf::local(&nodes[index]);
        let found = match parent[index] {
            Some(above) if depth < 256 => {
                times(&place(above, nodes, parent, world, depth + 1), &own)
            }
            _ => own,
        };
        world[index] = Some(found);
        found
    }
    for index in 0..nodes.len() {
        place(index, &nodes, &parent, &mut world, 0);
    }
    let world: Vec<Mat4> = world.into_iter().map(|m| m.unwrap_or(ONE)).collect();
    let mut out = Parsed {
        points: Vec::new(),
        tris: Vec::new(),
        skin: Vec::new(),
        bones: Vec::new(),
    };
    // The bones of every skin, each node once.
    let mut bone_of: HashMap<usize, usize> = HashMap::new();
    let skins = gltf.json["skins"].as_array().cloned().unwrap_or_default();
    for skin in &skins {
        for joint in skin["joints"].as_array().into_iter().flatten() {
            let Some(node) = joint.as_u64().map(|node| node as usize) else {
                continue;
            };
            if node < nodes.len() && !bone_of.contains_key(&node) {
                bone_of.insert(node, out.bones.len());
                let name = nodes[node]["name"]
                    .as_str()
                    .map_or_else(|| format!("Bone {}", out.bones.len() + 1), str::to_owned);
                out.bones.push((name, None, moved(&world[node], [0.0; 3])));
            }
        }
    }
    for (&node, &bone) in &bone_of {
        let mut above = parent[node];
        while let Some(at) = above {
            if let Some(found) = bone_of.get(&at) {
                out.bones[bone].1 = Some(*found);
                break;
            }
            above = parent[at];
        }
    }
    let mut any_skin = false;
    for (index, node) in nodes.iter().enumerate() {
        let Some(mesh) = node["mesh"].as_u64() else {
            continue;
        };
        let skin = node["skin"].as_u64().and_then(|skin| skins.get(skin as usize));
        // How each of the skin's joints carries a point as modeled.
        let carried: Vec<(usize, Mat4)> = skin
            .map(|skin| {
                let inverse = gltf.numbers(&skin["inverseBindMatrices"]);
                skin["joints"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .enumerate()
                    .filter_map(|(at, joint)| {
                        let node = joint.as_u64()? as usize;
                        let bind = inverse
                            .as_ref()
                            .filter(|(parts, values)| *parts == 16 && values.len() >= (at + 1) * 16)
                            .map_or(ONE, |(_, values)| {
                                let m = &values[at * 16..at * 16 + 16];
                                [0, 1, 2, 3].map(|row| [0, 1, 2, 3].map(|column| m[column * 4 + row]))
                            });
                        Some((*bone_of.get(&node)?, times(world.get(node)?, &bind)))
                    })
                    .collect()
            })
            .unwrap_or_default();
        for prim in gltf.json["meshes"][mesh as usize]["primitives"]
            .as_array()
            .into_iter()
            .flatten()
        {
            if prim["mode"].as_u64().unwrap_or(4) != 4 {
                continue;
            }
            let Some((3, positions)) = gltf.numbers(&prim["attributes"]["POSITION"]) else {
                continue;
            };
            let count = positions.len() / 3;
            let joints = gltf
                .numbers(&prim["attributes"]["JOINTS_0"])
                .filter(|(parts, values)| *parts == 4 && values.len() == count * 4);
            let weights = gltf
                .numbers(&prim["attributes"]["WEIGHTS_0"])
                .filter(|(parts, values)| *parts == 4 && values.len() == count * 4);
            let first = out.points.len() as u32;
            for at in 0..count {
                let p = [positions[at * 3], positions[at * 3 + 1], positions[at * 3 + 2]];
                let mut mine = [(0_u16, 0.0_f32); 4];
                let mut placed = [0.0_f32; 3];
                let mut total = 0.0;
                if let (Some((_, joints)), Some((_, weights)), false) =
                    (&joints, &weights, carried.is_empty())
                {
                    for k in 0..4 {
                        let weight = weights[at * 4 + k];
                        let Some((bone, carry)) = carried.get(joints[at * 4 + k] as usize) else {
                            continue;
                        };
                        if weight <= 0.0 {
                            continue;
                        }
                        mine[k] = (*bone as u16, weight);
                        let q = moved(carry, p);
                        for axis in 0..3 {
                            placed[axis] += q[axis] * weight;
                        }
                        total += weight;
                    }
                }
                if total > 1e-6 {
                    any_skin = true;
                    for slot in &mut mine {
                        slot.1 /= total;
                    }
                    out.points.push(placed.map(|value| value / total));
                } else {
                    mine = [(0, 0.0); 4];
                    out.points.push(moved(&world[index], p));
                }
                out.skin.push(mine);
            }
            match gltf.numbers(&prim["indices"]) {
                Some((1, indices)) => {
                    for t in indices.as_chunks::<3>().0 {
                        if t.iter().all(|i| (*i as usize) < count) {
                            out.tris.extend(t.map(|i| first + i as u32));
                        }
                    }
                }
                _ => {
                    for t in 0..count / 3 {
                        let at = first + (t * 3) as u32;
                        out.tris.extend([at, at + 1, at + 2]);
                    }
                }
            }
        }
    }
    if out.tris.is_empty() {
        return Err(tr!("file-has-no-model"));
    }
    if !any_skin {
        out.skin.clear();
        out.bones.clear();
    }
    Ok(out)
}

/// A name kept for as long as Sue runs, so joints can carry it. The same
/// name is kept once.
fn kept_name(name: &str) -> &'static str {
    static NAMES: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
    let mut names = NAMES.lock().unwrap_or_else(|poison| poison.into_inner());
    if let Some(found) = names.iter().find(|kept| **kept == name) {
        return found;
    }
    let kept: &'static str = Box::leak(name.to_owned().into_boxed_str());
    names.push(kept);
    kept
}

/// A skinned model as a figure to pose: every bone a joint, its turn
/// measured from the stance it was modeled in.
fn rigged(parsed: &Parsed) -> Option<Shaped> {
    if parsed.bones.is_empty() || parsed.bones.len() > 255 {
        return None;
    }
    let lower: Vec<String> = parsed.bones.iter().map(|bone| bone.0.to_lowercase()).collect();
    // Where each bone ends: at the middle of the bones hanging from it,
    // or a little further along from its parent.
    let mut joints: Vec<Joint> = Vec::with_capacity(parsed.bones.len());
    for (index, (name, parent, head)) in parsed.bones.iter().enumerate() {
        let children: Vec<[f32; 3]> = parsed
            .bones
            .iter()
            .filter(|bone| bone.1 == Some(index))
            .map(|bone| bone.2)
            .collect();
        let from = parent.map_or([0.0; 3], |parent| parsed.bones[parent].2);
        let offset = [0, 1, 2].map(|axis| head[axis] - from[axis]);
        let bone = if children.is_empty() {
            offset.map(|value| value * 0.4)
        } else {
            [0, 1, 2].map(|axis| {
                children.iter().map(|child| child[axis]).sum::<f32>() / children.len() as f32
                    - head[axis]
            })
        };
        // Left and right bones mirror each other when their names say so.
        let other = if lower[index].contains("left") {
            lower[index].replace("left", "right")
        } else if lower[index].contains("right") {
            lower[index].replace("right", "left")
        } else {
            String::new()
        };
        joints.push(Joint {
            name: kept_name(name),
            parent: *parent,
            offset: if parent.is_some() { offset } else { *head },
            bone,
            start: [0.01; 2],
            end: [0.01; 2],
            mirror: lower.iter().position(|name| !other.is_empty() && *name == other),
        });
    }
    // A hand or foot with two bones above it is a limb that follows it.
    let finger = ["index", "thumb", "middle", "ring", "pinky", "little", "finger", "toe"];
    let mut limbs = Vec::new();
    for (index, name) in lower.iter().enumerate() {
        if !(name.contains("hand") || name.contains("foot") || name.contains("ankle"))
            || finger.iter().any(|part| name.contains(part))
        {
            continue;
        }
        if let Some(low) = joints[index].parent
            && let Some(up) = joints[low].parent
        {
            limbs.push([up, low, index]);
        }
    }
    let mut order = Vec::with_capacity(joints.len());
    let mut done = vec![false; joints.len()];
    while order.len() < joints.len() {
        let before = order.len();
        for (index, joint) in joints.iter().enumerate() {
            if !done[index] && joint.parent.is_none_or(|parent| done[parent]) {
                done[index] = true;
                order.push(index);
            }
        }
        if order.len() == before {
            return None;
        }
    }
    let skin = parsed
        .skin
        .iter()
        .map(|mine| {
            let total: f32 = mine.iter().map(|(_, weight)| weight).sum();
            if total <= 1e-6 {
                // A point no bone carries stays with the first.
                let mut held = [(0_u8, 0.0_f32); 4];
                held[0] = (0, 1.0);
                held
            } else {
                mine.map(|(bone, weight)| (bone as u8, weight / total))
            }
        })
        .collect();
    Some(Shaped {
        rest: parsed.points.clone(),
        skin,
        tris: parsed.tris.clone(),
        bind: parsed
            .bones
            .iter()
            .map(|bone| (figure::IDENTITY, bone.2))
            .collect(),
        joints,
        order,
        guides: Vec::new(),
        detail: Vec::new(),
        axes: Vec::new(),
        relief: None,
        hands: [figure::IDENTITY; 2],
        thumb_axes: Vec::new(),
        frame: Some(crate::three::frame_of_points(&parsed.points)),
        limbs,
    })
}

/// The skeletons of the imported models seen so far, by the number
/// their file's bytes hash to.
static RIGS: Mutex<Vec<(u64, Option<Arc<Shaped>>)>> = Mutex::new(Vec::new());
/// The files hashed so far: where their bytes are, how many, and their
/// number.
static SEEN: Mutex<Vec<(usize, usize, u64)>> = Mutex::new(Vec::new());

fn hash(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for chunk in bytes.chunks(8) {
        let mut word = [0_u8; 8];
        word[..chunk.len()].copy_from_slice(chunk);
        hash = (hash ^ u64::from_le_bytes(word)).wrapping_mul(0x0100_0000_01b3);
    }
    hash | 1
}

/// The number of the skeleton of imported figure `entity` (0 when its
/// file holds none), read from its kept file the first time.
#[must_use]
pub fn rig_of(entity: &Entity) -> u64 {
    let Some(file) = entity.ext::<Encoded>() else {
        return 0;
    };
    let bytes: &[u8] = &file.bytes;
    let key = (bytes.as_ptr() as usize, bytes.len());
    let known = SEEN
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .iter()
        .find(|(at, length, _)| (*at, *length) == key)
        .map(|found| found.2);
    let id = known.unwrap_or_else(|| {
        let id = hash(bytes);
        let mut seen = SEEN.lock().unwrap_or_else(|poison| poison.into_inner());
        if seen.len() >= 64 {
            seen.remove(0);
        }
        seen.push((key.0, key.1, id));
        id
    });
    let mut rigs = RIGS.lock().unwrap_or_else(|poison| poison.into_inner());
    if let Some((_, found)) = rigs.iter().find(|(known, _)| *known == id) {
        return if found.is_some() { id } else { 0 };
    }
    let rig = Gltf::read(bytes, &|_| None)
        .and_then(|gltf| parse(&gltf))
        .ok()
        .and_then(|parsed| rigged(&parsed))
        .map(Arc::new);
    let found = rig.is_some();
    if rigs.len() >= 16 {
        rigs.remove(0);
    }
    rigs.push((id, rig));
    if found { id } else { 0 }
}

/// The skeleton numbered `id`, if it was read.
pub(crate) fn rig(id: u64) -> Option<Arc<Shaped>> {
    RIGS.lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .iter()
        .find(|(known, _)| *known == id)
        .and_then(|(_, rig)| rig.clone())
}

/// The model in the bytes of a `.glb`, `.gltf` or `.obj` file as a
/// model entity named `name`, its file kept inside it. `beside` fetches
/// a file kept next to a `.gltf` (its buffers), by name. A glTF model
/// with a skeleton is a figure to pose; any other is a plain model.
///
/// # Errors
/// If the file cannot be read as a model.
pub fn import(
    bytes: Vec<u8>,
    name: &str,
    extension: &str,
    beside: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> Result<Entity, String> {
    use sue_core::FormatHandler as _;
    if extension.eq_ignore_ascii_case("obj") {
        let bundle = sue_io::ObjFormat
            .import(&bytes, name, &sue_core::Properties::new())
            .map_err(|error| error.to_string())?;
        let model = bundle
            .entities
            .into_iter()
            .next()
            .ok_or_else(|| tr!("file-has-no-model"))?;
        if model
            .ext::<Geometry>()
            .is_none_or(|geometry| geometry.to_mesh().indices().is_empty())
        {
            return Err(tr!("file-has-no-model"));
        }
        return Ok(model
            .with_ext(Encoded::new("obj", bytes))
            .with_property(SMOOTH, Value::from(true)));
    }
    let gltf = Gltf::read(&bytes, beside)?;
    let parsed = parse(&gltf)?;
    // Kept as one file with its buffers inside, whatever it came as.
    let kept = if bytes.starts_with(b"glTF") {
        bytes
    } else {
        gltf.to_glb()
    };
    if rigged(&parsed).is_some() {
        let count = parsed.bones.len();
        return Ok(Entity::empty(name, figure::CLASS)
            .with_ext(Encoded::new("glb", kept))
            .with_property(figure::KIND, Value::from(Kind::Imported.key()))
            .with_property(
                figure::POSE,
                Value::from(figure::Pose(vec![[0.0; 3]; count]).to_text()),
            ));
    }
    let mut geometry = Geometry::new();
    let ids: Vec<u32> = parsed
        .points
        .iter()
        .map(|p| geometry.add_point(p.map(f64::from)))
        .collect();
    for t in parsed.tris.as_chunks::<3>().0 {
        let _ = geometry.add_prim(PrimKind::Polygon, &t.map(|i| ids[i as usize]));
    }
    Ok(Entity::empty(name, sue_model::class::MODEL)
        .with_ext(geometry)
        .with_ext(Encoded::new("glb", kept))
        .with_property(SMOOTH, Value::from(true)))
}

/// `true` if `entity` is a model that came from a file.
#[must_use]
pub fn is_imported(entity: &Entity) -> bool {
    entity.ext::<Encoded>().is_some()
        && (entity.ext::<Geometry>().is_some() || figure::is_figure(entity))
}

/// The kept file of imported model `entity`: its bytes and the
/// extension they are saved with.
#[must_use]
pub fn file_of(entity: &Entity) -> Option<(&[u8], &str)> {
    let file = entity.ext::<Encoded>()?;
    Some((&file.bytes, file.format.as_str()))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A small skinned model as `.gltf` text with its buffer inside: a
    /// square post of two boxes, the upper one carried by a second bone
    /// (`Arm`) hanging from the first (`Root`).
    pub(crate) fn post_gltf() -> Vec<u8> {
        let mut points: Vec<[f32; 3]> = Vec::new();
        let mut joints: Vec<[u8; 4]> = Vec::new();
        let mut weights: Vec<[f32; 4]> = Vec::new();
        for level in 0..3 {
            for (x, z) in [(-0.1, -0.1), (0.1, -0.1), (0.1, 0.1), (-0.1, 0.1)] {
                points.push([x, level as f32 * 0.5, z]);
                joints.push([0, 1, 0, 0]);
                weights.push(match level {
                    0 => [1.0, 0.0, 0.0, 0.0],
                    1 => [0.5, 0.5, 0.0, 0.0],
                    _ => [0.0, 1.0, 0.0, 0.0],
                });
            }
        }
        let mut indices: Vec<u16> = Vec::new();
        for level in 0..2_u16 {
            for side in 0..4_u16 {
                let (a, b) = (level * 4 + side, level * 4 + (side + 1) % 4);
                indices.extend([a, b, b + 4, a, b + 4, a + 4]);
            }
        }
        let mut bin: Vec<u8> = Vec::new();
        for p in &points {
            for value in p {
                bin.extend(value.to_le_bytes());
            }
        }
        let joints_at = bin.len();
        for j in &joints {
            bin.extend(j);
        }
        let weights_at = bin.len();
        for w in &weights {
            for value in w {
                bin.extend(value.to_le_bytes());
            }
        }
        let indices_at = bin.len();
        for index in &indices {
            bin.extend(index.to_le_bytes());
        }
        const TABLE: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut text = String::new();
        for chunk in bin.chunks(3) {
            let word = u32::from(chunk[0]) << 16
                | u32::from(*chunk.get(1).unwrap_or(&0)) << 8
                | u32::from(*chunk.get(2).unwrap_or(&0));
            for k in 0..4 {
                if k <= chunk.len() {
                    text.push(TABLE[(word >> (18 - 6 * k) & 63) as usize] as char);
                } else {
                    text.push('=');
                }
            }
        }
        serde_json::to_vec(&serde_json::json!({
            "asset": { "version": "2.0" },
            "scene": 0,
            "scenes": [{ "nodes": [0, 1] }],
            "nodes": [
                { "name": "Post", "mesh": 0, "skin": 0 },
                { "name": "Root", "children": [2] },
                { "name": "Arm", "translation": [0.0, 0.5, 0.0] }
            ],
            "skins": [{ "joints": [1, 2] }],
            "meshes": [{ "primitives": [{
                "attributes": { "POSITION": 0, "JOINTS_0": 1, "WEIGHTS_0": 2 },
                "indices": 3
            }] }],
            "buffers": [{
                "byteLength": bin.len(),
                "uri": format!("data:application/octet-stream;base64,{text}")
            }],
            "bufferViews": [
                { "buffer": 0, "byteOffset": 0, "byteLength": joints_at },
                { "buffer": 0, "byteOffset": joints_at, "byteLength": weights_at - joints_at },
                { "buffer": 0, "byteOffset": weights_at, "byteLength": indices_at - weights_at },
                { "buffer": 0, "byteOffset": indices_at, "byteLength": bin.len() - indices_at }
            ],
            "accessors": [
                { "bufferView": 0, "componentType": 5126, "count": 12, "type": "VEC3" },
                { "bufferView": 1, "componentType": 5121, "count": 12, "type": "VEC4" },
                { "bufferView": 2, "componentType": 5126, "count": 12, "type": "VEC4" },
                { "bufferView": 3, "componentType": 5123, "count": indices.len(), "type": "SCALAR" }
            ]
        }))
        .unwrap()
    }

    #[test]
    fn a_skinned_gltf_becomes_a_figure_posed_by_its_bones() {
        let model = import(post_gltf(), "Post", "gltf", &|_| None).unwrap();
        assert!(figure::is_figure(&model));
        let body = figure::Body::of(&model);
        assert_eq!(body.kind, Kind::Imported);
        let joints = body.joints();
        assert_eq!(joints.len(), 2);
        assert_eq!(joints[1].name, "Arm");
        assert_eq!(joints[1].parent, Some(0));
        // Kept inside as one self-contained GLB.
        let (bytes, extension) = file_of(&model).unwrap();
        assert_eq!(extension, "glb");
        assert!(bytes.starts_with(b"glTF"));
        // At rest the post stands a meter tall; turning the arm bone a
        // quarter round lays its top over to the side.
        let rest = figure::Pose::of(&model);
        let top = |pose: &figure::Pose| {
            figure::surface(pose, &body)
                .positions
                .iter()
                .fold(f32::MIN, |top, p| top.max(p[1]))
        };
        assert!((top(&rest) - 1.0).abs() < 1e-4);
        let mut bent = rest.clone();
        bent.0[1] = [0.0, 0.0, 90.0];
        assert!(top(&bent) < 0.75, "{}", top(&bent));
        let side = figure::surface(&bent, &body)
            .positions
            .iter()
            .fold(f32::MAX, |least, p| least.min(p[0]));
        assert!(side < -0.45, "the top lies over: {side}");
        // The pose text round trips, and the kept file reads again.
        let again = import(bytes.to_vec(), "Post", "glb", &|_| None).unwrap();
        assert_eq!(figure::Body::of(&again).joints().len(), 2);
    }

    #[test]
    fn an_obj_and_an_unskinned_gltf_are_plain_models_kept_whole() {
        let obj = b"v 0 0 0\nv 1 0 0\nv 0 1 0\nv 0 0 1\nf 1 2 3\nf 1 3 4\nf 1 4 2\nf 2 4 3\n";
        let model = import(obj.to_vec(), "Pyramid", "obj", &|_| None).unwrap();
        assert!(!figure::is_figure(&model));
        assert!(is_imported(&model));
        assert_eq!(file_of(&model).unwrap().0, obj);
        assert_eq!(model.property(SMOOTH), Some(&Value::from(true)));
        assert!(import(b"nothing".to_vec(), "Empty", "obj", &|_| None).is_err());
        assert!(import(b"{}".to_vec(), "Empty", "gltf", &|_| None).is_err());
        assert_eq!(base64("SGVsbG8="), b"Hello");
    }
}
