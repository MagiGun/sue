//! The tests of icon packs: reading, drawing, the order of fallback,
//! installing and removing, showing with no restart, and the round trip
//! of Sue's own set.

use std::path::{Path, PathBuf};

use egui::{Color32, Context, Rect, TextureId, pos2, vec2};
use sue_core::ReversibleTransaction as _;

use super::super::headless;
use super::super::{Icon, IconStyle, paint, weight};
use super::*;

/// A folder of its own for one test, emptied first.
fn scratch(name: &str) -> PathBuf {
    let base = std::env::var_os("SUE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.has_root())
        .unwrap_or_else(std::env::temp_dir);
    let folder = base.join("icon-pack-tests").join(name);
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).unwrap();
    folder
}

fn write(folder: &Path, files: &[(&str, &str)]) {
    std::fs::create_dir_all(folder).unwrap();
    for (name, text) in files {
        std::fs::write(folder.join(name), text).unwrap();
    }
}

/// A test pack, drawn for these tests, that uses every part of SVG the
/// reader knows: each shape element, path data with every command in
/// both forms, groups, inherited and `style` paint, transforms, a
/// `viewBox` of another size, a mask and the even and odd rule.
pub(super) const TEST_PACK: &[(&str, &str)] = &[
    (
        "icon-pack.txt",
        "# made for the tests\nformat = 1\nname = Test Blocks\nauthor = Sue tests\n\
         licence = CC0-1.0\nversion = 0.1.0\ndescription = Every SVG feature Sue reads.\n",
    ),
    // rect with round corners, circle, line; a viewBox twice the canvas.
    (
        "search.svg",
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 48 48" fill="none" stroke="#000" stroke-width="4">
  <circle cx="20" cy="20" r="12"/><line x1="29" y1="29" x2="41" y2="41"/></svg>"##,
    ),
    // path data filled with a hole by the even and odd rule: a star in
    // a star, both running the same way round.
    (
        "star.svg",
        r##"<svg viewBox="0 0 24 24"><path fill-rule="evenodd" d="M12 2 L14.6 8.6 L21.5 9 L16.2 13.6 L17.9 20.5 L12 16.8 L6.1 20.5 L7.8 13.6 L2.5 9 L9.4 8.6 Z M12 9 L13.2 11.6 L15.4 12 L13.7 13.6 L14.2 15.8 L12 14.6 L9.8 15.8 L10.3 13.6 L8.6 12 L10.8 11.6 Z"/>
  <path fill="#fff" d="M12 7 L13.5 11 L12 14 L10.5 11 Z" style="display:none"/></svg>"##,
    ),
    (
        "star.solid.svg",
        r##"<svg viewBox="0 0 24 24"><polygon points="12,2 14.6,8.6 21.5,9 16.2,13.6 17.9,20.5 12,16.8 6.1,20.5 7.8,13.6 2.5,9 9.4,8.6"/></svg>"##,
    ),
    // C and S, absolute and relative.
    (
        "heart.svg",
        r##"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M12 20 C4 14 2 10 4 6.5 S10 3 12 7 c1 -3 6 -4.5 8 -0.5 s-1 7.5 -8 14 Z"/></svg>"##,
    ),
    (
        "heart.solid.svg",
        r##"<svg viewBox="0 0 24 24"><path d="M12 20 C4 14 2 10 4 6.5 S10 3 12 7 c1 -3 6 -4.5 8 -0.5 s-1 7.5 -8 14 Z"/></svg>"##,
    ),
    // ellipse, and a mask that cuts the pupil's ring free.
    (
        "eye.svg",
        r##"<svg viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="1.5">
  <mask id="cut"><rect width="24" height="24" fill="white" stroke="none"/><circle cx="12" cy="12" r="4.5" fill="black" stroke="none"/></mask>
  <ellipse cx="12" cy="12" rx="9.5" ry="6" mask="url(#cut)"/><circle cx="12" cy="12" r="2.5" fill="#000" stroke="none"/></svg>"##,
    ),
    (
        "eye.solid.svg",
        r##"<svg viewBox="0 0 24 24"><defs><mask id="pupil"><rect width="24" height="24" fill="#fff"/><circle cx="12" cy="12" r="2.5" fill="#000"/></mask></defs>
  <ellipse cx="12" cy="12" rx="9.5" ry="6" mask="url(#pupil)"/></svg>"##,
    ),
    // A arcs, relative h and v.
    (
        "lock.svg",
        r##"<svg viewBox="0 0 24 24" fill="none" stroke="#111" stroke-width="1.5"><rect x="5" y="10.75" width="14" height="10" rx="2"/>
  <path d="M8 10.75 v-3 a4 4 0 0 1 8 0 v3"/><path d="M12 14.5 V17"/></svg>"##,
    ),
    (
        "lock.solid.svg",
        r##"<svg viewBox="0 0 24 24"><rect x="5" y="10" width="14" height="11" rx="2"/><path d="M8 10.75 v-3 a4 4 0 0 1 8 0 v3" fill="none" stroke="#000" stroke-width="1.5"/></svg>"##,
    ),
    // Q and T, absolute and relative.
    (
        "undo.svg",
        r##"<svg viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="1.5"><path d="M4 9 Q10 3 15 6 T20 15 t-4 4"/><polyline points="4,4 4,9 9,9"/></svg>"##,
    ),
    // transforms: a plus turned into a cross.
    (
        "close.svg",
        r##"<svg viewBox="0 0 24 24" stroke="#000" stroke-width="1.5"><g transform="rotate(45 12 12)"><line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/></g></svg>"##,
    ),
    // translate and scale, rects without corners, a group's paint.
    (
        "menu.svg",
        r##"<svg viewBox="0 0 24 24"><g fill="#000" transform="translate(3 5) scale(1.5)"><rect width="12" height="1.5"/><rect y="4" width="12" height="1.5"/><rect y="8" width="8" height="1.5"/></g></svg>"##,
    ),
    // polyline, relative m and l, and L written after numbers.
    (
        "check.svg",
        r##"<svg viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2"><path d="m4.5 12.5 l5 5 L19.5 6.5"/></svg>"##,
    ),
    (
        "home.svg",
        r##"<svg viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="1.5"><polyline points="3 11 12 3.5 21 11"/><path d="M5.5 9.5 V20 H18.5 V9.5"/><rect x="10" y="14" width="4" height="6"/></svg>"##,
    ),
    // the width and height in place of a viewBox, in px.
    (
        "plus.svg",
        r##"<svg width="12px" height="12px" fill="none" stroke="#000" stroke-width="1"><path d="M6 1.5 V10.5 M1.5 6 H10.5"/></svg>"##,
    ),
    // matrix(), skewX, nested svg, a path written with S after L.
    (
        "folder.svg",
        r##"<svg viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="1.5"><svg><path transform="matrix(1 0 0 1 0 0.5)" d="M3 6 H9 L11 8 H21 V18.5 H3 Z"/></svg><path transform="skewX(-10)" d="M6 13 H18"/></svg>"##,
    ),
    // a style attribute that says everything.
    (
        "info.svg",
        r##"<svg viewBox="0 0 24 24"><circle cx="12" cy="12" r="9" style="fill:none;stroke:#000;stroke-width:1.5px"/><rect x="11.25" y="10.5" width="1.5" height="6.5" style="fill:#000"/><circle cx="12" cy="7.75" r="1.1"/></svg>"##,
    ),
    (
        "picture.svg",
        r##"<svg viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="1.5"><rect x="3" y="4.5" width="18" height="15" rx="1"/><polygon points="5,17.5 10,11 13,15 15,13 19,17.5" fill="#000" stroke="none"/><circle cx="16" cy="8.5" r="1.5" fill="#000" stroke="none"/></svg>"##,
    ),
    (
        "settings.svg",
        r##"<svg viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="1.5"><circle cx="12" cy="12" r="3"/><polygon points="12,2.5 14,5.5 17.5,4.5 18,8 21.5,10 19,12 21.5,14 18,16 17.5,19.5 14,18.5 12,21.5 10,18.5 6.5,19.5 6,16 2.5,14 5,12 2.5,10 6,8 6.5,4.5 10,5.5"/></svg>"##,
    ),
];

#[test]
fn every_icon_has_a_stable_kebab_case_name() {
    let mut seen = std::collections::HashSet::new();
    for icon in Icon::ALL {
        let name = icon.name();
        assert!(seen.insert(name), "{name} twice");
        assert!(!name.is_empty() && !name.starts_with('-') && !name.ends_with('-'));
        assert!(!name.contains("--"), "{name}");
        assert!(
            name.bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'),
            "{name} is not kebab case"
        );
        assert_eq!(Icon::from_name(name), Some(*icon));
        // A file name says which icon and style it draws.
        assert_eq!(
            file_icon(&file_name(*icon, IconStyle::Line)),
            Some((Ok(*icon), IconStyle::Line))
        );
        assert_eq!(
            file_icon(&file_name(*icon, IconStyle::Solid)),
            Some((Ok(*icon), IconStyle::Solid))
        );
        // No name ends as the solid twin's file does.
        assert!(!name.ends_with(".solid"));
    }
}

#[test]
fn a_generated_pack_with_every_svg_feature_draws_every_file() {
    let files: Vec<(String, Vec<u8>)> = TEST_PACK
        .iter()
        .map(|(name, text)| ((*name).to_owned(), text.as_bytes().to_vec()))
        .collect();
    let pack = Pack::from_files("test", &files);
    assert!(pack.notes.is_empty(), "{:#?}", pack.notes);
    assert_eq!(pack.manifest.name, "Test Blocks");
    assert_eq!(pack.manifest.licence, "CC0-1.0");
    assert_eq!(pack.manifest.format, 1);
    assert_eq!(pack.drawn().len(), TEST_PACK.len() - 1);
    let set = pack.set();
    for (icon, style) in pack.drawn() {
        for pixels in [16_usize, 24, 48] {
            let stroke = weight(pixels as f32);
            let tile = set.tile(icon, style, pixels, stroke);
            assert_eq!(tile.size, pixels);
            let covered = tile.alpha.iter().filter(|alpha| **alpha > 127).count();
            assert!(covered >= pixels, "{} at {pixels} is all but empty", icon.name());
            // It is the pack's drawing, not Sue's.
            assert_ne!(tile, icon.tile(style, pixels, stroke), "{}", icon.name());
        }
    }
    // The viewBox of 48 is scaled to the canvas: the lens of search
    // reaches as far as 32 of 48 would, 16 of 24.
    let search = set.tile(Icon::Search, IconStyle::Line, 24, 1.5);
    assert!(search.alpha[16 * 24 + 16] > 0 || search.alpha[16 * 24 + 15] > 0);
    // The mask cut the eye's ring free of its pupil; even and odd cut the
    // star's middle.
    let eye = set.tile(Icon::Eye, IconStyle::Line, 48, 3.0);
    assert_eq!(eye.alpha[24 * 48 + 24 - 7], 0, "the gap of the mask");
    let star = set.tile(Icon::Star, IconStyle::Line, 48, 3.0);
    let solid = set.tile(Icon::Star, IconStyle::Solid, 48, 3.0);
    assert_eq!(star.alpha[26 * 48 + 24], 0, "the hole of even and odd");
    assert_eq!(solid.alpha[26 * 48 + 24], 255);
}

#[test]
fn malformed_files_are_named_and_the_rest_of_the_pack_is_used() {
    let files: Vec<(String, Vec<u8>)> = vec![
        ("icon-pack.txt".into(), b"name = Broken\nformat = 9\nweird line\ncolour = red\n".to_vec()),
        ("search.svg".into(), b"<svg viewBox=\"0 0 24 24\"><circle cx=\"11\" cy=\"11\" r=\"7\"/></svg>".to_vec()),
        ("folder.svg".into(), vec![0xff, 0xfe, 0x00, 0x80]),
        ("delete.svg".into(), b"<html><body>not an icon</body></html>".to_vec()),
        ("info.svg".into(), b"<svg viewBox=\"0 0 24 24\"><path d=\"M2 2 L10 10 Q\" stroke=\"#000\"/><text>i</text></svg>".to_vec()),
        ("star.svg".into(), b"<svg viewBox=\"0 0 24 24\"></svg>".to_vec()),
        ("no-such-icon.svg".into(), b"<svg/>".to_vec()),
        ("search.solid.svg".into(), b"<svg/>".to_vec()),
        ("notes.docx".into(), b"pk".to_vec()),
        ("heart.svg".into(), format!("<svg>{}</svg>", "x".repeat(LARGEST_FILE as usize)).into_bytes()),
        ("lock.svg".into(), b"<svg viewBox=\"0 0 24 24\"><rect x=\"6\" y=\"10\" width=\"12\" height=\"10\"/></svg>".to_vec()),
    ];
    let pack = Pack::from_files("broken", &files);
    let has = |file: &str, problem: Problem| {
        assert!(
            pack.notes.contains(&Note {
                file: file.to_owned(),
                problem: problem.clone()
            }),
            "{file}: {problem:?} in {:#?}",
            pack.notes
        );
    };
    has("icon-pack.txt", Problem::NewerFormat { format: 9 });
    has("icon-pack.txt", Problem::ManifestLine { line: 3 });
    has("icon-pack.txt", Problem::UnknownKey { key: "colour".into() });
    has("folder.svg", Problem::Svg(svg::Problem::NotText));
    has("delete.svg", Problem::Svg(svg::Problem::NoSvg));
    has(
        "info.svg",
        Problem::Svg(svg::Problem::PathData {
            element: "path".into(),
            fault: super::super::path::PathFault::MissingNumber,
            at: 13,
        }),
    );
    has("info.svg", Problem::Svg(svg::Problem::NotDrawn { element: "text".into() }));
    has("star.svg", Problem::Svg(svg::Problem::Empty));
    has("no-such-icon.svg", Problem::UnknownIcon { name: "no-such-icon".into() });
    has("search.solid.svg", Problem::NoSolidTwin { name: "search".into() });
    has("notes.docx", Problem::Skipped);
    has(
        "heart.svg",
        Problem::Svg(svg::Problem::TooLarge {
            bytes: LARGEST_FILE + 11,
        }),
    );
    has("lock.svg", Problem::NoSolid { name: "lock".into() });
    // What could be read is used; what could not falls back to Sue.
    let mut drawn = pack.drawn();
    drawn.sort();
    assert_eq!(
        drawn,
        vec![
            (Icon::Search, IconStyle::Line),
            (Icon::Info, IconStyle::Line),
            (Icon::Lock, IconStyle::Line),
        ]
    );
    let set = pack.set();
    assert_eq!(set.source(Icon::Folder, IconStyle::Line), Source::Sue);
    assert_eq!(set.source(Icon::Lock, IconStyle::Solid), Source::Sue);
    // Every note reads as a sentence in English.
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    for note in &pack.notes {
        let message = note.message();
        assert!(message.contains(&note.file), "{message}");
        assert!(!message.contains("icon-pack-"), "{message}");
    }
}

#[test]
fn nothing_in_a_file_makes_the_reader_panic() {
    // Every SVG of the test pack and of Sue's own set, cut short at
    // every few bytes and with bytes changed: read and drawn, never a
    // panic.
    let mut sources: Vec<String> = TEST_PACK.iter().map(|(_, text)| (*text).to_owned()).collect();
    sources.push(super::super::svg(Icon::Eye, IconStyle::Line));
    sources.push(super::super::svg(Icon::Delete, IconStyle::Line));
    let mut seed = 0x2545_f491_u32;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    for source in &sources {
        let bytes = source.as_bytes();
        for cut in (0..bytes.len()).step_by(7) {
            let (drawing, _) = read_svg("cut.svg", &bytes[..cut]);
            if let Some(drawing) = drawing {
                let _ = draw::render(&drawing, 24, 1.5);
            }
        }
        for _ in 0..40 {
            let mut changed = bytes.to_vec();
            for _ in 0..4 {
                let at = next() as usize % changed.len();
                changed[at] = b"<>\"/=-.e0119 #MLAZaczq(),"[next() as usize % 25];
            }
            let (drawing, _) = read_svg("changed.svg", &changed);
            if let Some(drawing) = drawing {
                let _ = draw::render(&drawing, 24, 1.5);
            }
        }
    }
    // Numbers at the edge of what a float holds.
    for svg in [
        r##"<svg viewBox="0 0 1e-30 1e-30"><path d="M0 0 L1e30 1e30" stroke="#000"/></svg>"##,
        r##"<svg viewBox="0 0 24 24"><path d="M0 0 L3e38 3e38 L-3e38 0 Z"/></svg>"##,
        r##"<svg viewBox="0 0 24 24"><g transform="scale(1e30)"><circle r="1e30"/></g></svg>"##,
        r##"<svg viewBox="0 0 24 24"><path d="M12 12 A1e-30 1e30 45 1 1 13 13" stroke="#000" stroke-width="1e30"/></svg>"##,
    ] {
        let (drawing, _) = read_svg("edge.svg", svg.as_bytes());
        if let Some(drawing) = drawing {
            let _ = draw::render(&drawing, 24, 1.5);
        }
    }
}

#[test]
fn an_override_wins_over_the_pack_and_the_pack_over_sue() {
    let home = IconHome::new(scratch("fallback"));
    let source = scratch("fallback-source").join("blocks");
    write(&source, TEST_PACK);
    let pack = home.install(&source).unwrap();
    assert_eq!(pack.id, "test-blocks");
    home.choose(Some("test-blocks")).unwrap();
    let mine = r##"<svg viewBox="0 0 24 24"><rect x="4" y="4" width="16" height="16"/></svg>"##;
    home.set_override(Icon::Search, IconStyle::Line, mine.as_bytes())
        .unwrap();
    let (set, notes) = home.load();
    assert!(notes.is_empty(), "{notes:#?}");
    assert_eq!(set.source(Icon::Search, IconStyle::Line), Source::Override);
    assert_eq!(set.source(Icon::Heart, IconStyle::Line), Source::Pack);
    assert_eq!(set.source(Icon::Heart, IconStyle::Solid), Source::Pack);
    assert_eq!(set.source(Icon::Delete, IconStyle::Line), Source::Sue);
    // An icon without a solid twin is its line in both styles.
    assert_eq!(set.source(Icon::Search, IconStyle::Solid), Source::Override);
    let block = set.tile(Icon::Search, IconStyle::Line, 24, 1.5);
    assert_eq!(block.alpha[12 * 24 + 12], 255);
    assert_eq!(
        set.tile(Icon::Delete, IconStyle::Line, 24, 1.5),
        Icon::Delete.tile(IconStyle::Line, 24, 1.5)
    );
    assert_eq!(home.overrides(), vec![(Icon::Search, IconStyle::Line)]);
    // Back to the pack, then to Sue.
    home.clear_override(Icon::Search, IconStyle::Line).unwrap();
    assert_eq!(home.load().0.source(Icon::Search, IconStyle::Line), Source::Pack);
    home.choose(None).unwrap();
    let (set, _) = home.load();
    assert!(set.is_empty());
    assert_eq!(set.key(), 0);
    // An override that draws nothing is refused, and so is a solid twin
    // for an icon without one.
    assert!(matches!(
        home.set_override(Icon::Folder, IconStyle::Line, b"<svg/>"),
        Err(PackError::Unusable(_))
    ));
    assert_eq!(
        home.set_override(Icon::Folder, IconStyle::Solid, mine.as_bytes()),
        Err(PackError::NoSolidTwin(Icon::Folder))
    );
    assert!(home.overrides().is_empty());
}

#[test]
fn packs_are_installed_listed_chosen_replaced_and_removed() {
    let home = IconHome::new(scratch("lifecycle"));
    assert!(home.packs().is_empty());
    assert_eq!(home.chosen(), None);
    // From a folder.
    let source = scratch("lifecycle-source").join("Blocks Folder");
    write(&source, TEST_PACK);
    std::fs::write(source.join("LICENSE.txt"), "CC0").unwrap();
    std::fs::write(source.join("thumbs.db"), "junk").unwrap();
    std::fs::create_dir_all(source.join("drafts")).unwrap();
    let installed = home.install(&source).unwrap();
    assert_eq!(installed.id, "test-blocks");
    assert!(installed.notes.iter().any(|note| note.file == "thumbs.db"));
    assert!(home.root().join("packs/test-blocks/LICENSE.txt").is_file());
    assert!(!home.root().join("packs/test-blocks/thumbs.db").exists());
    assert_eq!(home.ids(), vec!["test-blocks".to_owned()]);
    // From an archive whose files sit in one folder, with names that try
    // to leave it.
    let mut zip = ZipWriter::new();
    zip.add_text(
        "rounded/icon-pack.txt",
        "format = 1\nname = Rounded\nauthor = A\nlicence = MIT\nversion = 2\n",
    );
    zip.add_text(
        "rounded/folder.svg",
        r##"<svg viewBox="0 0 24 24"><rect x="3" y="5" width="18" height="14" rx="4"/></svg>"##,
    );
    zip.add_text("rounded/../../escape.svg", "<svg/>");
    zip.add_text("rounded/deeper/search.svg", "<svg/>");
    let archive = scratch("lifecycle-archive").join("rounded.zip");
    std::fs::write(&archive, zip.finish()).unwrap();
    let rounded = home.install(&archive).unwrap();
    assert_eq!(rounded.id, "rounded");
    assert_eq!(rounded.drawn(), vec![(Icon::Folder, IconStyle::Line)]);
    assert!(rounded.notes.iter().any(|note| note.problem == Problem::Skipped));
    assert!(!home.root().join("packs/escape.svg").exists());
    assert!(!home.root().parent().unwrap().join("escape.svg").exists());
    let listed: Vec<String> = home.packs().into_iter().map(|pack| pack.id).collect();
    assert_eq!(listed, vec!["rounded".to_owned(), "test-blocks".to_owned()]);
    // Chosen, previewed, replaced by a newer version, removed.
    home.choose(Some("rounded")).unwrap();
    assert_eq!(home.chosen().as_deref(), Some("rounded"));
    let (preview, _) = home.preview("test-blocks");
    assert_eq!(preview.source(Icon::Heart, IconStyle::Line), Source::Pack);
    assert_eq!(home.chosen().as_deref(), Some("rounded"), "a preview chooses nothing");
    let mut newer = ZipWriter::new();
    newer.add_text("icon-pack.txt", "format = 1\nname = Rounded\nversion = 3\n");
    newer.add_text(
        "search.svg",
        r##"<svg viewBox="0 0 24 24"><circle cx="11" cy="11" r="7"/></svg>"##,
    );
    let replaced = home.install_archive("rounded", &newer.finish()).unwrap();
    assert_eq!(replaced.manifest.version, "3");
    assert_eq!(replaced.drawn(), vec![(Icon::Search, IconStyle::Line)]);
    assert!(!home.root().join("packs/rounded/folder.svg").exists());
    assert_eq!(home.ids().len(), 2, "no folder is left over from the swap");
    home.remove("rounded").unwrap();
    assert_eq!(home.chosen(), None, "removing the chosen pack goes back to Sue");
    assert_eq!(home.ids(), vec!["test-blocks".to_owned()]);
    // What is refused.
    assert_eq!(
        home.remove("rounded"),
        Err(PackError::NotInstalled("rounded".into()))
    );
    assert_eq!(home.remove("../x"), Err(PackError::BadId("../x".into())));
    assert_eq!(home.choose(Some("..")), Err(PackError::BadId("..".into())));
    assert_eq!(
        home.choose(Some("nothing")),
        Err(PackError::NotInstalled("nothing".into()))
    );
    let empty = scratch("lifecycle-empty");
    std::fs::write(empty.join("notes.txt"), "hello").unwrap();
    assert_eq!(home.install(&empty).unwrap_err(), PackError::NotAPack);
    assert!(matches!(
        home.install_archive("x", b"not a zip"),
        Err(PackError::Archive(_))
    ));
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    assert!(!PackError::NotAPack.message().contains("icon-pack-"));
    // A chosen pack that was taken away by hand is said so, and Sue draws.
    home.choose(Some("test-blocks")).unwrap();
    std::fs::remove_dir_all(home.root().join("packs/test-blocks")).unwrap();
    let (set, notes) = home.load();
    assert!(set.is_empty());
    assert_eq!(
        notes,
        vec![Note {
            file: "test-blocks".into(),
            problem: Problem::ChosenMissing {
                id: "test-blocks".into()
            }
        }]
    );
}

#[test]
fn ids_and_manifests() {
    assert_eq!(id_for("Rounded Lines 2!"), "rounded-lines-2");
    assert_eq!(id_for("  "), "pack");
    assert_eq!(id_for("Ünïcode Ñame"), "n-code-ame");
    assert!(id_for(&"x".repeat(200)).len() <= 64);
    assert!(valid_id("a-1"));
    assert!(!valid_id("A") && !valid_id("") && !valid_id("a/b") && !valid_id("-a"));
    let manifest = Manifest {
        name: "Name\nwith a break".into(),
        author: "Me".into(),
        licence: "MIT".into(),
        version: "1.0".into(),
        format: FORMAT,
        description: String::new(),
    };
    let (read, notes) = Manifest::parse(&manifest.write(), MANIFEST);
    assert!(notes.is_empty(), "{notes:?}");
    assert_eq!(read.name, "Name with a break");
    assert_eq!(read.licence, "MIT");
    let (read, notes) = Manifest::parse("license = X\n", MANIFEST);
    assert_eq!(read.licence, "X");
    assert_eq!(read.format, FORMAT);
    assert_eq!(notes.len(), 2, "no name, no format: {notes:?}");
}

#[test]
fn export_then_import_draws_the_same_pixels() {
    // Tolerance: none. Every pixel of every icon and twin at 16, 20, 24,
    // 32 and 48 pixels, and at one and a half times 24, is the same byte.
    let home = IconHome::new(scratch("round-trip"));
    let folder = scratch("round-trip-export").join("sue");
    export(&folder, &sue_manifest()).unwrap();
    let pack = home.install(&folder).unwrap();
    assert!(pack.notes.is_empty(), "{:#?}", pack.notes);
    assert_eq!(pack.id, "sue");
    let twins = Icon::ALL.iter().filter(|icon| icon.has_solid()).count();
    assert_eq!(pack.drawn().len(), Icon::ALL.len() + twins);
    home.choose(Some("sue")).unwrap();
    let (set, notes) = home.load();
    assert!(notes.is_empty());
    let mut worst = 0_u8;
    for icon in Icon::ALL {
        for style in IconStyle::ALL {
            for (pixels, stroke) in [
                (16_usize, weight(16.0)),
                (20, weight(20.0)),
                (24, weight(24.0)),
                (32, weight(32.0)),
                (48, weight(48.0)),
                (36, weight(24.0) * 1.5),
            ] {
                let mine = set.tile(*icon, style, pixels, stroke);
                let own = icon.tile(style, pixels, stroke);
                for (a, b) in mine.alpha.iter().zip(&own.alpha) {
                    worst = worst.max(a.abs_diff(*b));
                }
            }
        }
    }
    assert_eq!(worst, 0, "the round trip changed a pixel by {worst}");
    // The archive holds the same files.
    let archive = export_archive(&sue_manifest());
    let files = read_zip(&archive).unwrap();
    assert_eq!(files.len(), Icon::ALL.len() + twins + 1);
    let again = Pack::from_files("sue", &files);
    assert!(again.notes.is_empty());
}

/// The textured quads of a frame: texture and color.
fn quads(ctx: &Context, output: &headless::Seen) -> Vec<TextureId> {
    ctx.tessellate(output.shapes.clone(), output.pixels_per_point)
        .into_iter()
        .filter_map(|clipped| match clipped.primitive {
            egui::epaint::Primitive::Mesh(mesh)
                if mesh.texture_id != TextureId::default() && mesh.vertices.len() == 4 =>
            {
                Some(mesh.texture_id)
            }
            _ => None,
        })
        .collect()
}

#[test]
fn switching_packs_changes_the_next_frame() {
    let ctx = Context::default();
    let rect = Rect::from_min_size(pos2(8.0, 8.0), vec2(24.0, 24.0));
    let draw = |painter: &egui::Painter, _| paint(painter, rect, Icon::Heart, Color32::WHITE);
    let pixels_of = |seen: &headless::Seen| -> Option<Vec<u8>> {
        seen.sets
            .iter()
            .find(|(_, delta)| delta.image.size() == [24, 24])
            .map(|(_, delta)| {
                let egui::ImageData::Color(image) = &delta.image;
                image.pixels.iter().map(|pixel| pixel.a()).collect()
            })
    };
    let first = headless::frame(&ctx, [64.0, 64.0], 1.0, draw);
    let own = Icon::Heart.tile(IconStyle::Line, 24, weight(24.0)).alpha;
    assert_eq!(pixels_of(&first).unwrap(), own);
    let sue_texture = quads(&ctx, &first)[0];
    // A pack is shown: the very next frame makes and draws its tile.
    let files: Vec<(String, Vec<u8>)> = TEST_PACK
        .iter()
        .map(|(name, text)| ((*name).to_owned(), text.as_bytes().to_vec()))
        .collect();
    let pack = Pack::from_files("test", &files);
    show(&ctx, pack.set());
    assert_eq!(super::super::cached(&ctx), 0, "the old tiles are dropped");
    let second = headless::frame(&ctx, [64.0, 64.0], 1.0, draw);
    let theirs = pack.set().tile(Icon::Heart, IconStyle::Line, 24, weight(24.0)).alpha;
    assert_eq!(pixels_of(&second).unwrap(), theirs);
    assert_ne!(theirs, own);
    assert_ne!(quads(&ctx, &second)[0], sue_texture);
    // And back to Sue's own.
    show(&ctx, IconSet::sue());
    let third = headless::frame(&ctx, [64.0, 64.0], 1.0, draw);
    assert_eq!(pixels_of(&third).unwrap(), own);
    // A preview draws from its own set and leaves the shown one alone.
    let preview = pack.set();
    let fourth = headless::frame(&ctx, [64.0, 64.0], 1.0, |painter, _| {
        paint_from(painter, rect, &preview, Icon::Heart, IconStyle::Line, Color32::WHITE);
    });
    assert_eq!(pixels_of(&fourth).unwrap(), theirs);
    assert!(showing(&ctx).is_empty());
}

#[test]
fn refresh_loads_the_home_and_shows_it() {
    let home = IconHome::new(scratch("refresh"));
    let ctx = Context::default();
    let mine = r##"<svg viewBox="0 0 24 24"><rect x="4" y="4" width="16" height="16"/></svg>"##;
    home.set_override(Icon::Folder, IconStyle::Line, mine.as_bytes())
        .unwrap();
    assert!(refresh(&ctx, &home).is_empty());
    assert_eq!(
        showing(&ctx).source(Icon::Folder, IconStyle::Line),
        Source::Override
    );
}

/// A picture made of tiles, for the eye.
struct Picture {
    width: usize,
    height: usize,
    rgb: Vec<u8>,
}

impl Picture {
    fn new(width: usize, height: usize, ground: Color32) -> Self {
        Self {
            width,
            height,
            rgb: [ground.r(), ground.g(), ground.b()].repeat(width * height),
        }
    }

    fn tile(&mut self, at: [usize; 2], tile: &Tile, zoom: usize, ink: Color32) {
        for (index, alpha) in tile.alpha.iter().enumerate() {
            let (tx, ty) = (index % tile.size, index / tile.size);
            let cover = f32::from(*alpha) / 255.0;
            for dy in 0..zoom {
                for dx in 0..zoom {
                    let (x, y) = (at[0] + tx * zoom + dx, at[1] + ty * zoom + dy);
                    if x >= self.width || y >= self.height {
                        continue;
                    }
                    let pixel = (y * self.width + x) * 3;
                    for (channel, color) in [ink.r(), ink.g(), ink.b()].into_iter().enumerate() {
                        let under = f32::from(self.rgb[pixel + channel]);
                        self.rgb[pixel + channel] =
                            (under + (f32::from(color) - under) * cover).round() as u8;
                    }
                }
            }
        }
    }

    fn save(&self, file: &Path) {
        use zune_core::bit_depth::BitDepth;
        use zune_core::colorspace::ColorSpace;
        use zune_core::options::EncoderOptions;
        let options = EncoderOptions::new(self.width, self.height, ColorSpace::RGB, BitDepth::Eight);
        let mut out = Vec::new();
        zune_png::PngEncoder::new(&self.rgb, options)
            .encode(&mut out)
            .unwrap();
        std::fs::write(file, out).unwrap();
    }
}

/// Writes, into the folder `SUE_PACK_SHEET` names, the contact sheet of
/// the test pack against Sue's own icons: for every icon the pack draws,
/// a row of Sue's own at 16, 24 and 48 pixels and the pack's beside it,
/// in the colors of the themes `void` and `paper`, and the same at 24
/// with every pixel six wide.
/// `cargo test -p sue-wm pack_contact_sheet -- --ignored`.
#[test]
#[ignore = "writes pictures for the eye; set SUE_PACK_SHEET to a folder"]
fn pack_contact_sheet() {
    let Some(folder) = std::env::var_os("SUE_PACK_SHEET").map(PathBuf::from) else {
        return;
    };
    std::fs::create_dir_all(&folder).unwrap();
    let files: Vec<(String, Vec<u8>)> = TEST_PACK
        .iter()
        .map(|(name, text)| ((*name).to_owned(), text.as_bytes().to_vec()))
        .collect();
    let pack = Pack::from_files("test", &files);
    let set = pack.set();
    let drawn = pack.drawn();
    for name in ["void", "paper"] {
        let theme = {
            let mut rice = sue_rice::RiceSchema::parse(sue_rice::DEFAULT_RICE).unwrap();
            let preset = sue_rice::builtin_presets()
                .into_iter()
                .find(|preset| preset.id == name)
                .unwrap();
            preset.transaction(&rice).apply(&mut rice).unwrap();
            crate::Theme::from_rice(&rice)
        };
        let zoom = 6;
        let row = 24 * zoom + 16;
        let half = 16 + 24 + 48 + 4 * 12 + 24 * zoom + 12;
        let mut picture = Picture::new(2 * half + 24, drawn.len() * row + 16, theme.void);
        for (index, (icon, style)) in drawn.iter().enumerate() {
            let top = 8 + index * row;
            for (column, from_pack) in [false, true].into_iter().enumerate() {
                let mut left = 12 + column * (half + 12);
                for pixels in [16_usize, 24, 48] {
                    let stroke = weight(pixels as f32);
                    let tile = if from_pack {
                        set.tile(*icon, *style, pixels, stroke)
                    } else {
                        icon.tile(*style, pixels, stroke)
                    };
                    picture.tile([left, top + (48 - pixels) / 2], &tile, 1, theme.text);
                    left += pixels + 12;
                }
                let tile = if from_pack {
                    set.tile(*icon, *style, 24, weight(24.0))
                } else {
                    icon.tile(*style, 24, weight(24.0))
                };
                picture.tile([left, top], &tile, zoom, theme.text);
            }
        }
        picture.save(&folder.join(format!("pack-vs-sue-{name}.png")));
    }
}
