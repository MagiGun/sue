//! The database's promises, and the checks every language must pass.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::*;

/// The languages Sue ships (tests add languages of their own).
fn shipped() -> Vec<LanguageInfo> {
    languages()
        .into_iter()
        .filter(|language| BUILT_IN.iter().any(|(tag, ..)| *tag == language.tag))
        .collect()
}

#[test]
fn every_built_in_file_parses() {
    assert_eq!(built_in_errors(), Vec::<String>::new());
    for language in shipped() {
        let report = check(&language.tag).expect("a listed language has a report");
        assert!(report.errors.is_empty(), "{report}");
    }
}

#[test]
fn every_language_names_itself() {
    for language in shipped() {
        assert_ne!(
            language.name, language.tag,
            "{} lacks language-name",
            language.tag
        );
        assert!(!language.name.trim().is_empty());
    }
    assert_eq!(shipped()[0].tag, ENGLISH);
    assert!(!shipped()[0].draft, "English is the source, never a draft");
}

#[test]
fn no_language_has_keys_english_lacks_or_other_variables() {
    for language in shipped() {
        let report = check(&language.tag).expect("a listed language has a report");
        assert!(report.stale.is_empty(), "{report}");
        assert!(report.variables.is_empty(), "{report}");
    }
}

#[test]
fn english_is_complete_against_itself() {
    let report = check(ENGLISH).expect("English is built in");
    assert!(report.complete(), "{report}");
    assert!(report.total > 0);
}

#[test]
fn a_missing_message_falls_back_to_english_never_blank() {
    add_resource("en", "test-fallback", "only-english = Only in English\n").expect("parses");
    add_resource("xx-test", "test-fallback", "both = Both, in xx\n").expect("parses");
    add_resource("en", "test-fallback", "both = Both\n").expect("parses");
    let _scope = scope("xx-test");
    assert_eq!(text("test-fallback", "both"), "Both, in xx");
    assert_eq!(text("test-fallback", "only-english"), "Only in English");
    // A key nobody has reads as words, not blank and not the raw key.
    assert_eq!(text("test-fallback", "no-such-thing"), "No such thing");
    assert_eq!(text_or("test-fallback", "no-such-thing", "Given"), "Given");
}

#[test]
fn scopes_nest_and_end() {
    add_resource("en", "test-scope", "hello = Hello\n").expect("parses");
    add_resource("yy-test", "test-scope", "hello = Hi from yy\n").expect("parses");
    add_resource("zz-test", "test-scope", "hello = Hi from zz\n").expect("parses");
    let outer = current();
    {
        let _yy = scope("yy-test");
        assert_eq!(tr!("test-scope", "hello"), "Hi from yy");
        {
            let _zz = scope("zz-test");
            assert_eq!(tr!("test-scope", "hello"), "Hi from zz");
        }
        assert_eq!(tr!("test-scope", "hello"), "Hi from yy");
    }
    assert_eq!(current(), outer);
    // Another thread does not see this thread's scope.
    let _yy = scope("yy-test");
    let other = std::thread::spawn(current).join().expect("thread runs");
    assert_eq!(other, global());
}

#[test]
fn variables_and_plurals() {
    add_resource(
        "en",
        "test-vars",
        "files = { $count ->\n    [one] One file in { $folder }\n   *[other] { $count } files in { $folder }\n}\n",
    )
    .expect("parses");
    let _en = scope(ENGLISH);
    assert_eq!(
        tr!("test-vars", "files", count = 1, folder = "Pictures"),
        "One file in Pictures"
    );
    assert_eq!(
        tr!("test-vars", "files", count = 3, folder = "Pictures"),
        "3 files in Pictures"
    );
}

#[test]
fn numbers_follow_the_language() {
    add_resource(
        "ww-test",
        LANGUAGE,
        "language-name = Ww\nnumber-decimal = ,\nnumber-group = .\nnumber-group-min = 2\n",
    )
    .expect("parses");
    add_resource("ww-test", "test-numbers", "size = { $size } MiB\n").expect("parses");
    add_resource("en", "test-numbers", "size = { $size } MiB\n").expect("parses");
    {
        let _ww = scope("ww-test");
        assert_eq!(number(1234.5, 1), "1234,5");
        assert_eq!(number(12_345.25, 2), "12.345,25");
        assert_eq!(tr!("test-numbers", "size", size = 2.5), "2,5 MiB");
    }
    let _en = scope(ENGLISH);
    assert_eq!(number(1234.5, 1), "1,234.5");
    assert_eq!(number(-0.0, 0), "0");
    assert_eq!(tr!("test-numbers", "size", size = 2.5), "2.5 MiB");
}

#[test]
fn settings_resolve_to_languages_sue_has() {
    assert_eq!(resolve("en"), ENGLISH);
    assert_eq!(
        resolve("tlh"),
        ENGLISH,
        "a language Sue lacks reads English"
    );
    assert_eq!(resolve_for(FOLLOW, "xx"), "xx");
    assert_eq!(resolve_for("", "xx"), "xx");
    assert_eq!(resolve_for("en", "xx"), ENGLISH);
    for language in shipped() {
        assert_eq!(resolve(&language.tag), language.tag);
    }
}

#[test]
fn close_languages_match_by_language_and_script() {
    let id = |tag: &str| -> LanguageIdentifier { tag.parse().expect("valid tag") };
    assert!(close(&id("pt-PT"), &id("pt-BR")));
    assert!(close(&id("zh-CN"), &id("zh-Hans")));
    assert!(close(&id("zh"), &id("zh-Hans")));
    assert!(!close(&id("zh-TW"), &id("zh-Hans")));
    assert!(!close(&id("zh-Hant"), &id("zh-Hans")));
    assert!(!close(&id("es"), &id("pt")));
}

#[test]
fn the_report_lists_missing_and_stale_keys() {
    add_resource("en", "test-report", "kept = Kept\nnew = New\n").expect("parses");
    add_resource("vv-test", "test-report", "kept = Behalten\nold = Alt\n").expect("parses");
    add_resource(
        "vv-test",
        LANGUAGE,
        "language-name = Vv\nlanguage-status = draft\n",
    )
    .expect("parses");
    let report = check("vv-test").expect("added above");
    assert!(report.draft);
    assert!(
        report
            .missing
            .iter()
            .any(|(space, key, line)| space == "test-report"
                && key == "new"
                && line == "new = New")
    );
    assert!(
        report
            .stale
            .contains(&("test-report".to_owned(), "old".to_owned()))
    );
    assert!(report.to_string().contains("new = New"));
}

// ----- The code against the database -------------------------------------------

/// The workspace's root.
fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/sue-lang sits two folders down")
        .to_path_buf()
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "target") {
                continue;
            }
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// The arguments of the macro call starting right after `open` (just past
/// its `(`), split at top-level commas, with the length read.
fn arguments(text: &str) -> Option<(Vec<String>, usize)> {
    let mut depth = 0_i32;
    let mut parts = vec![String::new()];
    let mut chars = text.char_indices().peekable();
    let mut in_string = false;
    while let Some((index, ch)) = chars.next() {
        if in_string {
            parts.last_mut()?.push(ch);
            if ch == '\\' {
                if let Some((_, escaped)) = chars.next() {
                    parts.last_mut()?.push(escaped);
                }
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => {
                in_string = true;
                parts.last_mut()?.push(ch);
            }
            '(' | '[' | '{' => {
                depth += 1;
                parts.last_mut()?.push(ch);
            }
            ')' | ']' | '}' if depth == 0 => {
                let parts = parts
                    .into_iter()
                    .map(|part| part.trim().to_owned())
                    .filter(|part| !part.is_empty())
                    .collect();
                return Some((parts, index));
            }
            ')' | ']' | '}' => {
                depth -= 1;
                parts.last_mut()?.push(ch);
            }
            ',' if depth == 0 => parts.push(String::new()),
            _ => parts.last_mut()?.push(ch),
        }
    }
    None
}

fn literal(text: &str) -> Option<String> {
    let inner = text.strip_prefix('"')?.strip_suffix('"')?;
    (!inner.contains(['"', '\\', '{'])).then(|| inner.to_owned())
}

/// Every `(namespace, key, variables, place)` the code reads with a
/// literal key: through `sue_lang::tr!`, `sue_lang::text`, or a crate's own
/// `tr!` wrapper (whose namespace its definition names).
fn uses_in_code() -> Vec<(String, String, BTreeSet<String>, String)> {
    let root = workspace();
    let mut crates: Vec<PathBuf> = Vec::new();
    for group in ["crates", "bins"] {
        if let Ok(entries) = std::fs::read_dir(root.join(group)) {
            crates.extend(entries.filter_map(Result::ok).map(|entry| entry.path()));
        }
    }
    let mut uses = Vec::new();
    for krate in crates {
        let mut files = Vec::new();
        rust_files(&krate, &mut files);
        let sources: Vec<(PathBuf, String)> = files
            .into_iter()
            .filter_map(|path| Some((path.clone(), std::fs::read_to_string(path).ok()?)))
            .map(|(path, source)| {
                // Comments (and the examples in them) are not code.
                let code = source
                    .lines()
                    .map(|line| {
                        if line.trim_start().starts_with("//") {
                            ""
                        } else {
                            line
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                (path, code)
            })
            .collect();
        // A wrapper: `sue_lang::tr!("ns", $($arg)*)`. A file's own wins,
        // else the crate root's (`lib.rs` or `main.rs`).
        let wrapper_of = |source: &str| {
            let at = source.find("sue_lang::tr!(\"")?;
            let rest = &source[at + "sue_lang::tr!(".len()..];
            let (parts, _) = arguments(rest)?;
            (parts.len() == 2 && parts[1].starts_with('$'))
                .then(|| literal(&parts[0]))
                .flatten()
        };
        let root = sources
            .iter()
            .filter(|(path, _)| {
                path.file_name()
                    .is_some_and(|name| name == "lib.rs" || name == "main.rs")
            })
            .find_map(|(_, source)| wrapper_of(source));
        for (path, source) in &sources {
            let wrapper = wrapper_of(source).or_else(|| root.clone());
            let place_of = |offset: usize| {
                let line = source[..offset].lines().count();
                format!("{}:{line}", path.display())
            };
            let code = source;
            for (needle, own) in [
                ("sue_lang::tr!(", false),
                ("sue_lang::text(", false),
                ("sue_lang::text_with(", false),
                ("tr!(", true),
            ] {
                let mut from = 0;
                while let Some(found) = code[from..].find(needle) {
                    let at = from + found;
                    from = at + needle.len();
                    let before = code[..at].chars().next_back();
                    if own && (before == Some(':') || before.is_some_and(char::is_alphanumeric)) {
                        continue;
                    }
                    let Some((parts, _)) = arguments(&code[from..]) else {
                        continue;
                    };
                    let (namespace, rest) = if own {
                        let Some(namespace) = wrapper.clone() else {
                            continue;
                        };
                        (namespace, &parts[..])
                    } else {
                        let Some(namespace) = parts.first().and_then(|part| literal(part)) else {
                            continue;
                        };
                        (namespace, &parts[1.min(parts.len())..])
                    };
                    let Some(key) = rest.first().and_then(|part| literal(part)) else {
                        continue;
                    };
                    let variables = if needle == "sue_lang::text_with(" {
                        continue;
                    } else {
                        rest[1..]
                            .iter()
                            .filter_map(|part| {
                                let (name, _) = part.split_once('=')?;
                                Some(name.trim().to_owned())
                            })
                            .collect()
                    };
                    uses.push((namespace, key, variables, place_of(at)));
                }
            }
        }
    }
    uses
}

#[test]
fn every_key_the_code_reads_is_in_english_with_its_variables() {
    let english = english_keys();
    let mut problems = Vec::new();
    let uses = uses_in_code();
    for (namespace, key, variables, place) in &uses {
        match english.get(&(namespace.clone(), key.clone())) {
            None => problems.push(format!("{place}: {namespace}.ftl lacks {key}")),
            Some(wanted) if wanted != variables => problems.push(format!(
                "{place}: {namespace}.ftl {key} uses {wanted:?}, the code gives {variables:?}"
            )),
            Some(_) => {}
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    let namespaces: BTreeMap<&str, usize> =
        uses.iter()
            .fold(BTreeMap::new(), |mut counts, (namespace, ..)| {
                *counts.entry(namespace.as_str()).or_default() += 1;
                counts
            });
    assert!(!namespaces.is_empty(), "the scan found the code's lookups");
}
