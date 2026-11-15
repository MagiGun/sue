//! Icon packs and single icons a person replaces: every icon of Sue can
//! be drawn by someone else.
//!
//! # What a pack is
//!
//! A folder (or a `.zip` of one) that holds a manifest, [`MANIFEST`], and
//! one SVG file per icon it replaces, named after the icon's stable name
//! ([`Icon::name`]): `folder.svg`, `layer-new.svg`. The solid twin of an
//! on and off pair (a filled heart, an open eye) is `heart.solid.svg`.
//! Anything a pack leaves out is drawn by Sue. The format is written up
//! for pack makers in `docs/icon-packs.md`.
//!
//! The SVG files are read by [`svg`] (the subset Sue draws, and a note
//! for each thing it did not read) and drawn by [`draw`] the way Sue's own
//! icons are: one ink, tinted by the theme, sharp on the pixel grid at
//! every size. A file that cannot be read is named in the pack's notes
//! and Sue's own icon stands in for it; one bad file never refuses a
//! pack, and nothing in a file can make Sue panic.
//!
//! # Where packs live
//!
//! In [`IconHome`], a folder of Sue's own data (`<data>/icons`, which is
//! `SUE_HOME\data\icons` when `SUE_HOME` is set):
//!
//! ```text
//! icons/
//!   chosen.txt          the id of the pack in use; none for Sue's own
//!   packs/<id>/         one folder per installed pack
//!   overrides/          single icons, <name>.svg and <name>.solid.svg
//! ```
//!
//! # Which drawing wins
//!
//! For each icon and style: a single override, then the chosen pack, then
//! Sue's own drawing ([`IconHome::load`]).
//!
//! # With no restart
//!
//! [`show`] hands a loaded [`IconSet`] to an egui context: the tiles made
//! from the set before are dropped and the next frame draws the new
//! icons. [`refresh`] loads and shows in one call, which is what a
//! settings page calls after it installed, chose, removed or replaced
//! something. [`paint_from`] draws from a set that is not shown, for a
//! preview.
//!
//! # A template for makers
//!
//! [`export`] writes Sue's own set as a pack: every icon as an SVG file
//! ([`super::svg`]), with a manifest to fill in.

pub mod data;
pub mod draw;
pub mod svg;

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use egui::{Color32, Context, Id, Painter, Rect};
use sue_io::zip::{ZipWriter, read_zip};
use sue_platform::PlatformPaths;

use super::raster::Tile;
use super::{Icon, IconStyle};
use svg::Drawing;

/// The name of a pack's manifest file.
pub const MANIFEST: &str = "icon-pack.txt";

/// The version of the pack format this Sue writes and reads. A pack made
/// for a newer one is still read, and says so in its notes.
pub const FORMAT: u32 = 1;

/// The largest SVG file of one icon, in bytes.
pub const LARGEST_FILE: u64 = 256 * 1024;

/// The most files a pack may hold.
pub const MOST_FILES: usize = 2048;

/// The largest pack, all of its files together, in bytes.
pub const LARGEST_PACK: u64 = 32 * 1024 * 1024;

/// The ending of the file of an icon's solid twin.
const SOLID: &str = ".solid.svg";

macro_rules! tr {
    ($($arg:tt)*) => { sue_lang::tr!("wm", $($arg)*) };
}

// ----- Notes and errors --------------------------------------------------------

/// Something in a pack that could not be read, or read only in part.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Problem {
    /// Something in one SVG file.
    Svg(svg::Problem),
    /// The pack has no manifest: its folder's name stands for its name.
    NoManifest,
    /// A line of the manifest that is not `key = value`.
    ManifestLine {
        /// Its number, from 1.
        line: usize,
    },
    /// A key the manifest format does not have.
    UnknownKey {
        /// The key.
        key: String,
    },
    /// The manifest gives no name.
    NoName,
    /// The manifest's `format` is missing or not a number: format 1 is
    /// assumed.
    NoFormat,
    /// The pack was made for a newer Sue.
    NewerFormat {
        /// The format it names.
        format: u32,
    },
    /// An SVG file named after no icon Sue has.
    UnknownIcon {
        /// The name.
        name: String,
    },
    /// A solid twin for an icon that has none in Sue: not used.
    NoSolidTwin {
        /// The icon.
        name: String,
    },
    /// The pack replaces an icon that has a solid twin, and not the twin:
    /// Sue's own solid twin is drawn for the state that is on.
    NoSolid {
        /// The icon.
        name: String,
    },
    /// A file that is no part of a pack (another kind of file, a folder,
    /// a link): left out.
    Skipped,
    /// More files than a pack may hold: the rest are left out.
    TooManyFiles,
    /// The pack that was chosen is not installed: Sue's own icons are
    /// drawn.
    ChosenMissing {
        /// Its id.
        id: String,
    },
}

/// A problem and the file it is in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    /// The file, as named in the pack (`folder.svg`), or the pack's id
    /// for a note about the whole pack.
    pub file: String,
    /// What is wrong.
    pub problem: Problem,
}

impl Note {
    /// The note in words, in the current language.
    #[must_use]
    pub fn message(&self) -> String {
        let file = self.file.as_str();
        match &self.problem {
            Problem::Svg(problem) => match problem {
                svg::Problem::NotText => tr!("icon-pack-not-text", file = file),
                svg::Problem::TooLarge { bytes } => {
                    tr!(
                        "icon-pack-too-large",
                        file = file,
                        bytes = bytes.to_string()
                    )
                }
                svg::Problem::NoSvg => tr!("icon-pack-no-svg", file = file),
                svg::Problem::Empty => tr!("icon-pack-empty", file = file),
                svg::Problem::PathData { element, at, .. } => tr!(
                    "icon-pack-path-data",
                    file = file,
                    element = element.as_str(),
                    at = at.to_string()
                ),
                svg::Problem::BadValue { element, attribute } => tr!(
                    "icon-pack-bad-value",
                    file = file,
                    element = element.as_str(),
                    attribute = attribute.as_str()
                ),
                svg::Problem::NotDrawn { element } => {
                    tr!(
                        "icon-pack-not-drawn",
                        file = file,
                        element = element.as_str()
                    )
                }
                svg::Problem::Ignored { attribute } => tr!(
                    "icon-pack-ignored",
                    file = file,
                    attribute = attribute.as_str()
                ),
                svg::Problem::StyleSheet => tr!("icon-pack-style-sheet", file = file),
                svg::Problem::PlainInk => tr!("icon-pack-plain-ink", file = file),
                svg::Problem::NoMask { id } => {
                    tr!("icon-pack-no-mask", file = file, id = id.as_str())
                }
                svg::Problem::TooDeep => tr!(
                    "icon-pack-too-deep",
                    file = file,
                    most = svg::DEEPEST.to_string()
                ),
                svg::Problem::TooDetailed => tr!(
                    "icon-pack-too-detailed",
                    file = file,
                    most = svg::MOST_SEGMENTS.to_string()
                ),
            },
            Problem::NoManifest => tr!("icon-pack-no-manifest", file = file),
            Problem::ManifestLine { line } => {
                tr!(
                    "icon-pack-manifest-line",
                    file = file,
                    line = line.to_string()
                )
            }
            Problem::UnknownKey { key } => {
                tr!("icon-pack-unknown-key", file = file, key = key.as_str())
            }
            Problem::NoName => tr!("icon-pack-no-name", file = file),
            Problem::NoFormat => tr!("icon-pack-no-format", file = file),
            Problem::NewerFormat { format } => tr!(
                "icon-pack-newer-format",
                file = file,
                format = format.to_string()
            ),
            Problem::UnknownIcon { name } => {
                tr!("icon-pack-unknown-icon", file = file, name = name.as_str())
            }
            Problem::NoSolidTwin { name } => {
                tr!("icon-pack-no-solid-twin", file = file, name = name.as_str())
            }
            Problem::NoSolid { name } => {
                tr!("icon-pack-no-solid", file = file, name = name.as_str())
            }
            Problem::Skipped => tr!("icon-pack-skipped", file = file),
            Problem::TooManyFiles => tr!("icon-pack-too-many-files", file = file),
            Problem::ChosenMissing { id } => tr!("icon-pack-chosen-missing", id = id.as_str()),
        }
    }
}

/// Why something asked of the icon home was not done.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PackError {
    /// A pack id that is not `a-z`, `0-9` and `-`, or longer than 64.
    BadId(String),
    /// No pack of that id is installed.
    NotInstalled(String),
    /// The source holds neither a manifest nor a single icon.
    NotAPack,
    /// The archive cannot be read.
    Archive(String),
    /// The source is larger than a pack may be.
    TooLarge,
    /// The icon has no solid twin to replace.
    NoSolidTwin(Icon),
    /// The SVG of a single icon draws nothing Sue can read.
    Unusable(Vec<Note>),
    /// Reading or writing a file failed.
    Io(String),
}

impl PackError {
    /// The error in words, in the current language.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::BadId(id) => tr!("icon-pack-bad-id", id = id.as_str()),
            Self::NotInstalled(id) => tr!("icon-pack-not-installed", id = id.as_str()),
            Self::NotAPack => tr!("icon-pack-not-a-pack"),
            Self::Archive(why) => tr!("icon-pack-archive", why = why.as_str()),
            Self::TooLarge => tr!("icon-pack-pack-too-large"),
            Self::NoSolidTwin(icon) => tr!("icon-pack-icon-no-solid", name = icon.name()),
            Self::Unusable(notes) => {
                let why: Vec<String> = notes.iter().map(Note::message).collect();
                tr!("icon-pack-unusable", why = why.join(" "))
            }
            Self::Io(why) => tr!("icon-pack-io", why = why.as_str()),
        }
    }
}

impl std::fmt::Display for PackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for PackError {}

fn io(error: &std::io::Error) -> PackError {
    PackError::Io(error.to_string())
}

// ----- The manifest -------------------------------------------------------------

/// What a pack says about itself.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Manifest {
    /// Its name, as people read it.
    pub name: String,
    /// Who made it.
    pub author: String,
    /// Its licence, best as an SPDX name (`CC-BY-4.0`, `MIT`).
    pub licence: String,
    /// Its own version.
    pub version: String,
    /// The pack format it is written in ([`FORMAT`]).
    pub format: u32,
    /// A sentence about it, if it has one.
    pub description: String,
}

impl Manifest {
    /// Reads a manifest: `key = value` lines, `#` for a comment.
    #[must_use]
    pub fn parse(text: &str, file: &str) -> (Self, Vec<Note>) {
        let mut manifest = Self::default();
        let mut notes = Vec::new();
        let mut note = |problem| {
            notes.push(Note {
                file: file.to_owned(),
                problem,
            });
        };
        let mut format = None;
        for (index, line) in text.lines().enumerate() {
            let line = line.trim().trim_start_matches('\u{feff}');
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                note(Problem::ManifestLine { line: index + 1 });
                continue;
            };
            let value = value.trim().to_owned();
            match key.trim().to_ascii_lowercase().as_str() {
                "name" => manifest.name = value,
                "author" => manifest.author = value,
                "licence" | "license" => manifest.licence = value,
                "version" => manifest.version = value,
                "description" => manifest.description = value,
                "format" => format = value.parse::<u32>().ok().filter(|format| *format > 0),
                other => note(Problem::UnknownKey {
                    key: other.to_owned(),
                }),
            }
        }
        if manifest.name.is_empty() {
            note(Problem::NoName);
        }
        manifest.format = format.unwrap_or_else(|| {
            note(Problem::NoFormat);
            FORMAT
        });
        if manifest.format > FORMAT {
            note(Problem::NewerFormat {
                format: manifest.format,
            });
        }
        (manifest, notes)
    }

    /// The manifest as its file.
    #[must_use]
    pub fn write(&self) -> String {
        let mut out = String::from("# A Sue icon pack: docs/icon-packs.md says how to make one.\n");
        let _ = writeln!(out, "format = {}", self.format.max(1));
        for (key, value) in [
            ("name", &self.name),
            ("author", &self.author),
            ("licence", &self.licence),
            ("version", &self.version),
            ("description", &self.description),
        ] {
            let _ = writeln!(out, "{key} = {}", value.replace(['\r', '\n'], " "));
        }
        out
    }
}

/// An id for a pack called `name`: lower case letters, digits and
/// hyphens. A name with letters outside `a-z` (`Ícones`, `アイコン`)
/// ends in a short number made from the whole name, so that two such
/// names never share an id while the same name always gets the same
/// one (installing it again updates it).
#[must_use]
pub fn id_for(name: &str) -> String {
    let mut id = String::new();
    let mut beyond = false;
    for ch in name.trim().chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            id.push(ch);
        } else {
            beyond |= ch.is_alphanumeric();
            if !id.is_empty() && !id.ends_with('-') {
                id.push('-');
            }
        }
    }
    id.truncate(if beyond { 55 } else { 64 });
    let mut id = id.trim_end_matches('-').to_owned();
    if beyond {
        // FNV-1a of the name: stable across builds and machines.
        let hash = name.trim().bytes().fold(0x811c_9dc5_u32, |hash, byte| {
            (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
        });
        if id.is_empty() {
            id.push_str("pack");
        }
        let _ = write!(id, "-{hash:08x}");
    }
    if id.is_empty() { "pack".to_owned() } else { id }
}

/// Whether `id` can name a pack.
#[must_use]
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !id.starts_with('-')
}

// ----- Reading drawings -------------------------------------------------------

/// The icon and style a file of a pack draws, from its name.
#[must_use]
pub fn file_icon(file: &str) -> Option<(Result<Icon, String>, IconStyle)> {
    let lower = file.to_ascii_lowercase();
    let (stem, style) = match lower.strip_suffix(SOLID) {
        Some(stem) => (stem.to_owned(), IconStyle::Solid),
        None => (lower.strip_suffix(".svg")?.to_owned(), IconStyle::Line),
    };
    Some((Icon::from_name(&stem).ok_or(stem), style))
}

/// The file name of `icon` in `style`.
#[must_use]
pub fn file_name(icon: Icon, style: IconStyle) -> String {
    match style {
        IconStyle::Line => format!("{}.svg", icon.name()),
        IconStyle::Solid => format!("{}{SOLID}", icon.name()),
    }
}

/// Reads the bytes of one SVG file: the drawing, if it draws anything,
/// and its notes.
#[must_use]
pub fn read_svg(file: &str, bytes: &[u8]) -> (Option<Drawing>, Vec<Note>) {
    let note = |problem| Note {
        file: file.to_owned(),
        problem: Problem::Svg(problem),
    };
    if bytes.len() as u64 > LARGEST_FILE {
        let bytes = bytes.len() as u64;
        return (None, vec![note(svg::Problem::TooLarge { bytes })]);
    }
    let Ok(text) = std::str::from_utf8(bytes) else {
        return (None, vec![note(svg::Problem::NotText)]);
    };
    let (drawing, problems) = svg::read(text.trim_start_matches('\u{feff}'));
    let usable = !drawing.is_empty();
    (
        usable.then_some(drawing),
        problems.into_iter().map(note).collect(),
    )
}

// ----- A set of drawings ------------------------------------------------------

/// Where the drawing of an icon comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Source {
    /// Sue's own.
    Sue,
    /// The chosen pack.
    Pack,
    /// A single icon replaced on its own.
    Override,
}

static NEXT_SET: AtomicU64 = AtomicU64::new(1);

/// The drawings that replace Sue's own: what is drawn in place of each
/// icon and style that has one. Anything else is Sue's own.
#[derive(Clone, Debug, Default)]
pub struct IconSet {
    /// Tells this set's tiles from those of another; 0 for none.
    id: u64,
    drawings: HashMap<(Icon, IconStyle), (Arc<Drawing>, Source)>,
}

impl IconSet {
    /// Sue's own icons and nothing else.
    #[must_use]
    pub fn sue() -> Self {
        Self::default()
    }

    fn with(drawings: HashMap<(Icon, IconStyle), (Arc<Drawing>, Source)>) -> Self {
        Self {
            id: NEXT_SET.fetch_add(1, Ordering::Relaxed),
            drawings,
        }
    }

    /// What tells its tiles apart from those of other sets: 0 for a set
    /// that replaces nothing.
    #[must_use]
    pub fn key(&self) -> u64 {
        if self.drawings.is_empty() { 0 } else { self.id }
    }

    /// How many drawings it holds, solid twins counted.
    #[must_use]
    pub fn len(&self) -> usize {
        self.drawings.len()
    }

    /// Whether it replaces nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.drawings.is_empty()
    }

    /// The style an icon is really drawn in: an icon without a solid
    /// twin is its line in both.
    const fn style_of(icon: Icon, style: IconStyle) -> IconStyle {
        if icon.has_solid() {
            style
        } else {
            IconStyle::Line
        }
    }

    /// Where the drawing of `icon` in `style` comes from.
    #[must_use]
    pub fn source(&self, icon: Icon, style: IconStyle) -> Source {
        self.drawings
            .get(&(icon, Self::style_of(icon, style)))
            .map_or(Source::Sue, |(_, source)| *source)
    }

    /// The drawing that replaces `icon` in `style`, if one does.
    #[must_use]
    pub fn drawing(&self, icon: Icon, style: IconStyle) -> Option<&Drawing> {
        self.drawings
            .get(&(icon, Self::style_of(icon, style)))
            .map(|(drawing, _)| drawing.as_ref())
    }

    /// `icon` in `style` as a tile `pixels` wide, where Sue's pen is
    /// `stroke` pixels wide: from this set's drawing, or Sue's own.
    #[must_use]
    pub fn tile(&self, icon: Icon, style: IconStyle, pixels: usize, stroke: f32) -> Tile {
        match self.drawing(icon, style) {
            Some(drawing) => draw::render(drawing, pixels, stroke),
            None => icon.tile(Self::style_of(icon, style), pixels, stroke),
        }
    }
}

// ----- A pack -----------------------------------------------------------------

/// A pack, read: what it says about itself, what it draws and what could
/// not be read.
#[derive(Clone, Debug)]
pub struct Pack {
    /// Its id: the name of its folder.
    pub id: String,
    /// Its manifest.
    pub manifest: Manifest,
    /// Its drawings.
    drawings: BTreeMap<(Icon, IconStyle), Arc<Drawing>>,
    /// What could not be read, file by file.
    pub notes: Vec<Note>,
}

impl Pack {
    /// Reads a pack from its files, by name: the manifest, the SVG
    /// files, and anything else, which is noted and left out. `id` stands
    /// for the name when there is no manifest.
    #[must_use]
    pub fn from_files(id: &str, files: &[(String, Vec<u8>)]) -> Self {
        let mut notes = Vec::new();
        let manifest = match files
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(MANIFEST))
        {
            Some((name, bytes)) => {
                let (manifest, found) = Manifest::parse(&String::from_utf8_lossy(bytes), name);
                notes.extend(found);
                manifest
            }
            None => {
                notes.push(Note {
                    file: id.to_owned(),
                    problem: Problem::NoManifest,
                });
                Manifest {
                    name: id.to_owned(),
                    format: FORMAT,
                    ..Manifest::default()
                }
            }
        };
        let mut drawings = BTreeMap::new();
        for (index, (name, bytes)) in files.iter().enumerate() {
            if name.eq_ignore_ascii_case(MANIFEST) || kept_beside(name) {
                continue;
            }
            if index >= MOST_FILES {
                notes.push(Note {
                    file: id.to_owned(),
                    problem: Problem::TooManyFiles,
                });
                break;
            }
            let note = |problem| Note {
                file: name.clone(),
                problem,
            };
            match file_icon(name) {
                None => notes.push(note(Problem::Skipped)),
                Some((Err(unknown), _)) => {
                    notes.push(note(Problem::UnknownIcon { name: unknown }));
                }
                Some((Ok(icon), IconStyle::Solid)) if !icon.has_solid() => {
                    notes.push(note(Problem::NoSolidTwin {
                        name: icon.name().to_owned(),
                    }));
                }
                Some((Ok(icon), style)) => {
                    let (drawing, found) = read_svg(name, bytes);
                    notes.extend(found);
                    if let Some(drawing) = drawing {
                        drawings.insert((icon, style), Arc::new(drawing));
                    }
                }
            }
        }
        for (icon, _) in drawings.keys() {
            if icon.has_solid() && !drawings.contains_key(&(*icon, IconStyle::Solid)) {
                notes.push(Note {
                    file: file_name(*icon, IconStyle::Line),
                    problem: Problem::NoSolid {
                        name: icon.name().to_owned(),
                    },
                });
            }
        }
        notes.dedup();
        Self {
            id: id.to_owned(),
            manifest,
            drawings,
            notes,
        }
    }

    /// Reads the pack in `folder`.
    ///
    /// # Errors
    /// The folder cannot be read, or holds neither a manifest nor an
    /// icon.
    pub fn read(folder: &Path) -> Result<Self, PackError> {
        let id = folder
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (files, skipped) = folder_files(folder)?;
        let mut pack = Self::from_files(&id, &files);
        pack.notes.extend(skipped);
        if pack.drawings.is_empty()
            && !files
                .iter()
                .any(|(name, _)| name.eq_ignore_ascii_case(MANIFEST))
        {
            return Err(PackError::NotAPack);
        }
        Ok(pack)
    }

    /// The icons it draws, solid twins counted.
    #[must_use]
    pub fn drawn(&self) -> Vec<(Icon, IconStyle)> {
        self.drawings.keys().copied().collect()
    }

    /// Its drawings as a set, with nothing replaced on its own: for a
    /// preview of the pack by itself.
    #[must_use]
    pub fn set(&self) -> IconSet {
        IconSet::with(
            self.drawings
                .iter()
                .map(|(key, drawing)| (*key, (Arc::clone(drawing), Source::Pack)))
                .collect(),
        )
    }
}

/// Files by name, with their bytes.
type Files = Vec<(String, Vec<u8>)>;

/// Files a pack may carry beside its icons, copied along: its licence
/// and a note from its maker.
fn kept_beside(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let stem = lower
        .strip_suffix(".txt")
        .or_else(|| lower.strip_suffix(".md"))
        .unwrap_or(&lower);
    matches!(stem, "licence" | "license" | "readme" | "copying")
}

/// The files at the top of `folder` (no links, no folders), with notes
/// for what was left out.
fn folder_files(folder: &Path) -> Result<(Files, Vec<Note>), PackError> {
    let mut files = Vec::new();
    let mut notes = Vec::new();
    let mut total = 0_u64;
    let mut entries: Vec<_> = std::fs::read_dir(folder)
        .map_err(|error| io(&error))?
        .filter_map(Result::ok)
        .collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(kind) = std::fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if !kind.is_file() {
            notes.push(Note {
                file: name,
                problem: Problem::Skipped,
            });
            continue;
        }
        let wanted =
            name.eq_ignore_ascii_case(MANIFEST) || kept_beside(&name) || file_icon(&name).is_some();
        if !wanted {
            notes.push(Note {
                file: name,
                problem: Problem::Skipped,
            });
            continue;
        }
        if files.len() >= MOST_FILES {
            notes.push(Note {
                file: name,
                problem: Problem::TooManyFiles,
            });
            break;
        }
        if kind.len() > LARGEST_FILE {
            notes.push(Note {
                file: name,
                problem: Problem::Svg(svg::Problem::TooLarge { bytes: kind.len() }),
            });
            continue;
        }
        total += kind.len();
        if total > LARGEST_PACK {
            return Err(PackError::TooLarge);
        }
        let bytes = std::fs::read(entry.path()).map_err(|error| io(&error))?;
        files.push((name, bytes));
    }
    Ok((files, notes))
}

/// The sizes a zip archive says its files have, from its central
/// directory, before anything is inflated.
fn declared_sizes(bytes: &[u8]) -> Option<Vec<u64>> {
    let u16_at = |at: usize| Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?));
    let u32_at = |at: usize| Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    let floor = bytes.len().saturating_sub(65_557);
    let end = (floor..bytes.len().saturating_sub(21))
        .rev()
        .find(|at| u32_at(*at) == Some(0x0605_4b50))?;
    let count = usize::from(u16_at(end + 10)?);
    let mut at = u32_at(end + 16)? as usize;
    let mut sizes = Vec::with_capacity(count.min(MOST_FILES));
    for _ in 0..count {
        if u32_at(at)? != 0x0201_4b50 {
            return None;
        }
        sizes.push(u64::from(u32_at(at + 24)?));
        let skip = 46
            + usize::from(u16_at(at + 28)?)
            + usize::from(u16_at(at + 30)?)
            + usize::from(u16_at(at + 32)?);
        at += skip;
    }
    Some(sizes)
}

/// The files of a pack archive: those at its top, or in its one top
/// folder when everything is in one.
fn archive_files(bytes: &[u8]) -> Result<(Files, Vec<Note>), PackError> {
    if bytes.len() as u64 > LARGEST_PACK {
        return Err(PackError::TooLarge);
    }
    let sizes = declared_sizes(bytes).ok_or_else(|| PackError::Archive(String::new()))?;
    if sizes.len() > 4 * MOST_FILES || sizes.iter().sum::<u64>() > LARGEST_PACK {
        return Err(PackError::TooLarge);
    }
    let entries = read_zip(bytes).map_err(PackError::Archive)?;
    let entries: Vec<(String, Vec<u8>)> = entries
        .into_iter()
        .map(|(name, data)| (name.replace('\\', "/"), data))
        .filter(|(name, _)| !name.starts_with("__MACOSX/") && !name.ends_with('/'))
        .collect();
    let top = |name: &str| name.split_once('/').map(|(first, _)| first.to_owned());
    let common = entries
        .first()
        .and_then(|(name, _)| top(name))
        .filter(|first| {
            entries
                .iter()
                .all(|(name, _)| top(name).as_deref() == Some(first.as_str()))
        });
    let mut files = Vec::new();
    let mut notes = Vec::new();
    for (name, data) in entries {
        let inner = match &common {
            Some(first) => name[first.len() + 1..].to_owned(),
            None => name,
        };
        let plain = !inner.contains('/') && !inner.is_empty() && inner != "." && inner != "..";
        let wanted = plain
            && (inner.eq_ignore_ascii_case(MANIFEST)
                || kept_beside(&inner)
                || file_icon(&inner).is_some());
        if !wanted {
            notes.push(Note {
                file: inner,
                problem: Problem::Skipped,
            });
        } else if data.len() as u64 > LARGEST_FILE {
            notes.push(Note {
                file: inner,
                problem: Problem::Svg(svg::Problem::TooLarge {
                    bytes: data.len() as u64,
                }),
            });
        } else if files.len() >= MOST_FILES {
            notes.push(Note {
                file: inner,
                problem: Problem::TooManyFiles,
            });
            break;
        } else {
            files.push((inner, data));
        }
    }
    Ok((files, notes))
}

// ----- Sue's own set as a pack --------------------------------------------------

/// The manifest of Sue's own set, the start of a new pack.
#[must_use]
pub fn sue_manifest() -> Manifest {
    Manifest {
        name: "Sue".to_owned(),
        author: "The Sue authors".to_owned(),
        licence: "the licence of Sue".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        format: FORMAT,
        description: "Sue's own icons, as a template for a new pack.".to_owned(),
    }
}

/// The files of Sue's own set as a pack, by name: `manifest` and an SVG
/// for every icon and every solid twin.
#[must_use]
pub fn sue_files(manifest: &Manifest) -> Vec<(String, String)> {
    let mut files = vec![(MANIFEST.to_owned(), manifest.write())];
    for icon in Icon::ALL {
        for style in IconStyle::ALL {
            if style == IconStyle::Solid && !icon.has_solid() {
                continue;
            }
            files.push((file_name(*icon, style), super::svg(*icon, style)));
        }
    }
    files
}

/// Writes Sue's own set into `folder` as a pack, so that a maker starts
/// from a template.
///
/// # Errors
/// A file cannot be written.
pub fn export(folder: &Path, manifest: &Manifest) -> Result<(), PackError> {
    std::fs::create_dir_all(folder).map_err(|error| io(&error))?;
    for (name, text) in sue_files(manifest) {
        std::fs::write(folder.join(name), text).map_err(|error| io(&error))?;
    }
    Ok(())
}

/// Sue's own set as a pack archive (`.zip`).
#[must_use]
pub fn export_archive(manifest: &Manifest) -> Vec<u8> {
    let mut zip = ZipWriter::new();
    for (name, text) in sue_files(manifest) {
        zip.add_text(&name, &text);
    }
    zip.finish()
}

// ----- The icon home --------------------------------------------------------------

/// The folder Sue keeps icon packs and replaced icons in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IconHome {
    root: PathBuf,
}

impl IconHome {
    /// The icon home in `root`.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The icon home among Sue's data: `<data>/icons`.
    #[must_use]
    pub fn of(paths: &PlatformPaths) -> Self {
        Self::new(paths.data_dir.join("icons"))
    }

    /// The icon home of this device, if its folders can be found.
    #[must_use]
    pub fn detect() -> Option<Self> {
        PlatformPaths::detect().ok().map(|paths| Self::of(&paths))
    }

    /// Its folder.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn packs_dir(&self) -> PathBuf {
        self.root.join("packs")
    }

    /// The folder of single icons replaced on their own.
    #[must_use]
    pub fn overrides_dir(&self) -> PathBuf {
        self.root.join("overrides")
    }

    fn chosen_file(&self) -> PathBuf {
        self.root.join("chosen.txt")
    }

    fn pack_dir(&self, id: &str) -> Result<PathBuf, PackError> {
        if valid_id(id) {
            Ok(self.packs_dir().join(id))
        } else {
            Err(PackError::BadId(id.to_owned()))
        }
    }

    /// The ids of the installed packs, in order.
    #[must_use]
    pub fn ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = std::fs::read_dir(self.packs_dir())
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|id| valid_id(id))
            .collect();
        ids.sort();
        ids
    }

    /// Every installed pack, read: its manifest, what it draws and its
    /// notes. This reads every file of every pack.
    #[must_use]
    pub fn packs(&self) -> Vec<Pack> {
        self.ids()
            .iter()
            .filter_map(|id| self.pack(id).ok())
            .collect()
    }

    /// The installed pack `id`, read.
    ///
    /// # Errors
    /// A bad id, or no such pack.
    pub fn pack(&self, id: &str) -> Result<Pack, PackError> {
        let folder = self.pack_dir(id)?;
        if !folder.is_dir() {
            return Err(PackError::NotInstalled(id.to_owned()));
        }
        Pack::read(&folder)
    }

    /// Installs the pack in `source`, a folder or a `.zip` archive, and
    /// gives it back read. Only the manifest, the icons' SVG files and a
    /// licence or read-me file are copied. A pack with the same id (from
    /// the name in its manifest) is replaced: that is how a pack is
    /// updated.
    ///
    /// # Errors
    /// The source cannot be read, is too large, or is no pack.
    pub fn install(&self, source: &Path) -> Result<Pack, PackError> {
        if source.is_dir() {
            let (files, notes) = folder_files(source)?;
            let fallback = source
                .file_name()
                .map(|name| id_for(&name.to_string_lossy()))
                .unwrap_or_default();
            self.install_files(&fallback, files, notes)
        } else {
            let length = std::fs::metadata(source).map_err(|error| io(&error))?.len();
            if length > LARGEST_PACK {
                return Err(PackError::TooLarge);
            }
            let bytes = std::fs::read(source).map_err(|error| io(&error))?;
            let fallback = source
                .file_stem()
                .map(|name| id_for(&name.to_string_lossy()))
                .unwrap_or_default();
            self.install_archive(&fallback, &bytes)
        }
    }

    /// Installs a pack from the bytes of a `.zip` archive; `fallback`
    /// names it when it has no manifest.
    ///
    /// # Errors
    /// As [`install`](Self::install).
    pub fn install_archive(&self, fallback: &str, bytes: &[u8]) -> Result<Pack, PackError> {
        let (files, notes) = archive_files(bytes)?;
        self.install_files(fallback, files, notes)
    }

    fn install_files(
        &self,
        fallback: &str,
        files: Vec<(String, Vec<u8>)>,
        notes: Vec<Note>,
    ) -> Result<Pack, PackError> {
        let fallback = id_for(fallback);
        let read = Pack::from_files(&fallback, &files);
        let has_manifest = files
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case(MANIFEST));
        if read.drawings.is_empty() && !has_manifest {
            return Err(PackError::NotAPack);
        }
        let id = if read.manifest.name.trim().is_empty() {
            fallback
        } else {
            id_for(&read.manifest.name)
        };
        let target = self.pack_dir(&id)?;
        let packs = self.packs_dir();
        std::fs::create_dir_all(&packs).map_err(|error| io(&error))?;
        // Written beside, then put in place, so that a pack is never
        // half there.
        let incoming = packs.join(format!(".incoming-{id}"));
        let _ = std::fs::remove_dir_all(&incoming);
        std::fs::create_dir_all(&incoming).map_err(|error| io(&error))?;
        let written = (|| {
            for (name, bytes) in &files {
                let name = if name.eq_ignore_ascii_case(MANIFEST) {
                    MANIFEST.to_owned()
                } else {
                    name.to_ascii_lowercase()
                };
                std::fs::write(incoming.join(name), bytes)?;
            }
            Ok::<_, std::io::Error>(())
        })();
        if let Err(error) = written {
            let _ = std::fs::remove_dir_all(&incoming);
            return Err(io(&error));
        }
        let outgoing = packs.join(format!(".outgoing-{id}"));
        let _ = std::fs::remove_dir_all(&outgoing);
        if target.exists() {
            std::fs::rename(&target, &outgoing).map_err(|error| io(&error))?;
        }
        if let Err(error) = std::fs::rename(&incoming, &target) {
            if outgoing.exists() {
                let _ = std::fs::rename(&outgoing, &target);
            }
            return Err(io(&error));
        }
        let _ = std::fs::remove_dir_all(&outgoing);
        let mut pack = Pack::read(&target)?;
        pack.notes.extend(notes);
        Ok(pack)
    }

    /// Removes the installed pack `id`. When it was the chosen one, Sue's
    /// own icons are chosen.
    ///
    /// # Errors
    /// A bad id, no such pack, or a folder that cannot be removed.
    pub fn remove(&self, id: &str) -> Result<(), PackError> {
        let folder = self.pack_dir(id)?;
        if !folder.is_dir() {
            return Err(PackError::NotInstalled(id.to_owned()));
        }
        std::fs::remove_dir_all(&folder).map_err(|error| io(&error))?;
        if self.chosen().as_deref() == Some(id) {
            self.choose(None)?;
        }
        Ok(())
    }

    /// The id of the chosen pack, or `None` for Sue's own icons.
    #[must_use]
    pub fn chosen(&self) -> Option<String> {
        let text = std::fs::read_to_string(self.chosen_file()).ok()?;
        let id = text.trim();
        valid_id(id).then(|| id.to_owned())
    }

    /// Chooses the pack `id`, or Sue's own icons with `None`.
    ///
    /// # Errors
    /// A bad id, no such pack, or a file that cannot be written.
    pub fn choose(&self, id: Option<&str>) -> Result<(), PackError> {
        match id {
            Some(id) => {
                if !self.pack_dir(id)?.is_dir() {
                    return Err(PackError::NotInstalled(id.to_owned()));
                }
                std::fs::create_dir_all(&self.root).map_err(|error| io(&error))?;
                std::fs::write(self.chosen_file(), format!("{id}\n")).map_err(|error| io(&error))
            }
            None => match std::fs::remove_file(self.chosen_file()) {
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(io(&error)),
                _ => Ok(()),
            },
        }
    }

    /// The icons replaced on their own, in order.
    #[must_use]
    pub fn overrides(&self) -> Vec<(Icon, IconStyle)> {
        let mut found: Vec<(Icon, IconStyle)> = std::fs::read_dir(self.overrides_dir())
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let (icon, style) = file_icon(&entry.file_name().to_string_lossy())?;
                icon.ok().map(|icon| (icon, style))
            })
            .collect();
        found.sort();
        found
    }

    /// Replaces `icon` in `style` on its own with the SVG `bytes`, and
    /// gives back what in them could not be read.
    ///
    /// # Errors
    /// The SVG draws nothing Sue can read, the icon has no solid twin to
    /// replace, or the file cannot be written.
    pub fn set_override(
        &self,
        icon: Icon,
        style: IconStyle,
        bytes: &[u8],
    ) -> Result<Vec<Note>, PackError> {
        if style == IconStyle::Solid && !icon.has_solid() {
            return Err(PackError::NoSolidTwin(icon));
        }
        let name = file_name(icon, style);
        let (drawing, notes) = read_svg(&name, bytes);
        if drawing.is_none() {
            return Err(PackError::Unusable(notes));
        }
        let folder = self.overrides_dir();
        std::fs::create_dir_all(&folder).map_err(|error| io(&error))?;
        std::fs::write(folder.join(name), bytes).map_err(|error| io(&error))?;
        Ok(notes)
    }

    /// Replaces `icon` in `style` on its own with the SVG file `source`.
    ///
    /// # Errors
    /// As [`set_override`](Self::set_override), and a file that cannot be
    /// read or is too large.
    pub fn set_override_from(
        &self,
        icon: Icon,
        style: IconStyle,
        source: &Path,
    ) -> Result<Vec<Note>, PackError> {
        let length = std::fs::metadata(source).map_err(|error| io(&error))?.len();
        if length > LARGEST_FILE {
            return Err(PackError::TooLarge);
        }
        let bytes = std::fs::read(source).map_err(|error| io(&error))?;
        self.set_override(icon, style, &bytes)
    }

    /// Gives `icon` in `style` back to the pack or to Sue.
    ///
    /// # Errors
    /// The file cannot be removed.
    pub fn clear_override(&self, icon: Icon, style: IconStyle) -> Result<(), PackError> {
        match std::fs::remove_file(self.overrides_dir().join(file_name(icon, style))) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(io(&error)),
            _ => Ok(()),
        }
    }

    /// The set to draw with `pack` in use (or none), and the notes of
    /// everything read: single overrides first, then the pack, then Sue.
    fn load_with(&self, pack: Option<&str>) -> (IconSet, Vec<Note>) {
        let mut notes = Vec::new();
        let mut drawings: HashMap<(Icon, IconStyle), (Arc<Drawing>, Source)> = HashMap::new();
        if let Some(id) = pack {
            match self.pack(id) {
                Ok(pack) => {
                    notes.extend(pack.notes.iter().cloned());
                    for (key, drawing) in &pack.drawings {
                        drawings.insert(*key, (Arc::clone(drawing), Source::Pack));
                    }
                }
                Err(_) => notes.push(Note {
                    file: id.to_owned(),
                    problem: Problem::ChosenMissing { id: id.to_owned() },
                }),
            }
        }
        for (icon, style) in self.overrides() {
            let name = file_name(icon, style);
            let Ok(bytes) = std::fs::read(self.overrides_dir().join(&name)) else {
                continue;
            };
            let (drawing, found) = read_svg(&name, &bytes);
            notes.extend(found);
            if let Some(drawing) = drawing
                && (style == IconStyle::Line || icon.has_solid())
            {
                drawings.insert((icon, style), (Arc::new(drawing), Source::Override));
            }
        }
        (IconSet::with(drawings), notes)
    }

    /// The set to draw: single overrides, then the chosen pack, then
    /// Sue's own icons; with the notes of every file read.
    #[must_use]
    pub fn load(&self) -> (IconSet, Vec<Note>) {
        self.load_with(self.chosen().as_deref())
    }

    /// The set as it would be with the pack `id` chosen (overrides
    /// included), for a preview before choosing it. This reads every
    /// file of the pack, and each set made has tiles of its own: keep
    /// the set while the preview is open rather than asking each frame.
    #[must_use]
    pub fn preview(&self, id: &str) -> (IconSet, Vec<Note>) {
        self.load_with(Some(id))
    }
}

// ----- Showing a set ----------------------------------------------------------------

/// The set an egui context draws its icons from.
#[derive(Clone, Default)]
struct Shown(Arc<Mutex<Arc<IconSet>>>);

fn shown(ctx: &Context) -> Shown {
    ctx.data_mut(|data| data.get_temp_mut_or_default::<Shown>(Id::NULL).clone())
}

/// The set `ctx` draws its icons from now: Sue's own until [`show`] is
/// called.
#[must_use]
pub fn showing(ctx: &Context) -> Arc<IconSet> {
    Arc::clone(&shown(ctx).0.lock().unwrap_or_else(PoisonError::into_inner))
}

/// Draws the icons of `ctx` from `set` from the next frame on: the tiles
/// of the set before are dropped and a frame is asked for.
pub fn show(ctx: &Context, set: IconSet) {
    *shown(ctx).0.lock().unwrap_or_else(PoisonError::into_inner) = Arc::new(set);
    super::cache::forget(ctx);
    ctx.request_repaint();
}

/// Loads the set of `home` and shows it in `ctx`: what to call after a
/// pack was installed, chosen or removed, or an icon replaced. Gives back
/// the notes of every file read.
pub fn refresh(ctx: &Context, home: &IconHome) -> Vec<Note> {
    let (set, notes) = home.load();
    show(ctx, set);
    notes
}

/// Draws `icon` in `style` and `tint` from `set` (which need not be the
/// one shown) into `rect`, as [`super::paint_styled`] draws: for the
/// preview of a pack.
pub fn paint_from(
    painter: &Painter,
    rect: Rect,
    set: &IconSet,
    icon: Icon,
    style: IconStyle,
    tint: Color32,
) {
    super::paint_with(painter, rect, icon, style, tint, Some(set));
}

#[cfg(test)]
mod tests;
