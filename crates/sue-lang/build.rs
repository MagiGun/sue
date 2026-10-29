//! Finds every language in `locales/` (one folder per language tag, one
//! `.ftl` file per namespace) and embeds them, so adding a language is
//! adding a folder: no list in the code names them.

use std::fmt::Write as _;
use std::path::Path;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("locales");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut files = Vec::new();
    let mut languages: Vec<_> = std::fs::read_dir(&root)
        .expect("crates/sue-lang/locales exists")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .collect();
    languages.sort_by_key(std::fs::DirEntry::file_name);
    for language in languages {
        let tag = language.file_name().to_string_lossy().into_owned();
        println!("cargo:rerun-if-changed={}", language.path().display());
        let mut namespaces: Vec<_> = std::fs::read_dir(language.path())
            .expect("a language folder can be read")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "ftl"))
            .collect();
        namespaces.sort();
        for path in namespaces {
            println!("cargo:rerun-if-changed={}", path.display());
            let namespace = path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_default();
            files.push((tag.clone(), namespace, path));
        }
    }
    let mut out =
        String::from("/// Every built-in `.ftl` file: language tag, namespace, source.\n");
    out.push_str("pub(crate) const BUILT_IN: &[(&str, &str, &str)] = &[\n");
    for (tag, namespace, path) in files {
        let path = path.to_string_lossy().into_owned();
        writeln!(out, "    ({tag:?}, {namespace:?}, include_str!({path:?})),")
            .expect("writing to a String");
    }
    out.push_str("];\n");
    let dest = Path::new(&std::env::var("OUT_DIR").expect("cargo sets OUT_DIR")).join("locales.rs");
    std::fs::write(dest, out).expect("OUT_DIR is writable");
}
