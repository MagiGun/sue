//! Optional packages on this machine: what Settings, Packages lists, and
//! what installs, cancels and removes them. The packages themselves, their
//! manifests and the all or nothing install are `sue-pkg`'s
//! ([`OptionalPackage`], [`PackageHome`]); here are the two things that
//! need the system: the network ([`HttpsFetch`], through `ureq`) and the
//! thread an install runs on (a `sue-core` [`JobPool`]), so the interface
//! never waits.
//!
//! A [`PackageCenter`] knows the packages Sue names itself
//! ([`BUILT_IN`], manifests kept as files in this crate) and the ones a
//! person added from a manifest file, which it remembers in the package
//! home's `catalog` folder. Everyone shares the one center
//! ([`PackageCenter::at`]): an install started in a dialog shows its
//! progress in Settings too, and either place can cancel it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use sue_core::{JobHandle, JobPool};
pub use sue_pkg::{
    Chunk, Fetch, FetchError, NoNetwork, OptionalPackage, PackageFile, PackageHome, PkgError,
    Progress,
};

/// The manifests of the packages Sue names itself.
pub const BUILT_IN: [&str; 1] = [include_str!("../packages/hand-tracking.suepkg")];

/// The id of the package that holds the Hand Scanner's models.
pub const HAND_TRACKING: &str = "hand-tracking";

/// The folder, inside the package home, that keeps the manifests people
/// added from files.
const CATALOG: &str = "catalog";

/// Where a package stands.
#[derive(Clone, Debug, PartialEq)]
pub enum PackageState {
    /// Not installed.
    Available,
    /// Being installed.
    Installing {
        /// Bytes had so far.
        done: u64,
        /// Of how many.
        total: u64,
        /// Bytes a second, lately.
        speed: f64,
    },
    /// Installed and whole.
    Installed,
    /// The last install failed: why. It can be tried again, and carries
    /// on from what it had.
    Failed(PkgError),
}

/// An install that runs.
struct Running {
    progress: Arc<Progress>,
    job: JobHandle<Result<(), PkgError>>,
    /// When the speed was last measured, the bytes had then, and the
    /// speed.
    measured: (Instant, u64, f64),
}

/// A package the center knows, and the folder its manifest came from
/// (for packages installed from a file).
#[derive(Clone)]
struct Known {
    package: OptionalPackage,
    folder: Option<PathBuf>,
}

/// The optional packages of this machine. See the module.
pub struct PackageCenter {
    home: PackageHome,
    fetch: Arc<dyn Fetch>,
    pool: JobPool,
    known: Mutex<Vec<Known>>,
    running: Mutex<HashMap<String, Running>>,
    failed: Mutex<HashMap<String, PkgError>>,
}

impl std::fmt::Debug for PackageCenter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PackageCenter")
            .field("home", &self.home)
            .finish_non_exhaustive()
    }
}

impl PackageCenter {
    /// A center over the package home at `root`, fetching with `fetch`.
    /// It knows the [`BUILT_IN`] packages and the ones added from files
    /// before.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>, fetch: Arc<dyn Fetch>) -> Self {
        let home = PackageHome::new(root);
        let mut known: Vec<Known> = BUILT_IN
            .iter()
            .filter_map(|text| OptionalPackage::parse(text).ok())
            .map(|package| Known {
                package,
                folder: None,
            })
            .collect();
        // Those added from files: remembered by their manifests, and
        // installed ones by their own folder's.
        let catalog = home.root().join(CATALOG);
        let remembered = std::fs::read_dir(&catalog)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| std::fs::read_to_string(entry.path()).ok())
            .filter_map(|text| OptionalPackage::parse(&text).ok());
        for package in remembered.chain(home.all_installed()) {
            if !known.iter().any(|found| found.package.id == package.id) {
                known.push(Known {
                    package,
                    folder: None,
                });
            }
        }
        Self {
            home,
            fetch,
            pool: JobPool::new(1),
            known: Mutex::new(known),
            running: Mutex::new(HashMap::new()),
            failed: Mutex::new(HashMap::new()),
        }
    }

    /// The center over this machine's own package folder
    /// ([`crate::PlatformPaths::packages_dir`]).
    #[must_use]
    pub fn shared() -> Arc<Self> {
        let root = crate::PlatformPaths::detect().map_or_else(
            |_| std::env::temp_dir().join("sue-packages"),
            |paths| paths.packages_dir(),
        );
        Self::at(&root)
    }

    /// The center over the package home at `root`, shared by everyone
    /// who asks for that folder: made (fetching over HTTPS) the first
    /// time, the same one after. Settings and the studios reach the
    /// center this way, with the packages folder Sue runs with.
    #[must_use]
    pub fn at(root: &Path) -> Arc<Self> {
        let mut centers = centers();
        if let Some(found) = centers.iter().find(|center| center.home.root() == root) {
            return Arc::clone(found);
        }
        let agent = format!("Sue/{}", env!("CARGO_PKG_VERSION"));
        let center = Arc::new(Self::new(root, Arc::new(HttpsFetch::new(agent))));
        centers.push(Arc::clone(&center));
        center
    }

    /// Makes `center` the one [`Self::at`] gives for its folder (tests
    /// put one there that fetches without a network).
    pub fn put(center: Arc<Self>) {
        let mut centers = centers();
        centers.retain(|found| found.home.root() != center.home.root());
        centers.push(center);
    }

    /// The home the packages are installed in.
    #[must_use]
    pub fn home(&self) -> &PackageHome {
        &self.home
    }

    /// Every package the center knows, Sue's own first.
    #[must_use]
    pub fn packages(&self) -> Vec<OptionalPackage> {
        self.known
            .lock()
            .map(|known| known.iter().map(|found| found.package.clone()).collect())
            .unwrap_or_default()
    }

    /// The package `id`, if the center knows it.
    #[must_use]
    pub fn package(&self, id: &str) -> Option<OptionalPackage> {
        self.packages().into_iter().find(|package| package.id == id)
    }

    /// `true` for a package Sue names itself (it stays in the list when
    /// removed).
    #[must_use]
    pub fn built_in(&self, id: &str) -> bool {
        BUILT_IN
            .iter()
            .filter_map(|text| OptionalPackage::parse(text).ok())
            .any(|package| package.id == id)
    }

    /// The folder package `id` is installed in, when it is whole.
    #[must_use]
    pub fn folder(&self, id: &str) -> Option<PathBuf> {
        self.home.installed(id).map(|_| self.home.folder(id))
    }

    /// Where package `id` stands now. Reading it also takes in an
    /// install that just ended.
    #[must_use]
    pub fn state(&self, id: &str) -> PackageState {
        if let Ok(mut running) = self.running.lock()
            && let Some(run) = running.get_mut(id)
        {
            match run.job.try_take() {
                None => {
                    let (done, total) = run.progress.bytes();
                    let (at, had, speed) = run.measured;
                    let passed = at.elapsed().as_secs_f64();
                    if passed >= 0.25 {
                        let now = done.saturating_sub(had) as f64 / passed;
                        // Half the old speed and half the new: steady
                        // enough to read.
                        let speed = if speed > 0.0 {
                            (speed + now) / 2.0
                        } else {
                            now
                        };
                        run.measured = (Instant::now(), done, speed);
                    }
                    return PackageState::Installing {
                        done,
                        total,
                        speed: run.measured.2,
                    };
                }
                Some(outcome) => {
                    running.remove(id);
                    let error = match outcome {
                        Ok(Ok(())) | Ok(Err(PkgError::Cancelled)) => None,
                        Ok(Err(error)) => Some(error),
                        Err(sue_core::JobError::Cancelled) => None,
                        Err(error) => Some(PkgError::Io(error.to_string())),
                    };
                    if let (Some(error), Ok(mut failed)) = (error, self.failed.lock()) {
                        failed.insert(id.to_owned(), error);
                    }
                }
            }
        }
        if self.home.installed(id).is_some() {
            return PackageState::Installed;
        }
        match self
            .failed
            .lock()
            .ok()
            .and_then(|failed| failed.get(id).cloned())
        {
            Some(error) => PackageState::Failed(error),
            None => PackageState::Available,
        }
    }

    /// `true` while any install runs, so the interface keeps drawing its
    /// progress.
    #[must_use]
    pub fn busy(&self) -> bool {
        self.running.lock().is_ok_and(|running| !running.is_empty())
    }

    /// Starts installing package `id` in the background, unless it is
    /// being installed already. `false` when the center does not know
    /// the package.
    pub fn install(&self, id: &str) -> bool {
        let Some(known) = self
            .known
            .lock()
            .ok()
            .and_then(|known| known.iter().find(|found| found.package.id == id).cloned())
        else {
            return false;
        };
        let Ok(mut running) = self.running.lock() else {
            return false;
        };
        if running.contains_key(id) {
            return true;
        }
        if let Ok(mut failed) = self.failed.lock() {
            failed.remove(id);
        }
        let progress = Arc::new(Progress::default());
        let (home, fetch, shared) = (
            self.home.clone(),
            Arc::clone(&self.fetch),
            Arc::clone(&progress),
        );
        let job = self.pool.spawn(format!("install {id}"), move |_| {
            Ok(home.install(
                &known.package,
                known.folder.as_deref(),
                fetch.as_ref(),
                &shared,
            ))
        });
        running.insert(
            id.to_owned(),
            Running {
                progress,
                job,
                measured: (Instant::now(), 0, 0.0),
            },
        );
        true
    }

    /// Cancels the install of package `id`, if one runs: it stops at its
    /// next piece and leaves nothing behind.
    pub fn cancel(&self, id: &str) {
        if let Ok(running) = self.running.lock()
            && let Some(run) = running.get(id)
        {
            run.progress.cancel();
            run.job.cancel();
        }
    }

    /// Waits until no install of package `id` runs (for tests and for
    /// closing down), then gives its state.
    #[must_use]
    pub fn settle(&self, id: &str) -> PackageState {
        loop {
            match self.state(id) {
                PackageState::Installing { .. } => {
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
                state => return state,
            }
        }
    }

    /// Removes package `id` and whatever a try at installing it left. A
    /// package added from a file is forgotten too. An install that runs
    /// is cancelled first.
    ///
    /// # Errors
    ///
    /// As [`PackageHome::uninstall`].
    pub fn uninstall(&self, id: &str) -> Result<bool, PkgError> {
        self.cancel(id);
        let _ = self.settle(id);
        if let Ok(mut failed) = self.failed.lock() {
            failed.remove(id);
        }
        let removed = self.home.uninstall(id)?;
        if !self.built_in(id) {
            let _ =
                std::fs::remove_file(self.home.root().join(CATALOG).join(format!("{id}.suepkg")));
            if let Ok(mut known) = self.known.lock() {
                known.retain(|found| found.package.id != id);
            }
        }
        Ok(removed)
    }

    /// Adds the package the manifest file at `path` describes and starts
    /// installing it; its files named by a path come from the manifest's
    /// own folder. Gives the package's id.
    ///
    /// # Errors
    ///
    /// [`PkgError::Io`] when the file cannot be read, and as
    /// [`OptionalPackage::parse`]. A manifest may not take the id of a
    /// package Sue names itself ([`PkgError::Manifest`]).
    pub fn add_file(&self, path: &Path) -> Result<String, PkgError> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| PkgError::Io(format!("{}: {error}", path.display())))?;
        let package = OptionalPackage::parse(&text)?;
        if self.built_in(&package.id) {
            return Err(PkgError::Manifest(format!(
                "`{}` is a package of Sue's own",
                package.id
            )));
        }
        let id = package.id.clone();
        // Remembered, so it is still listed after Sue starts again (its
        // remote files can then be fetched again; local ones need the
        // file once more).
        let catalog = self.home.root().join(CATALOG);
        if std::fs::create_dir_all(&catalog).is_ok() {
            let _ = std::fs::write(catalog.join(format!("{id}.suepkg")), package.to_text());
        }
        if let Ok(mut known) = self.known.lock() {
            known.retain(|found| found.package.id != id);
            known.push(Known {
                package,
                folder: path.parent().map(Path::to_path_buf),
            });
        }
        self.install(&id);
        Ok(id)
    }
}

/// The centers [`PackageCenter::at`] has made, one per folder.
fn centers() -> std::sync::MutexGuard<'static, Vec<Arc<PackageCenter>>> {
    static CENTERS: Mutex<Vec<Arc<PackageCenter>>> = Mutex::new(Vec::new());
    CENTERS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Fetches over HTTPS (and plain HTTP), carrying on from where an
/// earlier try stopped when the server can.
#[derive(Clone, Debug)]
pub struct HttpsFetch {
    #[cfg_attr(
        target_arch = "wasm32",
        allow(dead_code, reason = "the browser fetches nothing")
    )]
    user_agent: String,
}

impl HttpsFetch {
    /// A fetcher that names itself `user_agent` (such as `Sue/0.0.81`).
    #[must_use]
    pub fn new(user_agent: impl Into<String>) -> Self {
        Self {
            user_agent: user_agent.into(),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Fetch for HttpsFetch {
    fn fetch(
        &self,
        url: &str,
        from: u64,
        sink: &mut dyn FnMut(Chunk<'_>) -> bool,
    ) -> Result<(), FetchError> {
        use std::io::Read;
        use std::time::Duration;

        let agent: ureq::Agent = ureq::Agent::config_builder()
            .user_agent(&self.user_agent)
            .timeout_connect(Some(Duration::from_secs(20)))
            .timeout_recv_response(Some(Duration::from_secs(30)))
            .timeout_per_read(Some(Duration::from_secs(60)))
            .http_status_as_error(false)
            .build()
            .into();
        let offline = |error: &dyn std::fmt::Display| FetchError::Offline(error.to_string());
        let call = |from: u64| {
            let mut request = agent.get(url);
            if from > 0 {
                request = request.header("Range", format!("bytes={from}-"));
            }
            request.call().map_err(|error| match error {
                ureq::Error::Io(_)
                | ureq::Error::Timeout(_)
                | ureq::Error::HostNotFound
                | ureq::Error::ConnectionFailed => offline(&error),
                other => FetchError::Failed(other.to_string()),
            })
        };
        let mut response = call(from)?;
        // 416: the part on disk is as long as the file or longer, so the
        // server has no rest to give. The file is fetched anew.
        if response.status().as_u16() == 416 {
            response = call(0)?;
        }
        let status = response.status().as_u16();
        match status {
            206 => {}
            200 => {
                if from > 0 && !sink(Chunk::Restart) {
                    return Ok(());
                }
            }
            other => return Err(FetchError::Failed(format!("{url} answered {other}"))),
        }
        let mut reader = response.body_mut().as_reader();
        let mut buffer = vec![0_u8; 64 * 1024];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => return Ok(()),
                Ok(read) => {
                    if !sink(Chunk::Data(&buffer[..read])) {
                        return Ok(());
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) => return Err(offline(&error)),
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
impl Fetch for HttpsFetch {
    fn fetch(
        &self,
        url: &str,
        _from: u64,
        _sink: &mut dyn FnMut(Chunk<'_>) -> bool,
    ) -> Result<(), FetchError> {
        Err(FetchError::Failed(format!(
            "{url}: packages are not available in the browser yet"
        )))
    }
}

#[cfg(test)]
mod tests {
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use sue_pkg::ContentHash;

    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("sue-packages-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).expect("a scratch folder");
        folder
    }

    fn bytes(seed: u8, length: usize) -> Vec<u8> {
        (0..length)
            .map(|at| (at as u8).wrapping_mul(13).wrapping_add(seed))
            .collect()
    }

    /// What the server was asked: each path, and the byte it was asked
    /// to start from.
    type Asked = Arc<Mutex<Vec<(String, Option<u64>)>>>;

    /// A little web server on this machine: serves `files` by path,
    /// honors `Range`, counts what it was asked, and can send slowly (a
    /// piece, then a wait) so a test can cancel in the middle.
    struct Server {
        base: String,
        asked: Asked,
        stop: Arc<AtomicBool>,
        sent: Arc<AtomicUsize>,
    }

    impl Server {
        fn start(files: Vec<(&'static str, Vec<u8>)>, slow: bool) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("a port");
            let base = format!("http://{}", listener.local_addr().expect("an address"));
            let asked = Arc::new(Mutex::new(Vec::new()));
            let stop = Arc::new(AtomicBool::new(false));
            let sent = Arc::new(AtomicUsize::new(0));
            let (log, stopping, count) = (Arc::clone(&asked), Arc::clone(&stop), Arc::clone(&sent));
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    if stopping.load(Ordering::Relaxed) {
                        break;
                    }
                    let Ok(mut stream) = stream else { continue };
                    let mut reader = BufReader::new(stream.try_clone().expect("a clone"));
                    let mut first = String::new();
                    let _ = reader.read_line(&mut first);
                    let path = first.split_whitespace().nth(1).unwrap_or("/").to_owned();
                    let mut range = None;
                    loop {
                        let mut line = String::new();
                        if reader.read_line(&mut line).unwrap_or(0) == 0 || line.trim().is_empty() {
                            break;
                        }
                        if let Some(value) = line.to_ascii_lowercase().strip_prefix("range: bytes=")
                        {
                            range = value.trim().trim_end_matches('-').parse::<u64>().ok();
                        }
                    }
                    log.lock().expect("log").push((path.clone(), range));
                    let Some((_, content)) = files.iter().find(|(name, _)| *name == path) else {
                        let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n");
                        continue;
                    };
                    let from = range.unwrap_or(0).min(content.len() as u64) as usize;
                    let body = &content[from..];
                    let head = if range.is_some() {
                        format!(
                            "HTTP/1.1 206 Partial Content\r\ncontent-range: bytes {from}-{}/{}\r\n",
                            content.len().saturating_sub(1),
                            content.len()
                        )
                    } else {
                        "HTTP/1.1 200 OK\r\n".to_owned()
                    };
                    let head = format!(
                        "{head}content-length: {}\r\nconnection: close\r\n\r\n",
                        body.len()
                    );
                    if stream.write_all(head.as_bytes()).is_err() {
                        continue;
                    }
                    for piece in body.chunks(4096) {
                        if stream.write_all(piece).is_err() || stream.flush().is_err() {
                            break;
                        }
                        count.fetch_add(piece.len(), Ordering::Relaxed);
                        if slow {
                            std::thread::sleep(std::time::Duration::from_millis(3));
                        }
                    }
                }
            });
            Self {
                base,
                asked,
                stop,
                sent,
            }
        }
    }

    impl Drop for Server {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            // Wake the listener so its thread ends.
            let _ = std::net::TcpStream::connect(self.base.trim_start_matches("http://"));
        }
    }

    fn manifest(id: &str, files: &[(&str, &[u8], String)]) -> String {
        let mut text = format!(
            "format = 1\nid = {id}\nname = Test models\nversion = 1.2.0\nkind = ai-model\n\
             summary = Models for a test.\nlicence = Apache-2.0\nauthor = Nobody\n"
        );
        for (name, content, source) in files {
            text.push_str(&format!(
                "file = {name} | {} | {} | {source}\n",
                content.len(),
                ContentHash::of(content).to_hex()
            ));
        }
        text
    }

    #[test]
    fn the_packages_sue_names_are_listed_and_whole() {
        let root = scratch("built-in");
        let center = PackageCenter::new(&root, Arc::new(NoNetwork));
        let hands = center
            .package(HAND_TRACKING)
            .expect("the hand tracking package");
        assert!(center.built_in(HAND_TRACKING));
        assert_eq!(hands.licence, "Apache-2.0");
        assert_eq!(hands.files.len(), 2);
        assert!(
            hands
                .files
                .iter()
                .all(|file| file.source.starts_with("https://"))
        );
        assert!(hands.size() > 7_000_000 && hands.size() < 9_000_000);
        assert_eq!(center.state(HAND_TRACKING), PackageState::Available);
        assert!(center.folder(HAND_TRACKING).is_none());
        // With no network the install fails as offline, and says so.
        assert!(center.install(HAND_TRACKING));
        assert!(matches!(
            center.settle(HAND_TRACKING),
            PackageState::Failed(PkgError::Offline(_))
        ));
        assert!(!center.install("no-such-package"));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// The real thing, over the network: by hand only.
    #[test]
    #[ignore = "downloads the hand tracking models (8 MB) from the network"]
    fn the_real_hand_tracking_package_installs() {
        let root = scratch("real");
        let center = PackageCenter::new(&root, Arc::new(HttpsFetch::new("Sue/test")));
        let started = Instant::now();
        assert!(center.install(HAND_TRACKING));
        assert_eq!(center.settle(HAND_TRACKING), PackageState::Installed);
        println!("installed in {:?}", started.elapsed());
        let folder = center.folder(HAND_TRACKING).expect("installed");
        assert!(folder.join("palm_detection.onnx").is_file());
        assert!(folder.join("hand_landmark.onnx").is_file());
        assert!(center.uninstall(HAND_TRACKING).expect("removes"));
        assert!(
            center.package(HAND_TRACKING).is_some(),
            "Sue's own stays listed"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_package_installs_from_a_server_and_is_removed_again() {
        let root = scratch("server");
        let (a, b) = (bytes(1, 200_000), bytes(2, 50_000));
        let server = Server::start(vec![("/a.onnx", a.clone()), ("/b.onnx", b.clone())], false);
        let text = manifest(
            "test-models",
            &[
                ("a.onnx", &a, format!("{}/a.onnx", server.base)),
                ("b.onnx", &b, format!("{}/b.onnx", server.base)),
            ],
        );
        let file = root.join("shared").join("test-models.suepkg");
        std::fs::create_dir_all(file.parent().expect("a folder")).expect("folders");
        std::fs::write(&file, text).expect("the manifest");
        let home = root.join("home");
        let center = PackageCenter::new(&home, Arc::new(HttpsFetch::new("Sue/test")));
        let id = center.add_file(&file).expect("adds");
        assert_eq!(id, "test-models");
        assert!(center.busy() || center.state(&id) == PackageState::Installed);
        assert_eq!(center.settle(&id), PackageState::Installed);
        assert!(!center.busy());
        let folder = center.folder(&id).expect("installed");
        assert_eq!(std::fs::read(folder.join("a.onnx")).expect("a"), a);
        assert_eq!(std::fs::read(folder.join("b.onnx")).expect("b"), b);
        assert_eq!(center.packages().len(), 2);
        // A new center (Sue started again) still lists it.
        let again = PackageCenter::new(&home, Arc::new(NoNetwork));
        assert_eq!(again.state(&id), PackageState::Installed);
        assert_eq!(again.package(&id).expect("listed").name, "Test models");
        // Removed: gone from the disk and, not being Sue's own, the list.
        assert!(center.uninstall(&id).expect("removes"));
        assert!(center.package(&id).is_none());
        assert!(!home.join(&id).exists());
        assert!(
            PackageCenter::new(&home, Arc::new(NoNetwork))
                .package(&id)
                .is_none()
        );
        // Sue's own packages cannot be taken over by a file.
        std::fs::write(
            &file,
            manifest(HAND_TRACKING, &[("a.onnx", &a, "a.onnx".to_owned())]),
        )
        .expect("writes");
        assert!(matches!(center.add_file(&file), Err(PkgError::Manifest(_))));
        assert!(matches!(
            center.add_file(&root.join("none.suepkg")),
            Err(PkgError::Io(_))
        ));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn cancelling_a_download_stops_it_and_leaves_nothing() {
        let root = scratch("cancel");
        let a = bytes(3, 4_000_000);
        let server = Server::start(vec![("/a.onnx", a.clone())], true);
        let file = root.join("big.suepkg");
        std::fs::write(
            &file,
            manifest("big", &[("a.onnx", &a, format!("{}/a.onnx", server.base))]),
        )
        .expect("the manifest");
        let home = root.join("home");
        let center = PackageCenter::new(&home, Arc::new(HttpsFetch::new("Sue/test")));
        center.add_file(&file).expect("adds");
        // Wait until some of it has arrived, then cancel.
        let started = Instant::now();
        loop {
            match center.state("big") {
                PackageState::Installing { done, total, .. } if done > 0 => {
                    assert_eq!(total, 4_000_000);
                    assert!(done < total);
                    break;
                }
                PackageState::Installing { .. } => {}
                other => panic!("ended early: {other:?}"),
            }
            assert!(started.elapsed().as_secs() < 20, "nothing arrived");
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        center.cancel("big");
        assert_eq!(center.settle("big"), PackageState::Available);
        assert!(
            server.sent.load(Ordering::Relaxed) < 4_000_000,
            "the download went on"
        );
        assert!(!home.join("big").exists());
        assert!(!home.join(".staging").exists(), "a half download was left");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn everyone_who_asks_for_a_folder_gets_the_same_center() {
        let root = scratch("same");
        let first = PackageCenter::at(&root);
        assert!(Arc::ptr_eq(&first, &PackageCenter::at(&root)));
        let other = PackageCenter::at(&root.join("other"));
        assert!(!Arc::ptr_eq(&first, &other));
        // A center put there takes the folder's place.
        let put = Arc::new(PackageCenter::new(&root, Arc::new(NoNetwork)));
        PackageCenter::put(Arc::clone(&put));
        assert!(Arc::ptr_eq(&put, &PackageCenter::at(&root)));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_dropped_download_carries_on_with_a_range_request() {
        let root = scratch("range");
        let a = bytes(4, 300_000);
        let server = Server::start(vec![("/a.onnx", a.clone())], false);
        let url = format!("{}/a.onnx", server.base);
        // As if an earlier try had brought the first 100 000 bytes.
        let fetch = HttpsFetch::new("Sue/test");
        let mut got = a[..100_000].to_vec();
        fetch
            .fetch(&url, 100_000, &mut |chunk| {
                match chunk {
                    Chunk::Restart => got.clear(),
                    Chunk::Data(bytes) => got.extend_from_slice(bytes),
                }
                true
            })
            .expect("fetches");
        assert_eq!(got, a);
        assert_eq!(
            *server.asked.lock().expect("asked"),
            [("/a.onnx".to_owned(), Some(100_000))]
        );
        // A link that is not there fails; a server that is not there is
        // offline.
        assert!(matches!(
            fetch.fetch(&format!("{}/none", server.base), 0, &mut |_| true),
            Err(FetchError::Failed(_))
        ));
        assert!(matches!(
            fetch.fetch("http://127.0.0.1:9/a.onnx", 0, &mut |_| true),
            Err(FetchError::Offline(_))
        ));
        let _ = std::fs::remove_dir_all(&root);
    }
}
