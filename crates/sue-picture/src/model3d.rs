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
//! transforms, skins and their weights, base colors (the factor, the
//! picture and the colors at the points), morph targets; buffers and
//! pictures inside the file (GLB, or base64 data) or beside it. An OBJ
//! file with a material file beside it is read with its diffuse colors
//! and pictures and kept as one GLB.
//!
//! How a model is shown is its [`Look`]: its own colors or plain clay
//! ([`COLORS`]) and the weights of its morph targets ([`MORPHS`]), both
//! properties of the model entity.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use sue_core::{Entity, Value};
use sue_model::{Encoded, Geometry, PrimKind};

use crate::figure::human::Shaped;
use crate::figure::{self, Joint, Kind};
use crate::three::{Placement, Surface, Texture};

/// A plain imported model is drawn smooth, folds kept (`Bool`).
pub const SMOOTH: &str = "model.smooth";
/// An imported model shows its own colors and pictures (`Bool`; on
/// unless it is set off, which shows it as plain gray clay).
pub const COLORS: &str = "model.colors";
/// The weights of an imported model's morph targets that are not as
/// the file has them (`Text`: `name=value`, `;` between).
pub const MORPHS: &str = "model.morphs";
/// The most bones a model may have.
pub const MOST_BONES: usize = u16::MAX as usize;
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

/// A glTF file read: its JSON, its buffers and the pictures that were
/// kept in files beside it (by their number among its images).
struct Gltf {
    json: serde_json::Value,
    buffers: Vec<Vec<u8>>,
    beside: Vec<Option<Vec<u8>>>,
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
        // Pictures kept in files of their own, taken along.
        let beside = json["images"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|image| {
                image["uri"]
                    .as_str()
                    .filter(|uri| !uri.starts_with("data:"))
                    .and_then(beside)
            })
            .collect();
        Ok(Self {
            json,
            buffers,
            beside,
        })
    }

    /// The bytes of image `index`: a picture file's, whether it lies in
    /// a buffer, in the text itself or was beside the file.
    fn image(&self, index: usize) -> Option<Vec<u8>> {
        if let Some(Some(found)) = self.beside.get(index) {
            return Some(found.clone());
        }
        let image = &self.json["images"][index];
        if let Some(data) = image["uri"]
            .as_str()
            .and_then(|uri| uri.strip_prefix("data:"))
        {
            return Some(base64(data.split_once(',').map_or("", |(_, text)| text)));
        }
        let view = &self.json["bufferViews"][image["bufferView"].as_u64()? as usize];
        let buffer = self.buffers.get(view["buffer"].as_u64()? as usize)?;
        let start = view["byteOffset"].as_u64().unwrap_or(0) as usize;
        let length = view["byteLength"].as_u64()? as usize;
        buffer.get(start..start + length).map(<[u8]>::to_vec)
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
        // The pictures that were beside the file go inside too.
        for (index, picture) in self.beside.iter().enumerate() {
            let Some(picture) = picture else {
                continue;
            };
            let at = bin.len();
            bin.extend_from_slice(picture);
            while !bin.len().is_multiple_of(4) {
                bin.push(0);
            }
            let kind = if picture.starts_with(b"\x89PNG") {
                "image/png"
            } else {
                "image/jpeg"
            };
            let view = serde_json::json!({
                "buffer": 0, "byteOffset": at, "byteLength": picture.len()
            });
            let views = match json["bufferViews"].as_array_mut() {
                Some(views) => views,
                None => {
                    json["bufferViews"] = serde_json::json!([]);
                    json["bufferViews"]
                        .as_array_mut()
                        .expect("an array was just put there")
                }
            };
            views.push(view);
            let number = views.len() - 1;
            if let Some(image) = json["images"][index].as_object_mut() {
                image.remove("uri");
                image.insert("bufferView".into(), number.into());
                image.insert("mimeType".into(), kind.into());
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
                        if normalized {
                            (value / 127.0).max(-1.0)
                        } else {
                            value
                        }
                    }
                    5121 => {
                        let value = f32::from(raw[0]);
                        if normalized { value / 255.0 } else { value }
                    }
                    5122 => {
                        let value = f32::from(i16::from_le_bytes([raw[0], raw[1]]));
                        if normalized {
                            (value / 32767.0).max(-1.0)
                        } else {
                            value
                        }
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
            [
                1.0 - 2.0 * (y * y + z * z),
                2.0 * (x * y - z * w),
                2.0 * (x * z + y * w),
            ],
            [
                2.0 * (x * y + z * w),
                1.0 - 2.0 * (x * x + z * z),
                2.0 * (y * z - x * w),
            ],
            [
                2.0 * (x * z - y * w),
                2.0 * (y * z + x * w),
                1.0 - 2.0 * (x * x + y * y),
            ],
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

/// One morph target of a model: its name, its weight as the file has
/// it, and how far it moves each point at weight 1.
struct Morph {
    name: String,
    rest: f32,
    moves: Vec<[f32; 3]>,
}

/// A model read from a file: its points and triangles as modeled, its
/// skeleton when it has one, its colors and its morph targets.
struct Parsed {
    points: Vec<[f32; 3]>,
    tris: Vec<u32>,
    /// Up to four bones and weights a point (empty without a skeleton).
    skin: Vec<[(u16, f32); 4]>,
    /// Each bone: its name, its parent and where it is.
    bones: Vec<(String, Option<usize>, [f32; 3])>,
    /// Each point's own color, as shown (empty for a model with no
    /// colors of its own).
    colors: Vec<[f32; 3]>,
    /// Each point's place on its picture (empty without pictures).
    uvs: Vec<[f32; 2]>,
    /// For each triangle, which picture lies over it (`u32::MAX` for
    /// none; empty without pictures).
    skins: Vec<u32>,
    textures: Vec<Arc<Texture>>,
    morphs: Vec<Morph>,
}

/// A linear color value as it is shown.
fn shown(linear: f32) -> f32 {
    linear.clamp(0.0, 1.0).powf(1.0 / 2.2)
}

/// A direction carried by `m` (its turn and scale, not its shift).
fn carried(m: &Mat4, d: [f32; 3]) -> [f32; 3] {
    [0, 1, 2].map(|row| m[row][0] * d[0] + m[row][1] * d[1] + m[row][2] * d[2])
}

/// A picture file's bytes as a texture.
fn texture_of(bytes: &[u8]) -> Option<Texture> {
    let decoded = if bytes.starts_with(b"\x89PNG") {
        sue_io::decode_png(bytes)
    } else {
        sue_io::decode_jpeg(bytes)
    }
    .ok()?;
    Some(Texture {
        size: [decoded.tile.width(), decoded.tile.height()],
        rgba: decoded.tile.bytes()?.to_vec(),
    })
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
        colors: Vec::new(),
        uvs: Vec::new(),
        skins: Vec::new(),
        textures: Vec::new(),
        morphs: Vec::new(),
    };
    // The pictures read so far, by their number among the file's images.
    let mut pictures: HashMap<usize, Option<u32>> = HashMap::new();
    let mut tinted = false;
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
        let skin = node["skin"]
            .as_u64()
            .and_then(|skin| skins.get(skin as usize));
        // How each of the skin's joints carries a point as modeled.
        let carried_by: Vec<(usize, Mat4)> = skin
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
                                [0, 1, 2, 3]
                                    .map(|row| [0, 1, 2, 3].map(|column| m[column * 4 + row]))
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
            // Its base color: a factor, a picture, colors at the points.
            let pbr = prim["material"]
                .as_u64()
                .map(|material| &gltf.json["materials"][material as usize]["pbrMetallicRoughness"]);
            let factor = pbr
                .and_then(|pbr| pbr["baseColorFactor"].as_array())
                .map_or([1.0_f32; 3], |factor| {
                    [0, 1, 2].map(|k| {
                        factor
                            .get(k)
                            .and_then(serde_json::Value::as_f64)
                            .unwrap_or(1.0) as f32
                    })
                });
            let laid = pbr.map(|pbr| &pbr["baseColorTexture"]);
            let picture = laid
                .and_then(|laid| laid["index"].as_u64())
                .and_then(|texture| gltf.json["textures"][texture as usize]["source"].as_u64())
                .and_then(|image| {
                    *pictures.entry(image as usize).or_insert_with(|| {
                        let texture = texture_of(&gltf.image(image as usize)?)?;
                        out.textures.push(Arc::new(texture));
                        Some(out.textures.len() as u32 - 1)
                    })
                });
            let set = laid.and_then(|laid| laid["texCoord"].as_u64()).unwrap_or(0);
            let places = gltf
                .numbers(&prim["attributes"][format!("TEXCOORD_{set}").as_str()])
                .filter(|(parts, values)| *parts == 2 && values.len() == count * 2);
            let tints = gltf
                .numbers(&prim["attributes"]["COLOR_0"])
                .filter(|(parts, values)| (3..=4).contains(parts) && values.len() == count * parts);
            if factor != [1.0; 3] || tints.is_some() {
                tinted = true;
            }
            let mut blends: Vec<Mat4> = Vec::with_capacity(count);
            let joints = gltf
                .numbers(&prim["attributes"]["JOINTS_0"])
                .filter(|(parts, values)| *parts == 4 && values.len() == count * 4);
            let weights = gltf
                .numbers(&prim["attributes"]["WEIGHTS_0"])
                .filter(|(parts, values)| *parts == 4 && values.len() == count * 4);
            let first = out.points.len() as u32;
            for at in 0..count {
                let p = [
                    positions[at * 3],
                    positions[at * 3 + 1],
                    positions[at * 3 + 2],
                ];
                let mut mine = [(0_u16, 0.0_f32); 4];
                let mut placed = [0.0_f32; 3];
                let mut total = 0.0;
                if let (Some((_, joints)), Some((_, weights)), false) =
                    (&joints, &weights, carried_by.is_empty())
                {
                    for k in 0..4 {
                        let weight = weights[at * 4 + k];
                        let Some((bone, carry)) = carried_by.get(joints[at * 4 + k] as usize)
                        else {
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
                    // How the point's bones carry it together.
                    let mut blend = [[0.0_f32; 4]; 4];
                    if let (Some((_, joints)), Some((_, weights))) = (&joints, &weights) {
                        for k in 0..4 {
                            let weight = weights[at * 4 + k];
                            if weight <= 0.0 {
                                continue;
                            }
                            if let Some((_, carry)) = carried_by.get(joints[at * 4 + k] as usize) {
                                for row in 0..4 {
                                    for column in 0..4 {
                                        blend[row][column] += carry[row][column] * weight / total;
                                    }
                                }
                            }
                        }
                    }
                    blends.push(blend);
                } else {
                    mine = [(0, 0.0); 4];
                    out.points.push(moved(&world[index], p));
                    blends.push(world[index]);
                }
                out.skin.push(mine);
                let tint = tints.as_ref().map_or([1.0; 3], |(parts, values)| {
                    [0, 1, 2].map(|k| values[at * parts + k])
                });
                out.colors
                    .push([0, 1, 2].map(|k| shown(factor[k] * tint[k])));
                out.uvs.push(
                    places
                        .as_ref()
                        .map_or([0.0; 2], |(_, values)| [values[at * 2], values[at * 2 + 1]]),
                );
            }
            // Its morph targets, each named by the mesh or numbered.
            for (number, target) in prim["targets"].as_array().into_iter().flatten().enumerate() {
                let Some((3, moves)) = gltf.numbers(&target["POSITION"]) else {
                    continue;
                };
                if moves.len() != count * 3 {
                    continue;
                }
                let whole = &gltf.json["meshes"][mesh as usize];
                let name = whole["extras"]["targetNames"][number].as_str().map_or_else(
                    || format!("Shape {}", number + 1),
                    |name| name.replace([';', '='], " "),
                );
                let rest = whole["weights"][number].as_f64().unwrap_or(0.0) as f32;
                let found = match out.morphs.iter().position(|morph| morph.name == name) {
                    Some(found) => found,
                    None => {
                        out.morphs.push(Morph {
                            name,
                            rest,
                            moves: Vec::new(),
                        });
                        out.morphs.len() - 1
                    }
                };
                let into = &mut out.morphs[found].moves;
                into.resize(first as usize + count, [0.0; 3]);
                for at in 0..count {
                    let by = [moves[at * 3], moves[at * 3 + 1], moves[at * 3 + 2]];
                    into[first as usize + at] = carried(&blends[at], by);
                }
            }
            let before = out.tris.len();
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
            let added = (out.tris.len() - before) / 3;
            out.skins
                .extend(std::iter::repeat_n(picture.unwrap_or(u32::MAX), added));
        }
    }
    if out.tris.is_empty() {
        return Err(tr!("file-has-no-model"));
    }
    if !any_skin {
        out.skin.clear();
        out.bones.clear();
    }
    for morph in &mut out.morphs {
        morph.moves.resize(out.points.len(), [0.0; 3]);
    }
    // A model with no colors or pictures of its own is plain clay.
    if out.textures.is_empty() {
        out.uvs.clear();
        out.skins.clear();
        if !tinted {
            out.colors.clear();
        }
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

/// How an imported model is shown: with its own colors and pictures or
/// as plain clay, and how far each of its morph targets is brought in.
#[derive(Clone, Debug, PartialEq)]
pub struct Look {
    /// Its own colors and pictures show.
    pub colors: bool,
    /// Each morph target by name, with its weight (0 none, 1 whole).
    pub morphs: Vec<(String, f32)>,
}

impl Look {
    /// The look of imported model `entity`: its file's morph targets,
    /// each at the file's own weight unless [`MORPHS`] says another.
    #[must_use]
    pub fn of(entity: &Entity) -> Self {
        let parsed = parsed_of(entity).map(|(_, parsed)| parsed);
        Self::with(entity, parsed.as_deref())
    }

    fn with(entity: &Entity, parsed: Option<&Parsed>) -> Self {
        let set = entity
            .property(MORPHS)
            .and_then(Value::as_str)
            .map(morphs_from_text)
            .unwrap_or_default();
        let morphs = parsed
            .map(|parsed| {
                parsed
                    .morphs
                    .iter()
                    .map(|morph| {
                        let weight = set
                            .iter()
                            .find(|(name, _)| *name == morph.name)
                            .map_or(morph.rest, |(_, weight)| *weight);
                        (morph.name.clone(), weight)
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self {
            colors: entity
                .property(COLORS)
                .and_then(Value::as_bool)
                .unwrap_or(true),
            morphs,
        }
    }

    /// The same look with morph target `name` at `weight`.
    #[must_use]
    pub fn with_morph(mut self, name: &str, weight: f32) -> Self {
        if let Some(found) = self.morphs.iter_mut().find(|(known, _)| known == name)
            && weight.is_finite()
        {
            found.1 = weight.clamp(0.0, 1.0);
        }
        self
    }

    /// The morph weights as [`MORPHS`] keeps them.
    #[must_use]
    pub fn morphs_text(&self) -> String {
        self.morphs
            .iter()
            .map(|(name, weight)| format!("{name}={weight}"))
            .collect::<Vec<_>>()
            .join(";")
    }

    /// A number that tells this look from another.
    fn key(&self) -> u64 {
        let mut text = self.morphs_text().into_bytes();
        text.push(u8::from(self.colors));
        hash(&text)
    }
}

/// The names and weights in a [`MORPHS`] text; what cannot be read is
/// left out.
fn morphs_from_text(text: &str) -> Vec<(String, f32)> {
    text.split(';')
        .filter_map(|pair| {
            let (name, weight) = pair.rsplit_once('=')?;
            let weight = weight.trim().parse::<f32>().ok()?;
            weight
                .is_finite()
                .then(|| (name.to_owned(), weight.clamp(0.0, 1.0)))
        })
        .collect()
}

/// The points of `parsed` with its morph targets brought in as `look`
/// says.
fn morphed(parsed: &Parsed, look: &Look) -> Vec<[f32; 3]> {
    let mut points = parsed.points.clone();
    for morph in &parsed.morphs {
        let weight = look
            .morphs
            .iter()
            .find(|(name, _)| *name == morph.name)
            .map_or(morph.rest, |(_, weight)| *weight);
        if weight == 0.0 {
            continue;
        }
        for (p, by) in points.iter_mut().zip(&morph.moves) {
            for axis in 0..3 {
                p[axis] += by[axis] * weight;
            }
        }
    }
    points
}

/// A skinned model as a figure to pose: every bone a joint, its turn
/// measured from the stance it was modeled in, shown as `look` says.
fn rigged(parsed: &Parsed, look: &Look) -> Option<Shaped> {
    if parsed.bones.is_empty() || parsed.bones.len() > MOST_BONES {
        return None;
    }
    let lower: Vec<String> = parsed
        .bones
        .iter()
        .map(|bone| bone.0.to_lowercase())
        .collect();
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
            mirror: lower
                .iter()
                .position(|name| !other.is_empty() && *name == other),
        });
    }
    // A hand or foot with two bones above it is a limb that follows it.
    let finger = [
        "index", "thumb", "middle", "ring", "pinky", "little", "finger", "toe",
    ];
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
                let mut held = [(0_u16, 0.0_f32); 4];
                held[0] = (0, 1.0);
                held
            } else {
                mine.map(|(bone, weight)| (bone, weight / total))
            }
        })
        .collect();
    let colored = look.colors && !parsed.colors.is_empty();
    Some(Shaped {
        rest: morphed(parsed, look),
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
        twists: Vec::new(),
        along: Vec::new(),
        dual: Vec::new(),
        helpers: Vec::new(),
        hinges: Vec::new(),
        colors: if colored {
            parsed.colors.clone()
        } else {
            Vec::new()
        },
        uvs: if colored {
            parsed.uvs.clone()
        } else {
            Vec::new()
        },
        skins: if colored {
            parsed.skins.clone()
        } else {
            Vec::new()
        },
        textures: if colored {
            parsed.textures.clone()
        } else {
            Vec::new()
        },
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

/// The files read so far, by the number their bytes hash to.
static PARSED: Mutex<Vec<(u64, Option<Arc<Parsed>>)>> = Mutex::new(Vec::new());

/// The kept file of imported model `entity` read: the number its bytes
/// hash to, and the model in it. Read once, then remembered.
fn parsed_of(entity: &Entity) -> Option<(u64, Arc<Parsed>)> {
    let file = entity.ext::<Encoded>()?;
    if file.format != "glb" {
        return None;
    }
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
    let mut read = PARSED.lock().unwrap_or_else(|poison| poison.into_inner());
    if let Some((_, found)) = read.iter().find(|(known, _)| *known == id) {
        return found.clone().map(|parsed| (id, parsed));
    }
    let parsed = Gltf::read(bytes, &|_| None)
        .and_then(|gltf| parse(&gltf))
        .ok()
        .map(Arc::new);
    if read.len() >= 8 {
        read.remove(0);
    }
    read.push((id, parsed.clone()));
    parsed.map(|parsed| (id, parsed))
}

/// The number of the skeleton of imported figure `entity` as it is
/// shown now (0 when its file holds none), read from its kept file the
/// first time. Another look (colors off, a morph target moved) is
/// another number.
#[must_use]
pub fn rig_of(entity: &Entity) -> u64 {
    let Some((file, parsed)) = parsed_of(entity) else {
        return 0;
    };
    let look = Look::with(entity, Some(&parsed));
    let id = (file ^ look.key().rotate_left(17)) | 1;
    let mut rigs = RIGS.lock().unwrap_or_else(|poison| poison.into_inner());
    if let Some((_, found)) = rigs.iter().find(|(known, _)| *known == id) {
        return if found.is_some() { id } else { 0 };
    }
    let rig = rigged(&parsed, &look).map(Arc::new);
    let found = rig.is_some();
    if rigs.len() >= 16 {
        rigs.remove(0);
    }
    rigs.push((id, rig));
    if found { id } else { 0 }
}

/// A plain imported model (no skeleton) as a surface to draw, placed,
/// with its own colors and pictures and its morph targets brought in,
/// and the middle and size it is framed by. `None` for a model whose
/// kept file is not a glTF (an OBJ with no material file).
#[must_use]
pub fn surface_of(entity: &Entity, place: &Placement) -> Option<(Surface, ([f32; 3], f32))> {
    let (_, parsed) = parsed_of(entity)?;
    if !parsed.bones.is_empty() {
        return None;
    }
    let look = Look::with(entity, Some(&parsed));
    let points: Vec<[f32; 3]> = if place.is_identity() {
        morphed(&parsed, &look)
    } else {
        morphed(&parsed, &look)
            .into_iter()
            .map(|p| place.apply(p))
            .collect()
    };
    let mut surface = Surface::creased(&points, &parsed.tris, 50.0);
    if look.colors && !parsed.colors.is_empty() {
        // The folded surface has a point for every corner, in the
        // triangles' order.
        surface.colors = parsed
            .tris
            .iter()
            .map(|at| parsed.colors[*at as usize])
            .collect();
        if !parsed.textures.is_empty() {
            surface.uvs = parsed
                .tris
                .iter()
                .map(|at| parsed.uvs[*at as usize])
                .collect();
            surface.skins = parsed.skins.clone();
            surface.textures = parsed.textures.clone();
        }
    }
    Some((surface, crate::three::frame_of_points(&parsed.points)))
}

/// `true` if imported model `entity` has colors or pictures of its own
/// to show.
#[must_use]
pub fn has_colors(entity: &Entity) -> bool {
    parsed_of(entity).is_some_and(|(_, parsed)| !parsed.colors.is_empty())
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
    // An OBJ with a material file beside it: read with its colors and
    // pictures, and kept as one GLB that holds them.
    if extension.eq_ignore_ascii_case("obj")
        && let Some(gltf) = obj_bundle(&bytes, beside)
        && let Ok(parsed) = parse(&gltf)
    {
        return Ok(plain_model(name, &parsed, gltf.to_glb()));
    }
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
    if parsed.bones.len() > MOST_BONES {
        return Err(tr!(
            "model-too-many-bones",
            count = parsed.bones.len(),
            most = MOST_BONES
        ));
    }
    // Kept as one file with its buffers and pictures inside, whatever
    // it came as.
    let kept = if bytes.starts_with(b"glTF") && gltf.beside.iter().all(Option::is_none) {
        bytes
    } else {
        gltf.to_glb()
    };
    let look = Look {
        colors: true,
        morphs: Vec::new(),
    };
    if rigged(&parsed, &look).is_some() {
        let count = parsed.bones.len();
        return Ok(Entity::empty(name, figure::CLASS)
            .with_ext(Encoded::new("glb", kept))
            .with_property(figure::KIND, Value::from(Kind::Imported.key()))
            .with_property(
                figure::POSE,
                Value::from(figure::Pose(vec![[0.0; 3]; count]).to_text()),
            ));
    }
    Ok(plain_model(name, &parsed, kept))
}

/// A plain model entity named `name` of what `parsed` holds, its file
/// `kept` inside it as a GLB.
fn plain_model(name: &str, parsed: &Parsed, kept: Vec<u8>) -> Entity {
    let mut geometry = Geometry::new();
    let ids: Vec<u32> = parsed
        .points
        .iter()
        .map(|p| geometry.add_point(p.map(f64::from)))
        .collect();
    for t in parsed.tris.as_chunks::<3>().0 {
        let _ = geometry.add_prim(PrimKind::Polygon, &t.map(|i| ids[i as usize]));
    }
    Entity::empty(name, sue_model::class::MODEL)
        .with_ext(geometry)
        .with_ext(Encoded::new("glb", kept))
        .with_property(SMOOTH, Value::from(true))
}

/// An OBJ file and the material file it names (fetched by `beside`) as
/// a glTF: its points, their places on the pictures, a primitive for
/// each material with its diffuse color and picture. `None` when it
/// names no material file, that file is not there, or it holds no
/// faces.
fn obj_bundle(bytes: &[u8], beside: &dyn Fn(&str) -> Option<Vec<u8>>) -> Option<Gltf> {
    let text = std::str::from_utf8(bytes).ok()?;
    let library = text
        .lines()
        .find_map(|line| line.trim().strip_prefix("mtllib "))?
        .trim();
    let library = beside(library)?;
    let library = String::from_utf8_lossy(&library);
    // Each material: its name, its diffuse color and its picture.
    let mut materials: Vec<(String, [f32; 3], Option<String>)> = Vec::new();
    for line in library.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix("newmtl ") {
            materials.push((name.trim().to_owned(), [1.0; 3], None));
        } else if let Some(color) = line.strip_prefix("Kd ")
            && let Some(last) = materials.last_mut()
        {
            let parts: Vec<f32> = color
                .split_whitespace()
                .filter_map(|part| part.parse().ok())
                .collect();
            if parts.len() >= 3 {
                last.1 = [parts[0], parts[1], parts[2]];
            }
        } else if let Some(map) = line.strip_prefix("map_Kd ")
            && let Some(last) = materials.last_mut()
        {
            // Options come first; the file's name is the last word.
            last.2 = map.split_whitespace().last().map(str::to_owned);
        }
    }
    let number = |text: &str| text.parse::<f32>().unwrap_or(0.0);
    let (mut places, mut laid): (Vec<[f32; 3]>, Vec<[f32; 2]>) = (Vec::new(), Vec::new());
    // For each material in use: its corners (a point and its place on
    // the picture, each pair once) and its triangles.
    type Part = (Vec<(usize, Option<usize>)>, Vec<u32>);
    let mut parts: Vec<(Option<usize>, Part)> = vec![(None, (Vec::new(), Vec::new()))];
    let mut seen: HashMap<(usize, usize, Option<usize>), u32> = HashMap::new();
    let mut now = 0;
    for line in text.lines() {
        let mut words = line.split_whitespace();
        match words.next() {
            Some("v") => {
                let p: Vec<f32> = words.take(3).map(number).collect();
                if p.len() == 3 {
                    places.push([p[0], p[1], p[2]]);
                }
            }
            Some("vt") => {
                let p: Vec<f32> = words.take(2).map(number).collect();
                if p.len() == 2 {
                    // Up in an OBJ, down in a glTF.
                    laid.push([p[0], 1.0 - p[1]]);
                }
            }
            Some("usemtl") => {
                let material = words
                    .next()
                    .and_then(|name| materials.iter().position(|(known, ..)| known == name));
                now = match parts.iter().position(|(known, _)| *known == material) {
                    Some(found) => found,
                    None => {
                        parts.push((material, (Vec::new(), Vec::new())));
                        parts.len() - 1
                    }
                };
            }
            Some("f") => {
                let within = |text: &str, count: usize| -> Option<usize> {
                    let at = text.parse::<i64>().ok()?;
                    let at = if at < 0 { count as i64 + at } else { at - 1 };
                    usize::try_from(at).ok().filter(|at| *at < count)
                };
                let corners: Vec<u32> = words
                    .filter_map(|word| {
                        let mut ids = word.split('/');
                        let point = within(ids.next()?, places.len())?;
                        let place = ids.next().and_then(|id| within(id, laid.len()));
                        let (corners, _) = &mut parts[now].1;
                        Some(*seen.entry((now, point, place)).or_insert_with(|| {
                            corners.push((point, place));
                            corners.len() as u32 - 1
                        }))
                    })
                    .collect();
                for k in 1..corners.len().saturating_sub(1) {
                    parts[now]
                        .1
                        .1
                        .extend([corners[0], corners[k], corners[k + 1]]);
                }
            }
            _ => {}
        }
    }
    parts.retain(|(_, (_, tris))| !tris.is_empty());
    if parts.is_empty() {
        return None;
    }
    // As a glTF: one buffer, a primitive and a material for each part.
    let mut bin: Vec<u8> = Vec::new();
    let (mut views, mut accessors) = (Vec::new(), Vec::new());
    let (mut primitives, mut made) = (Vec::new(), Vec::new());
    let (mut images, mut textures, mut pictures) = (Vec::new(), Vec::new(), Vec::new());
    let mut view = |bin: &mut Vec<u8>, data: &[u8]| {
        while !bin.len().is_multiple_of(4) {
            bin.push(0);
        }
        views.push(serde_json::json!({
            "buffer": 0, "byteOffset": bin.len(), "byteLength": data.len()
        }));
        bin.extend_from_slice(data);
        views.len() - 1
    };
    for (material, (corners, tris)) in &parts {
        let mut points: Vec<u8> = Vec::new();
        let mut uvs: Vec<u8> = Vec::new();
        let (mut low, mut high) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
        for (point, place) in corners {
            let p = places[*point];
            for axis in 0..3 {
                low[axis] = low[axis].min(p[axis]);
                high[axis] = high[axis].max(p[axis]);
                points.extend(p[axis].to_le_bytes());
            }
            let uv = place.map_or([0.0; 2], |place| laid[place]);
            uvs.extend(uv[0].to_le_bytes());
            uvs.extend(uv[1].to_le_bytes());
        }
        let indices: Vec<u8> = tris.iter().flat_map(|at| at.to_le_bytes()).collect();
        let mut accessor = |view: usize, kind: u32, count: usize, shape: &str| {
            accessors.push(serde_json::json!({
                "bufferView": view, "componentType": kind, "count": count, "type": shape
            }));
            accessors.len() - 1
        };
        let at_points = accessor(view(&mut bin, &points), 5126, corners.len(), "VEC3");
        let at_uvs = accessor(view(&mut bin, &uvs), 5126, corners.len(), "VEC2");
        let at_indices = accessor(view(&mut bin, &indices), 5125, tris.len(), "SCALAR");
        accessors[at_points]["min"] = serde_json::json!(low);
        accessors[at_points]["max"] = serde_json::json!(high);
        let mut primitive = serde_json::json!({
            "attributes": { "POSITION": at_points, "TEXCOORD_0": at_uvs },
            "indices": at_indices
        });
        if let Some((name, color, map)) = material.and_then(|material| materials.get(material)) {
            // The file's colors are as shown; a glTF's factor is linear.
            let linear = color.map(|value| value.clamp(0.0, 1.0).powf(2.2));
            let mut pbr = serde_json::json!({
                "baseColorFactor": [linear[0], linear[1], linear[2], 1.0],
                "metallicFactor": 0.0
            });
            if let Some(picture) = map.as_deref().and_then(beside) {
                images.push(serde_json::json!({ "uri": map }));
                pictures.push(Some(picture));
                textures.push(serde_json::json!({ "source": images.len() - 1 }));
                pbr["baseColorTexture"] = serde_json::json!({ "index": textures.len() - 1 });
            }
            made.push(serde_json::json!({ "name": name, "pbrMetallicRoughness": pbr }));
            primitive["material"] = (made.len() - 1).into();
        }
        primitives.push(primitive);
    }
    let mut json = serde_json::json!({
        "asset": { "version": "2.0", "generator": "Sue" },
        "scene": 0,
        "scenes": [{ "nodes": [0] }],
        "nodes": [{ "mesh": 0 }],
        "meshes": [{ "primitives": primitives }],
        "buffers": [{ "byteLength": bin.len() }],
        "bufferViews": views,
        "accessors": accessors
    });
    if !made.is_empty() {
        json["materials"] = made.into();
    }
    if !images.is_empty() {
        json["images"] = images.into();
        json["textures"] = textures.into();
    }
    Some(Gltf {
        json,
        buffers: vec![bin],
        beside: pictures,
    })
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

/// A model made in code, for trying the importer where no file is at
/// hand (Sue's own tests and its tour of pictures).
pub mod sample {
    /// `bytes` as base64 text.
    fn base64(bytes: &[u8]) -> String {
        const TABLE: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut text = String::new();
        for chunk in bytes.chunks(3) {
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
        text
    }

    /// A small picture as a PNG file: two by two pixels, red and green
    /// above, blue and white below.
    #[must_use]
    pub fn four_colors_png() -> Vec<u8> {
        let pixels = vec![
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ];
        sue_core::RasterTile2D::from_bytes(2, 2, sue_core::PixelFormat::Rgba8, pixels)
            .ok()
            .and_then(|tile| sue_io::encode_png(&tile).ok())
            .unwrap_or_default()
    }

    /// A painted square as `.gltf` text with everything inside: a
    /// material with a base color factor and a picture
    /// ([`four_colors_png`]), a color at each point, and a morph target
    /// named `Taller` that lifts its top edge by one. `skinned` hangs
    /// it on a bone named `Bone`, so it imports as a figure.
    #[must_use]
    pub fn painted_gltf(skinned: bool) -> Vec<u8> {
        let points: [[f32; 3]; 4] = [
            [-0.5, 0.0, 0.0],
            [0.5, 0.0, 0.0],
            [0.5, 1.0, 0.0],
            [-0.5, 1.0, 0.0],
        ];
        let places: [[f32; 2]; 4] = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
        let lifts: [[f32; 3]; 4] = [[0.0; 3], [0.0; 3], [0.0, 1.0, 0.0], [0.0, 1.0, 0.0]];
        let mut bin: Vec<u8> = Vec::new();
        let mut views = Vec::new();
        let mut view = |bin: &mut Vec<u8>, data: Vec<u8>| {
            views.push(serde_json::json!({
                "buffer": 0, "byteOffset": bin.len(), "byteLength": data.len()
            }));
            bin.extend(data);
            while !bin.len().is_multiple_of(4) {
                bin.push(0);
            }
        };
        let floats =
            |values: &[f32]| -> Vec<u8> { values.iter().flat_map(|v| v.to_le_bytes()).collect() };
        view(&mut bin, floats(points.as_flattened()));
        view(&mut bin, floats(places.as_flattened()));
        // Every point half as bright as the material alone.
        view(&mut bin, floats(&[0.5; 12]));
        view(&mut bin, floats(lifts.as_flattened()));
        view(&mut bin, vec![0, 0, 1, 0, 2, 0, 0, 0, 2, 0, 3, 0]);
        view(&mut bin, vec![0; 16]);
        view(&mut bin, floats(&[1.0, 0.0, 0.0, 0.0].repeat(4)));
        let mut attributes = serde_json::json!({
            "POSITION": 0, "TEXCOORD_0": 1, "COLOR_0": 2
        });
        let mut nodes = serde_json::json!([{ "name": "Square", "mesh": 0 }]);
        let mut json = serde_json::json!({
            "asset": { "version": "2.0" },
            "scene": 0,
            "scenes": [{ "nodes": [0] }],
            "materials": [{ "pbrMetallicRoughness": {
                "baseColorFactor": [1.0, 0.2, 0.2, 1.0],
                "baseColorTexture": { "index": 0 }
            } }],
            "textures": [{ "source": 0 }],
            "images": [{
                "uri": format!("data:image/png;base64,{}", base64(&four_colors_png()))
            }],
            "accessors": [
                { "bufferView": 0, "componentType": 5126, "count": 4, "type": "VEC3" },
                { "bufferView": 1, "componentType": 5126, "count": 4, "type": "VEC2" },
                { "bufferView": 2, "componentType": 5126, "count": 4, "type": "VEC3" },
                { "bufferView": 3, "componentType": 5126, "count": 4, "type": "VEC3" },
                { "bufferView": 4, "componentType": 5123, "count": 6, "type": "SCALAR" },
                { "bufferView": 5, "componentType": 5121, "count": 4, "type": "VEC4" },
                { "bufferView": 6, "componentType": 5126, "count": 4, "type": "VEC4" }
            ]
        });
        if skinned {
            attributes["JOINTS_0"] = 5.into();
            attributes["WEIGHTS_0"] = 6.into();
            nodes = serde_json::json!([
                { "name": "Square", "mesh": 0, "skin": 0 },
                { "name": "Bone" }
            ]);
            json["skins"] = serde_json::json!([{ "joints": [1] }]);
            json["scenes"] = serde_json::json!([{ "nodes": [0, 1] }]);
        }
        json["nodes"] = nodes;
        json["meshes"] = serde_json::json!([{
            "primitives": [{
                "attributes": attributes,
                "indices": 4,
                "material": 0,
                "targets": [{ "POSITION": 3 }]
            }],
            "extras": { "targetNames": ["Taller"] }
        }]);
        json["bufferViews"] = views.into();
        json["buffers"] = serde_json::json!([{
            "byteLength": bin.len(),
            "uri": format!("data:application/octet-stream;base64,{}", base64(&bin))
        }]);
        serde_json::to_vec(&json).unwrap_or_default()
    }

    /// A model of `bones` bones side by side, one point hanging on the
    /// last of them, as `.gltf` text: for trying models with many
    /// bones.
    #[must_use]
    pub fn many_bones_gltf(bones: usize) -> Vec<u8> {
        let last = (bones.max(1) - 1).min(usize::from(u16::MAX)) as u16;
        let mut bin: Vec<u8> = Vec::new();
        for p in [[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]] {
            for value in p {
                bin.extend(value.to_le_bytes());
            }
        }
        for _ in 0..3 {
            for joint in [last, 0, 0, 0] {
                bin.extend(joint.to_le_bytes());
            }
        }
        for _ in 0..3 {
            for weight in [1.0_f32, 0.0, 0.0, 0.0] {
                bin.extend(weight.to_le_bytes());
            }
        }
        let mut nodes = vec![serde_json::json!({ "name": "Mesh", "mesh": 0, "skin": 0 })];
        for bone in 0..bones {
            nodes.push(serde_json::json!({ "name": format!("Bone {}", bone + 1) }));
        }
        let joints: Vec<usize> = (1..=bones).collect();
        serde_json::to_vec(&serde_json::json!({
            "asset": { "version": "2.0" },
            "scene": 0,
            "scenes": [{ "nodes": [0] }],
            "nodes": nodes,
            "skins": [{ "joints": joints }],
            "meshes": [{ "primitives": [{
                "attributes": { "POSITION": 0, "JOINTS_0": 1, "WEIGHTS_0": 2 }
            }] }],
            "buffers": [{
                "byteLength": bin.len(),
                "uri": format!("data:application/octet-stream;base64,{}", base64(&bin))
            }],
            "bufferViews": [
                { "buffer": 0, "byteOffset": 0, "byteLength": 36 },
                { "buffer": 0, "byteOffset": 36, "byteLength": 24 },
                { "buffer": 0, "byteOffset": 60, "byteLength": 48 }
            ],
            "accessors": [
                { "bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3" },
                { "bufferView": 1, "componentType": 5123, "count": 3, "type": "VEC4" },
                { "bufferView": 2, "componentType": 5126, "count": 3, "type": "VEC4" }
            ]
        }))
        .unwrap_or_default()
    }

    /// A painted pyramid as an OBJ file, the material file it names
    /// (`pyramid.mtl`: a red material and one with the picture
    /// `colors.png`), for trying OBJ files with materials.
    #[must_use]
    pub fn painted_obj() -> (Vec<u8>, Vec<u8>) {
        let obj = b"mtllib pyramid.mtl\nv 0 0 0\nv 1 0 0\nv 0 1 0\nv 0 0 1\n\
vt 0 0\nvt 1 0\nvt 0 1\nusemtl red\nf 1/1 2/2 3/3\nf 1/1 3/3 4/2\n\
usemtl pictured\nf 1/1 4/2 2/3\nf 2/1 4/2 3/3\n";
        let mtl = b"newmtl red\nKd 1 0 0\nnewmtl pictured\nKd 1 1 1\nmap_Kd colors.png\n";
        (obj.to_vec(), mtl.to_vec())
    }
    /// A small skinned model as `.gltf` text with its buffer inside: a
    /// square post of two boxes, the upper one carried by a second bone
    /// (`Arm`) hanging from the first (`Root`).
    #[must_use]
    pub fn post_gltf() -> Vec<u8> {
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
        // How each bone was placed when the mesh was bound: the root
        // where it is, the arm half a meter up.
        let binds_at = bin.len();
        for lift in [0.0_f32, -0.5] {
            for (at, value) in [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, lift, 0.0, 1.0,
            ]
            .iter()
            .enumerate()
            {
                let _ = at;
                bin.extend(f32::to_le_bytes(*value));
            }
        }
        let text = base64(&bin);
        serde_json::to_vec(&serde_json::json!({
            "asset": { "version": "2.0" },
            "scene": 0,
            "scenes": [{ "nodes": [0, 1] }],
            "nodes": [
                { "name": "Post", "mesh": 0, "skin": 0 },
                { "name": "Root", "children": [2] },
                { "name": "Arm", "translation": [0.0, 0.5, 0.0] }
            ],
            "skins": [{ "joints": [1, 2], "inverseBindMatrices": 4 }],
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
                { "buffer": 0, "byteOffset": indices_at, "byteLength": binds_at - indices_at },
                { "buffer": 0, "byteOffset": binds_at, "byteLength": bin.len() - binds_at }
            ],
            "accessors": [
                { "bufferView": 0, "componentType": 5126, "count": 12, "type": "VEC3" },
                { "bufferView": 1, "componentType": 5121, "count": 12, "type": "VEC4" },
                { "bufferView": 2, "componentType": 5126, "count": 12, "type": "VEC4" },
                { "bufferView": 3, "componentType": 5123, "count": indices.len(), "type": "SCALAR" },
                { "bufferView": 4, "componentType": 5126, "count": 2, "type": "MAT4" }
            ]
        }))
        .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::sample::post_gltf;

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

    /// The mean color of the model's pixels as `view` draws it.
    fn drawn(model: &Entity) -> [f32; 3] {
        let view = crate::three::View3D {
            yaw: 0.0,
            pitch: 0.0,
            style: 0,
            ambient: 1.0,
            ..crate::three::View3D::default()
        };
        let buffers =
            crate::three::render_model(model, &view, [64, 64], &Placement::default()).unwrap();
        let tile = crate::three::draw(&buffers, &view).unwrap();
        let mut sum = [0.0_f32; 3];
        let mut count = 0.0;
        for pixel in tile.bytes().unwrap().as_chunks::<4>().0 {
            if pixel[3] == 255 {
                for k in 0..3 {
                    sum[k] += f32::from(pixel[k]);
                }
                count += 1.0;
            }
        }
        assert!(count > 100.0, "the model shows: {count}");
        sum.map(|value| value / count / 255.0)
    }

    #[test]
    fn a_gltf_shows_its_colors_and_pictures_or_plain_gray() {
        let model = import(sample::painted_gltf(false), "Square", "gltf", &|_| None).unwrap();
        assert!(!figure::is_figure(&model) && is_imported(&model));
        assert!(has_colors(&model));
        let (surface, _) = surface_of(&model, &Placement::default()).unwrap();
        assert_eq!(surface.colors.len(), surface.positions.len());
        assert_eq!(surface.uvs.len(), surface.positions.len());
        assert_eq!((surface.skins.len(), surface.textures.len()), (2, 1));
        assert_eq!(surface.textures[0].size, [2, 2]);
        // The factor (mostly red) times the point colors (half) times
        // the picture (red, green, blue and white): red leads.
        let [r, g, b] = drawn(&model);
        assert!(r > g * 1.5 && r > b * 1.5, "red leads: {r} {g} {b}");
        assert!(r < 0.75, "the points darken it: {r}");
        // Plain gray when its colors are set off.
        let plain = model.clone().with_property(COLORS, Value::from(false));
        let [r, g, b] = drawn(&plain);
        assert!((r - g).abs() < 0.01 && (g - b).abs() < 0.01, "{r} {g} {b}");
        let (surface, _) = surface_of(&plain, &Placement::default()).unwrap();
        assert!(surface.colors.is_empty() && surface.textures.is_empty());
        // The same on a model with a skeleton, which is a figure.
        let figure = import(sample::painted_gltf(true), "Square", "gltf", &|_| None).unwrap();
        assert!(figure::is_figure(&figure));
        let body = figure::Body::of(&figure);
        let surface = figure::surface(&figure::Pose::of(&figure), &body);
        assert_eq!(surface.colors.len(), surface.positions.len());
        assert_eq!(surface.textures.len(), 1);
        let [r, g, _] = drawn(&figure);
        assert!(r > g * 1.5, "{r} {g}");
        let plain = figure.clone().with_property(COLORS, Value::from(false));
        assert_ne!(figure::Body::of(&plain).rig, body.rig, "another look");
        let [r, g, b] = drawn(&plain);
        assert!((r - g).abs() < 0.01 && (g - b).abs() < 0.01, "{r} {g} {b}");
    }

    #[test]
    fn morph_targets_are_brought_in_by_their_weights() {
        let top = |positions: &[[f32; 3]]| positions.iter().fold(f32::MIN, |top, p| top.max(p[1]));
        for skinned in [false, true] {
            let model = import(sample::painted_gltf(skinned), "Square", "gltf", &|_| None).unwrap();
            let look = Look::of(&model);
            assert_eq!(look.morphs, vec![("Taller".to_owned(), 0.0)]);
            let shape = |model: &Entity| {
                if skinned {
                    figure::surface(&figure::Pose::of(model), &figure::Body::of(model)).positions
                } else {
                    surface_of(model, &Placement::default())
                        .unwrap()
                        .0
                        .positions
                }
            };
            assert!((top(&shape(&model)) - 1.0).abs() < 1e-5);
            let half = look.clone().with_morph("Taller", 0.5);
            assert_eq!(half.morphs_text(), "Taller=0.5");
            let taller = model
                .clone()
                .with_property(MORPHS, Value::from(half.morphs_text()));
            assert_eq!(Look::of(&taller), half);
            assert!((top(&shape(&taller)) - 1.5).abs() < 1e-5, "{skinned}");
            // Past its range a weight stays at its most; a name the
            // file does not have changes nothing.
            assert_eq!(look.clone().with_morph("Taller", 7.0).morphs[0].1, 1.0);
            assert_eq!(look.clone().with_morph("Wider", 1.0), look);
            let odd = model
                .clone()
                .with_property(MORPHS, Value::from("Wider=1;Taller=x"));
            assert_eq!(Look::of(&odd), look);
        }
    }

    #[test]
    fn an_obj_takes_its_material_files_colors_and_pictures() {
        let (obj, mtl) = sample::painted_obj();
        let beside = |name: &str| match name {
            "pyramid.mtl" => Some(mtl.clone()),
            "colors.png" => Some(sample::four_colors_png()),
            _ => None,
        };
        let model = import(obj.clone(), "Pyramid", "obj", &beside).unwrap();
        assert!(is_imported(&model) && has_colors(&model));
        // Kept as one GLB with the picture inside, so it opens again
        // with nothing beside it.
        let (bytes, extension) = file_of(&model).unwrap();
        assert_eq!(extension, "glb");
        let again = import(bytes.to_vec(), "Pyramid", "glb", &|_| None).unwrap();
        let (surface, _) = surface_of(&again, &Placement::default()).unwrap();
        assert_eq!(surface.textures.len(), 1);
        assert_eq!(surface.indices.len(), 12, "four triangles");
        // Two triangles of the red material, two with the picture.
        assert_eq!(surface.skins, vec![u32::MAX, u32::MAX, 0, 0]);
        assert_eq!(surface.colors[0], [1.0, 0.0, 0.0]);
        assert_eq!(surface.colors[6], [1.0, 1.0, 1.0]);
        // With no material file to be found it is the plain model it
        // was, its own file kept.
        let plain = import(obj.clone(), "Pyramid", "obj", &|_| None).unwrap();
        assert_eq!(file_of(&plain).unwrap().1, "obj");
        assert!(!has_colors(&plain));
        assert!(surface_of(&plain, &Placement::default()).is_none());
    }

    #[test]
    fn models_of_many_bones_pose_and_too_many_are_refused_with_a_reason() {
        // More bones than one byte numbers: the last one still carries
        // its point.
        let model = import(sample::many_bones_gltf(300), "Many", "gltf", &|_| None).unwrap();
        let body = figure::Body::of(&model);
        assert_eq!(body.joints().len(), 300);
        let rest = figure::Pose::of(&model);
        let mut turned = rest.clone();
        turned.0[299] = [0.0, 0.0, 90.0];
        let (before, after) = (
            figure::surface(&rest, &body).positions,
            figure::surface(&turned, &body).positions,
        );
        assert!((before[1][0] - 1.0).abs() < 1e-5);
        assert!(after[1][0].abs() < 1e-4 && (after[1][1] - 1.0).abs() < 1e-4);
        // Joint angle limits are off on a model from a file until its
        // own setting turns them on.
        assert!(!body.limits);
        let wide = figure::Pose(vec![[170.0, 0.0, 0.0]; 300]);
        assert_eq!(figure::limited(&wide, &body), wide);
        let kept = figure::Body::of(
            &model
                .clone()
                .with_property(figure::LIMITS, Value::from(true)),
        );
        assert!(kept.limits);
        assert_eq!(figure::limited(&wide, &kept).0[5], [120.0, 0.0, 0.0]);
        // More bones than Sue Paint numbers: refused, saying why.
        let error = import(
            sample::many_bones_gltf(MOST_BONES + 1),
            "Too many",
            "gltf",
            &|_| None,
        )
        .unwrap_err();
        assert!(error.contains("bones") && error.contains("65"), "{error}");
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
