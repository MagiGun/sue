//! Compares two folders of snapshots, scene by scene: how many pixels
//! differ and where. A restyle shows up in the scenes it was meant to
//! change and in no other; a move shows up nowhere.

use std::collections::BTreeSet;
use std::fmt;
use std::path::Path;

use sue_io::decode_png;

/// How one scene's picture in the second folder stands against the first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Every pixel is the same.
    Same,
    /// Some pixels differ.
    Differs {
        /// How many pixels differ.
        pixels: u64,
        /// How many pixels the picture has.
        of: u64,
        /// The smallest box holding every differing pixel: left, top,
        /// right and bottom, the last two exclusive.
        bounds: [u32; 4],
    },
    /// The two pictures are not the same size.
    Resized {
        /// Width and height in the first folder.
        before: [u32; 2],
        /// Width and height in the second folder.
        after: [u32; 2],
    },
    /// Only the first folder has the scene.
    OnlyBefore,
    /// Only the second folder has the scene.
    OnlyAfter,
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Same => write!(f, "0"),
            Self::Differs {
                pixels,
                of,
                bounds: [left, top, right, bottom],
            } => write!(
                f,
                "{pixels} of {of} pixels differ, inside x {left} to {right}, y {top} to {bottom}"
            ),
            Self::Resized { before, after } => write!(
                f,
                "size changed from {} x {} to {} x {}",
                before[0], before[1], after[0], after[1]
            ),
            Self::OnlyBefore => write!(f, "only in the first folder"),
            Self::OnlyAfter => write!(f, "only in the second folder"),
        }
    }
}

/// One scene of a comparison.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scene {
    /// The scene's name: its file name without `.png`.
    pub name: String,
    /// How its two pictures compare.
    pub verdict: Verdict,
    /// Whether the scene was said to change (see [`expected`]).
    pub expected: bool,
}

impl Scene {
    /// Whether the scene differs although nobody said it would.
    #[must_use]
    pub fn unexpected(&self) -> bool {
        self.verdict != Verdict::Same && !self.expected
    }
}

/// Compares two `Rgba8` pictures of `width` pixels a row, pixel by pixel.
/// Both hold the same number of whole pixels.
#[must_use]
pub fn compare_pixels(width: u32, before: &[u8], after: &[u8]) -> Verdict {
    let width = width.max(1) as usize;
    let mut pixels = 0_u64;
    let mut bounds: Option<[usize; 4]> = None;
    for (index, (old, new)) in before
        .as_chunks::<4>()
        .0
        .iter()
        .zip(after.as_chunks::<4>().0)
        .enumerate()
    {
        if old == new {
            continue;
        }
        pixels += 1;
        let (x, y) = (index % width, index / width);
        bounds = Some(match bounds {
            None => [x, y, x + 1, y + 1],
            Some([left, top, right, bottom]) => {
                [left.min(x), top.min(y), right.max(x + 1), bottom.max(y + 1)]
            }
        });
    }
    match bounds {
        None => Verdict::Same,
        Some(bounds) => Verdict::Differs {
            pixels,
            of: (before.len() / 4) as u64,
            bounds: bounds.map(|edge| u32::try_from(edge).unwrap_or(u32::MAX)),
        },
    }
}

/// Compares two PNG files.
fn compare_files(before: &Path, after: &Path) -> Result<Verdict, String> {
    let read =
        |path: &Path| std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()));
    let (old, new) = (read(before)?, read(after)?);
    if old == new {
        return Ok(Verdict::Same);
    }
    let decode = |bytes: &[u8], path: &Path| {
        decode_png(bytes)
            .map(|picture| picture.tile)
            .map_err(|error| format!("{}: {error}", path.display()))
    };
    let (old, new) = (decode(&old, before)?, decode(&new, after)?);
    let (old_size, new_size) = ([old.width(), old.height()], [new.width(), new.height()]);
    if old_size != new_size {
        return Ok(Verdict::Resized {
            before: old_size,
            after: new_size,
        });
    }
    match (old.bytes(), new.bytes()) {
        (Some(old_bytes), Some(new_bytes)) => Ok(compare_pixels(old_size[0], old_bytes, new_bytes)),
        _ => Err(format!("{}: the pixels are not bytes", after.display())),
    }
}

/// The scenes in `dir`: the names of its PNG files, without `.png`.
fn scenes_in(dir: &Path) -> Result<BTreeSet<String>, String> {
    let entries = std::fs::read_dir(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    Ok(entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
        })
        .filter_map(|path| Some(path.file_stem()?.to_str()?.to_owned()))
        .collect())
}

/// Whether `name` is one of the scenes said to change. A pattern is a
/// scene's name, or the start of one followed by `*`.
#[must_use]
pub fn expected(name: &str, changed: &[String]) -> bool {
    changed
        .iter()
        .any(|pattern| match pattern.strip_suffix('*') {
            Some(start) => name.starts_with(start),
            None => name == pattern,
        })
}

/// Compares every scene of the folders `before` and `after`, in name
/// order. `changed` lists the scenes that were meant to change; every
/// other scene was meant to stay as it was.
///
/// # Errors
/// A folder or a picture that cannot be read.
pub fn compare_folders(
    before: &Path,
    after: &Path,
    changed: &[String],
) -> Result<Vec<Scene>, String> {
    let (old, new) = (scenes_in(before)?, scenes_in(after)?);
    if old.is_empty() && new.is_empty() {
        return Err("neither folder holds a PNG picture".to_owned());
    }
    old.union(&new)
        .map(|name| {
            let file = format!("{name}.png");
            let verdict = match (old.contains(name), new.contains(name)) {
                (true, true) => compare_files(&before.join(&file), &after.join(&file))?,
                (true, false) => Verdict::OnlyBefore,
                _ => Verdict::OnlyAfter,
            };
            Ok(Scene {
                name: name.clone(),
                verdict,
                expected: expected(name, changed),
            })
        })
        .collect()
}

/// The comparison as text: one line per scene, then the totals.
#[must_use]
pub fn report(scenes: &[Scene]) -> String {
    let width = scenes
        .iter()
        .map(|scene| scene.name.len())
        .max()
        .unwrap_or(0);
    let mut text = String::new();
    for scene in scenes {
        let note = match (&scene.verdict, scene.expected) {
            (Verdict::Same, true) => " (said to change, did not)",
            (Verdict::Same, false) => "",
            (_, true) => " (said to change)",
            (_, false) => " (UNEXPECTED)",
        };
        text.push_str(&format!(
            "{:<width$}  {}{note}\n",
            scene.name, scene.verdict
        ));
    }
    let differing = scenes
        .iter()
        .filter(|scene| scene.verdict != Verdict::Same)
        .count();
    let unexpected = scenes.iter().filter(|scene| scene.unexpected()).count();
    text.push_str(&format!(
        "{} scenes compared: {} the same, {differing} differ, {unexpected} of them unexpected.\n",
        scenes.len(),
        scenes.len() - differing
    ));
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_io::encode_png_rgba;

    /// A `width` by `height` picture, gray all over.
    fn gray(width: u32, height: u32) -> Vec<u8> {
        [90, 90, 90, 255].repeat((width * height) as usize)
    }

    fn set(pixels: &mut [u8], width: u32, x: u32, y: u32) {
        let at = ((y * width + x) * 4) as usize;
        pixels[at] = 200;
    }

    /// A fresh, empty folder for one test.
    fn folder(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "sue-snapshot-compare-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(dir: &Path, name: &str, width: u32, height: u32, pixels: &[u8]) {
        let png = encode_png_rgba(width, height, pixels).unwrap();
        std::fs::write(dir.join(format!("{name}.png")), png).unwrap();
    }

    #[test]
    fn equal_pictures_are_the_same() {
        let picture = gray(7, 5);
        assert_eq!(compare_pixels(7, &picture, &picture), Verdict::Same);
    }

    #[test]
    fn differing_pixels_are_counted_and_boxed() {
        let before = gray(10, 8);
        let mut after = before.clone();
        set(&mut after, 10, 2, 1);
        set(&mut after, 10, 6, 4);
        set(&mut after, 10, 3, 6);
        assert_eq!(
            compare_pixels(10, &before, &after),
            Verdict::Differs {
                pixels: 3,
                of: 80,
                bounds: [2, 1, 7, 7],
            }
        );
    }

    #[test]
    fn a_change_of_alpha_alone_counts() {
        let before = gray(4, 4);
        let mut after = before.clone();
        after[4 * 5 + 3] = 254;
        assert_eq!(
            compare_pixels(4, &before, &after),
            Verdict::Differs {
                pixels: 1,
                of: 16,
                bounds: [1, 1, 2, 2],
            }
        );
    }

    #[test]
    fn patterns_name_a_scene_or_the_start_of_several() {
        let changed = ["04-settings-appearance".to_owned(), "20-phone*".to_owned()];
        assert!(expected("04-settings-appearance", &changed));
        assert!(!expected("04b-color-wheel", &changed));
        assert!(expected("20-phone-hub", &changed));
        assert!(!expected("22-tablet-hub", &changed));
        assert!(!expected("00-hub", &[]));
    }

    #[test]
    fn folders_are_compared_scene_by_scene() {
        let (before, after) = (folder("before"), folder("after"));
        let same = gray(6, 4);
        let mut moved = same.clone();
        set(&mut moved, 6, 5, 3);
        write(&before, "a-same", 6, 4, &same);
        write(&after, "a-same", 6, 4, &same);
        write(&before, "b-differs", 6, 4, &same);
        write(&after, "b-differs", 6, 4, &moved);
        write(&before, "c-resized", 6, 4, &same);
        write(&after, "c-resized", 4, 6, &same);
        write(&before, "d-gone", 6, 4, &same);
        write(&after, "e-new", 6, 4, &same);
        // Not a picture: left alone.
        std::fs::write(after.join("notes.txt"), "x").unwrap();

        let scenes = compare_folders(&before, &after, &["b-*".to_owned()]).unwrap();
        let verdicts: Vec<(&str, &Verdict, bool)> = scenes
            .iter()
            .map(|scene| (scene.name.as_str(), &scene.verdict, scene.unexpected()))
            .collect();
        assert_eq!(
            verdicts,
            [
                ("a-same", &Verdict::Same, false),
                (
                    "b-differs",
                    &Verdict::Differs {
                        pixels: 1,
                        of: 24,
                        bounds: [5, 3, 6, 4],
                    },
                    false
                ),
                (
                    "c-resized",
                    &Verdict::Resized {
                        before: [6, 4],
                        after: [4, 6],
                    },
                    true
                ),
                ("d-gone", &Verdict::OnlyBefore, true),
                ("e-new", &Verdict::OnlyAfter, true),
            ]
        );
        let text = report(&scenes);
        assert!(text.contains("a-same     0\n"), "{text}");
        assert!(
            text.contains(
                "b-differs  1 of 24 pixels differ, inside x 5 to 6, y 3 to 4 (said to change)"
            ),
            "{text}"
        );
        assert!(text.contains("d-gone     only in the first folder (UNEXPECTED)"));
        assert!(
            text.ends_with("5 scenes compared: 1 the same, 4 differ, 3 of them unexpected.\n"),
            "{text}"
        );
        for dir in [before, after] {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    #[test]
    fn two_empty_folders_are_an_error() {
        let (before, after) = (folder("empty-a"), folder("empty-b"));
        assert!(compare_folders(&before, &after, &[]).is_err());
        for dir in [before, after] {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}
