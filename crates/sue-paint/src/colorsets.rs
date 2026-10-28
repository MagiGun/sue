//! Color sets: the swatches of the Color Set palette. Several sets ship
//! (Default, Grays, Skin Tones, Pastels), people make their own, and every
//! set can gain, replace and lose colors. They are kept in Sue's data
//! folder, read GIMP palettes (`.gpl`) and lists of hex colors, and write
//! GIMP palettes.
//!
//! The file is plain text under a versioned header, a set a line:
//!
//! ```text
//! # sue paint color sets 1
//! Default <tab> #000000 #ffffff ...
//! ```

use std::path::{Path, PathBuf};

use crate::color;

/// The header of the color set file.
pub const HEADER: &str = "# sue paint color sets 1";

/// The file in Sue Paint's data folder.
pub const FILE: &str = "color_sets.txt";

/// A named list of colors.
pub(crate) type ColorSet = (String, Vec<[u8; 3]>);

/// Every set, which one shows, and the swatch picked in it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ColorSets {
    /// The sets.
    pub sets: Vec<ColorSet>,
    /// The set showing.
    pub current: usize,
    /// The swatch Replace and Delete act on.
    pub selected: Option<usize>,
    /// Side of a swatch, in points.
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

/// The sets Sue Paint ships with.
#[must_use]
pub fn builtin() -> Vec<ColorSet> {
    let grays = (0..=16).map(|step| {
        let value = (step * 255 / 16) as u8;
        [value, value, value]
    });
    let skin = [
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
    ];
    let pastels = [
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
    ];
    vec![
        ("Default".to_owned(), color::DEFAULT_SET.to_vec()),
        ("Grays".to_owned(), grays.collect()),
        ("Skin Tones".to_owned(), skin.to_vec()),
        ("Pastels".to_owned(), pastels.to_vec()),
    ]
}

/// Reads the color set file. Lines it does not understand are skipped.
#[must_use]
pub fn parse(text: &str) -> Vec<ColorSet> {
    text.lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| {
            let (name, colors) = line.split_once('\t')?;
            let colors: Vec<[u8; 3]> = colors
                .split_whitespace()
                .filter_map(color::parse_hex)
                .collect();
            Some((name.trim().to_owned(), colors))
        })
        .filter(|(name, _)| !name.is_empty())
        .collect()
}

/// The color set file's text.
#[must_use]
pub fn write(sets: &[ColorSet]) -> String {
    let mut text = format!("{HEADER}\n");
    for (name, colors) in sets {
        let hexes: Vec<String> = colors.iter().map(|rgb| color::hex(*rgb)).collect();
        text.push_str(&format!(
            "{}\t{}\n",
            name.replace(['\t', '\n'], " "),
            hexes.join(" ")
        ));
    }
    text
}

/// Reads a GIMP palette (`R G B name` lines) or a list of hex colors.
#[must_use]
pub fn import(text: &str) -> Vec<[u8; 3]> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') && line.len() != 7 && line.len() != 4 {
                return None;
            }
            if let Some(rgb) = hex_any(line.split_whitespace().next()?) {
                return Some(rgb);
            }
            let mut numbers = line.split_whitespace().map(str::parse::<u8>);
            match (numbers.next(), numbers.next(), numbers.next()) {
                (Some(Ok(r)), Some(Ok(g)), Some(Ok(b))) => Some([r, g, b]),
                _ => None,
            }
        })
        .collect()
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

/// A GIMP palette of `colors` named `name`.
#[must_use]
pub fn export_gpl(name: &str, colors: &[[u8; 3]]) -> String {
    let mut text = format!("GIMP Palette\nName: {name}\nColumns: 8\n#\n");
    for [r, g, b] in colors {
        text.push_str(&format!(
            "{r:3} {g:3} {b:3}\t{}\n",
            color::hex([*r, *g, *b])
        ));
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
            folder.and_then(|folder| std::fs::read_to_string(folder.join(FILE)).ok())
        {
            let sets = parse(&text);
            if !sets.is_empty() {
                self.sets = sets;
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
        std::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
        sue_platform::write_atomic(&folder.join(FILE), write(&self.sets).as_bytes())
            .map_err(|error| error.to_string())
    }

    /// The set showing.
    pub fn colors(&self) -> &[[u8; 3]] {
        self.sets
            .get(self.current)
            .map_or(&[], |(_, colors)| colors.as_slice())
    }

    /// The set showing, to change it.
    pub fn colors_mut(&mut self) -> Option<&mut Vec<[u8; 3]>> {
        self.sets.get_mut(self.current).map(|(_, colors)| colors)
    }

    /// A name no set has yet, from `base`.
    #[must_use]
    pub fn fresh_name(&self, base: &str) -> String {
        let taken = |name: &str| self.sets.iter().any(|(found, _)| found == name);
        if !taken(base) {
            return base.to_owned();
        }
        (2..)
            .map(|n| format!("{base} {n}"))
            .find(|name| !taken(name))
            .unwrap_or_else(|| base.to_owned())
    }

    /// Adds a set and shows it.
    pub fn add_set(&mut self, name: &str, colors: Vec<[u8; 3]>) {
        let name = self.fresh_name(name);
        self.sets.push((name, colors));
        self.current = self.sets.len() - 1;
        self.selected = None;
    }
}

/// Sue Paint's data folder, where the color sets live.
#[must_use]
pub fn folder(cx: &sue_wm::AppletContext<'_>) -> Option<PathBuf> {
    cx.catalog.folders.get("paint").cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sets_round_trip_and_palettes_import() {
        let sets = builtin();
        assert_eq!(parse(&write(&sets)), sets);
        let gpl = export_gpl("Mine", &[[255, 0, 0], [1, 2, 3]]);
        assert_eq!(import(&gpl), [[255, 0, 0], [1, 2, 3]]);
        assert_eq!(
            import("#00ff00\n#123\nnot a color\n"),
            [[0, 255, 0], [17, 34, 51]]
        );
        let mut chosen = ColorSets::default();
        chosen.add_set("Default", vec![[9, 9, 9]]);
        assert_eq!(chosen.sets.last().unwrap().0, "Default 2");
        assert_eq!(chosen.colors(), [[9, 9, 9]]);
    }
}
