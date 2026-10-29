//! Crash-safe file writes and directory listings.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// One entry of a directory listing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirEntryInfo {
    /// File or folder name.
    pub name: String,
    /// Full path.
    pub path: PathBuf,
    /// `true` for folders (and links to folders).
    pub is_dir: bool,
    /// Size in bytes (0 for folders).
    pub size: u64,
    /// Last modification time, if the OS reports one.
    pub modified: Option<SystemTime>,
}

/// Compatibility junctions Windows keeps in every profile for programs from
/// before Vista. They cannot be opened, so they are never listed unless hidden
/// entries are shown.
const LEGACY_JUNCTIONS: [&str; 10] = [
    "Application Data",
    "Cookies",
    "Local Settings",
    "My Documents",
    "NetHood",
    "PrintHood",
    "Recent",
    "SendTo",
    "Start Menu",
    "Templates",
];

/// `true` for entries the OS hides from people: dot files everywhere, and
/// the legacy junctions on Windows.
#[must_use]
pub fn hidden_by_os(name: &str) -> bool {
    name.starts_with('.') || (cfg!(windows) && LEGACY_JUNCTIONS.contains(&name))
}

/// Lists `dir`: folders first, then files, each sorted by name without
/// regard to case. Names starting with a dot are skipped unless
/// `show_hidden`. Entries that vanish or cannot be read while listing are
/// left out.
///
/// # Errors
/// If `dir` itself cannot be read.
pub fn list_dir(dir: &Path, show_hidden: bool) -> io::Result<Vec<DirEntryInfo>> {
    let mut entries: Vec<DirEntryInfo> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !show_hidden && hidden_by_os(&name) {
                return None;
            }
            let path = entry.path();
            // `metadata` follows links, so a link to a folder lists as one.
            let metadata = fs::metadata(&path).ok()?;
            Some(DirEntryInfo {
                name,
                path,
                is_dir: metadata.is_dir(),
                size: if metadata.is_dir() { 0 } else { metadata.len() },
                modified: metadata.modified().ok(),
            })
        })
        .collect();
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(entries)
}

/// Writes `bytes` to `path` atomically: a temporary file in the same directory
/// is written, flushed to disk and renamed over `path`. A crash leaves either
/// the old file or the new one, never a torn one. Parent directories are
/// created as needed.
///
/// # Errors
/// Any I/O error, or [`io::ErrorKind::InvalidInput`] if `path` has no file name.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let file_name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?;
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    let temporary = path.with_file_name(format!(
        ".{}.{}.tmp",
        file_name.to_string_lossy(),
        std::process::id()
    ));
    let result = (|| {
        let mut file = fs::File::create(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    if result.is_err() {
        // Best effort: the temporary file is garbage either way.
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_replace_files_atomically() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let dir = std::env::temp_dir().join(format!("sue-fs-test-{}-{unique}", std::process::id()));
        let file = dir.join("nested").join("default.suerice");
        write_atomic(&file, b"gap = 8\n").unwrap();
        write_atomic(&file, b"gap = 12\n").unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"gap = 12\n");
        let entries = fs::read_dir(file.parent().unwrap()).unwrap().count();
        fs::remove_dir_all(&dir).unwrap();
        assert_eq!(entries, 1, "no temporary files are left behind");
        assert!(write_atomic(Path::new(""), b"").is_err());
    }

    #[test]
    fn listings_put_folders_first_and_hide_dotfiles() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let dir = std::env::temp_dir().join(format!("sue-ls-test-{}-{unique}", std::process::id()));
        fs::create_dir_all(dir.join("Zeta")).unwrap();
        fs::create_dir_all(dir.join("alpha")).unwrap();
        fs::write(dir.join("b.txt"), b"hello").unwrap();
        fs::write(dir.join("A.jxl"), b"x").unwrap();
        fs::write(dir.join(".hidden"), b"").unwrap();
        let names = |show_hidden| -> Vec<String> {
            list_dir(&dir, show_hidden)
                .unwrap()
                .into_iter()
                .map(|entry| entry.name)
                .collect()
        };
        assert_eq!(names(false), ["alpha", "Zeta", "A.jxl", "b.txt"]);
        assert_eq!(names(true).len(), 5);
        let listing = list_dir(&dir, false).unwrap();
        assert!(listing[0].is_dir && listing[0].size == 0);
        assert_eq!(listing[3].size, 5);
        fs::remove_dir_all(&dir).unwrap();
        assert!(list_dir(&dir, false).is_err());
    }
}
