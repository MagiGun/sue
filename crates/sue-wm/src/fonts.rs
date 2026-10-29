//! Sue's font library, shared by every studio that sets text. It finds the
//! fonts installed on the device (a background scan that reads only each
//! file's name tables), groups them into families with their weights and
//! italics, and installs a face into egui the first time something draws
//! with it. Studios ask for a family by name with [`resolve`] and get the
//! egui family to draw with, plus whether bold or italic must be faked
//! because the family lacks that face. [`face_bytes`] hands the font file to
//! exporters that embed fonts (PDF, e-books).
//!
//! Two families are always there, from the fonts Sue bundles: [`SANS`] and
//! [`MONO`].

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use egui::{FontData, FontDefinitions, FontFamily};

/// The bundled proportional family.
pub const SANS: &str = "Sue Sans";
/// The bundled monospace family.
pub const MONO: &str = "Sue Mono";

/// One face of an installed font file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontFace {
    /// The file.
    pub path: PathBuf,
    /// Which face of a collection (`.ttc`), else 0.
    pub index: u32,
    /// The family it belongs to, e.g. `Georgia`.
    pub family: String,
    /// Its style name, e.g. `Bold Italic`.
    pub style: String,
    /// Its weight, 100 (thin) to 900 (black); 400 is regular, 700 bold.
    pub weight: u16,
    /// `true` for italic and oblique faces.
    pub italic: bool,
    /// `true` when every glyph is as wide.
    pub monospace: bool,
    /// The Unicode blocks the face says it covers (its OS/2 table's
    /// Unicode range bits; 0 when it says nothing).
    pub ranges: u128,
}

/// A family and its faces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontFamilyInfo {
    /// Its name.
    pub name: String,
    /// Its faces, lightest first.
    pub faces: Vec<FontFace>,
}

impl FontFamilyInfo {
    /// The face closest to `bold` and `italic`.
    #[must_use]
    pub fn face(&self, bold: bool, italic: bool) -> Option<&FontFace> {
        let want = if bold { 700 } else { 400 };
        self.faces.iter().min_by_key(|face| {
            let slant = u32::from(face.italic != italic) * 10_000;
            slant + u32::from(face.weight.abs_diff(want))
        })
    }

    /// `true` if the family has a face of this weight and slant (bold is
    /// 600 or heavier).
    #[must_use]
    pub fn has(&self, bold: bool, italic: bool) -> bool {
        self.faces
            .iter()
            .any(|face| face.italic == italic && (face.weight >= 600) == bold)
    }

    /// `true` if it is a monospace family.
    #[must_use]
    pub fn monospace(&self) -> bool {
        self.faces.iter().all(|face| face.monospace)
    }
}

/// What to draw a family's text with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedFont {
    /// The egui family.
    pub family: FontFamily,
    /// `true` when bold was asked for and no bold face is installed (draw
    /// it a little wider).
    pub fake_bold: bool,
    /// `true` when italic was asked for and no italic face is installed
    /// (slant it).
    pub fake_italic: bool,
}

#[derive(Default)]
enum Scan {
    #[default]
    Idle,
    Running(Receiver<Vec<FontFamilyInfo>>),
    Done,
}

/// The library's state, behind one lock.
#[derive(Default)]
struct Library {
    scan: Scan,
    families: Arc<Vec<FontFamilyInfo>>,
    /// Faces installed into egui: name, file bytes, face index.
    installed: Vec<(String, Arc<Vec<u8>>, u32)>,
    /// File contents read so far.
    files: HashMap<PathBuf, Arc<Vec<u8>>>,
    /// The font the interface uses.
    ui_font: String,
    /// Extra folders to scan (Sue's own fonts).
    extra_dirs: Vec<PathBuf>,
    /// Faces that draw the scripts the bundled fonts lack (Japanese,
    /// Chinese, Korean): name, file bytes, face index.
    scripts: Vec<(String, Arc<Vec<u8>>, u32)>,
    /// The text [`cover`] last made sure of, and whether the scan had
    /// finished then.
    covered: (String, bool),
}

fn library() -> MutexGuard<'static, Library> {
    static LIBRARY: OnceLock<Mutex<Library>> = OnceLock::new();
    LIBRARY
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Library {
    /// Starts the scan if it has not run, and takes its result once done.
    fn poll(&mut self) {
        match &self.scan {
            Scan::Idle => {
                let (send, receive) = channel();
                let mut dirs = sue_platform::system_font_dirs();
                dirs.extend(self.extra_dirs.iter().cloned());
                let spawned = std::thread::Builder::new()
                    .name("sue-fonts".to_owned())
                    .spawn(move || {
                        let _ = send.send(scan_dirs(&dirs));
                    });
                self.scan = match spawned {
                    Ok(_) => Scan::Running(receive),
                    Err(_) => Scan::Done,
                };
            }
            Scan::Running(receive) => {
                if let Ok(families) = receive.try_recv() {
                    self.families = Arc::new(families);
                    self.scan = Scan::Done;
                }
            }
            Scan::Done => {}
        }
    }

    fn file(&mut self, path: &Path) -> Option<Arc<Vec<u8>>> {
        if let Some(bytes) = self.files.get(path) {
            return Some(bytes.clone());
        }
        let bytes = Arc::new(std::fs::read(path).ok()?);
        self.files.insert(path.to_path_buf(), bytes.clone());
        Some(bytes)
    }
}

/// Adds a folder of Sue's own fonts to the scan (call before the first
/// look at the library).
pub fn add_font_dir(dir: PathBuf) {
    let mut library = library();
    if !library.extra_dirs.contains(&dir) {
        library.extra_dirs.push(dir);
    }
}

/// `true` once the installed fonts are known.
#[must_use]
pub fn ready() -> bool {
    let mut library = library();
    library.poll();
    matches!(library.scan, Scan::Done)
}

/// Every installed family, by name (empty until [`ready`]), after the two
/// bundled ones.
#[must_use]
pub fn families() -> Vec<String> {
    let mut library = library();
    library.poll();
    let mut names = vec![SANS.to_owned(), MONO.to_owned()];
    names.extend(library.families.iter().map(|family| family.name.clone()));
    names
}

/// The installed family named `name` (any case).
#[must_use]
pub fn find(name: &str) -> Option<FontFamilyInfo> {
    let mut library = library();
    library.poll();
    library
        .families
        .iter()
        .find(|family| family.name.eq_ignore_ascii_case(name))
        .cloned()
}

/// The first of `names` that is installed, else [`SANS`].
#[must_use]
pub fn first_installed(names: &[&str]) -> String {
    names
        .iter()
        .find_map(|name| find(name).map(|family| family.name))
        .unwrap_or_else(|| SANS.to_owned())
}

/// The egui family to draw `name` with, `bold` and `italic`. The first time
/// a face is needed it is read and installed; until egui has it (the next
/// frame) this answers with the bundled family, and asks for that frame.
#[must_use]
pub fn resolve(ctx: &egui::Context, name: &str, bold: bool, italic: bool) -> ResolvedFont {
    let bundled = |family: FontFamily| ResolvedFont {
        family,
        fake_bold: bold,
        fake_italic: italic,
    };
    if name.eq_ignore_ascii_case(MONO) {
        return bundled(FontFamily::Monospace);
    }
    if name.eq_ignore_ascii_case(SANS) || name.is_empty() {
        return bundled(FontFamily::Proportional);
    }
    let Some(info) = find(name) else {
        return bundled(FontFamily::Proportional);
    };
    let Some(face) = info.face(bold, italic).cloned() else {
        return bundled(FontFamily::Proportional);
    };
    let key = format!(
        "{}#{}{}",
        face.family,
        face.weight,
        if face.italic { "i" } else { "" }
    );
    let family = FontFamily::Name(key.clone().into());
    let resolved = ResolvedFont {
        family: family.clone(),
        fake_bold: bold && face.weight < 600,
        fake_italic: italic && !face.italic,
    };
    let in_egui = ctx.fonts(|fonts| fonts.definitions().families.contains_key(&family));
    if in_egui {
        return resolved;
    }
    let stand_in = bundled(if info.monospace() {
        FontFamily::Monospace
    } else {
        FontFamily::Proportional
    });
    let definitions = {
        let mut library = library();
        if library.installed.iter().any(|(known, ..)| *known == key) {
            None
        } else {
            let Some(bytes) = library.file(&face.path) else {
                return stand_in;
            };
            library.installed.push((key, bytes, face.index));
            Some(definitions_of(&library))
        }
    };
    if let Some(definitions) = definitions {
        ctx.set_fonts(definitions);
    }
    ctx.request_repaint();
    stand_in
}

/// The file of `name`'s face closest to `bold` and `italic`, with its index
/// in a collection, for exporters that embed fonts. The bundled families
/// answer with the fonts Sue ships.
#[must_use]
pub fn face_bytes(name: &str, bold: bool, italic: bool) -> Option<(Arc<Vec<u8>>, u32)> {
    let bundled = |font: &str| {
        let definitions = FontDefinitions::default();
        let data = definitions.font_data.get(font)?;
        Some((Arc::new((*data.font).as_ref().to_vec()), data.index))
    };
    if name.eq_ignore_ascii_case(MONO) {
        return bundled("Hack");
    }
    if name.eq_ignore_ascii_case(SANS) || name.is_empty() {
        return bundled("Ubuntu-Light");
    }
    let face = find(name)?.face(bold, italic)?.clone();
    let mut library = library();
    let bytes = library.file(&face.path)?;
    Some((bytes, face.index))
}

/// The font definitions egui uses: the bundled fonts with `ui_font` first
/// for proportional text, and every face a studio has used so far.
#[must_use]
pub fn definitions(ui_font: &str) -> FontDefinitions {
    let mut library = library();
    ui_font.clone_into(&mut library.ui_font);
    definitions_of(&library)
}

fn definitions_of(library: &Library) -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    if library.ui_font == "Hack"
        && let Some(proportional) = fonts.families.get_mut(&FontFamily::Proportional)
    {
        proportional.retain(|name| name != "Hack");
        proportional.insert(0, "Hack".to_owned());
    }
    // The faces for other scripts come last, after the bundled ones, for
    // every family.
    for (key, bytes, index) in &library.scripts {
        let mut data = FontData::from_owned(bytes.as_ref().clone());
        data.index = *index;
        fonts.font_data.insert(key.clone(), Arc::new(data));
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            if let Some(chain) = fonts.families.get_mut(&family) {
                chain.push(key.clone());
            }
        }
    }
    let fallbacks = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    for (key, bytes, index) in &library.installed {
        let mut data = FontData::from_owned(bytes.as_ref().clone());
        data.index = *index;
        fonts.font_data.insert(key.clone(), Arc::new(data));
        let mut chain = vec![key.clone()];
        chain.extend(fallbacks.iter().cloned());
        fonts
            .families
            .insert(FontFamily::Name(key.clone().into()), chain);
    }
    fonts
}

// ----- Other scripts -----------------------------------------------------------

/// `true` if one of the bundled fonts draws `ch`.
fn bundled_has(ch: char) -> bool {
    static BUNDLED: OnceLock<Vec<Vec<u8>>> = OnceLock::new();
    let fonts = BUNDLED.get_or_init(|| {
        FontDefinitions::default()
            .font_data
            .values()
            .map(|data| (*data.font).as_ref().to_vec())
            .collect()
    });
    fonts.iter().any(|bytes| {
        ttf_parser::Face::parse(bytes, 0).is_ok_and(|face| face.glyph_index(ch).is_some())
    })
}

/// Makes sure the interface's fonts draw every character of `text` (the
/// languages in use and every language's name): for characters no bundled
/// font has, the fewest installed faces that cover them join every family
/// as fallbacks. Cheap when nothing changed; call it every frame.
pub fn cover(ctx: &egui::Context, text: &str) {
    let ready = ready();
    {
        let library = library();
        if library.covered.0 == text && (library.covered.1 || !ready) {
            return;
        }
    }
    let mut missing: Vec<char> = text
        .chars()
        .filter(|ch| !ch.is_ascii() && !ch.is_whitespace() && !ch.is_control())
        .collect();
    missing.sort_unstable();
    missing.dedup();
    missing.retain(|&ch| !bundled_has(ch));
    let mut library = library();
    library.covered = (text.to_owned(), ready);
    // What the faces already chosen draw.
    let chosen: Vec<Arc<Vec<u8>>> = library
        .scripts
        .iter()
        .map(|(_, bytes, _)| bytes.clone())
        .collect();
    let indices: Vec<u32> = library.scripts.iter().map(|(_, _, index)| *index).collect();
    missing.retain(|&ch| {
        !chosen.iter().zip(&indices).any(|(bytes, index)| {
            ttf_parser::Face::parse(bytes, *index).is_ok_and(|face| face.glyph_index(ch).is_some())
        })
    });
    let mut added = false;
    while !missing.is_empty() {
        // The upright, regular face whose blocks take the most of what is
        // missing; ties go to the one listed first.
        let families = library.families.clone();
        let best = families
            .iter()
            .flat_map(|family| family.face(false, false))
            .filter(|face| face.ranges != 0)
            .map(|face| {
                let ranges = ttf_parser::os2::UnicodeRanges(face.ranges);
                let count = missing
                    .iter()
                    .filter(|&&ch| ranges.contains_char(ch))
                    .count();
                (count, face)
            })
            .filter(|(count, _)| *count > 0)
            .max_by_key(|(count, _)| *count)
            .map(|(_, face)| face.clone());
        let Some(face) = best else {
            break;
        };
        let Some(bytes) = library.file(&face.path) else {
            break;
        };
        let Ok(parsed) = ttf_parser::Face::parse(&bytes, face.index) else {
            break;
        };
        let before = missing.len();
        missing.retain(|&ch| parsed.glyph_index(ch).is_none());
        // Never the same face twice, even when its blocks promised more.
        let ranges = ttf_parser::os2::UnicodeRanges(face.ranges);
        missing.retain(|&ch| !ranges.contains_char(ch));
        if missing.len() < before {
            let key = format!("script:{}#{}", face.family, face.index);
            library.scripts.push((key, bytes, face.index));
            added = true;
        }
    }
    if added {
        let definitions = definitions_of(&library);
        drop(library);
        ctx.set_fonts(definitions);
        ctx.request_repaint();
    }
}

// ----- Scanning ------------------------------------------------------------

/// Every family in the font files under `dirs` (two folders deep), sorted by
/// name. Unreadable files are skipped.
#[must_use]
pub fn scan_dirs(dirs: &[PathBuf]) -> Vec<FontFamilyInfo> {
    let mut files = Vec::new();
    let mut pending: Vec<(PathBuf, usize)> = dirs.iter().map(|dir| (dir.clone(), 0)).collect();
    while let Some((dir, depth)) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if depth < 3 {
                    pending.push((path, depth + 1));
                }
            } else if is_font_file(&path) {
                files.push(path);
            }
        }
    }
    let mut by_name: HashMap<String, FontFamilyInfo> = HashMap::new();
    for path in files {
        for face in read_faces(&path) {
            let key = face.family.to_lowercase();
            let family = by_name.entry(key).or_insert_with(|| FontFamilyInfo {
                name: face.family.clone(),
                faces: Vec::new(),
            });
            // The same face twice (system and user folders): keep the first.
            if !family
                .faces
                .iter()
                .any(|known| known.weight == face.weight && known.italic == face.italic)
            {
                family.faces.push(face);
            }
        }
    }
    let mut families: Vec<FontFamilyInfo> = by_name
        .into_values()
        .filter(|family| !family.name.starts_with('.'))
        .collect();
    for family in &mut families {
        family.faces.sort_by_key(|face| (face.weight, face.italic));
    }
    families.sort_by_key(|family| family.name.to_lowercase());
    families
}

fn is_font_file(path: &Path) -> bool {
    path.extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
        .is_some_and(|ext| matches!(ext.as_str(), "ttf" | "otf" | "ttc" | "otc"))
}

fn be_u16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn be_u32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn read_at(file: &mut File, offset: u64, len: usize) -> Option<Vec<u8>> {
    // No font table we read is this large; a bigger one is a broken file.
    if len > 16 << 20 {
        return None;
    }
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut bytes = vec![0; len];
    file.read_exact(&mut bytes).ok()?;
    Some(bytes)
}

/// The faces in one font file, read from its name, OS/2 and post tables
/// only (never the whole file).
#[must_use]
pub fn read_faces(path: &Path) -> Vec<FontFace> {
    let Ok(mut file) = File::open(path) else {
        return Vec::new();
    };
    let Some(head) = read_at(&mut file, 0, 12) else {
        return Vec::new();
    };
    let offsets: Vec<u32> = if &head[..4] == b"ttcf" {
        let count = be_u32(&head, 8).unwrap_or(0).min(64) as usize;
        match read_at(&mut file, 12, count * 4) {
            Some(table) => (0..count).filter_map(|i| be_u32(&table, i * 4)).collect(),
            None => Vec::new(),
        }
    } else {
        vec![0]
    };
    offsets
        .into_iter()
        .enumerate()
        .filter_map(|(index, offset)| read_face(&mut file, path, index as u32, u64::from(offset)))
        .collect()
}

fn read_face(file: &mut File, path: &Path, index: u32, offset: u64) -> Option<FontFace> {
    let header = read_at(file, offset, 12)?;
    let tables = usize::from(be_u16(&header, 4)?).min(128);
    let directory = read_at(file, offset + 12, tables * 16)?;
    let mut find = |tag: &[u8; 4]| -> Option<Vec<u8>> {
        (0..tables)
            .find_map(|i| {
                let entry = &directory[i * 16..i * 16 + 16];
                (&entry[..4] == tag).then_some(())?;
                let at = be_u32(entry, 8)?;
                let len = be_u32(entry, 12)?;
                Some((at, len))
            })
            .and_then(|(at, len)| read_at(file, u64::from(at), len as usize))
    };
    let name = find(b"name")?;
    let os2 = find(b"OS/2");
    let post = find(b"post");
    let names = ttf_parser::name::Table::parse(&name)?;
    let read = |id: u16| -> Option<String> {
        let mut english = None;
        let mut any = None;
        for record in names.names {
            if record.name_id != id || !record.is_unicode() {
                continue;
            }
            let Some(text) = record.to_string() else {
                continue;
            };
            if record.language_id == 0x0409 || record.language_id == 0 {
                english.get_or_insert(text);
            } else {
                any.get_or_insert(text);
            }
        }
        english.or(any).filter(|text| !text.trim().is_empty())
    };
    let family = read(ttf_parser::name_id::TYPOGRAPHIC_FAMILY)
        .or_else(|| read(ttf_parser::name_id::FAMILY))?;
    let style = read(ttf_parser::name_id::TYPOGRAPHIC_SUBFAMILY)
        .or_else(|| read(ttf_parser::name_id::SUBFAMILY))
        .unwrap_or_else(|| "Regular".to_owned());
    let os2 = os2.as_deref().and_then(ttf_parser::os2::Table::parse);
    let lower = style.to_lowercase();
    let weight = os2.as_ref().map_or_else(
        || if lower.contains("bold") { 700 } else { 400 },
        |table| table.weight().to_number(),
    );
    let italic = os2
        .as_ref()
        .is_some_and(|table| table.style() != ttf_parser::Style::Normal)
        || lower.contains("italic")
        || lower.contains("oblique");
    let monospace = post
        .as_deref()
        .and_then(ttf_parser::post::Table::parse)
        .is_some_and(|table| table.is_monospaced);
    Some(FontFace {
        path: path.to_path_buf(),
        index,
        family,
        style,
        weight,
        italic,
        monospace,
        ranges: os2.as_ref().map_or(0, |table| table.unicode_ranges().0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_fonts_are_read_like_installed_ones() {
        // Write the bundled fonts out as files and scan them.
        let dir = std::env::temp_dir().join(format!("sue-fonts-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let definitions = FontDefinitions::default();
        for name in ["Ubuntu-Light", "Hack"] {
            let data = &definitions.font_data[name];
            std::fs::write(dir.join(format!("{name}.ttf")), (*data.font).as_ref()).unwrap();
        }
        std::fs::write(dir.join("notes.txt"), "not a font").unwrap();
        let families = scan_dirs(std::slice::from_ref(&dir));
        let names: Vec<&str> = families.iter().map(|family| family.name.as_str()).collect();
        assert!(names.contains(&"Hack"), "{names:?}");
        assert!(
            names.iter().any(|name| name.starts_with("Ubuntu")),
            "{names:?}"
        );
        let hack = families
            .iter()
            .find(|family| family.name == "Hack")
            .unwrap();
        assert!(hack.monospace());
        let face = hack.face(true, false).unwrap();
        assert!(!face.italic);
        assert!(!hack.has(true, true));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_bundled_families_always_resolve() {
        let ctx = egui::Context::default();
        let sans = resolve(&ctx, SANS, true, false);
        assert_eq!(sans.family, FontFamily::Proportional);
        assert!(sans.fake_bold && !sans.fake_italic);
        assert_eq!(
            resolve(&ctx, MONO, false, false).family,
            FontFamily::Monospace
        );
        assert_eq!(
            resolve(&ctx, "No Such Family 123", false, true).family,
            FontFamily::Proportional
        );
        assert!(face_bytes(SANS, false, false).is_some_and(|(bytes, _)| bytes.len() > 1000));
        assert_eq!(&families()[..2], &[SANS.to_owned(), MONO.to_owned()]);
    }
}
