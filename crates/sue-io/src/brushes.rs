//! Brushes from other painting programs' files, so people can use the
//! brushes they download: Clip Studio Paint's sub tools (`.sut`),
//! Photoshop's brushes (`.abr`, sampled tips), GIMP's brushes (`.gbr`, and
//! `.gih` pipes of several pictures) and plain pictures (a `.png` tip).
//!
//! Each brush comes back as its tip pictures (straight RGBA8) and the
//! settings the file has, in Sue's terms; the studio that imports them
//! makes its own brushes from them. Nothing is run and nothing outside the
//! file is read.
//!
//! The layouts were learnt from real files: a `.sut` is a SQLite database
//! whose `Node` rows name the sub tools, whose `Variant` rows hold their
//! settings (`BrushSize`, `Opacity`, `BrushInterval`, `BrushRibbon`, ...)
//! and whose `MaterialFile` rows each hold a tar archive with the tip's
//! `thumbnail/thumbnail.png`. An `.abr` (version 6) keeps its tips in an
//! `8BIM samp` block, each a gray picture stored raw or PackBits packed.

use crate::sqlite::{self, Table, Value};

/// One brush read from a file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImportedBrush {
    /// What the file calls it.
    pub name: String,
    /// Its tip pictures: width, height and straight RGBA8 (gray tips are
    /// black with the paint in the alpha). More than one for a brush that
    /// lays several pictures in turn.
    pub tips: Vec<(u32, u32, Vec<u8>)>,
    /// Diameter in pixels.
    pub size: Option<f32>,
    /// 0 to 1.
    pub opacity: Option<f32>,
    /// 0 to 1.
    pub hardness: Option<f32>,
    /// The distance between tips, as a share of the size.
    pub spacing: Option<f32>,
    /// The tip's turn, in degrees.
    pub angle: Option<f32>,
    /// Tips turn with the line.
    pub follow: bool,
    /// How far tips fall from the line, in sizes (a spray).
    pub scatter: Option<f32>,
    /// The tip is laid along the stroke as a ribbon.
    pub ribbon: bool,
    /// The tips paint their own colors.
    pub keep_colors: bool,
}

/// The brushes in `bytes`, a file named `name` (its extension says the
/// kind when its first bytes do not).
///
/// # Errors
/// When the file is not a brush file Sue reads, or is damaged.
pub fn read(name: &str, bytes: &[u8]) -> Result<Vec<ImportedBrush>, String> {
    let stem = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(name)
        .rsplit_once('.')
        .map_or(name, |(stem, _)| stem)
        .to_owned();
    let extension = name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default();
    let brushes = if bytes.starts_with(b"SQLite format 3\0") {
        sut(bytes, &stem)?
    } else if bytes.starts_with(b"\x89PNG") {
        vec![picture(&stem, bytes)?]
    } else if extension == "abr" {
        abr(bytes, &stem)?
    } else if extension == "gih" {
        vec![gih(bytes, &stem)?]
    } else if extension == "gbr" {
        let (brush, _) = gbr(bytes)?;
        vec![ImportedBrush {
            name: if brush.name.is_empty() {
                stem
            } else {
                brush.name.clone()
            },
            ..brush
        }]
    } else {
        return Err(format!("{name} is not a brush file Sue reads"));
    };
    if brushes.is_empty() || brushes.iter().all(|brush| brush.tips.is_empty()) {
        return Err(format!("{name} has no brush tip in it"));
    }
    Ok(brushes)
}

fn be32(bytes: &[u8], at: usize) -> Result<u32, String> {
    bytes
        .get(at..at + 4)
        .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or_else(|| "the file ends early".to_owned())
}

fn be16(bytes: &[u8], at: usize) -> Result<u16, String> {
    bytes
        .get(at..at + 2)
        .map(|b| u16::from_be_bytes([b[0], b[1]]))
        .ok_or_else(|| "the file ends early".to_owned())
}

/// A picture file as one tip.
fn picture(name: &str, bytes: &[u8]) -> Result<ImportedBrush, String> {
    let decoded = crate::decode_png(bytes)?;
    let tile = decoded.tile;
    let pixels = tile.bytes().ok_or("the picture has no pixels")?.to_vec();
    Ok(ImportedBrush {
        name: name.to_owned(),
        tips: vec![(tile.width(), tile.height(), pixels)],
        spacing: Some(0.25),
        ..ImportedBrush::default()
    })
}

/// A gray picture (paint amount per pixel) as black RGBA8.
fn gray_to_rgba(gray: &[u8]) -> Vec<u8> {
    gray.iter().flat_map(|value| [0, 0, 0, *value]).collect()
}

// ----- GIMP ------------------------------------------------------------------

/// A GIMP brush (`.gbr`, versions 1 and 2): its name, tip and spacing,
/// and how many bytes it took.
fn gbr(bytes: &[u8]) -> Result<(ImportedBrush, usize), String> {
    let header = be32(bytes, 0)? as usize;
    let version = be32(bytes, 4)?;
    let (width, height, depth) = (be32(bytes, 8)?, be32(bytes, 12)?, be32(bytes, 16)?);
    if !(1..=2).contains(&version) || !matches!(depth, 1 | 4) || width == 0 || height == 0 {
        return Err("not a GIMP brush".to_owned());
    }
    if width > 8192 || height > 8192 || header < 20 || header > bytes.len() {
        return Err("a GIMP brush is damaged".to_owned());
    }
    let (spacing, name_at) = if version == 2 && bytes.get(20..24) == Some(b"GIMP") {
        (Some(be32(bytes, 24)? as f32 / 100.0), 28)
    } else {
        (None, 20)
    };
    let name = bytes
        .get(name_at..header)
        .map(|raw| {
            String::from_utf8_lossy(raw.split(|byte| *byte == 0).next().unwrap_or_default())
                .trim()
                .to_owned()
        })
        .unwrap_or_default();
    let length = (width * height * depth) as usize;
    let data = bytes
        .get(header..header + length)
        .ok_or("a GIMP brush's picture is cut short")?;
    let pixels = if depth == 1 {
        gray_to_rgba(data)
    } else {
        data.to_vec()
    };
    Ok((
        ImportedBrush {
            name,
            tips: vec![(width, height, pixels)],
            spacing: spacing.filter(|spacing| *spacing > 0.0),
            keep_colors: depth == 4,
            ..ImportedBrush::default()
        },
        header + length,
    ))
}

/// A GIMP image pipe (`.gih`): a name line, a line of parameters, then its
/// pictures as GIMP brushes one after the other.
fn gih(bytes: &[u8], stem: &str) -> Result<ImportedBrush, String> {
    let first = bytes
        .iter()
        .position(|byte| *byte == b'\n')
        .ok_or("not a GIMP image pipe")?;
    let second = bytes[first + 1..]
        .iter()
        .position(|byte| *byte == b'\n')
        .ok_or("not a GIMP image pipe")?
        + first
        + 1;
    let name = String::from_utf8_lossy(&bytes[..first]).trim().to_owned();
    let mut at = second + 1;
    let mut brush = ImportedBrush {
        name: if name.is_empty() {
            stem.to_owned()
        } else {
            name
        },
        ..ImportedBrush::default()
    };
    while at < bytes.len() && brush.tips.len() < 256 {
        let (cell, used) = gbr(&bytes[at..])?;
        brush.spacing = brush.spacing.or(cell.spacing);
        brush.keep_colors |= cell.keep_colors;
        brush.tips.extend(cell.tips);
        at += used;
    }
    Ok(brush)
}

// ----- Photoshop -------------------------------------------------------------

/// PackBits: runs and literals, `length` bytes out.
fn packbits(packed: &[u8], length: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(length);
    let mut at = 0;
    while out.len() < length && at < packed.len() {
        let count = packed[at];
        at += 1;
        if count < 128 {
            let run = usize::from(count) + 1;
            out.extend_from_slice(packed.get(at..at + run).unwrap_or_default());
            at += run;
        } else if count > 128 {
            if let Some(byte) = packed.get(at) {
                out.extend(std::iter::repeat_n(*byte, 257 - usize::from(count)));
            }
            at += 1;
        }
    }
    out.resize(length, 0);
    out
}

/// A Photoshop brush file (version 6): its sampled tips, named after the
/// file and numbered.
fn abr(bytes: &[u8], stem: &str) -> Result<Vec<ImportedBrush>, String> {
    let version = be16(bytes, 0)?;
    if version != 6 {
        return Err(format!(
            "this Photoshop brush file is version {version}; Sue reads version 6 (Photoshop 7 and later)"
        ));
    }
    let mut at = 4;
    let mut tips = Vec::new();
    while at + 12 <= bytes.len() && bytes.get(at..at + 4) == Some(b"8BIM") {
        let key = &bytes[at + 4..at + 8];
        let length = be32(bytes, at + 8)? as usize;
        let start = at + 12;
        let end = start.saturating_add(length).min(bytes.len());
        if key == b"samp" {
            let mut sample = start;
            while sample + 4 <= end {
                let size = be32(bytes, sample)? as usize;
                let next = sample + 4 + size + (4 - size % 4) % 4;
                if let Ok(tip) = abr_sample(bytes, sample + 4, (sample + 4 + size).min(end)) {
                    tips.push(tip);
                }
                if size == 0 {
                    break;
                }
                sample = next;
            }
        }
        at = start + length + length % 2;
    }
    let count = tips.len();
    Ok(tips
        .into_iter()
        .enumerate()
        .map(|(index, tip)| ImportedBrush {
            name: if count == 1 {
                stem.to_owned()
            } else {
                format!("{stem} {}", index + 1)
            },
            tips: vec![tip],
            spacing: Some(0.25),
            ..ImportedBrush::default()
        })
        .collect())
}

/// One sampled tip of an `.abr`, between `at` and `end`: its id, then a
/// list of channels of which the first written one is the tip.
fn abr_sample(bytes: &[u8], at: usize, end: usize) -> Result<(u32, u32, Vec<u8>), String> {
    let id = usize::from(*bytes.get(at).ok_or("a brush is cut short")?);
    // The id, then two unknown words, the list's version and length, its
    // bounds and how many channels it has.
    let mut q = at + 1 + id + 4 + 8 + 16;
    let channels = be32(bytes, q)?;
    q += 4;
    for _ in 0..channels.saturating_add(2).min(64) {
        if q + 4 > end {
            break;
        }
        let written = be32(bytes, q)?;
        q += 4;
        if written == 0 {
            continue;
        }
        let length = be32(bytes, q)? as usize;
        q += 4;
        let data = q;
        let top = be32(bytes, q + 4)?;
        let left = be32(bytes, q + 8)?;
        let bottom = be32(bytes, q + 12)?;
        let right = be32(bytes, q + 16)?;
        let depth = be16(bytes, q + 20)?;
        let compression = *bytes.get(q + 22).ok_or("a brush is cut short")?;
        let (width, height) = (right.saturating_sub(left), bottom.saturating_sub(top));
        if width == 0 || height == 0 || width > 8192 || height > 8192 || depth != 8 {
            return Err("an unusual brush picture".to_owned());
        }
        let pixels = (width * height) as usize;
        let mut p = q + 23;
        let gray = if compression == 1 {
            let mut rows = Vec::with_capacity(height as usize);
            for row in 0..height as usize {
                rows.push(usize::from(be16(bytes, p + row * 2)?));
            }
            p += height as usize * 2;
            let mut gray = Vec::with_capacity(pixels);
            for row in rows {
                let packed = bytes.get(p..p + row).ok_or("a brush is cut short")?;
                gray.extend(packbits(packed, width as usize));
                p += row;
            }
            gray
        } else {
            bytes
                .get(p..p + pixels)
                .ok_or("a brush is cut short")?
                .to_vec()
        };
        let _ = data + length;
        return Ok((width, height, gray_to_rgba(&gray)));
    }
    Err("a brush without a picture".to_owned())
}

// ----- Clip Studio Paint ------------------------------------------------------

/// A `Variant` row by the `VariantID` a node names.
fn by_id(table: &Table, id: i64) -> Option<&[Value]> {
    let key = table.column("VariantID")?;
    table
        .rows
        .iter()
        .find(|(_, row)| row.get(key).and_then(Value::int) == Some(id))
        .map(|(_, row)| row.as_slice())
}

/// A file inside a tar archive by its name.
fn untar<'a>(archive: &'a [u8], wanted: &str) -> Option<&'a [u8]> {
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

/// Clip Studio Paint sub tools (`.sut`): every sub tool with settings, its
/// tips from the file's materials.
fn sut(bytes: &[u8], stem: &str) -> Result<Vec<ImportedBrush>, String> {
    let tables = sqlite::read(bytes)?;
    let table = |name: &str| tables.iter().find(|table| table.name == name);
    let (Some(nodes), Some(variants)) = (table("Node"), table("Variant")) else {
        return Err("not a Clip Studio Paint sub tool".to_owned());
    };
    // The tips: each material's picture.
    let mut tips = Vec::new();
    if let Some(materials) = table("MaterialFile")
        && let Some(data) = materials.column("FileData")
    {
        for (_, row) in &materials.rows {
            if let Some(png) = row
                .get(data)
                .and_then(Value::bytes)
                .and_then(|archive| untar(archive, "thumbnail/thumbnail.png"))
                && let Ok(decoded) = crate::decode_png(png)
                && let Some(pixels) = decoded.tile.bytes()
            {
                tips.push((decoded.tile.width(), decoded.tile.height(), pixels.to_vec()));
            }
        }
    }
    let column = |table: &Table, row: &[Value], name: &str| -> Option<f64> {
        let value = row.get(table.column(name)?)?;
        value
            .real()
            .or_else(|| value.int().map(|value| value as f64))
    };
    let mut out = Vec::new();
    let (name_at, variant_at) = (nodes.column("NodeName"), nodes.column("NodeVariantID"));
    for (_, node) in &nodes.rows {
        let Some(variant) = variant_at
            .and_then(|at| node.get(at))
            .and_then(Value::int)
            .filter(|id| *id > 0)
        else {
            continue;
        };
        let Some(row) = by_id(variants, variant) else {
            continue;
        };
        let name = name_at
            .and_then(|at| node.get(at))
            .and_then(Value::text)
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(stem)
            .to_owned();
        let get = |name: &str| column(variants, row, name);
        let on = |name: &str| get(name).is_some_and(|value| value != 0.0);
        // Sizes in pixels; other units (millimeters, points) fall back to a
        // size that suits most decorations.
        let size = get("BrushSize").map(|size| {
            if get("BrushSizeUnit").unwrap_or(0.0) == 0.0 {
                size as f32
            } else {
                40.0
            }
        });
        let uses_pictures = on("BrushUsePatternImage");
        // Clip Studio's turn effectors: the 32 bit turns tips along the line.
        let effector = get("BrushRotationEffector").unwrap_or(0.0) as u32;
        let scatter = (on("BrushUseSpray"))
            .then(|| {
                let spray = get("BrushSpraySize").unwrap_or(0.0) as f32;
                size.map(|size| spray / size.max(1.0)).unwrap_or(1.0)
            })
            .filter(|scatter| *scatter > 0.0);
        out.push(ImportedBrush {
            name,
            tips: if uses_pictures {
                tips.clone()
            } else {
                Vec::new()
            },
            size,
            opacity: get("Opacity").map(|opacity| (opacity / 100.0) as f32),
            hardness: get("BrushHardness").map(|hardness| (hardness / 100.0) as f32),
            spacing: get("BrushInterval")
                .map(|interval| (interval as f32 / 100.0).clamp(0.02, 5.0)),
            angle: get("BrushRotation").map(|angle| angle as f32),
            follow: effector & 32 != 0,
            scatter,
            ribbon: on("BrushRibbon") && uses_pictures,
            keep_colors: !on("BrushChangePatternColor"),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_gimp_brush_reads_its_name_picture_and_spacing() {
        let mut file = Vec::new();
        let name = b"Dot\0";
        file.extend((28 + name.len() as u32).to_be_bytes());
        file.extend(2_u32.to_be_bytes());
        file.extend([2_u32, 2, 1].iter().flat_map(|v| v.to_be_bytes()));
        file.extend(b"GIMP");
        file.extend(40_u32.to_be_bytes());
        file.extend(name);
        file.extend([0, 255, 128, 0]);
        let brushes = read("dot.gbr", &file).unwrap();
        assert_eq!(brushes[0].name, "Dot");
        assert_eq!(brushes[0].spacing, Some(0.4));
        assert_eq!(brushes[0].tips[0].0, 2);
        assert_eq!(
            &brushes[0].tips[0].2[4..8],
            &[0, 0, 0, 255],
            "the paint in the alpha"
        );
        // Two of them in a pipe.
        let mut pipe = b"Dots\n2 ncells:2\n".to_vec();
        pipe.extend(&file);
        pipe.extend(&file);
        let pipe = read("dots.gih", &pipe).unwrap();
        assert_eq!((pipe[0].name.as_str(), pipe[0].tips.len()), ("Dots", 2));
    }

    #[test]
    fn packbits_runs_and_literals() {
        // A literal of 2, then a run of 3.
        assert_eq!(packbits(&[1, 7, 8, 254, 9], 5), [7, 8, 9, 9, 9]);
    }

    #[test]
    fn a_tar_member_is_found_by_name() {
        let mut archive = vec![0_u8; 512];
        archive[..5].copy_from_slice(b"a.txt");
        archive[124..135].copy_from_slice(b"00000000003");
        archive.extend(b"abc");
        archive.resize(1024, 0);
        archive.extend(vec![0; 1024]);
        assert_eq!(untar(&archive, "a.txt"), Some(&b"abc"[..]));
        assert_eq!(untar(&archive, "b.txt"), None);
    }

    #[test]
    fn other_files_are_refused() {
        assert!(read("notes.txt", b"hello").is_err());
        assert!(read("old.abr", &[0, 1, 0, 0]).is_err());
    }

    /// Real brush files, when `SUE_BRUSH_SAMPLES` names a folder of them
    /// (downloaded brushes are not kept in the repository).
    #[test]
    fn downloaded_brushes_read() {
        let Ok(folder) = std::env::var("SUE_BRUSH_SAMPLES") else {
            return;
        };
        let mut read_any = false;
        for entry in std::fs::read_dir(folder).unwrap().flatten() {
            let path = entry.path();
            let name = path.to_string_lossy().into_owned();
            let bytes = std::fs::read(&path).unwrap();
            match read(&name, &bytes) {
                Ok(brushes) => {
                    read_any = true;
                    for brush in brushes {
                        let tips: Vec<(u32, u32)> =
                            brush.tips.iter().map(|(w, h, _)| (*w, *h)).collect();
                        println!(
                            "{name}: {} tips {tips:?} size {:?} spacing {:?} ribbon {} colors {} scatter {:?}",
                            brush.name,
                            brush.size,
                            brush.spacing,
                            brush.ribbon,
                            brush.keep_colors,
                            brush.scatter
                        );
                    }
                }
                Err(error) => println!("{name}: {error}"),
            }
        }
        assert!(read_any);
    }
}
