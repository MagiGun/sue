//! The legal guard: Sue reaches and names nobody else's material site,
//! store or hardware.
//!
//! Sue 0.0.8 removed the start page's browser of another company's
//! material site, with its scraper, its strings and the settings page
//! named after that company's controller. This test fails when any of it
//! comes back: an address of that company, its name, the store's name or
//! the controller's name, in the code, the language files, the packaging
//! or the guides. File format names (a `.clip` file is still called what
//! it is) are ordinary descriptive use and do not match.
//!
//! The patterns are put together from pieces, so this file does not hold
//! what it forbids and passes its own scan.

use std::path::{Path, PathBuf};

/// The folders scanned whole, and the guides scanned beside them, from the
/// repository's root.
const FOLDERS: [&str; 3] = ["crates", "bins", "packaging"];
const GUIDES: [&str; 3] = ["HUMANS.md", "HUMANZ.md", "README.md"];

/// What must not appear, in any mix of capitals, each in pieces.
const FORBIDDEN: [&[&str]; 7] = [
    // The company's sites.
    &["clip", "-studio", ".com"],
    &["clip", "studio", ".net"],
    // The company, and the name its markup and files carry.
    &["cel", "sys"],
    &["cel", "clip", "material"],
    // Its material store, by name.
    &["clip studio", " assets"],
    // Its controller.
    &["tab", "mate"],
    // The removed page's own name, taken from that company's start page.
    &["browse", " assets"],
];

fn patterns() -> Vec<String> {
    FORBIDDEN.iter().map(|pieces| pieces.concat()).collect()
}

/// The patterns `text` holds, without regard to case.
fn found_in(text: &str, patterns: &[String]) -> Vec<String> {
    let lower = text.to_lowercase();
    patterns
        .iter()
        .filter(|pattern| lower.contains(pattern.as_str()))
        .cloned()
        .collect()
}

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("bins/sue sits two levels below the repository")
        .to_path_buf()
}

/// Every file under `dir`, skipping build output and Git.
fn files(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let name = entry.file_name();
        if name == "target" || name == ".git" {
            continue;
        }
        if path.is_dir() {
            files(&path, found);
        } else {
            found.push(path);
        }
    }
}

#[test]
fn the_guard_knows_each_name_in_any_case_and_lets_format_names_be() {
    let patterns = patterns();
    assert_eq!(patterns.len(), FORBIDDEN.len());
    // Each pattern is found alone, in lower case, in capitals and inside
    // other text.
    for pattern in &patterns {
        for text in [
            pattern.clone(),
            pattern.to_uppercase(),
            format!("see https://assets.{pattern}/en-us/ for more"),
            format!("a line\nthen {} here", pattern.to_uppercase()),
        ] {
            assert_eq!(
                found_in(&text, &patterns),
                std::slice::from_ref(pattern),
                "{text}"
            );
        }
    }
    // Descriptive use of a file format's name stays allowed.
    for allowed in [
        "Clip Studio Paint file",
        "Clip Studio Paint (.clip)",
        ".clip (Clip Studio format file)...",
        "application/x-clip-studio-paint",
        "Brushes you downloaded: Clip Studio Paint (.sut), Photoshop (.abr)",
        "Manage Materials",
        "the assets of a release",
        "a table of contents",
    ] {
        assert_eq!(
            found_in(allowed, &patterns),
            Vec::<String>::new(),
            "{allowed}"
        );
    }
}

#[test]
fn nothing_reaches_or_names_another_companys_material_site() {
    let root = repository();
    let mut scanned = Vec::new();
    for folder in FOLDERS {
        let before = scanned.len();
        files(&root.join(folder), &mut scanned);
        assert!(scanned.len() > before, "{folder} was not found to scan");
    }
    for guide in GUIDES {
        let path = root.join(guide);
        assert!(path.is_file(), "{guide} was not found to scan");
        scanned.push(path);
    }
    for reached in [
        "crates/sue-paint/src/start.rs",
        "crates/sue-lang/locales/ja/paint.ftl",
        "bins/sue/src/legal.rs",
    ] {
        assert!(
            scanned.iter().any(|path| path.ends_with(reached)),
            "the scan must reach {reached}"
        );
    }
    let patterns = patterns();
    let mut offenders = Vec::new();
    for path in &scanned {
        // Bytes that are not text are read as far as they are, so a name
        // inside a binary file is seen too.
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        if found_in(&text, &patterns).is_empty() {
            continue;
        }
        for (index, line) in text.lines().enumerate() {
            for pattern in found_in(line, &patterns) {
                offenders.push(format!("{}:{}: {pattern}", path.display(), index + 1));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "Sue must not reach or name another company's material site, store \
         or hardware (see bins/sue/src/legal.rs):\n{}",
        offenders.join("\n")
    );
}
