//! Rice format migrations: bring an older `.suerice` file up to the shipped
//! format while keeping the values the user changed.
//!
//! Each released default is frozen in `rice/history/`. A value that still
//! equals the old default was never customized, so it takes the new default;
//! a value the user changed is carried over if the new format still has the
//! key and the value passes its new type and range checks.

use crate::{DEFAULT_RICE, RiceError, RiceSchema, RiceValue};

/// The format of [`DEFAULT_RICE`], declared in its header as
/// `# sue-rice format 5`.
pub const RICE_FORMAT: u32 = 5;

const MARKER: &str = "# sue-rice format ";

/// Frozen defaults of every earlier format.
const HISTORY: &[(u32, &str)] = &[
    (1, include_str!("../rice/history/v1.suerice")),
    (2, include_str!("../rice/history/v2.suerice")),
    (3, include_str!("../rice/history/v3.suerice")),
    (4, include_str!("../rice/history/v4.suerice")),
];

/// Keys that moved, as (old path, new path, old values that mean "no
/// value" and do not carry over). Sue's boot mode became the Hub's pin: a
/// studio chosen on purpose stays pinned, the Hub and the canvas mean none.
const RENAMED: &[(&str, &str, &[&str])] = &[(
    "launchpad.boot_mode",
    "launchpad.pinned",
    &["hub", "canvas"],
)];

/// The format a `.suerice` text declares in its header; files written before
/// the marker existed are format 1.
#[must_use]
pub fn format_of(text: &str) -> u32 {
    text.lines()
        .take_while(|line| line.trim_start().starts_with('#') || line.trim().is_empty())
        .find_map(|line| line.trim().strip_prefix(MARKER)?.trim().parse().ok())
        .unwrap_or(1)
}

/// The result of migrating a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Migration {
    /// The format the file had.
    pub from: u32,
    /// The migrated file: the shipped default with the carried-over values.
    pub text: String,
    /// Paths whose customized values were carried over.
    pub kept: Vec<String>,
    /// Paths whose customized values could not be carried over (the key is
    /// gone, or the value no longer fits).
    pub dropped: Vec<String>,
    /// Colors reset because they were not gray: Sue's palette is pure
    /// monochrome, so a tinted color from an older release is replaced.
    pub purged: Vec<String>,
}

/// `true` for a color that is not a gray.
fn chromatic(value: &RiceValue) -> bool {
    matches!(value, RiceValue::Color(rgba) if !(rgba.r == rgba.g && rgba.g == rgba.b))
}

/// Migrates `text` to [`RICE_FORMAT`]. Colors from before format 3 that are
/// not gray are reset (the old tinted defaults); a color picked since is
/// the user's own and stays. Returns `None` when the file is current.
///
/// # Errors
/// If `text` does not parse; the caller then falls back to the default.
pub fn migrate(text: &str) -> Result<Option<Migration>, RiceError> {
    let from = format_of(text);
    let user = RiceSchema::parse(text)?;
    if from >= RICE_FORMAT {
        return Ok(None);
    }
    let tinted_era = from < 3;
    let old_defaults = HISTORY
        .iter()
        .find(|(format, _)| *format == from)
        .map(|(_, source)| RiceSchema::parse(source))
        .transpose()?;
    let mut migrated = RiceSchema::parse(DEFAULT_RICE)?;
    let (mut kept, mut dropped, mut purged) = (Vec::new(), Vec::new(), Vec::new());
    for entry in user.entries() {
        let value = entry.parameter().value();
        if tinted_era && chromatic(&value) {
            purged.push(entry.path().to_owned());
            continue;
        }
        let untouched = old_defaults
            .as_ref()
            .is_some_and(|defaults| defaults.get(entry.path()).as_ref() == Some(&value));
        if untouched {
            continue;
        }
        let renamed = RENAMED.iter().find(|(old, _, _)| *old == entry.path());
        if let Some((old, _, empty)) = renamed
            && matches!(&value, RiceValue::Text(text) if empty.contains(&text.as_str()))
        {
            dropped.push((*old).to_owned());
            continue;
        }
        let path = renamed.map_or(entry.path(), |(_, new, _)| new);
        if migrated.get(path).as_ref() == Some(&value) {
            continue;
        }
        if migrated.parameter(path).is_some() && migrated.set(path, value).is_ok() {
            kept.push(path.to_owned());
        } else {
            dropped.push(entry.path().to_owned());
        }
    }
    Ok(Some(Migration {
        from,
        text: migrated.to_suerice(),
        kept,
        dropped,
        purged,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const V1: &str = include_str!("../rice/history/v1.suerice");
    const V2: &str = include_str!("../rice/history/v2.suerice");
    const V3: &str = include_str!("../rice/history/v3.suerice");
    const V4: &str = include_str!("../rice/history/v4.suerice");

    #[test]
    fn the_shipped_default_is_current() {
        assert_eq!(format_of(DEFAULT_RICE), RICE_FORMAT);
        assert_eq!(format_of(V1), 1);
        assert_eq!(format_of(V2), 2);
        assert_eq!(format_of(V3), 3);
        assert_eq!(format_of(V4), 4);
        assert_eq!(migrate(DEFAULT_RICE).unwrap(), None);
    }

    #[test]
    fn an_untouched_old_file_becomes_the_new_default() {
        let migration = migrate(V1).unwrap().unwrap();
        assert_eq!(migration.from, 1);
        assert_eq!(migration.text, DEFAULT_RICE.replace("\r\n", "\n"));
        assert!(migration.kept.is_empty());
        let old_colors = ["#14111a", "#c9a227", "#2a2333"];
        assert!(old_colors.iter().all(|hex| !migration.text.contains(hex)));
    }

    #[test]
    fn customized_values_survive_and_retired_keys_are_reported() {
        let custom = V1
            .replace("ui_size = 14 ", "ui_size = 17 ")
            .replace("blur_passes = 2", "blur_passes = 5");
        let migration = migrate(&custom).unwrap().unwrap();
        assert_eq!(migration.kept, ["typography.ui_size"]);
        assert_eq!(migration.dropped, ["shaders.blur_passes"]);
        let schema = RiceSchema::parse(&migration.text).unwrap();
        assert_eq!(schema.get_i64("typography.ui_size"), Some(17));
        assert_eq!(schema.get_i64("layout.status_bar_height"), Some(22));
        assert_eq!(format_of(&migration.text), RICE_FORMAT);
    }

    #[test]
    fn the_blue_accent_era_becomes_pure_monochrome() {
        let migration = migrate(V2).unwrap().unwrap();
        assert_eq!(migration.from, 2);
        assert_eq!(migration.text, DEFAULT_RICE.replace("\r\n", "\n"));
        assert!(!migration.text.contains("#3b82f6"));
        // A blue picked on the color wheel since then is the user's own.
        let tinted = DEFAULT_RICE.replace("\ntext = \"#ffffff\"", "\ntext = \"#3b82f6\"");
        assert_eq!(migrate(&tinted).unwrap(), None);
        let v3 = V3.replace("\ntext = \"#ffffff\"", "\ntext = \"#3b82f6\"");
        let migration = migrate(&v3).unwrap().unwrap();
        assert!(migration.purged.is_empty());
        assert_eq!(migration.kept, ["colors.text"]);
    }

    #[test]
    fn the_canvas_era_opens_on_the_hub_and_keeps_its_choices() {
        // Booting into the canvas was the old default, so it takes the new
        // one; the retired Local AI and sketch pad keys go away.
        let migration = migrate(V3).unwrap().unwrap();
        assert_eq!(migration.from, 3);
        assert_eq!(migration.text, DEFAULT_RICE.replace("\r\n", "\n"));
        let schema = RiceSchema::parse(&migration.text).unwrap();
        assert_eq!(schema.get_str("launchpad.pinned"), Some(""));
        assert!(schema.get("launchpad.boot_mode").is_none());
        assert!(schema.get("ai.backend").is_none());
        assert!(schema.get("paint.brush_size").is_none());
        // A studio chosen on purpose stays: it is pinned now.
        let chosen = V3.replace("boot_mode = \"canvas\"", "boot_mode = \"sue-paint\"");
        let migration = migrate(&chosen).unwrap().unwrap();
        assert_eq!(migration.kept, ["launchpad.pinned"]);
        let schema = RiceSchema::parse(&migration.text).unwrap();
        assert_eq!(schema.get_str("launchpad.pinned"), Some("sue-paint"));
    }

    #[test]
    fn the_side_panel_era_keeps_what_still_exists() {
        // The Hub era of format 4, with the canvas, drawers, File Manager,
        // Notes and Player: their keys go, the rest keeps its choices.
        let migration = migrate(V4).unwrap().unwrap();
        assert_eq!(migration.from, 4);
        assert_eq!(migration.text, DEFAULT_RICE.replace("\r\n", "\n"));
        let custom = V4
            .replace("rail_width = 36 ", "rail_width = 50 ")
            .replace("show_hidden = false", "show_hidden = true")
            .replace("ui_size = 13 ", "ui_size = 15 ")
            .replace("boot_mode = \"hub\"", "boot_mode = \"canvas\"");
        let migration = migrate(&custom).unwrap().unwrap();
        assert_eq!(migration.kept, ["typography.ui_size"]);
        assert_eq!(
            migration.dropped,
            [
                "launchpad.boot_mode",
                "layout.rail_width",
                "files.show_hidden"
            ]
        );
        let schema = RiceSchema::parse(&migration.text).unwrap();
        assert_eq!(schema.get_i64("typography.ui_size"), Some(15));
        assert_eq!(schema.get_str("launchpad.pinned"), Some(""));
        assert!(schema.get("canvas.zoom_min").is_none());
        assert!(schema.get("notes.font_size").is_none());
    }

    #[test]
    fn unreadable_files_are_errors() {
        assert!(migrate("[broken\n").is_err());
    }
}
