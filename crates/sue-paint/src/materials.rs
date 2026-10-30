//! Materials, as in Clip Studio Paint's Material palette: tones, patterns,
//! pictures and brush tips ready to put on the canvas. Sue Paint brings
//! screentones and patterns of its own, drawn at any size; pictures
//! registered with Edit, Register Material are PNG files in the materials
//! location (`materials/`), and brush tips made from pictures sit beside
//! them (`tips/`), so every project has them.
//!
//! The kinds of material are not a fixed list: each registers in a
//! [`MaterialKinds`] registry with how it is found and placed, and a later
//! kind (3D models, brushes) registers the same way. The materials found
//! are entities of a [`Library`], an arena of its own beside the project,
//! whose favorites and use change through transactions.

use std::path::{Path, PathBuf};

use sue_core::txn::SetProperty;
use sue_core::{Arena, Entity, EntityId, PixelFormat, RasterTile2D, SueFile, UndoStack, Value};
use sue_wm::start::Glyph;

/// How a material goes onto a canvas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    /// It covers the canvas (tones and patterns), in the drawing color.
    Cover,
    /// It sits in the middle, shrunk to fit (pictures).
    Fit,
}

/// Finds the materials of a kind in the materials location, if any.
pub type Scan = fn(Option<&Path>) -> Vec<Material>;

/// A kind of material, registered with [`MaterialKinds::register`].
#[derive(Clone, Debug)]
pub struct MaterialKind {
    /// Its id, under the registering feature's namespace, e.g.
    /// `sue_paint.tone`.
    pub id: String,
    /// What people read, kept in English (`localize` shows it).
    pub name: String,
    /// Its picture in lists.
    pub glyph: Glyph,
    /// How it goes onto a canvas.
    pub placement: Placement,
    /// Finds its materials.
    pub scan: Scan,
}

/// The kinds of material Sue Paint knows, in the order they register.
#[derive(Clone, Debug, Default)]
pub struct MaterialKinds {
    kinds: Vec<MaterialKind>,
}

impl MaterialKinds {
    /// Sue Paint's own kinds: tones, patterns, pictures and brush tips.
    #[must_use]
    pub fn builtin() -> Self {
        let mut kinds = Self::default();
        let own: [(&str, &str, Glyph, Placement, Scan); 4] = [
            (TONE, "Tones", Glyph::Tone, Placement::Cover, scan_tones),
            (
                PATTERN,
                "Patterns",
                Glyph::Pattern,
                Placement::Cover,
                scan_patterns,
            ),
            (IMAGE, "Images", Glyph::Picture, Placement::Fit, scan_images),
            (TIP, "Brush tips", Glyph::Brush, Placement::Fit, scan_tips),
        ];
        for (id, name, glyph, placement, scan) in own {
            kinds
                .register(MaterialKind {
                    id: id.to_owned(),
                    name: name.to_owned(),
                    glyph,
                    placement,
                    scan,
                })
                .expect("distinct built-in kinds");
        }
        kinds
    }

    /// Adds a kind.
    ///
    /// # Errors
    /// If a kind with its id is already registered.
    pub fn register(&mut self, kind: MaterialKind) -> Result<(), String> {
        if self.get(&kind.id).is_some() {
            return Err(format!("material kind {} is already registered", kind.id));
        }
        self.kinds.push(kind);
        Ok(())
    }

    /// The kind with `id`.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&MaterialKind> {
        self.kinds.iter().find(|kind| kind.id == id)
    }

    /// Every kind, in order.
    pub fn iter(&self) -> impl Iterator<Item = &MaterialKind> {
        self.kinds.iter()
    }

    /// Every material of every kind found in `folder` (the materials
    /// location), kind by kind.
    #[must_use]
    pub fn scan(&self, folder: Option<&Path>) -> Vec<Material> {
        self.kinds
            .iter()
            .flat_map(|kind| (kind.scan)(folder))
            .collect()
    }
}

/// Sue Paint's screentones.
pub const TONE: &str = "sue_paint.tone";
/// Sue Paint's patterns.
pub const PATTERN: &str = "sue_paint.pattern";
/// Pictures registered as materials.
pub const IMAGE: &str = "sue_paint.image";
/// Brush tips made from pictures.
pub const TIP: &str = "sue_paint.tip";

/// One material in the library.
#[derive(Clone, Debug, PartialEq)]
pub struct Material {
    /// Its name.
    pub name: String,
    /// Its kind's id.
    pub kind: String,
    /// How it goes onto a canvas.
    pub placement: Placement,
    /// Where it comes from.
    pub source: Source,
}

impl Material {
    /// `true` for one the person added (a file), which they may delete.
    #[must_use]
    pub fn removable(&self) -> bool {
        matches!(self.source, Source::File(_))
    }

    /// What tells it apart from every other material, kept with its
    /// favorite and when it was used.
    #[must_use]
    pub fn key(&self) -> String {
        match &self.source {
            Source::Builtin(index) => format!(
                "builtin:{}",
                BUILTIN.get(*index).map_or("", |(name, ..)| *name)
            ),
            Source::File(path) => format!("file:{}", path.display()),
        }
    }
}

/// Where a material comes from.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// One of Sue Paint's own, by number.
    Builtin(usize),
    /// A registered picture.
    File(PathBuf),
}

/// Sue Paint's own tones and patterns: a name, a kind and how much ink
/// each pixel has.
type Builtin = (&'static str, &'static str, fn(f32, f32) -> f32);

/// Halftone dots on a 45 degree grid of 8 pixel cells covering `share`.
fn dots(x: f32, y: f32, share: f32) -> f32 {
    let cell = 8.0;
    let (s, c) = std::f32::consts::FRAC_PI_4.sin_cos();
    let (u, v) = ((x * c + y * s) / cell, (-x * s + y * c) / cell);
    let (du, dv) = (u - u.round(), v - v.round());
    let away = du.hypot(dv) * cell;
    let radius = cell * (share / std::f32::consts::PI).sqrt();
    (radius - away + 0.5).clamp(0.0, 1.0)
}

fn noise(x: f32, y: f32) -> f32 {
    let mut h =
        (x as i64 as u32).wrapping_mul(0x9E37_79B1) ^ (y as i64 as u32).wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h & 0xFF) as f32 / 255.0
}

const BUILTIN: [Builtin; 12] = [
    ("Dot 10%", TONE, |x, y| dots(x, y, 0.1)),
    ("Dot 20%", TONE, |x, y| dots(x, y, 0.2)),
    ("Dot 30%", TONE, |x, y| dots(x, y, 0.3)),
    ("Dot 50%", TONE, |x, y| dots(x, y, 0.5)),
    ("Lines 20%", TONE, |_, y| {
        if y.rem_euclid(6.0) < 1.2 { 1.0 } else { 0.0 }
    }),
    (
        "Sand",
        TONE,
        |x, y| {
            if noise(x, y) > 0.82 { 1.0 } else { 0.0 }
        },
    ),
    ("Crosshatch", PATTERN, |x, y| {
        let a = (x + y).rem_euclid(8.0) < 1.0;
        let b = (x - y).rem_euclid(8.0) < 1.0;
        if a || b { 1.0 } else { 0.0 }
    }),
    ("Grid", PATTERN, |x, y| {
        if x.rem_euclid(16.0) < 1.0 || y.rem_euclid(16.0) < 1.0 {
            1.0
        } else {
            0.0
        }
    }),
    ("Checker", PATTERN, |x, y| {
        if ((x / 16.0).floor() + (y / 16.0).floor()).rem_euclid(2.0) < 1.0 {
            1.0
        } else {
            0.0
        }
    }),
    ("Bricks", PATTERN, |x, y| {
        let row = (y / 12.0).floor();
        let shift = if row.rem_euclid(2.0) < 1.0 { 0.0 } else { 12.0 };
        if y.rem_euclid(12.0) < 1.5 || (x + shift).rem_euclid(24.0) < 1.5 {
            1.0
        } else {
            0.0
        }
    }),
    ("Stripes", PATTERN, |x, y| {
        if (x + y).rem_euclid(12.0) < 4.0 {
            1.0
        } else {
            0.0
        }
    }),
    ("Speckles", PATTERN, |x, y| noise(x, y) * 0.6),
];

/// Every material of Sue Paint's own kinds found in `folder`, the
/// materials location.
#[must_use]
pub fn library(folder: Option<&Path>) -> Vec<Material> {
    MaterialKinds::builtin().scan(folder)
}

/// Sue Paint's own materials of `kind`.
fn scan_builtin(kind: &str, placement: Placement) -> Vec<Material> {
    BUILTIN
        .iter()
        .enumerate()
        .filter(|(_, (_, of, _))| *of == kind)
        .map(|(index, (name, _, _))| Material {
            name: (*name).to_owned(),
            kind: kind.to_owned(),
            placement,
            source: Source::Builtin(index),
        })
        .collect()
}

fn scan_tones(_: Option<&Path>) -> Vec<Material> {
    scan_builtin(TONE, Placement::Cover)
}

fn scan_patterns(_: Option<&Path>) -> Vec<Material> {
    scan_builtin(PATTERN, Placement::Cover)
}

fn scan_images(folder: Option<&Path>) -> Vec<Material> {
    scan_pngs(folder.map(|folder| folder.join("materials")), IMAGE)
}

fn scan_tips(folder: Option<&Path>) -> Vec<Material> {
    scan_pngs(folder.map(|folder| folder.join("tips")), TIP)
}

/// The PNG files in `dir` as materials of `kind`, by name.
fn scan_pngs(dir: Option<PathBuf>, kind: &str) -> Vec<Material> {
    let Some(entries) = dir.and_then(|dir| sue_platform::list_dir(&dir, true).ok()) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .into_iter()
        .map(|entry| entry.path)
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("png"))
        })
        .collect();
    files.sort();
    files
        .into_iter()
        .map(|path| Material {
            name: path.file_stem().map_or_else(
                || tr!("material"),
                |stem| stem.to_string_lossy().into_owned(),
            ),
            kind: kind.to_owned(),
            placement: Placement::Fit,
            source: Source::File(path),
        })
        .collect()
}

/// `ink` (0 to 1 a pixel) as black on clear, `size`.
fn draw(size: [u32; 2], color: [u8; 3], ink: fn(f32, f32) -> f32) -> RasterTile2D {
    let mut pixels = Vec::with_capacity((size[0] * size[1] * 4) as usize);
    for y in 0..size[1] {
        for x in 0..size[0] {
            let alpha = (ink(x as f32 + 0.5, y as f32 + 0.5).clamp(0.0, 1.0) * 255.0).round() as u8;
            pixels.extend_from_slice(&[color[0], color[1], color[2], alpha]);
        }
    }
    RasterTile2D::from_bytes(size[0], size[1], PixelFormat::Rgba8, pixels)
        .unwrap_or_else(|_| RasterTile2D::new(1, 1, PixelFormat::Rgba8).expect("one pixel"))
}

/// The material as a picture: Sue Paint's own at `size` in `color`, a
/// registered one as it is.
#[must_use]
pub fn picture(material: &Material, size: [u32; 2], color: [u8; 3]) -> Option<RasterTile2D> {
    match &material.source {
        Source::Builtin(index) => BUILTIN
            .get(*index)
            .map(|(_, _, ink)| draw(size, color, *ink)),
        Source::File(path) => {
            let bytes = sue_platform::fs::read(path).ok()?;
            sue_io::decode_png(&bytes).ok().map(|decoded| decoded.tile)
        }
    }
}

/// The layer that puts `material` on a `size` canvas: tones and patterns
/// cover it (in `color`), pictures sit in the middle, shrunk to fit.
#[must_use]
pub fn layer(material: &Material, size: [u32; 2], color: [u8; 3]) -> Option<RasterTile2D> {
    if material.placement == Placement::Cover {
        return picture(material, size, color);
    }
    let tile = picture(material, size, color)?;
    fitted(&tile, size)
}

/// `tile` (straight RGBA8) made to fit inside `size` keeping its shape,
/// never larger than it is, in the middle of a clear picture of `size`.
#[must_use]
pub fn fitted(tile: &RasterTile2D, size: [u32; 2]) -> Option<RasterTile2D> {
    let from = [tile.width(), tile.height()];
    let scale = (f64::from(size[0]) / f64::from(from[0]))
        .min(f64::from(size[1]) / f64::from(from[1]))
        .min(1.0);
    let to = [
        ((f64::from(from[0]) * scale).round() as u32).max(1),
        ((f64::from(from[1]) * scale).round() as u32).max(1),
    ];
    let rgba = match tile.format() {
        PixelFormat::Rgba8 => tile.bytes()?.to_vec(),
        _ => return None,
    };
    let fitted = if to == from {
        rgba
    } else {
        crate::doc::resize_rgba(&rgba, from, to)
    };
    let mut out = vec![0_u8; (size[0] * size[1] * 4) as usize];
    let (left, top) = ((size[0] - to[0]) / 2, (size[1] - to[1]) / 2);
    for y in 0..to[1] {
        let row = (y * to[0] * 4) as usize;
        let at = (((top + y) * size[0] + left) * 4) as usize;
        out[at..at + (to[0] * 4) as usize]
            .copy_from_slice(&fitted[row..row + (to[0] * 4) as usize]);
    }
    RasterTile2D::from_bytes(size[0], size[1], PixelFormat::Rgba8, out).ok()
}

/// `tile` (straight RGBA8) repeated over a clear picture of `size` from
/// its top left corner (File, Import, Pattern from Image).
#[must_use]
pub fn tiled(tile: &RasterTile2D, size: [u32; 2]) -> Option<RasterTile2D> {
    let (w, h) = (tile.width(), tile.height());
    if w == 0 || h == 0 || tile.format() != PixelFormat::Rgba8 {
        return None;
    }
    let bytes = tile.bytes()?;
    let mut out = vec![0_u8; (size[0] * size[1] * 4) as usize];
    for y in 0..size[1] {
        for x in 0..size[0] {
            let from = (((y % h) * w + x % w) * 4) as usize;
            let to = ((y * size[0] + x) * 4) as usize;
            out[to..to + 4].copy_from_slice(&bytes[from..from + 4]);
        }
    }
    RasterTile2D::from_bytes(size[0], size[1], PixelFormat::Rgba8, out).ok()
}

/// Keeps `tile` (RGBA8) as a picture material named `name` in Sue Paint's
/// `folder`, cut to what it covers; returns its file.
///
/// # Errors
/// If it is empty, or will not encode or write.
pub fn register(folder: &Path, name: &str, tile: &RasterTile2D) -> Result<PathBuf, String> {
    let bytes = tile
        .bytes()
        .ok_or(tr!("only-rgba-layers-register-as-materials"))?;
    let (w, h) = (tile.width(), tile.height());
    let (mut min, mut max) = ([u32::MAX; 2], [0_u32; 2]);
    for y in 0..h {
        for x in 0..w {
            if bytes[((y * w + x) * 4 + 3) as usize] > 0 {
                min = [min[0].min(x), min[1].min(y)];
                max = [max[0].max(x), max[1].max(y)];
            }
        }
    }
    if min[0] > max[0] {
        return Err(tr!("layer-empty-draw-something-register-first"));
    }
    let (cw, ch) = (max[0] - min[0] + 1, max[1] - min[1] + 1);
    let mut cut = Vec::with_capacity((cw * ch * 4) as usize);
    for y in min[1]..=max[1] {
        let at = ((y * w + min[0]) * 4) as usize;
        cut.extend_from_slice(&bytes[at..at + (cw * 4) as usize]);
    }
    let png = sue_io::encode_png_rgba(cw, ch, &cut)?;
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let safe = if safe.trim().is_empty() {
        tr!("material")
    } else {
        safe.trim().to_owned()
    };
    let dir = folder.join("materials");
    sue_platform::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let path = dir.join(format!("{safe}.png"));
    sue_platform::write_atomic(&path, &png).map_err(|error| error.to_string())?;
    Ok(path)
}

/// The class of a material entity in a [`Library`].
pub const CLASS: &str = "sue_paint.material";
/// A material's kind id (text).
pub const KIND_KEY: &str = "sue_paint.material.kind";
/// What tells a material apart ([`Material::key`], text).
pub const SOURCE_KEY: &str = "sue_paint.material.source";
/// `true` for a favorite.
pub const FAVORITE_KEY: &str = "sue_paint.material.favorite";
/// When it was last used, in seconds since 1970.
pub const USED_KEY: &str = "sue_paint.material.used";
/// When it was added, in seconds since 1970 (0 for Sue Paint's own).
pub const ADDED_KEY: &str = "sue_paint.material.added";
/// The file in the materials location keeping the library's entities.
const LIBRARY_FILE: &str = "library.sue";
/// The rice key of the materials location ("" is Sue Paint's folder).
pub const LOCATION_KEY: &str = "sue_paint.materials_dir";

/// Where the materials are kept: the folder Settings names, or Sue
/// Paint's own folder.
#[must_use]
pub fn location(cx: &sue_wm::AppletContext<'_>) -> Option<PathBuf> {
    let chosen = cx.rice().get_str(LOCATION_KEY).unwrap_or("").trim();
    if chosen.is_empty() {
        crate::colorsets::folder(cx)
    } else {
        Some(PathBuf::from(chosen))
    }
}

/// One material of a [`Library`], as its entity says.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// Its entity.
    pub id: EntityId,
    /// The material.
    pub material: Material,
    /// `true` for a favorite.
    pub favorite: bool,
    /// When it was last used (0: never).
    pub used: i64,
    /// When it was added (0 for Sue Paint's own).
    pub added: i64,
}

/// Every material found, as entities of an arena of its own: their kind,
/// favorite and use are properties, changed through transactions on the
/// library's own history, and kept in the materials location so the ids
/// and favorites survive a restart.
#[derive(Debug)]
pub struct Library {
    arena: Arena,
    history: UndoStack<Arena>,
    materials: Vec<(EntityId, Material)>,
    folder: Option<PathBuf>,
}

impl Library {
    /// Finds every material of `kinds` in `folder`, keeping the entities,
    /// favorites and use an earlier run kept there.
    #[must_use]
    pub fn scan(kinds: &MaterialKinds, folder: Option<&Path>) -> Self {
        let kept: Vec<Entity> = folder
            .and_then(|folder| sue_platform::fs::read(folder.join(LIBRARY_FILE)).ok())
            .and_then(|bytes| SueFile::from_bytes(&bytes).ok())
            .and_then(|file| file.arena(&sue_model::registry()).ok())
            .map(|arena| {
                arena
                    .iter()
                    .filter(|(_, entity)| entity.class() == CLASS)
                    .map(|(_, entity)| entity.clone())
                    .collect()
            })
            .unwrap_or_default();
        let mut arena = Arena::new();
        let mut materials = Vec::new();
        for material in kinds.scan(folder) {
            let key = material.key();
            let entity = match kept.iter().find(|entity| {
                entity.property(SOURCE_KEY).and_then(Value::as_str) == Some(key.as_str())
            }) {
                Some(entity) => entity.clone(),
                None => {
                    let added = match &material.source {
                        Source::File(path) => sue_platform::fs::metadata(path)
                            .ok()
                            .and_then(|metadata| metadata.modified())
                            .and_then(|time| {
                                time.duration_since(sue_platform::time::UNIX_EPOCH).ok()
                            })
                            .map_or(0, |since| i64::try_from(since.as_secs()).unwrap_or(0)),
                        Source::Builtin(_) => 0,
                    };
                    Entity::empty(material.name.clone(), CLASS)
                        .with_property(SOURCE_KEY, key)
                        .with_property(ADDED_KEY, added)
                }
            }
            .with_property(KIND_KEY, material.kind.clone());
            let id = entity.id();
            if arena.insert_keep_id(entity).is_ok() {
                materials.push((id, material));
            }
        }
        Self {
            arena,
            history: UndoStack::new(),
            materials,
            folder: folder.map(Path::to_path_buf),
        }
    }

    /// The library's arena.
    #[must_use]
    pub fn arena(&self) -> &Arena {
        &self.arena
    }

    /// Every material, as found.
    #[must_use]
    pub fn entries(&self) -> Vec<Entry> {
        self.materials
            .iter()
            .filter_map(|(id, material)| {
                let entity = self.arena.get_by_id(*id).ok()?;
                let int = |key: &str| entity.property(key).and_then(Value::as_i64).unwrap_or(0);
                Some(Entry {
                    id: *id,
                    material: material.clone(),
                    favorite: entity
                        .property(FAVORITE_KEY)
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    used: int(USED_KEY),
                    added: int(ADDED_KEY),
                })
            })
            .collect()
    }

    /// The material of entity `id`.
    #[must_use]
    pub fn material(&self, id: EntityId) -> Option<&Material> {
        self.materials
            .iter()
            .find(|(of, _)| *of == id)
            .map(|(_, material)| material)
    }

    /// Makes `id` a favorite (`on`) or not, and keeps it.
    ///
    /// # Errors
    /// If it is not in the library or the library cannot be written.
    pub fn set_favorite(&mut self, id: EntityId, on: bool) -> Result<(), String> {
        self.set(id, FAVORITE_KEY, Value::Bool(on))
    }

    /// Notes that `id` was used at `now` (seconds since 1970).
    ///
    /// # Errors
    /// If it is not in the library or the library cannot be written.
    pub fn used(&mut self, id: EntityId, now: u64) -> Result<(), String> {
        self.set(
            id,
            USED_KEY,
            Value::Int(i64::try_from(now).unwrap_or(i64::MAX)),
        )
    }

    fn set(&mut self, id: EntityId, key: &str, value: Value) -> Result<(), String> {
        self.history
            .apply(&mut self.arena, SetProperty::new(id, key, value))
            .map_err(|error| error.to_string())?;
        self.save()
    }

    /// Takes back the last change (`redo` puts it back). Returns `true`
    /// if there was one.
    ///
    /// # Errors
    /// If the change cannot be reverted or the library written.
    pub fn undo(&mut self, redo: bool) -> Result<bool, String> {
        let done = if redo {
            self.history.redo(&mut self.arena)
        } else {
            self.history.undo(&mut self.arena)
        }
        .map_err(|error| error.to_string())?;
        self.save()?;
        Ok(done)
    }

    /// Writes the library's entities into the materials location.
    ///
    /// # Errors
    /// If the file cannot be written.
    pub fn save(&self) -> Result<(), String> {
        let Some(folder) = &self.folder else {
            return Ok(());
        };
        let mut file = SueFile::new();
        file.set_arena(&self.arena);
        sue_platform::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
        sue_platform::write_atomic(&folder.join(LIBRARY_FILE), &file.to_bytes())
            .map_err(|error| error.to_string())
    }
}

/// Copies the picture file `from` into the materials location as a
/// material of `kind` (images or brush tips), under a name not taken.
///
/// # Errors
/// If it is not a PNG or cannot be copied.
pub fn add_file(folder: &Path, kind: &str, from: &Path) -> Result<PathBuf, String> {
    let png = from
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("png"));
    if !png {
        return Err(tr!("materials-are-png-files"));
    }
    let dir = folder.join(if kind == TIP { "tips" } else { "materials" });
    sue_platform::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let name = from
        .file_name()
        .map_or_else(|| "material.png".into(), |name| name.to_os_string());
    let mut to = dir.join(&name);
    let stem = to
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut n = 2;
    while sue_platform::fs::exists(&to) {
        to = dir.join(format!("{stem} ({n}).png"));
        n += 1;
    }
    let bytes = sue_platform::fs::read(from).map_err(|error| error.to_string())?;
    sue_platform::write_atomic(&to, &bytes).map_err(|error| error.to_string())?;
    Ok(to)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coverage(tile: &RasterTile2D) -> f32 {
        let bytes = tile.bytes().unwrap();
        bytes
            .chunks(4)
            .map(|pixel| f32::from(pixel[3]) / 255.0)
            .sum::<f32>()
            / (bytes.len() / 4) as f32
    }

    #[test]
    fn tones_cover_their_share_and_pictures_register_and_place() {
        let library = library(None);
        assert!(library.iter().any(|material| material.kind == TONE));
        assert!(library.iter().any(|material| material.kind == PATTERN));
        // A 30% dot tone covers about 30% of the canvas, in its color.
        let dot = library
            .iter()
            .find(|material| material.name == "Dot 30%")
            .unwrap();
        let tone = layer(dot, [256, 256], [10, 20, 30]).unwrap();
        assert_eq!((tone.width(), tone.height()), (256, 256));
        let share = coverage(&tone);
        assert!((share - 0.3).abs() < 0.04, "{share}");
        assert_eq!(&tone.bytes().unwrap()[..3], &[10, 20, 30]);

        // A picture registered is cut to what it covers and comes back.
        let folder = std::env::temp_dir().join(format!("sue-materials-{}", std::process::id()));
        let _ = sue_platform::fs::remove_dir_all(&folder);
        let mut pixels = vec![0_u8; 40 * 30 * 4];
        for y in 10..20 {
            for x in 5..25 {
                pixels[(y * 40 + x) * 4..][..4].copy_from_slice(&[200, 0, 0, 255]);
            }
        }
        let tile = RasterTile2D::from_bytes(40, 30, PixelFormat::Rgba8, pixels).unwrap();
        let empty = RasterTile2D::new(4, 4, PixelFormat::Rgba8).unwrap();
        assert!(register(&folder, "Nothing", &empty).is_err());
        register(&folder, "Red / box", &tile).unwrap();
        let mine: Vec<Material> = super::library(Some(&folder))
            .into_iter()
            .filter(|material| material.kind == IMAGE)
            .collect();
        assert_eq!(mine.len(), 1);
        assert_eq!(mine[0].name, "Red _ box");
        let back = picture(&mine[0], [0, 0], [0; 3]).unwrap();
        assert_eq!((back.width(), back.height()), (20, 10));
        // Placed in the middle of a canvas.
        let placed = layer(&mine[0], [100, 50], [0; 3]).unwrap();
        let at = |x: usize, y: usize| placed.bytes().unwrap()[(y * 100 + x) * 4 + 3];
        assert_eq!(at(50, 25), 255);
        assert_eq!(at(5, 5), 0);
        let _ = sue_platform::fs::remove_dir_all(&folder);
    }

    #[test]
    fn kinds_register_once_and_the_library_keeps_favorites_and_ids() {
        let mut kinds = MaterialKinds::builtin();
        let tones = kinds.get(TONE).unwrap().clone();
        assert!(kinds.register(tones).is_err());
        kinds
            .register(MaterialKind {
                id: "test.nothing".to_owned(),
                name: "Nothing".to_owned(),
                glyph: Glyph::Cube,
                placement: Placement::Fit,
                scan: |_| Vec::new(),
            })
            .unwrap();
        assert_eq!(kinds.iter().count(), 5);

        let folder = std::env::temp_dir().join(format!("sue-library-{}", std::process::id()));
        let _ = sue_platform::fs::remove_dir_all(&folder);
        let mut pixels = vec![0_u8; 8 * 8 * 4];
        pixels[3] = 255;
        let tile = RasterTile2D::from_bytes(8, 8, PixelFormat::Rgba8, pixels).unwrap();
        register(&folder, "Mine", &tile).unwrap();
        let mut library = Library::scan(&kinds, Some(&folder));
        let entries = library.entries();
        assert!(entries.iter().any(|entry| entry.material.kind == TONE));
        let mine = entries
            .iter()
            .find(|entry| entry.material.name == "Mine")
            .unwrap()
            .clone();
        assert!(mine.material.removable() && mine.added > 0);
        let entity = library.arena().get_by_id(mine.id).unwrap();
        assert_eq!(entity.class(), CLASS);
        assert_eq!(
            entity.property(KIND_KEY).and_then(Value::as_str),
            Some(IMAGE)
        );

        // A favorite is a transaction: undone and redone, and kept.
        library.set_favorite(mine.id, true).unwrap();
        library.used(mine.id, 1_000).unwrap();
        assert!(library.undo(false).unwrap());
        assert_eq!(
            library
                .entries()
                .iter()
                .find(|entry| entry.id == mine.id)
                .unwrap()
                .used,
            0
        );
        assert!(library.undo(true).unwrap());
        let again = Library::scan(&kinds, Some(&folder));
        let back = again
            .entries()
            .into_iter()
            .find(|entry| entry.material.name == "Mine")
            .unwrap();
        assert_eq!(back.id, mine.id, "the entity keeps its id");
        assert!(back.favorite);
        assert_eq!(back.used, 1_000);

        // A PNG copies in under a free name; anything else does not.
        let copied = add_file(&folder, TIP, &folder.join("materials").join("Mine.png")).unwrap();
        assert!(copied.ends_with("tips/Mine.png") || copied.ends_with("tips\\Mine.png"));
        let twice = add_file(&folder, TIP, &copied).unwrap();
        assert!(twice.to_string_lossy().contains("Mine (2)"));
        assert!(add_file(&folder, IMAGE, Path::new("notes.txt")).is_err());
        let found = Library::scan(&kinds, Some(&folder));
        assert_eq!(
            found
                .entries()
                .iter()
                .filter(|entry| entry.material.kind == TIP)
                .count(),
            2
        );
        let _ = sue_platform::fs::remove_dir_all(&folder);
    }
}
