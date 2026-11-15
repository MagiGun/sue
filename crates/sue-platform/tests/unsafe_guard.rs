//! The guard of rule 13 (v0.0.8 plan, section 3) and of rule 6 in
//! `CLAUDE.md`: safe Rust is the default. The workspace forbids `unsafe`,
//! and only the small audited sub-crates of `crates/sue-platform` named
//! below may relax that, to `deny` (so each item allows it on its own,
//! with a `// SAFETY:` comment on every block). A new sub-crate is added
//! to the list together with its reason on the agent board.

use std::path::{Path, PathBuf};

/// The audited sub-crates, relative to the repository's root.
const AUDITED: [&str; 5] = [
    "crates/sue-platform/android",
    "crates/sue-platform/ios",
    "crates/sue-platform/wayland",
    "crates/sue-platform/windows",
    "crates/sue-platform/winpen",
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repository is there")
}

/// Every `Cargo.toml` of the repository, outside build output and Git.
fn manifests(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("a readable folder") {
        let path = entry.expect("a readable entry").path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if path.is_dir() {
            if !matches!(name, "target" | ".git" | "node_modules") {
                manifests(&path, out);
            }
        } else if name == "Cargo.toml" {
            out.push(path);
        }
    }
}

/// The value of `key` under `[section]`, as written.
fn value(manifest: &str, section: &str, key: &str) -> Option<String> {
    manifest
        .lines()
        .map(str::trim)
        .skip_while(|line| *line != section)
        .skip(1)
        .take_while(|line| !line.starts_with('['))
        .find_map(|line| {
            let (name, value) = line.split_once('=')?;
            (name.trim() == key).then(|| value.trim().to_owned())
        })
}

fn relative(path: &Path, root: &Path) -> String {
    path.parent()
        .and_then(|dir| dir.strip_prefix(root).ok())
        .map(|dir| dir.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default()
}

#[test]
fn no_manifest_outside_the_audited_sub_crates_relaxes_unsafe_code() {
    let root = root();
    let workspace = std::fs::read_to_string(root.join("Cargo.toml")).expect("the workspace");
    assert_eq!(
        value(&workspace, "[workspace.lints.rust]", "unsafe_code").as_deref(),
        Some("\"forbid\""),
        "the workspace forbids unsafe"
    );
    let mut found = Vec::new();
    manifests(&root, &mut found);
    assert!(found.len() > 10, "the manifests are found: {found:?}");
    let mut audited_seen = Vec::new();
    for path in found {
        let dir = relative(&path, &root);
        if dir.is_empty() {
            continue;
        }
        let manifest = std::fs::read_to_string(&path).expect("a readable manifest");
        // Any way of writing the lint in this manifest.
        let mentions = manifest
            .lines()
            .map(str::trim)
            .any(|line| !line.starts_with('#') && line.contains("unsafe_code"));
        if AUDITED.contains(&dir.as_str()) {
            audited_seen.push(dir.clone());
            assert_eq!(
                value(&manifest, "[lints.rust]", "unsafe_code").as_deref(),
                Some("\"deny\""),
                "{dir}: an audited sub-crate relaxes forbid to deny, never further"
            );
        } else {
            assert!(
                !mentions,
                "{dir}: only the audited sub-crates of crates/sue-platform may relax unsafe_code"
            );
            assert_eq!(
                value(&manifest, "[lints]", "workspace").as_deref(),
                Some("true"),
                "{dir}: every other crate takes the workspace's lints, forbid included"
            );
        }
    }
    audited_seen.sort();
    assert_eq!(audited_seen, AUDITED, "each audited sub-crate is there");
}

#[test]
fn the_windows_sub_crate_documents_every_unsafe_block() {
    let dir = root().join("crates/sue-platform/windows");
    let manifest = std::fs::read_to_string(dir.join("Cargo.toml")).expect("its manifest");
    assert_eq!(
        value(&manifest, "[lints.clippy]", "undocumented_unsafe_blocks").as_deref(),
        Some("\"deny\""),
        "clippy holds every block to a SAFETY comment"
    );
    let mut sources = Vec::new();
    let mut stack = vec![dir.join("src")];
    while let Some(folder) = stack.pop() {
        for entry in std::fs::read_dir(folder).expect("its sources") {
            let path = entry.expect("an entry").path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                sources.push(path);
            }
        }
    }
    let mut blocks = 0;
    for path in sources {
        let text = std::fs::read_to_string(&path).expect("a source");
        let lines: Vec<&str> = text.lines().collect();
        for (at, line) in lines.iter().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            if code.contains("unsafe {") {
                blocks += 1;
                // The comment sits right above the statement, which may
                // start a few lines up.
                let above = lines[at.saturating_sub(8)..at]
                    .iter()
                    .any(|line| line.trim_start().starts_with("// SAFETY:"));
                assert!(
                    above,
                    "{}:{}: an unsafe block without // SAFETY:",
                    path.display(),
                    at + 1
                );
            }
            // `#[allow(unsafe_code, reason = "...")]`, one item at a time.
            if code.contains("unsafe_code") {
                let item = lines[at..lines.len().min(at + 3)].join(" ");
                assert!(
                    item.contains("reason"),
                    "{}:{}: allow(unsafe_code) names its reason",
                    path.display(),
                    at + 1
                );
            }
        }
    }
    assert!(blocks > 10, "the media module's blocks are found: {blocks}");
}
