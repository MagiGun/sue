//! Optional packages: files Sue can use but does not ship (the hand
//! tracking models, and whatever comes later), installed and removed by
//! the person in Settings, Packages.
//!
//! A package is described by a small text manifest ([`OptionalPackage`],
//! format in `docs/packages.md`): who made it, its licence, and each of
//! its files with its size, its SHA-256 and where it comes from. The
//! manifest is a file people can write and share: one that names its
//! files by a path instead of a link installs from the folder it is in,
//! with no network.
//!
//! A [`PackageHome`] is the folder they are installed in, one folder per
//! package. Installing is all or nothing: every file is fetched into a
//! staging folder next to the home (as a `.part` file that a later try
//! carries on from), checked against its size and SHA-256, and only when
//! all of them are whole is the staging folder moved into place.
//! Cancelling removes the staging folder, so nothing half installed is
//! ever left; a failure (no network) keeps the parts for the next try.
//!
//! This module does no networking itself: a [`Fetch`] brings the bytes
//! (sue-platform's is HTTPS; tests give one that needs no network).

use std::fmt;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use semver::Version;
use sha2::{Digest, Sha256};

use crate::{ContentHash, PackageKind, PackageManifest, PkgError};

/// The name of the manifest inside an installed package's folder, and
/// the extension of a manifest file people share.
pub const MANIFEST: &str = "package.suepkg";
/// The manifest format this build writes and reads.
pub const FORMAT: u32 = 1;
/// The staging folder's name inside a package home.
const STAGING: &str = ".staging";

/// One file of a package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageFile {
    /// Its name in the package's folder: a plain file name.
    pub name: String,
    /// Its size in bytes.
    pub size: u64,
    /// Its SHA-256.
    pub hash: ContentHash,
    /// Where it comes from: an `https://` link, or a path relative to
    /// the manifest's own folder.
    pub source: String,
}

impl PackageFile {
    /// `true` when the file is fetched from the network.
    #[must_use]
    pub fn remote(&self) -> bool {
        self.source.starts_with("https://") || self.source.starts_with("http://")
    }
}

/// An optional package, as its manifest describes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OptionalPackage {
    /// Its id: `[a-z0-9][a-z0-9._-]*`, also its folder's name.
    pub id: String,
    /// What people read.
    pub name: String,
    /// Its version.
    pub version: Version,
    /// What it installs.
    pub kind: PackageKind,
    /// What it is for, in a sentence.
    pub summary: String,
    /// Its licence, as an SPDX expression (`Apache-2.0`).
    pub licence: String,
    /// Who made what is in it.
    pub author: String,
    /// Where it comes from: the project's page.
    pub homepage: String,
    /// Its files.
    pub files: Vec<PackageFile>,
}

fn kind_named(name: &str) -> Option<PackageKind> {
    [
        PackageKind::Applet,
        PackageKind::Codec,
        PackageKind::AiModel,
        PackageKind::RiceTheme,
        PackageKind::CoreUpdate,
    ]
    .into_iter()
    .find(|kind| kind.name() == name)
}

fn valid_id(id: &str) -> bool {
    let mut bytes = id.bytes();
    let first = bytes
        .next()
        .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit());
    first
        && id.len() <= 128
        && bytes.all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-')
        })
}

/// A plain file name: no folders, no drive, nothing hidden.
fn plain_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('.')
        && !name.contains(['/', '\\', ':'])
        && name != MANIFEST
        && !name.ends_with(".part")
}

impl OptionalPackage {
    /// The package a manifest's `text` describes.
    ///
    /// A manifest is lines of `key = value`; lines that are empty or
    /// start with `#` are passed over. A `file` line is
    /// `name | size | sha256 | source`, one per file. Keys this build
    /// does not know are passed over, so newer manifests still load.
    ///
    /// # Errors
    ///
    /// [`PkgError::Manifest`] saying which line is wrong or what is
    /// missing, [`PkgError::InvalidId`], [`PkgError::InvalidVersion`] and
    /// [`PkgError::InvalidHex`] for those values.
    pub fn parse(text: &str) -> Result<Self, PkgError> {
        let bad = |what: String| PkgError::Manifest(what);
        let mut format = None;
        let mut id = None;
        let mut name = None;
        let mut version = None;
        let mut kind = PackageKind::AiModel;
        let (mut summary, mut licence, mut author, mut homepage) =
            (String::new(), String::new(), String::new(), String::new());
        let mut files: Vec<PackageFile> = Vec::new();
        for (number, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(bad(format!("line {} is not `key = value`", number + 1)));
            };
            let (key, value) = (key.trim(), value.trim());
            match key {
                "format" => {
                    format = Some(value.parse::<u32>().map_err(|_| {
                        bad(format!("line {}: the format is not a number", number + 1))
                    })?);
                }
                "id" => id = Some(value.to_owned()),
                "name" => name = Some(value.to_owned()),
                "version" => {
                    version = Some(
                        Version::parse(value)
                            .map_err(|_| PkgError::InvalidVersion(value.to_owned()))?,
                    );
                }
                "kind" => {
                    kind = kind_named(value).ok_or_else(|| {
                        bad(format!(
                            "line {}: `{value}` is no kind of package",
                            number + 1
                        ))
                    })?;
                }
                "summary" => summary = value.to_owned(),
                "licence" | "license" => licence = value.to_owned(),
                "author" => author = value.to_owned(),
                "homepage" => homepage = value.to_owned(),
                "file" => {
                    let parts: Vec<&str> = value.split('|').map(str::trim).collect();
                    let [file, size, hash, source] = parts.as_slice() else {
                        return Err(bad(format!(
                            "line {}: a file is `name | size | sha256 | source`",
                            number + 1
                        )));
                    };
                    if !plain_name(file) || files.iter().any(|found| found.name == *file) {
                        return Err(bad(format!(
                            "line {}: `{file}` is not a plain file name of its own",
                            number + 1
                        )));
                    }
                    let size = size.parse::<u64>().map_err(|_| {
                        bad(format!(
                            "line {}: the size is not a number of bytes",
                            number + 1
                        ))
                    })?;
                    if source.is_empty() {
                        return Err(bad(format!("line {}: the file has no source", number + 1)));
                    }
                    files.push(PackageFile {
                        name: (*file).to_owned(),
                        size,
                        hash: ContentHash::from_hex(hash)?,
                        source: (*source).to_owned(),
                    });
                }
                _ => {}
            }
        }
        match format {
            Some(FORMAT) => {}
            Some(other) => {
                return Err(bad(format!(
                    "the manifest is format {other}; this Sue reads format {FORMAT}"
                )));
            }
            None => return Err(bad("the manifest has no `format`".to_owned())),
        }
        let id = id.ok_or_else(|| bad("the manifest has no `id`".to_owned()))?;
        if !valid_id(&id) {
            return Err(PkgError::InvalidId(id));
        }
        if files.is_empty() {
            return Err(bad("the manifest lists no `file`".to_owned()));
        }
        Ok(Self {
            name: name
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| id.clone()),
            id,
            version: version.ok_or_else(|| bad("the manifest has no `version`".to_owned()))?,
            kind,
            summary,
            licence,
            author,
            homepage,
            files,
        })
    }

    /// The manifest's text, which [`Self::parse`] reads back.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = format!(
            "format = {FORMAT}\nid = {}\nname = {}\nversion = {}\nkind = {}\n",
            self.id, self.name, self.version, self.kind
        );
        for (key, value) in [
            ("summary", &self.summary),
            ("licence", &self.licence),
            ("author", &self.author),
            ("homepage", &self.homepage),
        ] {
            if !value.is_empty() {
                out.push_str(&format!("{key} = {value}\n"));
            }
        }
        for file in &self.files {
            out.push_str(&format!(
                "file = {} | {} | {} | {}\n",
                file.name,
                file.size,
                file.hash.to_hex(),
                file.source
            ));
        }
        out
    }

    /// The bytes of all its files.
    #[must_use]
    pub fn size(&self) -> u64 {
        self.files.iter().map(|file| file.size).sum()
    }

    /// `true` when any of its files comes from the network.
    #[must_use]
    pub fn remote(&self) -> bool {
        self.files.iter().any(PackageFile::remote)
    }

    /// The package as the resolver and the updater know packages: its
    /// payload is the hash of its files' hashes, in order.
    ///
    /// # Errors
    ///
    /// As [`PackageManifest::new`].
    pub fn manifest(&self) -> Result<PackageManifest, PkgError> {
        let mut all = Vec::with_capacity(self.files.len() * 32);
        for file in &self.files {
            all.extend_from_slice(file.hash.to_hex().as_bytes());
        }
        let mut manifest =
            PackageManifest::new(&self.id, &self.version.to_string(), self.kind, &all)?
                .with_summary(self.summary.clone());
        manifest.payload_size = self.size();
        Ok(manifest)
    }
}

/// Why bytes could not be fetched.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FetchError {
    /// There is no way to the source now: no network, or it did not
    /// answer.
    Offline(String),
    /// The source answered, but not with the file.
    Failed(String),
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Offline(why) | Self::Failed(why) => f.write_str(why),
        }
    }
}

/// What a [`Fetch`] hands over as it goes.
#[derive(Debug)]
pub enum Chunk<'a> {
    /// The source could not carry on from where it was asked to: what
    /// follows is the file from its first byte.
    Restart,
    /// The next bytes of the file.
    Data(&'a [u8]),
}

/// Brings a file's bytes from a link.
pub trait Fetch: Send + Sync {
    /// Fetches `url` from byte `from` on, handing each piece to `sink`
    /// as it arrives; it stops (with `Ok`) as soon as `sink` answers
    /// `false`.
    ///
    /// # Errors
    ///
    /// [`FetchError`] when the bytes cannot be had.
    fn fetch(
        &self,
        url: &str,
        from: u64,
        sink: &mut dyn FnMut(Chunk<'_>) -> bool,
    ) -> Result<(), FetchError>;
}

/// A [`Fetch`] that refuses every link: for installing from a folder.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoNetwork;

impl Fetch for NoNetwork {
    fn fetch(
        &self,
        url: &str,
        _from: u64,
        _sink: &mut dyn FnMut(Chunk<'_>) -> bool,
    ) -> Result<(), FetchError> {
        Err(FetchError::Offline(format!("{url} needs the network")))
    }
}

/// How far an install is, shared between the thread that installs and
/// the interface that shows it and may cancel it.
#[derive(Debug, Default)]
pub struct Progress {
    done: AtomicU64,
    total: AtomicU64,
    cancelled: AtomicBool,
}

impl Progress {
    /// Bytes had so far, and of how many.
    #[must_use]
    pub fn bytes(&self) -> (u64, u64) {
        (
            self.done.load(Ordering::Relaxed),
            self.total.load(Ordering::Relaxed),
        )
    }

    /// Asks the install to stop; it does at its next piece.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    /// `true` once [`Self::cancel`] was called.
    #[must_use]
    pub fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

fn io(path: &Path) -> impl Fn(std::io::Error) -> PkgError + '_ {
    move |error| PkgError::Io(format!("{}: {error}", path.display()))
}

/// The folder optional packages are installed in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageHome {
    root: PathBuf,
}

impl PackageHome {
    /// The home at `root` (made when something is first installed).
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The home's folder.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The folder package `id` is, or would be, installed in.
    #[must_use]
    pub fn folder(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }

    fn staging(&self, id: &str) -> PathBuf {
        self.root.join(STAGING).join(id)
    }

    /// The installed package `id`, as its own manifest describes it;
    /// `None` when it is not installed, or a file of it is missing or of
    /// the wrong size.
    #[must_use]
    pub fn installed(&self, id: &str) -> Option<OptionalPackage> {
        if !valid_id(id) {
            return None;
        }
        let folder = self.folder(id);
        let package =
            OptionalPackage::parse(&fs::read_to_string(folder.join(MANIFEST)).ok()?).ok()?;
        let whole = package.id == id
            && package.files.iter().all(|file| {
                fs::metadata(folder.join(&file.name)).is_ok_and(|found| found.len() == file.size)
            });
        whole.then_some(package)
    }

    /// Every installed package, by id.
    #[must_use]
    pub fn all_installed(&self) -> Vec<OptionalPackage> {
        let Ok(entries) = fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut found: Vec<OptionalPackage> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| self.installed(entry.file_name().to_str()?))
            .collect();
        found.sort_by(|a, b| a.id.cmp(&b.id));
        found
    }

    /// `true` while a try at installing `id` left parts to carry on from.
    #[must_use]
    pub fn has_parts(&self, id: &str) -> bool {
        self.staging(id).is_dir()
    }

    /// Installs `package`: fetches every file (through `fetch`, or from
    /// `folder`, the manifest's own, for files named by a path), checks
    /// each against its size and SHA-256, and moves them into place
    /// together. `progress` hears how far it is and may cancel it.
    ///
    /// A cancelled install leaves nothing behind. A failed one keeps
    /// what it had, and the next try carries on from there; a file that
    /// turns out wrong is thrown away so the next try starts it anew.
    ///
    /// # Errors
    ///
    /// [`PkgError::Cancelled`], [`PkgError::Offline`] when the network
    /// cannot be reached, [`PkgError::Fetch`] when a source does not
    /// give the file, [`PkgError::HashMismatch`] and
    /// [`PkgError::SizeMismatch`] for a file that is not the one the
    /// manifest names, [`PkgError::Io`] when a file cannot be written.
    pub fn install(
        &self,
        package: &OptionalPackage,
        folder: Option<&Path>,
        fetch: &dyn Fetch,
        progress: &Progress,
    ) -> Result<(), PkgError> {
        if !valid_id(&package.id) {
            return Err(PkgError::InvalidId(package.id.clone()));
        }
        progress.total.store(package.size(), Ordering::Relaxed);
        progress.done.store(0, Ordering::Relaxed);
        let staging = self.staging(&package.id);
        let result = self.stage(package, folder, fetch, progress, &staging);
        if matches!(result, Err(PkgError::Cancelled)) {
            let _ = fs::remove_dir_all(&staging);
            let _ = fs::remove_dir(self.root.join(STAGING));
        }
        result
    }

    fn stage(
        &self,
        package: &OptionalPackage,
        folder: Option<&Path>,
        fetch: &dyn Fetch,
        progress: &Progress,
        staging: &Path,
    ) -> Result<(), PkgError> {
        fs::create_dir_all(staging).map_err(io(staging))?;
        let mut before = 0_u64;
        for file in &package.files {
            if progress.cancelled() {
                return Err(PkgError::Cancelled);
            }
            let whole = staging.join(&file.name);
            let had = fs::metadata(&whole).is_ok_and(|found| found.len() == file.size);
            if !had {
                self.bring(package, file, folder, fetch, progress, staging, before)?;
            }
            before += file.size;
            progress.done.store(before, Ordering::Relaxed);
        }
        if progress.cancelled() {
            return Err(PkgError::Cancelled);
        }
        let manifest = staging.join(MANIFEST);
        fs::write(&manifest, package.to_text()).map_err(io(&manifest))?;
        // Into place: the old version (if any) out, the new one in.
        let target = self.folder(&package.id);
        if target.exists() {
            fs::remove_dir_all(&target).map_err(io(&target))?;
        }
        fs::rename(staging, &target).map_err(io(&target))?;
        let _ = fs::remove_dir(self.root.join(STAGING));
        Ok(())
    }

    /// Brings one file into the staging folder, whole and checked.
    #[allow(clippy::too_many_arguments, reason = "one private step of install")]
    fn bring(
        &self,
        package: &OptionalPackage,
        file: &PackageFile,
        folder: Option<&Path>,
        fetch: &dyn Fetch,
        progress: &Progress,
        staging: &Path,
        before: u64,
    ) -> Result<(), PkgError> {
        let part = staging.join(format!("{}.part", file.name));
        // What an earlier try left, if it is not more than the file.
        let mut hasher = Sha256::new();
        let mut have = 0_u64;
        if let Ok(mut old) = fs::File::open(&part) {
            let mut buffer = vec![0_u8; 64 * 1024];
            loop {
                let read = old.read(&mut buffer).map_err(io(&part))?;
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
                have += read as u64;
            }
            if have > file.size {
                hasher = Sha256::new();
                have = 0;
            }
        }
        // Opened to write, not to append: a file opened to append cannot
        // be emptied when the source starts anew.
        let mut out = fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(have == 0)
            .open(&part)
            .map_err(io(&part))?;
        out.seek(SeekFrom::Start(have)).map_err(io(&part))?;
        progress.done.store(before + have, Ordering::Relaxed);

        let complete = have == file.size;
        let start = have;
        let mut failed: Option<PkgError> = None;
        let mut sink = |chunk: Chunk<'_>| -> bool {
            if progress.cancelled() {
                failed = Some(PkgError::Cancelled);
                return false;
            }
            let wrote = match chunk {
                Chunk::Restart => {
                    hasher = Sha256::new();
                    have = 0;
                    out.set_len(0)
                        .and_then(|()| out.seek(SeekFrom::Start(0)).map(|_| ()))
                        .map_err(io(&part))
                }
                Chunk::Data(bytes) => {
                    have += bytes.len() as u64;
                    if have > file.size {
                        Err(PkgError::SizeMismatch {
                            id: package.id.clone(),
                            expected: file.size,
                            actual: have,
                        })
                    } else {
                        hasher.update(bytes);
                        out.write_all(bytes).map_err(io(&part))
                    }
                }
            };
            progress
                .done
                .store(before + have.min(file.size), Ordering::Relaxed);
            match wrote {
                Ok(()) => true,
                Err(error) => {
                    failed = Some(error);
                    false
                }
            }
        };
        let fetched = if complete {
            Ok(())
        } else if file.remote() {
            fetch
                .fetch(&file.source, start, &mut sink)
                .map_err(|error| match error {
                    FetchError::Offline(why) => PkgError::Offline(why),
                    FetchError::Failed(why) => PkgError::Fetch(why),
                })
        } else {
            copy_local(file, folder, start, &mut sink)
        };
        let synced = out.sync_all().map_err(io(&part));
        drop(out);
        if let Some(error) = failed {
            // A file that grew past its size is not the one named: the
            // next try starts it anew.
            if matches!(error, PkgError::SizeMismatch { .. }) {
                let _ = fs::remove_file(&part);
            }
            return Err(error);
        }
        fetched?;
        synced?;
        if have != file.size {
            // The source stopped early: the part stays for the next try
            // unless it was the whole answer of a source that is wrong.
            return Err(PkgError::Fetch(format!(
                "{} ended after {have} of {} bytes",
                file.name, file.size
            )));
        }
        let digest: [u8; 32] = hasher.finalize().into();
        let actual = ContentHash::from_hex(
            &digest
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
        )?;
        if actual != file.hash {
            let _ = fs::remove_file(&part);
            return Err(PkgError::HashMismatch {
                id: package.id.clone(),
                expected: file.hash,
                actual,
            });
        }
        let whole = staging.join(&file.name);
        fs::rename(&part, &whole).map_err(io(&whole))
    }

    /// Removes package `id` and anything a try at installing it left.
    /// `false` when there was nothing to remove.
    ///
    /// # Errors
    ///
    /// [`PkgError::InvalidId`], and [`PkgError::Io`] when its folder
    /// cannot be removed.
    pub fn uninstall(&self, id: &str) -> Result<bool, PkgError> {
        if !valid_id(id) {
            return Err(PkgError::InvalidId(id.to_owned()));
        }
        let mut removed = false;
        for folder in [self.folder(id), self.staging(id)] {
            if folder.is_dir() {
                fs::remove_dir_all(&folder).map_err(io(&folder))?;
                removed = true;
            }
        }
        let _ = fs::remove_dir(self.root.join(STAGING));
        Ok(removed)
    }
}

/// Copies a file named by a path, from the manifest's folder.
fn copy_local(
    file: &PackageFile,
    folder: Option<&Path>,
    from: u64,
    sink: &mut dyn FnMut(Chunk<'_>) -> bool,
) -> Result<(), PkgError> {
    let relative = Path::new(&file.source);
    let inside = relative
        .components()
        .all(|part| matches!(part, std::path::Component::Normal(_)));
    let Some(folder) = folder.filter(|_| inside) else {
        return Err(PkgError::Fetch(format!(
            "{} is not a file next to the manifest",
            file.source
        )));
    };
    let path = folder.join(relative);
    let mut source = fs::File::open(&path).map_err(io(&path))?;
    if from > 0 && !sink(Chunk::Restart) {
        return Ok(());
    }
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = source.read(&mut buffer).map_err(io(&path))?;
        if read == 0 || !sink(Chunk::Data(&buffer[..read])) {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// A folder of this test's own, under the system's temporary one.
    fn scratch(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("sue-pkg-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        fs::create_dir_all(&folder).expect("a scratch folder");
        folder
    }

    fn bytes(seed: u8, length: usize) -> Vec<u8> {
        (0..length)
            .map(|at| (at as u8).wrapping_mul(31).wrapping_add(seed))
            .collect()
    }

    fn package(files: &[(&str, &[u8], &str)]) -> OptionalPackage {
        OptionalPackage {
            id: "hand-tracking".to_owned(),
            name: "Hand tracking".to_owned(),
            version: Version::new(1, 0, 0),
            kind: PackageKind::AiModel,
            summary: "Finds a hand in the camera's picture.".to_owned(),
            licence: "Apache-2.0".to_owned(),
            author: "Someone".to_owned(),
            homepage: "https://example.org/hands".to_owned(),
            files: files
                .iter()
                .map(|(name, content, source)| PackageFile {
                    name: (*name).to_owned(),
                    size: content.len() as u64,
                    hash: ContentHash::of(content),
                    source: (*source).to_owned(),
                })
                .collect(),
        }
    }

    /// Serves files from memory, in pieces of `piece` bytes; counts what
    /// it was asked, can cancel after some pieces, fail after some, and
    /// ignore where it was asked to start.
    #[derive(Default)]
    struct Served {
        files: Vec<(String, Vec<u8>)>,
        piece: usize,
        asked: Mutex<Vec<(String, u64)>>,
        fail_after: Option<usize>,
        cancel_after: Option<(usize, std::sync::Arc<Progress>)>,
        no_ranges: bool,
        offline: bool,
    }

    impl Fetch for Served {
        fn fetch(
            &self,
            url: &str,
            from: u64,
            sink: &mut dyn FnMut(Chunk<'_>) -> bool,
        ) -> Result<(), FetchError> {
            self.asked
                .lock()
                .expect("asked")
                .push((url.to_owned(), from));
            if self.offline {
                return Err(FetchError::Offline("no network".to_owned()));
            }
            let Some((_, content)) = self.files.iter().find(|(name, _)| name == url) else {
                return Err(FetchError::Failed(format!("{url}: 404")));
            };
            let mut from = from as usize;
            if self.no_ranges && from > 0 {
                from = 0;
                if !sink(Chunk::Restart) {
                    return Ok(());
                }
            }
            for (sent, piece) in content[from..].chunks(self.piece.max(1)).enumerate() {
                if self.fail_after == Some(sent) {
                    return Err(FetchError::Offline("the connection dropped".to_owned()));
                }
                if let Some((after, progress)) = &self.cancel_after
                    && *after == sent
                {
                    progress.cancel();
                }
                if !sink(Chunk::Data(piece)) {
                    return Ok(());
                }
            }
            Ok(())
        }
    }

    fn tree(folder: &Path) -> Vec<String> {
        let mut out = Vec::new();
        let mut walk = vec![folder.to_path_buf()];
        while let Some(next) = walk.pop() {
            for entry in fs::read_dir(&next).into_iter().flatten().flatten() {
                let path = entry.path();
                out.push(
                    path.strip_prefix(folder)
                        .expect("inside")
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
                if path.is_dir() {
                    walk.push(path);
                }
            }
        }
        out.sort();
        out
    }

    #[test]
    fn a_manifest_round_trips_through_its_text() {
        let made = package(&[
            ("a.onnx", &bytes(1, 10), "https://example.org/a.onnx"),
            ("b.onnx", &bytes(2, 20), "files/b.onnx"),
        ]);
        let text = made.to_text();
        assert_eq!(OptionalPackage::parse(&text).expect("parses"), made);
        assert_eq!(made.size(), 30);
        assert!(made.remote());
        // Comments, blank lines, unknown keys and the other spelling of
        // licence are all fine.
        let text = format!(
            "# A package\n\n{}future = thing\n",
            text.replace("licence =", "license =")
        );
        assert_eq!(OptionalPackage::parse(&text).expect("parses"), made);
        let manifest = made.manifest().expect("a manifest");
        assert_eq!(manifest.id, "hand-tracking");
        assert_eq!(manifest.payload_size, 30);
        assert_eq!(manifest.kind, PackageKind::AiModel);
    }

    #[test]
    fn a_wrong_manifest_says_what_is_wrong() {
        let good = package(&[("a.onnx", &bytes(1, 10), "a.onnx")]).to_text();
        let wrong = |text: String| OptionalPackage::parse(&text).unwrap_err();
        assert!(
            matches!(wrong(good.replace("format = 1", "format = 9")), PkgError::Manifest(why) if why.contains("format 9"))
        );
        assert!(matches!(
            wrong(good.replace("format = 1\n", "")),
            PkgError::Manifest(_)
        ));
        assert!(matches!(
            wrong(good.replace("id = hand-tracking", "id = ../up")),
            PkgError::InvalidId(_)
        ));
        assert!(matches!(
            wrong(good.replace("version = 1.0.0", "version = one")),
            PkgError::InvalidVersion(_)
        ));
        assert!(
            matches!(wrong(good.replace("file = a.onnx", "file = ../a.onnx")), PkgError::Manifest(why) if why.contains("plain file name"))
        );
        assert!(matches!(
            wrong(good.replace("file = a.onnx", "file = package.suepkg")),
            PkgError::Manifest(_)
        ));
        assert!(
            matches!(wrong(format!("{good}just words\n")), PkgError::Manifest(why) if why.contains("key = value"))
        );
        let no_file: String = good
            .lines()
            .filter(|line| !line.starts_with("file"))
            .map(|l| format!("{l}\n"))
            .collect();
        assert!(matches!(wrong(no_file), PkgError::Manifest(why) if why.contains("no `file`")));
        assert!(matches!(
            wrong(format!(
                "{good}file = a.onnx | 10 | {} | a.onnx\n",
                "0".repeat(64)
            )),
            PkgError::Manifest(_)
        ));
        assert!(matches!(
            wrong(format!("{good}file = b.onnx | 10 | xyz | b.onnx\n")),
            PkgError::InvalidHex(_)
        ));
    }

    #[test]
    fn installing_fetches_checks_and_moves_into_place() {
        let root = scratch("install");
        let home = PackageHome::new(&root);
        let (a, b) = (bytes(1, 1000), bytes(2, 300));
        let made = package(&[("a.onnx", &a, "https://x/a"), ("b.onnx", &b, "https://x/b")]);
        let served = Served {
            files: vec![
                ("https://x/a".to_owned(), a.clone()),
                ("https://x/b".to_owned(), b.clone()),
            ],
            piece: 128,
            ..Served::default()
        };
        assert!(home.installed("hand-tracking").is_none());
        assert!(home.all_installed().is_empty());
        let progress = Progress::default();
        home.install(&made, None, &served, &progress)
            .expect("installs");
        assert_eq!(progress.bytes(), (1300, 1300));
        assert_eq!(home.installed("hand-tracking"), Some(made.clone()));
        assert_eq!(home.all_installed(), vec![made.clone()]);
        assert_eq!(
            fs::read(home.folder("hand-tracking").join("a.onnx")).expect("a"),
            a
        );
        assert_eq!(
            tree(&root),
            [
                "hand-tracking",
                "hand-tracking/a.onnx",
                "hand-tracking/b.onnx",
                "hand-tracking/package.suepkg"
            ]
        );
        // A file gone missing: no longer installed.
        fs::remove_file(home.folder("hand-tracking").join("b.onnx")).expect("removes");
        assert!(home.installed("hand-tracking").is_none());
        // Installing again repairs it, and removing removes everything.
        home.install(&made, None, &served, &Progress::default())
            .expect("installs again");
        assert!(home.installed("hand-tracking").is_some());
        assert!(home.uninstall("hand-tracking").expect("removes"));
        assert!(!home.uninstall("hand-tracking").expect("nothing left"));
        assert!(tree(&root).is_empty());
        assert!(matches!(
            home.uninstall("../x"),
            Err(PkgError::InvalidId(_))
        ));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn cancelling_mid_download_leaves_nothing() {
        let root = scratch("cancel");
        let home = PackageHome::new(&root);
        let (a, b) = (bytes(1, 1000), bytes(2, 3000));
        let made = package(&[("a.onnx", &a, "https://x/a"), ("b.onnx", &b, "https://x/b")]);
        let progress = std::sync::Arc::new(Progress::default());
        let served = Served {
            files: vec![("https://x/a".to_owned(), a), ("https://x/b".to_owned(), b)],
            piece: 100,
            // Cancelled a few pieces into a file.
            cancel_after: Some((4, std::sync::Arc::clone(&progress))),
            ..Served::default()
        };
        assert_eq!(
            home.install(&made, None, &served, &progress),
            Err(PkgError::Cancelled)
        );
        assert!(progress.cancelled());
        assert!(tree(&root).is_empty(), "left {:?}", tree(&root));
        assert!(home.installed("hand-tracking").is_none());
        assert!(!home.has_parts("hand-tracking"));
        // An install cancelled before it starts fetches nothing at all.
        let before = served.asked.lock().expect("asked").len();
        assert_eq!(
            home.install(&made, None, &served, &progress),
            Err(PkgError::Cancelled)
        );
        assert_eq!(served.asked.lock().expect("asked").len(), before);
        assert!(tree(&root).is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_failed_download_is_carried_on_by_the_next_try() {
        let root = scratch("resume");
        let home = PackageHome::new(&root);
        let (a, b) = (bytes(1, 1000), bytes(2, 3000));
        let made = package(&[("a.onnx", &a, "https://x/a"), ("b.onnx", &b, "https://x/b")]);
        let files = vec![
            ("https://x/a".to_owned(), a),
            ("https://x/b".to_owned(), b.clone()),
        ];
        let dropping = Served {
            files: files.clone(),
            piece: 500,
            fail_after: Some(2),
            ..Served::default()
        };
        // The first file is whole (two pieces); the second drops after
        // two of its six.
        let failed = home.install(&made, None, &dropping, &Progress::default());
        assert!(matches!(failed, Err(PkgError::Offline(_))), "{failed:?}");
        assert!(home.installed("hand-tracking").is_none());
        assert!(home.has_parts("hand-tracking"));
        assert_eq!(
            tree(&root),
            [
                ".staging",
                ".staging/hand-tracking",
                ".staging/hand-tracking/a.onnx",
                ".staging/hand-tracking/b.onnx.part"
            ]
        );
        // The next try asks only for the rest of the second file.
        let working = Served {
            files: files.clone(),
            piece: 500,
            ..Served::default()
        };
        let progress = Progress::default();
        home.install(&made, None, &working, &progress)
            .expect("carries on");
        assert_eq!(
            *working.asked.lock().expect("asked"),
            [("https://x/b".to_owned(), 1000)]
        );
        assert_eq!(progress.bytes(), (4000, 4000));
        assert_eq!(
            fs::read(home.folder("hand-tracking").join("b.onnx")).expect("b"),
            b
        );
        assert!(!home.has_parts("hand-tracking"));

        // A source that cannot carry on sends the file from its start.
        home.uninstall("hand-tracking").expect("removes");
        let _ = home.install(&made, None, &dropping, &Progress::default());
        let whole = Served {
            files,
            piece: 500,
            no_ranges: true,
            ..Served::default()
        };
        home.install(&made, None, &whole, &Progress::default())
            .expect("starts anew");
        assert_eq!(
            fs::read(home.folder("hand-tracking").join("b.onnx")).expect("b"),
            b
        );
        // Removing also clears what a failed try left.
        let _ = home.install(&made, None, &dropping, &Progress::default());
        home.uninstall("hand-tracking").expect("removes");
        assert!(tree(&root).is_empty());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_file_that_is_not_the_one_named_is_refused() {
        let root = scratch("wrong");
        let home = PackageHome::new(&root);
        let a = bytes(1, 1000);
        let made = package(&[("a.onnx", &a, "https://x/a")]);
        // The same size, other bytes.
        let other = Served {
            files: vec![("https://x/a".to_owned(), bytes(9, 1000))],
            piece: 300,
            ..Served::default()
        };
        assert!(matches!(
            home.install(&made, None, &other, &Progress::default()),
            Err(PkgError::HashMismatch { .. })
        ));
        // Too long, and too short.
        let long = Served {
            files: vec![("https://x/a".to_owned(), bytes(1, 1200))],
            piece: 300,
            ..Served::default()
        };
        assert!(matches!(
            home.install(&made, None, &long, &Progress::default()),
            Err(PkgError::SizeMismatch { expected: 1000, .. })
        ));
        assert_eq!(
            tree(&root),
            [".staging", ".staging/hand-tracking"],
            "wrong files are thrown away"
        );
        let short = Served {
            files: vec![("https://x/a".to_owned(), bytes(1, 600))],
            piece: 300,
            ..Served::default()
        };
        assert!(matches!(
            home.install(&made, None, &short, &Progress::default()),
            Err(PkgError::Fetch(_))
        ));
        assert!(home.installed("hand-tracking").is_none());
        // Offline, and a link that is not there.
        let offline = Served {
            offline: true,
            ..Served::default()
        };
        assert!(matches!(
            home.install(&made, None, &offline, &Progress::default()),
            Err(PkgError::Offline(_))
        ));
        assert!(matches!(
            home.install(&made, None, &Served::default(), &Progress::default()),
            Err(PkgError::Fetch(_))
        ));
        assert!(matches!(
            home.install(&made, None, &NoNetwork, &Progress::default()),
            Err(PkgError::Offline(_))
        ));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_shared_package_installs_from_its_own_folder() {
        let root = scratch("local");
        let shared = root.join("shared");
        fs::create_dir_all(shared.join("files")).expect("folders");
        let (a, b) = (bytes(1, 70_000), bytes(2, 300));
        fs::write(shared.join("a.onnx"), &a).expect("a");
        fs::write(shared.join("files").join("b.onnx"), &b).expect("b");
        let made = package(&[("a.onnx", &a, "a.onnx"), ("b.onnx", &b, "files/b.onnx")]);
        assert!(!made.remote());
        let home = PackageHome::new(root.join("home"));
        home.install(&made, Some(&shared), &NoNetwork, &Progress::default())
            .expect("installs");
        assert_eq!(
            fs::read(home.folder("hand-tracking").join("a.onnx")).expect("a"),
            a
        );
        assert_eq!(home.installed("hand-tracking"), Some(made.clone()));
        // Without its folder, or with a path that leaves it, it cannot.
        home.uninstall("hand-tracking").expect("removes");
        assert!(matches!(
            home.install(&made, None, &NoNetwork, &Progress::default()),
            Err(PkgError::Fetch(_))
        ));
        let mut escaping = made;
        escaping.files[0].source = "../a.onnx".to_owned();
        assert!(matches!(
            home.install(
                &escaping,
                Some(&shared.join("files")),
                &NoNetwork,
                &Progress::default()
            ),
            Err(PkgError::Fetch(_))
        ));
        let _ = fs::remove_dir_all(&root);
    }
}
