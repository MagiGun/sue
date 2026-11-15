//! Color sets: the swatches of the Color Set palette. Several sets ship
//! (Default, Grays, Skin Tones, Pastels), people make their own, and every
//! set can gain, replace, name and lose colors. A set is a row of cells,
//! and a cell may be empty: a new set starts as empty cells to fill, and
//! deleting a color leaves its cell where it was, so a set arranged by
//! hand keeps its arrangement.
//!
//! The sets are kept in Sue Paint's data folder, in one plain text file
//! under a versioned header, a set a line (`docs/paint-color-sets.md`):
//!
//! ```text
//! # sue paint color sets 2
//! Default <tab> #000000/Black #ffffff - #f03c5a ...
//! ```
//!
//! A cell is `#rrggbb`, `#rrggbb/name` with the name's `%`, spaces, tabs
//! and line breaks written as `%25`, `%20`, `%09`, `%0A` and `%0D`, or `-`
//! for an empty cell. A set's name has its `%`, tabs and line breaks
//! written so too, and its spaces as they are. Version 1 (Sue 0.0.75)
//! had no names and no empty cells, so its lines read the same way.
//!
//! A set is shared as a GIMP palette (`.gpl`), which GIMP, Krita, Inkscape
//! and most painting programs read: Sue writes the names and marks each
//! empty cell with a comment line that other programs skip. Sue reads
//! GIMP palettes, lists of hex colors and Clip Studio Paint's color set
//! files (`.cls`).

use std::path::{Path, PathBuf};

use crate::color;

/// The header of the color set file.
pub const HEADER: &str = "# sue paint color sets 2";

/// The file in Sue Paint's data folder.
pub const FILE: &str = "color_sets.txt";

/// How many empty cells a new set starts with: four rows of eight.
pub const NEW_CELLS: usize = 32;

/// The comment line that stands for an empty cell in a GIMP palette Sue
/// writes. Other programs skip it, as every line that starts with `#`.
pub const GPL_EMPTY: &str = "# Sue: empty cell";

/// One color of a set, with the name someone gave it (empty for none).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Swatch {
    /// The color.
    pub rgb: [u8; 3],
    /// Its name, as people gave it; empty when it has none.
    pub name: String,
}

impl Swatch {
    /// A swatch of `rgb` with no name.
    #[must_use]
    pub fn of(rgb: [u8; 3]) -> Self {
        Self {
            rgb,
            name: String::new(),
        }
    }

    /// What a hover over it says: its name and its hex code.
    #[must_use]
    pub fn label(&self) -> String {
        let hex = color::hex(self.rgb);
        if self.name.is_empty() {
            hex
        } else {
            format!("{} ({hex})", self.name)
        }
    }
}

/// A place in a set: a color, or `None` for an empty cell.
pub(crate) type Cell = Option<Swatch>;

/// A named row of cells.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ColorSet {
    /// What the set is called.
    pub name: String,
    /// Its cells, in order.
    pub cells: Vec<Cell>,
}

impl ColorSet {
    /// A set of `colors`, none of them named.
    #[must_use]
    pub fn of(name: &str, colors: &[[u8; 3]]) -> Self {
        Self {
            name: name.to_owned(),
            cells: colors.iter().map(|rgb| Some(Swatch::of(*rgb))).collect(),
        }
    }

    /// How many cells hold a color.
    #[must_use]
    pub fn filled(&self) -> usize {
        self.cells.iter().filter(|cell| cell.is_some()).count()
    }
}

/// The sets Sue Paint ships with, which Add Standard Set adds again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Standard {
    /// Sue's default colors.
    Default,
    /// Seventeen steps from black to white.
    Grays,
    /// Skin from light to dark.
    SkinTones,
    /// Soft, light colors.
    Pastels,
}

impl Standard {
    /// Every standard set, in the order they ship.
    pub const ALL: [Self; 4] = [Self::Default, Self::Grays, Self::SkinTones, Self::Pastels];

    /// What the set is called, in the language in use.
    #[must_use]
    pub fn name(self) -> String {
        match self {
            Self::Default => tr!("color-set-standard-default"),
            Self::Grays => tr!("color-set-standard-grays"),
            Self::SkinTones => tr!("color-set-standard-skin"),
            Self::Pastels => tr!("color-set-standard-pastels"),
        }
    }

    /// The set.
    #[must_use]
    pub fn set(self) -> ColorSet {
        let colors: Vec<[u8; 3]> = match self {
            Self::Default => color::DEFAULT_SET.to_vec(),
            Self::Grays => (0..=16_u32)
                .map(|step| {
                    let value = u8::try_from(step * 255 / 16).unwrap_or(u8::MAX);
                    [value, value, value]
                })
                .collect(),
            Self::SkinTones => vec![
                [255, 224, 196],
                [250, 208, 176],
                [241, 194, 160],
                [224, 172, 135],
                [198, 141, 104],
                [168, 112, 80],
                [141, 88, 60],
                [113, 68, 45],
                [86, 52, 36],
                [60, 38, 28],
                [255, 200, 190],
                [230, 160, 150],
            ],
            Self::Pastels => vec![
                [255, 209, 220],
                [255, 228, 196],
                [255, 250, 205],
                [214, 245, 214],
                [204, 236, 255],
                [221, 214, 255],
                [245, 214, 255],
                [255, 236, 236],
                [230, 255, 245],
                [240, 240, 255],
            ],
        };
        ColorSet::of(&self.name(), &colors)
    }
}

/// The sets Sue Paint starts with.
#[must_use]
pub(crate) fn builtin() -> Vec<ColorSet> {
    Standard::ALL
        .iter()
        .map(|standard| standard.set())
        .collect()
}

/// Every set, which one shows, and the swatch picked in it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ColorSets {
    /// The sets.
    pub sets: Vec<ColorSet>,
    /// The set showing.
    pub current: usize,
    /// The cell Replace, Delete and Name act on.
    pub selected: Option<usize>,
    /// Side of a swatch, in points; 0 fits eight to a row.
    pub tile: f32,
    /// The file was read (or found missing).
    pub loaded: bool,
}

impl Default for ColorSets {
    fn default() -> Self {
        Self {
            sets: builtin(),
            current: 0,
            selected: None,
            tile: 0.0,
            loaded: false,
        }
    }
}

/// Writes `text` so that it reads back as one token of the color set
/// file: `%`, spaces, tabs and line breaks become `%25`, `%20`, `%09`,
/// `%0A` and `%0D`.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '%' => out.push_str("%25"),
            ' ' => out.push_str("%20"),
            '\t' => out.push_str("%09"),
            '\n' => out.push_str("%0A"),
            '\r' => out.push_str("%0D"),
            c => out.push(c),
        }
    }
    out
}

/// Writes a set's name so that it reads back as what comes before the
/// tab of its line: only `%`, tabs and line breaks are escaped, so the
/// file stays easy to read.
fn escape_name(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '%' => out.push_str("%25"),
            '\t' => out.push_str("%09"),
            '\n' => out.push_str("%0A"),
            '\r' => out.push_str("%0D"),
            c => out.push(c),
        }
    }
    out
}

/// Reads what [`escape`] wrote. A `%` not followed by two hex digits
/// stays as it is.
fn unescape(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%'
            && let Some(byte) = text
                .get(at + 1..at + 3)
                .and_then(|digits| u8::from_str_radix(digits, 16).ok())
        {
            out.push(byte);
            at += 3;
            continue;
        }
        out.push(bytes[at]);
        at += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// One cell of the color set file.
fn parse_cell(token: &str) -> Option<Cell> {
    if token == "-" {
        return Some(None);
    }
    let (hex, name) = token.split_once('/').unwrap_or((token, ""));
    let rgb = color::parse_hex(hex)?;
    Some(Some(Swatch {
        rgb,
        name: unescape(name),
    }))
}

/// Reads the color set file, of either version. Lines it does not
/// understand, and cells it cannot read, are skipped.
#[must_use]
pub(crate) fn parse(text: &str) -> Vec<ColorSet> {
    // Version 1 wrote names as they were (a tab or a line break became a
    // space); version 2 escapes them.
    let escaped = text
        .lines()
        .next()
        .is_some_and(|line| line.trim() == HEADER);
    text.lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| {
            let (name, cells) = line.split_once('\t')?;
            let name = if escaped {
                unescape(name.trim())
            } else {
                name.trim().to_owned()
            };
            let cells = cells.split_whitespace().filter_map(parse_cell).collect();
            Some(ColorSet { name, cells })
        })
        .filter(|set| !set.name.is_empty())
        .collect()
}

/// The color set file's text.
#[must_use]
pub(crate) fn write(sets: &[ColorSet]) -> String {
    let mut text = format!("{HEADER}\n");
    for set in sets {
        let cells: Vec<String> = set
            .cells
            .iter()
            .map(|cell| match cell {
                None => "-".to_owned(),
                Some(swatch) if swatch.name.is_empty() => color::hex(swatch.rgb),
                Some(swatch) => format!("{}/{}", color::hex(swatch.rgb), escape(&swatch.name)),
            })
            .collect();
        text.push_str(&format!(
            "{}\t{}\n",
            escape_name(&set.name),
            cells.join(" ")
        ));
    }
    text
}

/// A set read from a file someone brought in: its name when the file
/// says one, and its cells.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Imported {
    /// The name the file gives the set, if any.
    pub name: Option<String>,
    /// The cells.
    pub cells: Vec<Cell>,
}

/// Reads a GIMP palette (a `Name:` line, then `R G B name` lines) or a
/// list of hex colors.
#[must_use]
pub(crate) fn import(text: &str) -> Imported {
    let mut imported = Imported::default();
    for line in text.lines() {
        let line = line.trim();
        if line == GPL_EMPTY {
            imported.cells.push(None);
            continue;
        }
        if let Some(name) = line.strip_prefix("Name:") {
            let name = name.trim();
            if !name.is_empty() {
                imported.name = Some(name.to_owned());
            }
            continue;
        }
        if line.is_empty() || line.starts_with('#') && line.len() != 7 && line.len() != 4 {
            continue;
        }
        let mut words = line.split_whitespace();
        let Some(first) = words.next() else {
            continue;
        };
        if let Some(rgb) = hex_any(first) {
            imported.cells.push(Some(Swatch::of(rgb)));
            continue;
        }
        let numbers: Vec<Option<u8>> = line
            .split_whitespace()
            .take(3)
            .map(|word| word.parse::<u8>().ok())
            .collect();
        if let [Some(r), Some(g), Some(b)] = numbers[..] {
            let rgb = [r, g, b];
            // The name is what follows the three numbers. Sue 0.0.75 and
            // many programs write the hex code or "Untitled" there, which
            // names nothing.
            let name = line
                .split_whitespace()
                .skip(3)
                .collect::<Vec<_>>()
                .join(" ");
            let unnamed = name.eq_ignore_ascii_case(&color::hex(rgb))
                || name.eq_ignore_ascii_case("untitled");
            imported.cells.push(Some(Swatch {
                rgb,
                name: if unnamed { String::new() } else { name },
            }));
        }
    }
    imported
}

/// `#rrggbb`, `rrggbb` or the short `#rgb`.
fn hex_any(token: &str) -> Option<[u8; 3]> {
    let digits = token.trim_start_matches('#');
    if token.starts_with('#') && digits.len() == 3 && digits.is_ascii() {
        let part = |at: usize| {
            u8::from_str_radix(&digits[at..=at], 16)
                .ok()
                .map(|value| value * 17)
        };
        return Some([part(0)?, part(1)?, part(2)?]);
    }
    color::parse_hex(token)
}

/// Why a Clip Studio Paint color set could not be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ClsError {
    /// The file does not start as a color set does.
    NotAColorSet,
    /// The file ends inside a part it announced.
    Cut,
}

/// Reads `count` bytes at `*at`, moving past them.
fn take<'a>(bytes: &'a [u8], at: &mut usize, count: usize) -> Result<&'a [u8], ClsError> {
    let end = at.checked_add(count).ok_or(ClsError::Cut)?;
    let part = bytes.get(*at..end).ok_or(ClsError::Cut)?;
    *at = end;
    Ok(part)
}

/// A little-endian `u32` at `*at`.
fn take_u32(bytes: &[u8], at: &mut usize) -> Result<usize, ClsError> {
    let part = take(bytes, at, 4)?;
    let value = u32::from_le_bytes([part[0], part[1], part[2], part[3]]);
    usize::try_from(value).map_err(|_| ClsError::Cut)
}

/// A little-endian `u16` at `*at`.
fn take_u16(bytes: &[u8], at: &mut usize) -> Result<usize, ClsError> {
    let part = take(bytes, at, 2)?;
    Ok(usize::from(u16::from_le_bytes([part[0], part[1]])))
}

/// Reads a Clip Studio Paint color set (`.cls`), as its files are laid
/// out (seen in the files a Clip Studio Paint install on this machine
/// holds; nothing of theirs is kept):
///
/// - `SLCC`, two bytes of version, then a part (a little-endian `u32`
///   length, then that many bytes) holding the set's name twice: a `u16`
///   length and the name in the system's old code page, four bytes, then
///   a `u16` length and the name in UTF-8.
/// - A part holding the number of cells as a `u32`.
/// - The cells' part: each cell a part of its own holding red, green,
///   blue and alpha bytes, a `u32` that is 1 when a name follows, and
///   then a `u16` byte length and the name in UTF-16. A cell whose alpha
///   is 0 is empty.
///
/// # Errors
/// [`ClsError`] when the bytes are not a color set or end too soon.
pub(crate) fn import_cls(bytes: &[u8]) -> Result<Imported, ClsError> {
    if bytes.get(..4) != Some(b"SLCC".as_slice()) {
        return Err(ClsError::NotAColorSet);
    }
    let mut at = 6;
    // The name part: the old code page's name, then the UTF-8 one.
    let length = take_u32(bytes, &mut at)?;
    let head = take(bytes, &mut at, length)?;
    let mut inner = 0;
    let old = take_u16(head, &mut inner)?;
    take(head, &mut inner, old)?;
    let name = take(head, &mut inner, 4)
        .and_then(|_| take_u16(head, &mut inner))
        .and_then(|length| take(head, &mut inner, length))
        .ok()
        .and_then(|name| std::str::from_utf8(name).ok())
        .map(|name| name.trim_end_matches('\0').trim().to_owned())
        .filter(|name| !name.is_empty());
    // How many cells.
    let length = take_u32(bytes, &mut at)?;
    let counted = take(bytes, &mut at, length)?;
    let count = take_u32(counted, &mut 0)?;
    // The cells.
    let length = take_u32(bytes, &mut at)?;
    let body = take(bytes, &mut at, length)?;
    let mut inner = 0;
    let mut cells = Vec::with_capacity(count.min(body.len() / 8));
    for _ in 0..count {
        let length = take_u32(body, &mut inner)?;
        let cell = take(body, &mut inner, length)?;
        let mut place = 0;
        let rgba = take(cell, &mut place, 4)?;
        if rgba[3] == 0 {
            cells.push(None);
            continue;
        }
        let named = take_u32(cell, &mut place).unwrap_or(0) == 1;
        let name = if named {
            take_u16(cell, &mut place)
                .and_then(|length| take(cell, &mut place, length))
                .map(|units| {
                    let units: Vec<u16> = units
                        .chunks_exact(2)
                        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                        .collect();
                    String::from_utf16_lossy(&units)
                })
                .unwrap_or_default()
        } else {
            String::new()
        };
        cells.push(Some(Swatch {
            rgb: [rgba[0], rgba[1], rgba[2]],
            name: name.trim_end_matches('\0').to_owned(),
        }));
    }
    Ok(Imported { name, cells })
}

/// A GIMP palette of `set`: its names, and a comment line for each empty
/// cell, so that Sue reads it back as it was and other programs read its
/// colors.
#[must_use]
pub(crate) fn export_gpl(set: &ColorSet) -> String {
    let name = set.name.replace(['\n', '\r'], " ");
    let mut text = format!("GIMP Palette\nName: {name}\nColumns: 8\n#\n");
    for cell in &set.cells {
        match cell {
            None => {
                text.push_str(GPL_EMPTY);
                text.push('\n');
            }
            Some(swatch) => {
                let [r, g, b] = swatch.rgb;
                let label = if swatch.name.is_empty() {
                    color::hex(swatch.rgb)
                } else {
                    swatch.name.replace(['\n', '\r'], " ")
                };
                text.push_str(&format!("{r:3} {g:3} {b:3}\t{label}\n"));
            }
        }
    }
    text
}

impl ColorSets {
    /// Reads the sets from `folder` once (keeping the built-ins when there
    /// is no file yet).
    pub fn load(&mut self, folder: Option<&Path>) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        if let Some(text) =
            folder.and_then(|folder| sue_platform::fs::read_to_string(folder.join(FILE)).ok())
        {
            let sets = parse(&text);
            if !sets.is_empty() {
                self.sets = sets;
                self.current = self.current.min(self.sets.len() - 1);
            }
        }
    }

    /// Writes the sets into `folder`.
    ///
    /// # Errors
    /// The file system's message.
    pub fn save(&self, folder: Option<&Path>) -> Result<(), String> {
        let Some(folder) = folder else {
            return Ok(());
        };
        sue_platform::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
        sue_platform::write_atomic(&folder.join(FILE), write(&self.sets).as_bytes())
            .map_err(|error| error.to_string())
    }

    /// The set showing.
    pub fn set(&self) -> Option<&ColorSet> {
        self.sets.get(self.current)
    }

    /// The cells of the set showing.
    pub fn cells(&self) -> &[Cell] {
        self.set().map_or(&[], |set| set.cells.as_slice())
    }

    /// The cells of the set showing, to change them.
    pub fn cells_mut(&mut self) -> Option<&mut Vec<Cell>> {
        self.sets.get_mut(self.current).map(|set| &mut set.cells)
    }

    /// The picked cell, when it is still in the set showing.
    pub fn picked(&self) -> Option<usize> {
        self.selected.filter(|cell| *cell < self.cells().len())
    }

    /// A name no set but the one at `except` has yet, from `base`.
    #[must_use]
    pub fn fresh_name_except(&self, base: &str, except: Option<usize>) -> String {
        let base = base.trim();
        let taken = |name: &str| {
            self.sets
                .iter()
                .enumerate()
                .any(|(index, set)| Some(index) != except && set.name == name)
        };
        if !taken(base) {
            return base.to_owned();
        }
        // "Default 2" taken numbers on from "Default", never "Default 2 2".
        let stem = base
            .rsplit_once(' ')
            .filter(|(stem, number)| {
                !stem.is_empty() && !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit())
            })
            .map_or(base, |(stem, _)| stem);
        (2..)
            .map(|n| format!("{stem} {n}"))
            .find(|name| !taken(name))
            .unwrap_or_else(|| base.to_owned())
    }

    /// A name no set has yet, from `base`.
    #[must_use]
    pub fn fresh_name(&self, base: &str) -> String {
        self.fresh_name_except(base, None)
    }

    /// Adds a set (under a name no other set has) and shows it. Returns
    /// where it went.
    pub fn add_set(&mut self, name: &str, cells: Vec<Cell>) -> usize {
        let name = self.fresh_name(name);
        self.sets.push(ColorSet { name, cells });
        self.current = self.sets.len() - 1;
        self.selected = None;
        self.current
    }

    /// A new set of empty cells named `name`, shown.
    pub fn new_set(&mut self, name: &str) -> usize {
        self.add_set(name, vec![None; NEW_CELLS])
    }

    /// A copy of a standard set, shown.
    pub fn add_standard(&mut self, standard: Standard) -> usize {
        let set = standard.set();
        self.add_set(&set.name, set.cells)
    }

    /// A copy of the set at `index`, right after it, shown.
    pub fn duplicate(&mut self, index: usize) -> Option<usize> {
        let set = self.sets.get(index)?.clone();
        let name = self.fresh_name(&set.name);
        self.sets.insert(
            index + 1,
            ColorSet {
                name,
                cells: set.cells,
            },
        );
        self.current = index + 1;
        self.selected = None;
        Some(self.current)
    }

    /// Gives the set at `index` the name `name` (trimmed, and numbered when
    /// another set has it). `false` when the name is empty or there is no
    /// such set.
    pub fn rename(&mut self, index: usize, name: &str) -> bool {
        if name.trim().is_empty() || index >= self.sets.len() {
            return false;
        }
        let name = self.fresh_name_except(name, Some(index));
        self.sets[index].name = name;
        true
    }

    /// Deletes the set at `index`; the last set is never deleted.
    pub fn delete(&mut self, index: usize) -> bool {
        if self.sets.len() <= 1 || index >= self.sets.len() {
            return false;
        }
        self.sets.remove(index);
        if self.current > index || self.current >= self.sets.len() {
            self.current = self.current.saturating_sub(1);
        }
        self.selected = None;
        true
    }

    /// Moves the set at `from` to `to`; the set showing stays the one
    /// showing.
    pub fn move_set(&mut self, from: usize, to: usize) -> bool {
        let len = self.sets.len();
        if from >= len || to >= len || from == to {
            return false;
        }
        let showing = self.current;
        let set = self.sets.remove(from);
        self.sets.insert(to, set);
        self.current = if showing == from {
            to
        } else if from < showing && to >= showing {
            showing - 1
        } else if from > showing && to <= showing {
            showing + 1
        } else {
            showing
        };
        true
    }

    /// Puts `rgb` into the first empty cell of the set showing, or after
    /// its last cell when none is empty, and picks it. Returns the cell.
    pub fn add_color(&mut self, rgb: [u8; 3]) -> Option<usize> {
        let cells = self.cells_mut()?;
        let at = if let Some(at) = cells.iter().position(Option::is_none) {
            cells[at] = Some(Swatch::of(rgb));
            at
        } else {
            cells.push(Some(Swatch::of(rgb)));
            cells.len() - 1
        };
        self.selected = Some(at);
        Some(at)
    }

    /// Gives the cell `at` the color `rgb`, keeping its name; an empty
    /// cell gets a swatch. Picks it.
    pub fn put(&mut self, at: usize, rgb: [u8; 3]) -> bool {
        let Some(cell) = self.cells_mut().and_then(|cells| cells.get_mut(at)) else {
            return false;
        };
        match cell {
            Some(swatch) => swatch.rgb = rgb,
            None => *cell = Some(Swatch::of(rgb)),
        }
        self.selected = Some(at);
        true
    }

    /// Empties the cell `at`: the cells after it stay where they are.
    pub fn delete_color(&mut self, at: usize) -> bool {
        let Some(cell) = self.cells_mut().and_then(|cells| cells.get_mut(at)) else {
            return false;
        };
        let had = cell.take().is_some();
        if self.selected == Some(at) {
            self.selected = None;
        }
        had
    }

    /// Names the swatch in the cell `at` (an empty name takes its name
    /// away). `false` for an empty cell.
    pub fn name_color(&mut self, at: usize, name: &str) -> bool {
        match self
            .cells_mut()
            .and_then(|cells| cells.get_mut(at))
            .and_then(Option::as_mut)
        {
            Some(swatch) => {
                name.trim().clone_into(&mut swatch.name);
                true
            }
            None => false,
        }
    }

    /// Moves the cell `from` to `to`: the cell there and those between
    /// shift over by one, as a card moved in a row of cards. Picks it.
    pub fn move_color(&mut self, from: usize, to: usize) -> bool {
        let Some(cells) = self.cells_mut() else {
            return false;
        };
        if from >= cells.len() || to >= cells.len() || from == to {
            return false;
        }
        let cell = cells.remove(from);
        cells.insert(to, cell);
        self.selected = Some(to);
        true
    }
}

/// Which numbers the Color Slider palette shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SliderMode {
    /// Hue, saturation and value.
    #[default]
    Hsv,
    /// Red, green and blue.
    Rgb,
    /// Cyan, magenta, yellow and black.
    Cmyk,
}

impl SliderMode {
    /// Every mode, in the order the palette's switch shows them.
    pub const ALL: [Self; 3] = [Self::Hsv, Self::Rgb, Self::Cmyk];

    /// The mode's name on the switch.
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Self::Hsv => tr!("color-mode-hsv"),
            Self::Rgb => tr!("color-mode-rgb"),
            Self::Cmyk => tr!("color-mode-cmyk"),
        }
    }
}

/// What the color palettes keep between frames.
#[derive(Clone, Debug, Default)]
pub(crate) struct ColorUi {
    /// The Color Slider palette's numbers.
    pub slider_mode: SliderMode,
    /// The Color Set palette's settings menu.
    pub set_menu: sue_wm::ui::MenuState,
    /// The cell being dragged to another place in the set, if any.
    pub dragging: Option<usize>,
}

/// Sue Paint's data folder, where the color sets live.
#[must_use]
pub fn folder(cx: &sue_wm::AppletContext<'_>) -> Option<PathBuf> {
    cx.catalog.folders.get("paint").cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(rgb: [u8; 3], name: &str) -> Cell {
        Some(Swatch {
            rgb,
            name: name.to_owned(),
        })
    }

    #[test]
    fn sets_round_trip_with_names_and_empty_cells() {
        let mut sets = builtin();
        sets.push(ColorSet {
            name: "Mine 100% done\twell".to_owned(),
            cells: vec![
                named([255, 0, 0], "Red  apple / 50%"),
                None,
                Some(Swatch::of([1, 2, 3])),
                None,
            ],
        });
        let text = write(&sets);
        assert!(text.starts_with(HEADER));
        assert_eq!(parse(&text), sets);
    }

    #[test]
    fn a_version_1_file_still_reads() {
        // As Sue 0.0.75 wrote it: names as they were, every cell a color.
        let old = "# sue paint color sets 1\nDefault\t#000000 #ffffff\nMy Colors 2\t\n\
                   Skin Tones\t#ffe0c4 nonsense #fad0b0\n";
        let sets = parse(old);
        assert_eq!(
            sets,
            vec![
                ColorSet::of("Default", &[[0, 0, 0], [255, 255, 255]]),
                ColorSet::of("My Colors 2", &[]),
                ColorSet::of("Skin Tones", &[[255, 224, 196], [250, 208, 176]]),
            ]
        );
        // A name with a percent sign in an old file is not decoded.
        assert_eq!(
            parse("# sue paint color sets 1\n50%20off\t#000000\n")[0].name,
            "50%20off"
        );
    }

    #[test]
    fn gimp_palettes_round_trip_and_hex_lists_import() {
        let set = ColorSet {
            name: "Mine".to_owned(),
            cells: vec![
                named([255, 0, 0], "Apple red"),
                None,
                Some(Swatch::of([1, 2, 3])),
            ],
        };
        let gpl = export_gpl(&set);
        assert!(gpl.starts_with("GIMP Palette\nName: Mine\n"));
        assert_eq!(
            import(&gpl),
            Imported {
                name: Some("Mine".to_owned()),
                cells: set.cells.clone(),
            }
        );
        // What Sue 0.0.75 wrote: the hex code as the name.
        let old = "GIMP Palette\nName: Old\nColumns: 8\n#\n255   0   0\t#ff0000\n";
        assert_eq!(import(old).cells, vec![Some(Swatch::of([255, 0, 0]))]);
        assert_eq!(
            import("#00ff00\n#123\nnot a color\n").cells,
            vec![
                Some(Swatch::of([0, 255, 0])),
                Some(Swatch::of([17, 34, 51]))
            ]
        );
    }

    /// A color set laid out as Clip Studio Paint lays its `.cls` files
    /// out, written here from that description (no file of theirs).
    fn cls(name: &str, cells: &[Option<([u8; 3], &str)>]) -> Vec<u8> {
        let part = |bytes: &[u8]| {
            let mut out = u32::try_from(bytes.len()).unwrap().to_le_bytes().to_vec();
            out.extend_from_slice(bytes);
            out
        };
        let short = |bytes: &[u8]| {
            let mut out = u16::try_from(bytes.len()).unwrap().to_le_bytes().to_vec();
            out.extend_from_slice(bytes);
            out
        };
        let mut head = short(b"legacy");
        head.extend_from_slice(&[0; 4]);
        head.extend(short(name.as_bytes()));
        let mut body = Vec::new();
        for cell in cells {
            let mut one = Vec::new();
            match cell {
                None => one.extend_from_slice(&[0; 8]),
                Some((rgb, name)) => {
                    one.extend_from_slice(rgb);
                    one.push(255);
                    if name.is_empty() {
                        one.extend_from_slice(&0_u32.to_le_bytes());
                    } else {
                        one.extend_from_slice(&1_u32.to_le_bytes());
                        let units: Vec<u8> =
                            name.encode_utf16().flat_map(u16::to_le_bytes).collect();
                        one.extend(short(&units));
                    }
                }
            }
            body.extend(part(&one));
        }
        let mut file = b"SLCC\x00\x01".to_vec();
        file.extend(part(&head));
        file.extend(part(&u32::try_from(cells.len()).unwrap().to_le_bytes()));
        file.extend(part(&body));
        file
    }

    #[test]
    fn clip_studio_color_sets_import() {
        let bytes = cls(
            "Warm colors",
            &[Some(([0, 0, 0], "Ink")), None, Some(([255, 50, 100], ""))],
        );
        assert_eq!(
            import_cls(&bytes),
            Ok(Imported {
                name: Some("Warm colors".to_owned()),
                cells: vec![
                    named([0, 0, 0], "Ink"),
                    None,
                    Some(Swatch::of([255, 50, 100]))
                ],
            })
        );
        assert_eq!(import_cls(b"GIMP Palette"), Err(ClsError::NotAColorSet));
        // Cut anywhere, it is refused, never read past its end.
        for end in 4..bytes.len() {
            assert_eq!(
                import_cls(&bytes[..end]),
                Err(ClsError::Cut),
                "cut at {end}"
            );
        }
    }

    #[test]
    fn sets_are_added_named_renamed_duplicated_moved_and_deleted() {
        let mut sets = ColorSets::default();
        let count = sets.sets.len();
        let mine = sets.new_set("My Colors");
        assert_eq!(sets.current, mine);
        assert_eq!(sets.cells().len(), NEW_CELLS);
        assert!(sets.cells().iter().all(Option::is_none));
        let second = sets.new_set("My Colors");
        assert_eq!(sets.sets[second].name, "My Colors 2");
        // Renaming to a name another set has numbers it; an empty name is
        // refused; a set keeps its own name.
        assert!(sets.rename(mine, "Skin"));
        assert!(sets.rename(mine, "Default"));
        assert_eq!(sets.sets[mine].name, "Default 2");
        assert!(sets.rename(mine, "Default 2"));
        assert_eq!(sets.sets[mine].name, "Default 2");
        assert!(!sets.rename(mine, "  "));
        // A copy goes right after it.
        let copy = sets.duplicate(mine).unwrap();
        assert_eq!(copy, mine + 1);
        assert_eq!(sets.sets[copy].name, "Default 3");
        // Moving keeps the set showing.
        sets.current = 0;
        assert!(sets.move_set(0, 2));
        assert_eq!(sets.current, 2);
        assert!(sets.move_set(3, 0));
        assert_eq!(sets.current, 3);
        // Standard sets come back as copies.
        let grays = sets.add_standard(Standard::Grays);
        assert_eq!(sets.sets[grays].name, "Grays 2");
        assert_eq!(sets.cells().len(), 17);
        // Deleting never leaves no set.
        let mut one = ColorSets {
            sets: vec![ColorSet::of("Only", &[])],
            ..ColorSets::default()
        };
        assert!(!one.delete(0));
        assert!(sets.delete(grays));
        assert_eq!(sets.sets.len(), count + 3);
    }

    #[test]
    fn colors_fill_empty_cells_keep_their_places_and_take_names() {
        let mut sets = ColorSets::default();
        sets.new_set("Mine");
        assert_eq!(sets.add_color([1, 1, 1]), Some(0));
        assert_eq!(sets.add_color([2, 2, 2]), Some(1));
        assert!(sets.put(5, [5, 5, 5]));
        // Deleting leaves the cell empty; the next color fills it.
        assert!(sets.delete_color(0));
        assert_eq!(sets.cells()[0], None);
        assert_eq!(sets.cells()[1], Some(Swatch::of([2, 2, 2])));
        assert_eq!(sets.add_color([3, 3, 3]), Some(0));
        // A name stays when the color is replaced.
        assert!(sets.name_color(5, "  Five "));
        assert!(sets.put(5, [6, 6, 6]));
        assert_eq!(sets.cells()[5], named([6, 6, 6], "Five"));
        assert!(!sets.name_color(7, "Nothing here"));
        // A full set grows.
        let full = sets.add_set("Full", vec![Some(Swatch::of([0, 0, 0]))]);
        assert_eq!(sets.current, full);
        assert_eq!(sets.add_color([9, 9, 9]), Some(1));
        // Moving a cell shifts the ones between.
        sets.add_color([8, 8, 8]);
        assert!(sets.move_color(2, 0));
        assert_eq!(
            sets.cells()
                .iter()
                .flatten()
                .map(|swatch| swatch.rgb)
                .collect::<Vec<_>>(),
            vec![[8, 8, 8], [0, 0, 0], [9, 9, 9]],
            "moved to the front"
        );
        assert_eq!(sets.selected, Some(0));
    }
}
