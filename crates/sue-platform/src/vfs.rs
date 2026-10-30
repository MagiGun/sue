//! Files kept in memory, for the browser, which gives a page no file
//! system: settings, autosaves, opened and exported files all live here
//! under one root, and the parts that must survive a reload are copied
//! to the browser's storage (`web/store.rs`). The store itself is plain
//! Rust, so its behaviour is tested on every system.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use crate::time::SystemTime;

/// One file: its bytes and when they were written.
#[derive(Clone, Debug)]
pub(crate) struct Stored {
    pub(crate) bytes: Arc<[u8]>,
    pub(crate) modified: SystemTime,
}

/// What a path names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    File { len: u64 },
    Folder,
}

/// One entry of a folder: its name, path and what it is.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Entry {
    pub(crate) name: String,
    pub(crate) path: PathBuf,
    pub(crate) kind: Kind,
    pub(crate) modified: Option<SystemTime>,
}

/// The files and folders. Folders exist when made or when a file is
/// inside them, as on a disk.
#[derive(Debug, Default)]
pub(crate) struct Vfs {
    files: BTreeMap<PathBuf, Stored>,
    folders: BTreeSet<PathBuf>,
}

/// `path` with `.` and `..` resolved and made absolute, so one file has
/// one name.
pub(crate) fn normal(path: &Path) -> PathBuf {
    let mut out = PathBuf::from("/");
    for component in path.components() {
        match component {
            Component::Normal(part) => out.push(part),
            Component::ParentDir => {
                out.pop();
            }
            Component::RootDir | Component::CurDir | Component::Prefix(_) => {}
        }
    }
    out
}

fn not_found(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::NotFound,
        format!("{} does not exist", path.display()),
    )
}

impl Vfs {
    /// What `path` names, if anything.
    pub(crate) fn kind(&self, path: &Path) -> Option<Kind> {
        let path = normal(path);
        if let Some(file) = self.files.get(&path) {
            return Some(Kind::File {
                len: file.bytes.len() as u64,
            });
        }
        (path == Path::new("/")
            || self.folders.contains(&path)
            || self.files.keys().any(|file| file.starts_with(&path)))
        .then_some(Kind::Folder)
    }

    /// The file at `path`.
    pub(crate) fn get(&self, path: &Path) -> Option<&Stored> {
        self.files.get(&normal(path))
    }

    /// The bytes of the file at `path`.
    pub(crate) fn read(&self, path: &Path) -> io::Result<Vec<u8>> {
        match self.kind(path) {
            Some(Kind::File { .. }) => Ok(self
                .get(path)
                .map(|file| file.bytes.to_vec())
                .unwrap_or_default()),
            Some(Kind::Folder) => Err(io::Error::new(
                io::ErrorKind::IsADirectory,
                format!("{} is a folder", path.display()),
            )),
            None => Err(not_found(path)),
        }
    }

    /// Writes `bytes` to `path`, making its folders. Returns the path's
    /// normal form.
    pub(crate) fn write(
        &mut self,
        path: &Path,
        bytes: &[u8],
        modified: SystemTime,
    ) -> io::Result<PathBuf> {
        let path = normal(path);
        if path == Path::new("/") || self.folders.contains(&path) {
            return Err(io::Error::new(
                io::ErrorKind::IsADirectory,
                format!("{} is a folder", path.display()),
            ));
        }
        if let Some(parent) = path.parent() {
            self.create_dir_all(parent);
        }
        self.files.insert(
            path.clone(),
            Stored {
                bytes: Arc::from(bytes),
                modified,
            },
        );
        Ok(path)
    }

    /// Makes the folder `path` and every folder above it.
    pub(crate) fn create_dir_all(&mut self, path: &Path) {
        let mut path = normal(path);
        while path != Path::new("/") && self.folders.insert(path.clone()) {
            path.pop();
        }
    }

    /// Removes the file at `path`.
    pub(crate) fn remove_file(&mut self, path: &Path) -> io::Result<()> {
        self.files
            .remove(&normal(path))
            .map(|_| ())
            .ok_or_else(|| not_found(path))
    }

    /// Removes the folder `path` and everything in it. Returns the files
    /// it held.
    pub(crate) fn remove_dir_all(&mut self, path: &Path) -> io::Result<Vec<PathBuf>> {
        if self.kind(path) != Some(Kind::Folder) {
            return Err(not_found(path));
        }
        let path = normal(path);
        let gone: Vec<PathBuf> = self
            .files
            .keys()
            .filter(|file| file.starts_with(&path))
            .cloned()
            .collect();
        for file in &gone {
            self.files.remove(file);
        }
        self.folders.retain(|folder| !folder.starts_with(&path));
        Ok(gone)
    }

    /// Moves the file at `from` to `to`.
    pub(crate) fn rename(&mut self, from: &Path, to: &Path) -> io::Result<PathBuf> {
        let file = self
            .files
            .remove(&normal(from))
            .ok_or_else(|| not_found(from))?;
        self.write(to, &file.bytes, file.modified)
    }

    /// What is right inside the folder `dir`.
    pub(crate) fn list(&self, dir: &Path) -> io::Result<Vec<Entry>> {
        if self.kind(dir) != Some(Kind::Folder) {
            return Err(not_found(dir));
        }
        let dir = normal(dir);
        let mut entries: BTreeMap<PathBuf, Entry> = BTreeMap::new();
        let child = |path: &Path| -> Option<PathBuf> {
            let rest = path.strip_prefix(&dir).ok()?;
            let first = rest.components().next()?;
            Some(dir.join(first))
        };
        for folder in &self.folders {
            if let Some(path) = child(folder) {
                entries.entry(path.clone()).or_insert_with(|| Entry {
                    name: name_of(&path),
                    path,
                    kind: Kind::Folder,
                    modified: None,
                });
            }
        }
        for (file, stored) in &self.files {
            let Some(path) = child(file) else {
                continue;
            };
            if &path == file {
                entries.insert(
                    path.clone(),
                    Entry {
                        name: name_of(&path),
                        path,
                        kind: Kind::File {
                            len: stored.bytes.len() as u64,
                        },
                        modified: Some(stored.modified),
                    },
                );
            } else {
                entries.entry(path.clone()).or_insert_with(|| Entry {
                    name: name_of(&path),
                    path,
                    kind: Kind::Folder,
                    modified: None,
                });
            }
        }
        Ok(entries.into_values().collect())
    }
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> SystemTime {
        SystemTime::now()
    }

    #[test]
    fn files_are_written_read_and_named_one_way() {
        let mut vfs = Vfs::default();
        let path = vfs
            .write(
                Path::new("/sue/config/./rice/../rice/default.suerice"),
                b"gap = 8\n",
                now(),
            )
            .unwrap();
        assert_eq!(path, Path::new("/sue/config/rice/default.suerice"));
        assert_eq!(vfs.read(&path).unwrap(), b"gap = 8\n");
        assert_eq!(vfs.kind(&path), Some(Kind::File { len: 8 }));
        assert_eq!(vfs.kind(Path::new("/sue/config")), Some(Kind::Folder));
        assert_eq!(vfs.kind(Path::new("/sue/other")), None);
        assert!(vfs.read(Path::new("/sue/config")).is_err());
        assert!(vfs.read(Path::new("/nothing")).is_err());
        assert!(vfs.write(Path::new("/sue/config"), b"x", now()).is_err());
    }

    #[test]
    fn folders_list_their_children_and_go_away_whole() {
        let mut vfs = Vfs::default();
        vfs.write(Path::new("/sue/a.txt"), b"a", now()).unwrap();
        vfs.write(Path::new("/sue/deep/b.txt"), b"bb", now())
            .unwrap();
        vfs.create_dir_all(Path::new("/sue/empty"));
        let names: Vec<(String, Kind)> = vfs
            .list(Path::new("/sue"))
            .unwrap()
            .into_iter()
            .map(|entry| (entry.name, entry.kind))
            .collect();
        assert_eq!(
            names,
            [
                ("a.txt".to_owned(), Kind::File { len: 1 }),
                ("deep".to_owned(), Kind::Folder),
                ("empty".to_owned(), Kind::Folder),
            ]
        );
        assert_eq!(
            vfs.remove_dir_all(Path::new("/sue/deep")).unwrap(),
            [PathBuf::from("/sue/deep/b.txt")]
        );
        assert_eq!(vfs.kind(Path::new("/sue/deep")), None);
        vfs.rename(Path::new("/sue/a.txt"), Path::new("/sue/empty/a.txt"))
            .unwrap();
        assert_eq!(vfs.read(Path::new("/sue/empty/a.txt")).unwrap(), b"a");
        assert!(vfs.remove_file(Path::new("/sue/a.txt")).is_err());
        assert!(vfs.list(Path::new("/sue/none")).is_err());
    }
}
