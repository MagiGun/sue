//! The ledger of icon names. An icon's name (`layer-new`) is saved data:
//! icon packs name their files after it and people's single overrides
//! are stored under it (`docs/icon-packs.md`). So a name, once given, is
//! kept. `golden/icon-names.txt` holds every name of 0.0.8 in the order
//! of the table; it is read, never rewritten. A name leaves only by being
//! listed in [`RETIRED`], with the package that retired it and what a
//! pack should draw instead, and the docs for pack makers say so.

use std::path::{Path, PathBuf};

use sue_wm::icons::Icon;

/// Names that left on purpose: `(name, package and what stands in its
/// place)`. The ledger file itself is never edited.
const RETIRED: &[(&str, &str)] = &[];

/// Every committed fixture of this test and its SHA-256 (rule 12 of the
/// 0.0.8 plan: golden files are read, never rewritten).
const LEDGER: &[(&str, &str)] = &[(
    "icon-names.txt",
    "62f3698afc13858ee632a372f987506c756406744a7e5a12f2618cab24ab7d4f",
)];

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn names() -> Vec<String> {
    let text =
        std::fs::read_to_string(golden_dir().join("icon-names.txt")).expect("the ledger is there");
    assert!(!text.contains('\r'), "the ledger keeps its line ends");
    text.lines().map(str::to_owned).collect()
}

#[test]
fn no_icon_name_is_lost() {
    let names = names();
    assert_eq!(names.len(), 260, "the names 0.0.8 gave");
    let mut lost = Vec::new();
    for name in &names {
        if RETIRED.iter().any(|(retired, _)| retired == name) {
            assert!(
                Icon::from_name(name).is_none(),
                "{name} is retired but still an icon"
            );
            continue;
        }
        if Icon::from_name(name).is_none() {
            lost.push(name.as_str());
        }
    }
    assert!(
        lost.is_empty(),
        "icon names are saved data (packs and overrides are named after \
         them); these are gone: {lost:?}. Bring them back, or list each in \
         RETIRED with what stands in its place and say so in \
         docs/icon-packs.md."
    );
}

#[test]
fn every_retired_name_was_in_the_ledger() {
    let names = names();
    for (name, why) in RETIRED {
        assert!(names.iter().any(|known| known == name), "{name}");
        assert!(!why.is_empty(), "{name} needs a reason");
    }
}

#[test]
fn the_docs_for_pack_makers_list_every_icon() {
    let docs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/icon-packs.md");
    let docs = std::fs::read_to_string(docs).expect("docs/icon-packs.md is there");
    let missing: Vec<&str> = Icon::ALL
        .iter()
        .map(|icon| icon.name())
        .filter(|name| !docs.contains(&format!("| `{name}` |")))
        .collect();
    assert!(
        missing.is_empty(),
        "docs/icon-packs.md does not list these icons: {missing:?}"
    );
}

#[test]
fn the_golden_files_are_as_committed() {
    let mut found: Vec<String> = std::fs::read_dir(golden_dir())
        .expect("tests/golden is there")
        .map(|entry| {
            entry
                .expect("a readable entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    found.sort();
    let listed: Vec<&str> = LEDGER.iter().map(|(name, _)| *name).collect();
    assert_eq!(found, listed, "tests/golden and the ledger hold the same");
    for (name, sum) in LEDGER {
        let found = sue_platform::update::sha256_of(&golden_dir().join(name))
            .unwrap_or_else(|error| panic!("cannot read {name}: {error}"));
        assert_eq!(
            found, *sum,
            "{name} changed: golden files are read, never rewritten"
        );
    }
}
