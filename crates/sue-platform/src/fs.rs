//! Files: crash-safe writes, directory listings, and the file calls
//! everything above the platform makes. On computers, phones and tablets
//! these are std's own; in the browser, which has no file system, they
//! reach the page's files (kept in memory and in IndexedDB).

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::time::SystemTime;

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
    let mut entries = listing(dir, show_hidden)?;
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok(entries)
}

#[cfg(target_arch = "wasm32")]
fn listing(dir: &Path, show_hidden: bool) -> io::Result<Vec<DirEntryInfo>> {
    use crate::vfs::Kind;
    Ok(crate::web::store::with(|vfs| vfs.list(dir))?
        .into_iter()
        .filter(|entry| show_hidden || !hidden_by_os(&entry.name))
        .map(|entry| DirEntryInfo {
            name: entry.name,
            path: entry.path,
            is_dir: entry.kind == Kind::Folder,
            size: match entry.kind {
                Kind::File { len } => len,
                Kind::Folder => 0,
            },
            modified: entry.modified,
        })
        .collect())
}

#[cfg(not(target_arch = "wasm32"))]
fn listing(dir: &Path, show_hidden: bool) -> io::Result<Vec<DirEntryInfo>> {
    Ok(fs::read_dir(dir)?
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
        .collect())
}

/// Writes `bytes` to `path` atomically: a temporary file in the same directory
/// is written, flushed to disk and renamed over `path`. A crash leaves either
/// the old file or the new one, never a torn one. Parent directories are
/// created as needed.
///
/// # Errors
/// Any I/O error, or [`io::ErrorKind::InvalidInput`] if `path` has no file name.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    if cfg!(target_arch = "wasm32") {
        // The page's files are written whole or not at all.
        return write(path, bytes);
    }
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

/// What [`metadata`] says about a path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Metadata {
    is_dir: bool,
    len: u64,
    modified: Option<SystemTime>,
}

impl Metadata {
    /// `true` for a folder.
    #[must_use]
    pub fn is_dir(&self) -> bool {
        self.is_dir
    }

    /// `true` for a file.
    #[must_use]
    pub fn is_file(&self) -> bool {
        !self.is_dir
    }

    /// The size in bytes (0 for a folder).
    #[must_use]
    pub fn len(&self) -> u64 {
        self.len
    }

    /// `true` for an empty file or a folder.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// When the file was last written, if known.
    #[must_use]
    pub fn modified(&self) -> Option<SystemTime> {
        self.modified
    }
}

/// What is at `path`, following links.
///
/// # Errors
/// If nothing is there or it cannot be read.
pub fn metadata(path: impl AsRef<Path>) -> io::Result<Metadata> {
    let path = path.as_ref();
    #[cfg(target_arch = "wasm32")]
    {
        use crate::vfs::Kind;
        crate::web::store::with(|vfs| {
            let modified = vfs.get(path).map(|file| file.modified);
            match vfs.kind(path) {
                Some(Kind::File { len }) => Ok(Metadata {
                    is_dir: false,
                    len,
                    modified,
                }),
                Some(Kind::Folder) => Ok(Metadata {
                    is_dir: true,
                    len: 0,
                    modified: None,
                }),
                None => Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("{} does not exist", path.display()),
                )),
            }
        })
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let metadata = fs::metadata(path)?;
        Ok(Metadata {
            is_dir: metadata.is_dir(),
            len: if metadata.is_dir() { 0 } else { metadata.len() },
            modified: metadata.modified().ok(),
        })
    }
}

/// `true` if anything is at `path`.
#[must_use]
pub fn exists(path: impl AsRef<Path>) -> bool {
    metadata(path).is_ok()
}

/// `true` if a file is at `path`.
#[must_use]
pub fn is_file(path: impl AsRef<Path>) -> bool {
    metadata(path).is_ok_and(|metadata| metadata.is_file())
}

/// `true` if a folder is at `path`.
#[must_use]
pub fn is_dir(path: impl AsRef<Path>) -> bool {
    metadata(path).is_ok_and(|metadata| metadata.is_dir())
}

/// The bytes of the file at `path`.
///
/// # Errors
/// If it is missing, a folder, or cannot be read.
pub fn read(path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
    let path = path.as_ref();
    #[cfg(target_arch = "wasm32")]
    {
        crate::web::store::with(|vfs| vfs.read(path))
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        fs::read(path)
    }
}

/// The text of the UTF-8 file at `path`.
///
/// # Errors
/// As [`read`], and [`io::ErrorKind::InvalidData`] for text that is not
/// UTF-8.
pub fn read_to_string(path: impl AsRef<Path>) -> io::Result<String> {
    String::from_utf8(read(path)?)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

/// Writes `bytes` to the file at `path`, replacing it. On disk the
/// folder must exist; in the browser it is made.
///
/// # Errors
/// Any I/O error.
pub fn write(path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> io::Result<()> {
    let (path, bytes) = (path.as_ref(), bytes.as_ref());
    #[cfg(target_arch = "wasm32")]
    {
        crate::web::store::write(path, bytes)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        fs::write(path, bytes)
    }
}

/// Makes the folder `path` and every folder above it.
///
/// # Errors
/// Any I/O error.
pub fn create_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
    let path = path.as_ref();
    #[cfg(target_arch = "wasm32")]
    {
        crate::web::store::with(|vfs| vfs.create_dir_all(path));
        Ok(())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        fs::create_dir_all(path)
    }
}

/// Removes the file at `path`.
///
/// # Errors
/// If it is missing or cannot be removed.
pub fn remove_file(path: impl AsRef<Path>) -> io::Result<()> {
    let path = path.as_ref();
    #[cfg(target_arch = "wasm32")]
    {
        crate::web::store::remove_file(path)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        fs::remove_file(path)
    }
}

/// Removes the folder `path` and everything in it.
///
/// # Errors
/// If it is missing or cannot be removed.
pub fn remove_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
    let path = path.as_ref();
    #[cfg(target_arch = "wasm32")]
    {
        crate::web::store::remove_dir_all(path)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        fs::remove_dir_all(path)
    }
}

/// Moves the file at `from` to `to`, replacing it.
///
/// # Errors
/// If `from` is missing or the move fails.
pub fn rename(from: impl AsRef<Path>, to: impl AsRef<Path>) -> io::Result<()> {
    let (from, to) = (from.as_ref(), to.as_ref());
    #[cfg(target_arch = "wasm32")]
    {
        crate::web::store::rename(from, to)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        fs::rename(from, to)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_portable_calls_reach_the_files() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let dir = std::env::temp_dir().join(format!("sue-io-test-{}-{unique}", std::process::id()));
        let file = dir.join("deep").join("a.txt");
        create_dir_all(file.parent().unwrap()).unwrap();
        write(&file, b"words").unwrap();
        assert!(exists(&file) && is_file(&file) && is_dir(&dir));
        assert_eq!(metadata(&file).unwrap().len(), 5);
        assert!(metadata(&dir).unwrap().is_dir());
        assert_eq!(read_to_string(&file).unwrap(), "words");
        let moved = dir.join("b.txt");
        rename(&file, &moved).unwrap();
        assert_eq!(read(&moved).unwrap(), b"words");
        remove_file(&moved).unwrap();
        assert!(!exists(&moved));
        remove_dir_all(&dir).unwrap();
        assert!(metadata(&dir).is_err());
    }

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
