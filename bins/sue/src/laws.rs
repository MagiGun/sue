//! Tests that enforce the agent laws over the whole repository.

use std::path::{Path, PathBuf};

/// LAW 8: no em dashes, en dashes or middle dots anywhere.
const FORBIDDEN: [char; 3] = ['\u{2014}', '\u{2013}', '\u{00B7}'];

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("bins/sue sits two levels below the repository")
        .to_path_buf()
}

/// Every text file in the repository, skipping build output and Git.
fn text_files(dir: &Path, found: &mut Vec<(PathBuf, String)>) {
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
            text_files(&path, found);
        } else if let Ok(text) = std::fs::read_to_string(&path) {
            found.push((path, text));
        }
    }
}

#[test]
fn law_8_no_em_dashes_en_dashes_or_middle_dots() {
    let mut files = Vec::new();
    text_files(&repository(), &mut files);
    assert!(
        files.iter().any(|(path, _)| path.ends_with("AGENTS.md")),
        "the scan must reach the repository root"
    );
    let offenders: Vec<String> = files
        .iter()
        .flat_map(|(path, text)| {
            text.lines()
                .enumerate()
                .filter(|(_, line)| line.contains(FORBIDDEN))
                .map(move |(index, _)| format!("{}:{}", path.display(), index + 1))
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "LAW 8 forbids em dashes, en dashes and middle dots:\n{}",
        offenders.join("\n")
    );
}
