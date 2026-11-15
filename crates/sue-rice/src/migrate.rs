//! Rice format migrations: bring an older `.suerice` file up to the shipped
//! format while keeping the values the user changed.
//!
//! Each released default is frozen in `rice/history/`. A value that still
//! equals the old default was never customized, so it takes the new default;
//! a value the user changed is carried over if the new format still has the
//! key and the value passes its new type and range checks.

use crate::{DEFAULT_RICE, RiceError, RiceSchema, RiceValue};

/// The format of [`DEFAULT_RICE`], declared in its header as
/// `# sue-rice format 7`.
pub const RICE_FORMAT: u32 = 7;

const MARKER: &str = "# sue-rice format ";

/// Frozen defaults of every earlier format.
const HISTORY: &[(u32, &str)] = &[
    (1, include_str!("../rice/history/v1.suerice")),
    (2, include_str!("../rice/history/v2.suerice")),
    (3, include_str!("../rice/history/v3.suerice")),
    (4, include_str!("../rice/history/v4.suerice")),
    (5, include_str!("../rice/history/v5.suerice")),
    (6, include_str!("../rice/history/v6.suerice")),
];

/// The theme presets as they shipped from format 3 to format 6, by id.
/// Format 7 raised their secondary text to a readable contrast and parted
/// their hairlines from the hover color, so a file that wore one of them
/// takes the preset as it ships now: its old colors are no choice of the
/// person's, only the preset they picked.
const OLD_PRESETS: &[(&str, &str)] = &[
    (
        "void",
        include_str!("../rice/history/presets-v6/void.suerice"),
    ),
    (
        "graphite",
        include_str!("../rice/history/presets-v6/graphite.suerice"),
    ),
    (
        "paper",
        include_str!("../rice/history/presets-v6/paper.suerice"),
    ),
    (
        "gray",
        include_str!("../rice/history/presets-v6/gray.suerice"),
    ),
];

/// The formats that shipped [`OLD_PRESETS`].
const OLD_PRESET_FORMATS: std::ops::RangeInclusive<u32> = 3..=6;

/// How many colors of a preset a person may have changed after picking it
/// and still be said to wear it.
const PRESET_SLACK: usize = 3;

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
    /// Colors of a theme preset the file wore, moved to that preset as it
    /// ships now (the old preset's values are not the person's choice).
    pub restyled: Vec<String>,
}

/// The old preset `user` wears, if any: the one whose colors it holds,
/// all but at most [`PRESET_SLACK`] of them, the closest first. Gives the
/// preset's id and its old values.
fn worn_preset(user: &RiceSchema) -> Result<Option<(&'static str, RiceSchema)>, RiceError> {
    let mut best: Option<(usize, &'static str, RiceSchema)> = None;
    for (id, source) in OLD_PRESETS {
        let old = RiceSchema::parse(source)?;
        let total = old.entries().len();
        let same = old
            .entries()
            .iter()
            .filter(|entry| user.get(entry.path()) == Some(entry.parameter().value()))
            .count();
        if same + PRESET_SLACK >= total && best.as_ref().is_none_or(|(most, ..)| same > *most) {
            best = Some((same, id, old));
        }
    }
    Ok(best.map(|(_, id, old)| (id, old)))
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
    migrate_onto(text, DEFAULT_RICE)
}

/// Migrates `text` onto `current`, a default file of a newer format. The
/// shipped default is the only `current` outside the tests; they pass a
/// default with changes that have not shipped yet.
fn migrate_onto(text: &str, current: &str) -> Result<Option<Migration>, RiceError> {
    let from = format_of(text);
    let user = RiceSchema::parse(text)?;
    if from >= format_of(current) {
        return Ok(None);
    }
    let tinted_era = from < 3;
    let old_defaults = HISTORY
        .iter()
        .find(|(format, _)| *format == from)
        .map(|(_, source)| RiceSchema::parse(source))
        .transpose()?;
    let mut migrated = RiceSchema::parse(current)?;
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
    // A preset worn before format 7 is worn as it ships now: each color
    // still at the old preset's value takes the new one.
    let mut restyled = Vec::new();
    if OLD_PRESET_FORMATS.contains(&from)
        && let Some((id, old)) = worn_preset(&user)?
        && let Some(new) = crate::builtin_presets()
            .into_iter()
            .find(|preset| preset.id == id)
    {
        for (path, value) in &new.values {
            let was = old.get(path);
            // A color still at the old default follows the default (Void
            // was the default), as every untouched value does.
            let untouched = old_defaults
                .as_ref()
                .is_some_and(|defaults| defaults.get(path) == was);
            if was.is_some()
                && !untouched
                && user.get(path) == was
                && migrated.get(path).as_ref() != Some(value)
                && migrated.set(path, value.clone()).is_ok()
            {
                kept.retain(|kept| kept != path);
                restyled.push(path.clone());
            }
        }
    }
    Ok(Some(Migration {
        from,
        text: migrated.to_suerice(),
        kept,
        dropped,
        purged,
        restyled,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const V1: &str = include_str!("../rice/history/v1.suerice");
    const V2: &str = include_str!("../rice/history/v2.suerice");
    const V3: &str = include_str!("../rice/history/v3.suerice");
    const V4: &str = include_str!("../rice/history/v4.suerice");
    const V5: &str = include_str!("../rice/history/v5.suerice");
    const V6: &str = include_str!("../rice/history/v6.suerice");

    /// The frozen format 6 default as a format 7 file with `changes` made
    /// to it: what the shipped default may become. It starts from the
    /// frozen file, so it stays the same when the shipped default moves.
    fn next_default(changes: &[(&str, &str)]) -> String {
        let header = [("# sue-rice format 6", "# sue-rice format 7")];
        header
            .iter()
            .chain(changes)
            .fold(V6.to_owned(), |text, (old, new)| {
                assert_eq!(text.matches(old).count(), 1, "{old} is there once");
                text.replace(old, new)
            })
    }

    /// `text` without the table `[name]` and its keys.
    fn without_section(text: &str, name: &str) -> String {
        let header = format!("[{name}]");
        let mut inside = false;
        let mut out = String::new();
        for line in text.split_inclusive('\n') {
            if line.trim_start().starts_with('[') {
                inside = line.trim() == header;
            }
            if !inside {
                out.push_str(line);
            }
        }
        assert_ne!(out, text, "{name} is a section");
        out
    }

    /// `text` with `language` of the table `[section]` set to `tag`.
    fn with_language(text: &str, section: &str, tag: &str) -> String {
        let header = format!("[{section}]");
        let mut inside = false;
        let mut found = false;
        let mut out = String::new();
        for line in text.split_inclusive('\n') {
            if line.trim_start().starts_with('[') {
                inside = line.trim() == header;
            }
            if inside && line.starts_with("language = \"sue\"") {
                found = true;
                out.push_str(&line.replacen("\"sue\"", &format!("\"{tag}\""), 1));
            } else {
                out.push_str(line);
            }
        }
        assert!(found, "{section} has a language");
        out
    }

    #[test]
    fn the_shipped_default_is_current() {
        assert_eq!(format_of(DEFAULT_RICE), RICE_FORMAT);
        assert_eq!(format_of(V1), 1);
        assert_eq!(format_of(V2), 2);
        assert_eq!(format_of(V3), 3);
        assert_eq!(format_of(V4), 4);
        assert_eq!(format_of(V5), 5);
        assert_eq!(format_of(V6), 6);
        assert_eq!(migrate(DEFAULT_RICE).unwrap(), None);
    }

    #[test]
    fn every_earlier_format_has_its_frozen_default() {
        let formats: Vec<u32> = HISTORY.iter().map(|(format, _)| *format).collect();
        assert_eq!(formats, (1..RICE_FORMAT).collect::<Vec<_>>());
        for (format, source) in HISTORY {
            assert_eq!(format_of(source), *format);
            assert!(RiceSchema::parse(source).is_ok(), "format {format} reads");
        }
    }

    #[test]
    fn an_untouched_format_6_file_takes_the_new_defaults() {
        let migration = migrate(V6).unwrap().unwrap();
        assert_eq!(migration.from, 6);
        assert_eq!(migration.text, DEFAULT_RICE.replace("\r\n", "\n"));
        assert!(migration.kept.is_empty());
        assert!(migration.dropped.is_empty());
        assert!(migration.purged.is_empty());
        assert_eq!(migrate(&migration.text).unwrap(), None);
        // A default that changes later in format 7 reaches the file too.
        let next = next_default(&[
            ("small_size = 11 ", "small_size = 12 "),
            ("confirm_on_exit = false", "confirm_on_exit = true "),
        ]);
        let migration = migrate_onto(V6, &next).unwrap().unwrap();
        assert_eq!(migration.text, next.replace("\r\n", "\n"));
        assert!(migration.kept.is_empty());
        assert!(migration.dropped.is_empty());
        let schema = RiceSchema::parse(&migration.text).unwrap();
        assert_eq!(schema.get_i64("typography.small_size"), Some(12));
        assert_eq!(schema.get_bool("general.confirm_on_exit"), Some(true));
    }

    #[test]
    fn a_value_changed_in_format_6_is_kept() {
        let custom = V6
            .replace("ui_size = 13 ", "ui_size = 15 ")
            .replace("small_size = 11 ", "small_size = 10 ");
        assert_ne!(custom, V6);
        let migration = migrate(&custom).unwrap().unwrap();
        assert_eq!(
            migration.kept,
            ["typography.ui_size", "typography.small_size"]
        );
        assert!(migration.dropped.is_empty());
        let schema = RiceSchema::parse(&migration.text).unwrap();
        assert_eq!(schema.get_i64("typography.ui_size"), Some(15));
        assert_eq!(schema.get_i64("typography.small_size"), Some(10));
        assert_eq!(format_of(&migration.text), RICE_FORMAT);
        // Also where the default of that very key changes: the choice
        // wins, and the untouched key beside it follows the new default.
        let next = next_default(&[
            ("small_size = 11 ", "small_size = 12 "),
            ("ui_size = 13 ", "ui_size = 14 "),
        ]);
        let custom = V6.replace("small_size = 11 ", "small_size = 10 ");
        let migration = migrate_onto(&custom, &next).unwrap().unwrap();
        assert_eq!(migration.kept, ["typography.small_size"]);
        let schema = RiceSchema::parse(&migration.text).unwrap();
        assert_eq!(schema.get_i64("typography.small_size"), Some(10));
        assert_eq!(schema.get_i64("typography.ui_size"), Some(14));
        // A choice that happens to be the new default is nothing to carry.
        let custom = V6.replace("small_size = 11 ", "small_size = 12 ");
        let migration = migrate_onto(&custom, &next).unwrap().unwrap();
        assert!(migration.kept.is_empty());
        assert_eq!(migration.text, next.replace("\r\n", "\n"));
    }

    #[test]
    fn a_section_removed_after_format_6_is_reported_as_dropped() {
        let next = without_section(
            &without_section(&next_default(&[]), "convert"),
            "image_viewer",
        );
        // Nobody changed anything in them: the sections go without a word.
        let migration = migrate_onto(V6, &next).unwrap().unwrap();
        assert_eq!(migration.text, next.replace("\r\n", "\n"));
        assert!(migration.dropped.is_empty());
        let schema = RiceSchema::parse(&migration.text).unwrap();
        assert!(schema.get("convert.language").is_none());
        assert!(schema.get("image_viewer.language").is_none());
        assert!(schema.get("image_viewer_keys.viewer_next").is_some());
        // A value someone set in them cannot be carried and is reported.
        let custom = with_language(&with_language(V6, "convert", "de"), "image_viewer", "ja")
            .replace("ui_size = 13 ", "ui_size = 15 ");
        let schema = RiceSchema::parse(&custom).unwrap();
        assert_eq!(schema.get_str("convert.language"), Some("de"));
        assert_eq!(schema.get_str("image_viewer.language"), Some("ja"));
        let migration = migrate_onto(&custom, &next).unwrap().unwrap();
        assert_eq!(
            migration.dropped,
            ["image_viewer.language", "convert.language"]
        );
        assert_eq!(migration.kept, ["typography.ui_size"]);
        let schema = RiceSchema::parse(&migration.text).unwrap();
        assert!(schema.get("convert.language").is_none());
        assert!(schema.get("image_viewer.language").is_none());
        assert_eq!(schema.get_i64("typography.ui_size"), Some(15));
    }

    #[test]
    fn the_image_viewer_and_the_converter_left_the_shipped_settings() {
        // Sue no longer has the two tools: their language and key settings
        // are gone from the shipped file, and the io settings stay.
        let shipped = RiceSchema::parse(DEFAULT_RICE).unwrap();
        for gone in [
            "image_viewer.language",
            "convert.language",
            "image_viewer_keys.viewer_next",
            "image_viewer_keys.viewer_save_as",
        ] {
            assert!(shipped.get(gone).is_none(), "{gone}");
        }
        for kept in ["io.convert_dir", "io.jpeg_quality", "io.jxl_effort"] {
            assert!(shipped.get(kept).is_some(), "{kept}");
        }
        // A 0.0.75 file that never touched them migrates without a word.
        let migration = migrate(V6).unwrap().unwrap();
        assert!(migration.dropped.is_empty(), "{:?}", migration.dropped);
        // One that set a language or rebound a key has it reported.
        let custom = with_language(V6, "image_viewer", "ja")
            .replace("viewer_crop = \"C\" ", "viewer_crop = \"K\" ");
        let schema = RiceSchema::parse(&custom).unwrap();
        assert_eq!(schema.get_str("image_viewer.language"), Some("ja"));
        let migration = migrate(&custom).unwrap().unwrap();
        assert_eq!(
            migration.dropped,
            ["image_viewer.language", "image_viewer_keys.viewer_crop"]
        );
        let schema = RiceSchema::parse(&migration.text).unwrap();
        assert!(schema.get("image_viewer_keys.viewer_crop").is_none());
    }

    #[test]
    fn quit_sue_is_no_longer_asked_unless_someone_chose_it() {
        // Format 6 stops asking "Quit Sue?" (work not saved is still asked
        // about); a format 5 file that never changed it follows.
        let migration = migrate(V5).unwrap().unwrap();
        assert_eq!(migration.from, 5);
        assert_eq!(migration.text, DEFAULT_RICE.replace("\r\n", "\n"));
        let schema = RiceSchema::parse(&migration.text).unwrap();
        assert_eq!(schema.get_bool("general.confirm_on_exit"), Some(false));
        // Someone who had turned it off keeps it off, by the same rule.
        let off = V5.replace("confirm_on_exit = true ", "confirm_on_exit = false");
        let schema = RiceSchema::parse(&migrate(&off).unwrap().unwrap().text).unwrap();
        assert_eq!(schema.get_bool("general.confirm_on_exit"), Some(false));
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

    /// `text` with the colors of `[colors]` set to those of `preset`
    /// (`.suerice` text), each line's comment kept.
    fn wearing(text: &str, preset: &str) -> String {
        let preset = RiceSchema::parse(preset).unwrap();
        let mut inside = false;
        let mut out = String::new();
        for line in text.split_inclusive('\n') {
            if line.trim_start().starts_with('[') {
                inside = line.trim() == "[colors]";
            }
            let key = line.split('=').next().unwrap_or("").trim();
            match preset.get(&format!("colors.{key}")) {
                Some(RiceValue::Color(color)) if inside && line.contains('=') => {
                    let start = line.find('"').unwrap();
                    let end = start + 1 + line[start + 1..].find('"').unwrap();
                    out.push_str(&line[..=start]);
                    out.push_str(&color.to_string());
                    out.push_str(&line[end..]);
                }
                _ => out.push_str(line),
            }
        }
        out
    }

    #[test]
    fn a_preset_worn_before_format_7_is_worn_as_it_ships_now() {
        let presets = crate::builtin_presets();
        let graphite = presets.iter().find(|p| p.id == "graphite").unwrap();
        let old = OLD_PRESETS
            .iter()
            .find(|(id, _)| *id == "graphite")
            .unwrap()
            .1;
        let file = wearing(V6, old);
        assert_ne!(file, V6);
        let user = RiceSchema::parse(&file).unwrap();
        assert!(
            !graphite.is_active(&user),
            "the old Graphite is not the new one"
        );
        let migration = migrate(&file).unwrap().unwrap();
        let schema = RiceSchema::parse(&migration.text).unwrap();
        // The defect: the old muted text and hairline stayed as if the
        // person had picked them, and no preset read as worn.
        assert!(graphite.is_active(&schema), "{}", migration.text);
        assert_eq!(
            migration.restyled,
            ["colors.border", "colors.muted"],
            "only what format 7 changed in Graphite moves"
        );
        assert!(
            migration
                .kept
                .iter()
                .all(|path| !migration.restyled.contains(path)),
            "{:?}",
            migration.kept
        );
        assert!(migration.kept.iter().any(|path| path == "colors.void"));
        // Every preset of format 6: worn, it migrates to itself as it is
        // now.
        for (id, old) in OLD_PRESETS {
            let migration = migrate(&wearing(V6, old)).unwrap().unwrap();
            let schema = RiceSchema::parse(&migration.text).unwrap();
            let now = presets.iter().find(|p| p.id == *id).unwrap();
            assert!(now.is_active(&schema), "{id}");
        }
        // Also from format 4 and 5, which shipped the same presets.
        for older in [V4, V5] {
            let migration = migrate(&wearing(older, old)).unwrap().unwrap();
            let schema = RiceSchema::parse(&migration.text).unwrap();
            assert!(graphite.is_active(&schema));
        }
    }

    #[test]
    fn a_color_picked_on_top_of_an_old_preset_stays() {
        let old = OLD_PRESETS
            .iter()
            .find(|(id, _)| *id == "graphite")
            .unwrap()
            .1;
        // Graphite, with the muted text picked by hand.
        let file = wearing(V6, old).replace("muted = \"#8c8c8c\"", "muted = \"#b0b0b0\"");
        let schema = RiceSchema::parse(&file).unwrap();
        assert_eq!(schema.get("colors.muted").unwrap().to_string(), "#b0b0b0");
        let migration = migrate(&file).unwrap().unwrap();
        let schema = RiceSchema::parse(&migration.text).unwrap();
        assert_eq!(schema.get("colors.muted").unwrap().to_string(), "#b0b0b0");
        assert_eq!(migration.restyled, ["colors.border"]);
        assert!(migration.kept.iter().any(|path| path == "colors.muted"));
        // Colors of no preset at all are the person's own and stay.
        let own = V6
            .replace("muted = \"#808080\"", "muted = \"#777777\"")
            .replace("void = \"#0a0a0a\"", "void = \"#050505\"")
            .replace("panel = \"#111111\"", "panel = \"#151515\"")
            .replace("pill = \"#181818\"", "pill = \"#1b1b1b\"")
            .replace("border = \"#262626\"", "border = \"#222222\"");
        let migration = migrate(&own).unwrap().unwrap();
        assert!(migration.restyled.is_empty(), "{:?}", migration.restyled);
        let schema = RiceSchema::parse(&migration.text).unwrap();
        assert_eq!(schema.get("colors.muted").unwrap().to_string(), "#777777");
    }

    /// Sue Paint's Custom colors start as the Gray theme. Before format 7
    /// they started as the old Gray, with its hard to read secondary text;
    /// a file that never changed them takes Gray's as it ships now, and a
    /// color someone picked stays.
    #[test]
    fn sue_paints_custom_colors_leave_the_old_gray() {
        let gray = crate::builtin_presets()
            .into_iter()
            .find(|preset| preset.id == "gray")
            .unwrap();
        let now = RiceSchema::parse(DEFAULT_RICE).unwrap();
        for (path, value) in &gray.values {
            let own = path.replace("colors.", "sue_paint_colors.");
            assert_eq!(now.get(&own).as_ref(), Some(value), "{own} starts as Gray");
        }
        let old = RiceSchema::parse(V6).unwrap();
        assert_eq!(
            old.get("sue_paint_colors.muted").unwrap().to_string(),
            "#a4a4a4"
        );
        let migration = migrate(V6).unwrap().unwrap();
        let schema = RiceSchema::parse(&migration.text).unwrap();
        assert_eq!(
            schema.get("sue_paint_colors.muted").unwrap().to_string(),
            "#c4c4c4"
        );
        let picked = V6.replacen("muted = \"#a4a4a4\"", "muted = \"#b5b5b5\"", 1);
        assert_ne!(picked, V6);
        let migration = migrate(&picked).unwrap().unwrap();
        let schema = RiceSchema::parse(&migration.text).unwrap();
        assert_eq!(
            schema.get("sue_paint_colors.muted").unwrap().to_string(),
            "#b5b5b5"
        );
    }

    #[test]
    fn unreadable_files_are_errors() {
        assert!(migrate("[broken\n").is_err());
    }
}
