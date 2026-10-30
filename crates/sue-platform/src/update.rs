//! Sue's own updates, from the GitHub releases of MagiGun/sue: asking for
//! the latest release, telling whether it is newer than this build, choosing
//! the file made for this system and how it was installed, downloading it
//! in the background (checked against the release's `SHA256SUMS.txt`) and
//! handing it to the system: the setup program on Windows, the disk image
//! on macOS, the package installer on Android, or a new AppImage in place
//! of the running one on Linux. Everything else opens the release page or
//! names the package manager's command.
//!
//! The checking and the choosing are plain functions of a [`Release`], an
//! [`Install`] and an architecture, so every system's choice is tested on
//! any machine. Specification: `HUMANZ.md` §5.5.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Where the latest release is described (GitHub's REST API).
pub const LATEST_RELEASE_API: &str = "https://api.github.com/repos/MagiGun/sue/releases/latest";

/// The latest release's page, for people.
pub const LATEST_RELEASE_PAGE: &str = "https://github.com/MagiGun/sue/releases/latest";

/// The file in every release listing each file's SHA-256.
pub const SUMS: &str = "SHA256SUMS.txt";

/// One file of a release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asset {
    /// Its file name, such as `Sue-0.0.4-windows-x64-setup.exe`.
    pub name: String,
    /// Where to download it.
    pub url: String,
    /// Its size in bytes, as GitHub says.
    pub size: u64,
}

/// A release of Sue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    /// Its tag, such as `v0.0.4`.
    pub tag: String,
    /// Its title.
    pub name: String,
    /// Its notes (Markdown).
    pub notes: String,
    /// Its page, for people.
    pub page: String,
    /// Its files.
    pub assets: Vec<Asset>,
}

impl Release {
    /// Reads GitHub's description of a release.
    ///
    /// # Errors
    /// When the text is not JSON or has no tag.
    pub fn parse(json: &str) -> Result<Self, String> {
        let value: serde_json::Value =
            serde_json::from_str(json).map_err(|error| error.to_string())?;
        let text = |value: &serde_json::Value, key: &str| {
            value
                .get(key)
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .to_owned()
        };
        let tag = text(&value, "tag_name");
        if tag.is_empty() {
            return Err("the release has no tag".to_owned());
        }
        let assets = value
            .get("assets")
            .and_then(serde_json::Value::as_array)
            .map(|assets| {
                assets
                    .iter()
                    .map(|asset| Asset {
                        name: text(asset, "name"),
                        url: text(asset, "browser_download_url"),
                        size: asset
                            .get("size")
                            .and_then(serde_json::Value::as_u64)
                            .unwrap_or(0),
                    })
                    .filter(|asset| !asset.name.is_empty() && !asset.url.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        let page = text(&value, "html_url");
        Ok(Self {
            name: text(&value, "name"),
            notes: text(&value, "body"),
            page: if page.is_empty() {
                LATEST_RELEASE_PAGE.to_owned()
            } else {
                page
            },
            tag,
            assets,
        })
    }

    /// Its version: the tag without the `v`.
    #[must_use]
    pub fn version(&self) -> &str {
        self.tag.strip_prefix('v').unwrap_or(&self.tag)
    }

    /// The file named `name`.
    #[must_use]
    pub fn asset(&self, name: &str) -> Option<&Asset> {
        self.assets.iter().find(|asset| asset.name == name)
    }
}

/// `true` when `release` (a tag or version, such as `v0.0.5`) is newer than
/// `current`, the version this build reports. A development build after a
/// release (`0.0.4+dev.3`) counts as newer than that release, so it is
/// not offered the release it was built after. Anything unreadable is not
/// newer.
#[must_use]
pub fn is_newer(release: &str, current: &str) -> bool {
    let read = |text: &str| {
        let mut version = semver::Version::parse(text.trim().trim_start_matches('v')).ok()?;
        version.build = semver::BuildMetadata::EMPTY;
        Some(version)
    };
    match (read(release), read(current)) {
        (Some(release), Some(current)) => release > current,
        _ => false,
    }
}

/// How this copy of Sue was installed, which decides how it updates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Install {
    /// Windows, from the setup program or the .msi (also winget's), in
    /// Program Files.
    WindowsSetup,
    /// Windows, unpacked from the portable zip.
    WindowsPortable,
    /// Windows, through Scoop.
    Scoop,
    /// macOS, the app from the disk image.
    MacApp,
    /// Linux, an AppImage at this path.
    AppImage(PathBuf),
    /// Linux, the Flatpak.
    Flatpak,
    /// Linux, the .deb.
    Deb,
    /// Linux, the .rpm.
    Rpm,
    /// Linux, the Arch package (`sue-bin`).
    Arch,
    /// Linux, unpacked from the tarball.
    LinuxFiles,
    /// FreeBSD and other systems, unpacked from a tarball.
    Unpacked,
    /// Android, the APK.
    Android,
    /// iOS and iPadOS.
    Ios,
    /// The browser.
    Browser,
}

impl Install {
    /// How this copy was installed. `SUE_UPDATE_AS` (`setup`, `portable`,
    /// `scoop`, `appimage`, `deb` and the other names of
    /// [`Install::from_name`]) overrides it, to try another way of
    /// updating.
    #[must_use]
    pub fn detect() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(install) = std::env::var("SUE_UPDATE_AS")
            .ok()
            .and_then(|name| Self::from_name(&name))
        {
            return install;
        }
        detect()
    }

    /// The install named `name`, as `SUE_UPDATE_AS` gives it. An AppImage
    /// is the one in `APPIMAGE`.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name.trim().to_ascii_lowercase().as_str() {
            "setup" | "msi" | "winget" => Self::WindowsSetup,
            "portable" => Self::WindowsPortable,
            "scoop" => Self::Scoop,
            "mac" | "macos" | "dmg" => Self::MacApp,
            "appimage" => Self::AppImage(PathBuf::from(std::env::var_os("APPIMAGE")?)),
            "flatpak" => Self::Flatpak,
            "deb" => Self::Deb,
            "rpm" => Self::Rpm,
            "arch" | "aur" => Self::Arch,
            "tarball" | "linux" => Self::LinuxFiles,
            "unpacked" | "freebsd" => Self::Unpacked,
            "android" | "apk" => Self::Android,
            "ios" => Self::Ios,
            "browser" | "web" => Self::Browser,
            _ => return None,
        })
    }
}

#[cfg(target_os = "windows")]
fn detect() -> Install {
    let Ok(exe) = std::env::current_exe() else {
        return Install::WindowsPortable;
    };
    let exe = exe.to_string_lossy().to_lowercase().replace('/', "\\");
    if exe.contains("\\scoop\\apps\\") {
        return Install::Scoop;
    }
    let installed = ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .any(|dir| {
            let dir = dir.to_lowercase();
            exe.starts_with(&format!("{}\\", dir.trim_end_matches('\\')))
        });
    if installed {
        Install::WindowsSetup
    } else {
        Install::WindowsPortable
    }
}

#[cfg(target_os = "linux")]
fn detect() -> Install {
    if let Some(path) = std::env::var_os("APPIMAGE") {
        return Install::AppImage(PathBuf::from(path));
    }
    if std::env::var_os("FLATPAK_ID").is_some() || Path::new("/.flatpak-info").exists() {
        return Install::Flatpak;
    }
    let packaged = std::env::current_exe().is_ok_and(|exe| exe.starts_with("/usr"));
    if packaged {
        if Path::new("/var/lib/dpkg/info/sue.list").exists() {
            return Install::Deb;
        }
        let pacman = std::fs::read_dir("/var/lib/pacman/local").is_ok_and(|entries| {
            entries
                .flatten()
                .any(|entry| entry.file_name().to_string_lossy().starts_with("sue-bin-"))
        });
        if pacman {
            return Install::Arch;
        }
        if Path::new("/var/lib/rpm").exists() || Path::new("/usr/lib/sysimage/rpm").exists() {
            return Install::Rpm;
        }
    }
    Install::LinuxFiles
}

#[cfg(target_os = "macos")]
fn detect() -> Install {
    Install::MacApp
}

#[cfg(target_os = "android")]
fn detect() -> Install {
    Install::Android
}

#[cfg(target_os = "ios")]
fn detect() -> Install {
    Install::Ios
}

#[cfg(target_arch = "wasm32")]
fn detect() -> Install {
    Install::Browser
}

#[cfg(not(any(
    target_os = "windows",
    target_os = "linux",
    target_os = "macos",
    target_os = "android",
    target_os = "ios",
    target_arch = "wasm32"
)))]
fn detect() -> Install {
    Install::Unpacked
}

/// How to update from one install to a release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Plan {
    /// Download this file and hand it to the system: the setup program on
    /// Windows, the disk image on macOS, the package installer on Android.
    Run(Asset),
    /// Download this AppImage and put it in place of the one at `path`.
    AppImage {
        /// The new AppImage.
        asset: Asset,
        /// The running one.
        path: PathBuf,
    },
    /// Download this package and install it with `command` followed by
    /// its path (see [`package_command`]).
    Package {
        /// The package.
        asset: Asset,
        /// The package manager's command, such as `sudo apt install`.
        command: String,
    },
    /// The package manager updates Sue with this command.
    Command(String),
    /// Open the release page and download it by hand.
    Page,
    /// Always the newest already (the browser).
    Current,
}

/// The architecture names a release's files use for `arch`
/// (`std::env::consts::ARCH`): Windows', Debian's and everyone else's.
fn arch_names(arch: &str) -> Option<(&'static str, &'static str, &'static str)> {
    Some(match arch {
        "x86_64" => ("x64", "amd64", "x86_64"),
        "x86" => ("x86", "i386", "x86"),
        "aarch64" => ("arm64", "arm64", "aarch64"),
        "riscv64" => ("riscv64", "riscv64", "riscv64"),
        _ => return None,
    })
}

/// How `install`, on a processor of `arch` (`std::env::consts::ARCH`),
/// updates to `release`. When the release lacks the file it needs, the
/// release page.
#[must_use]
pub fn plan(release: &Release, install: &Install, arch: &str) -> Plan {
    let version = release.version();
    let (windows, debian, other) = arch_names(arch).unwrap_or(("", "", ""));
    let file = |name: String| release.asset(&name).cloned();
    let package = |name: String, command: &str| {
        file(name).map(|asset| Plan::Package {
            asset,
            command: command.to_owned(),
        })
    };
    let chosen = match install {
        Install::WindowsSetup => {
            file(format!("Sue-{version}-windows-{windows}-setup.exe")).map(Plan::Run)
        }
        Install::Scoop => Some(Plan::Command("scoop update sue".to_owned())),
        Install::MacApp => file(format!("Sue-{version}-macos-universal.dmg")).map(Plan::Run),
        Install::AppImage(path) => {
            file(format!("Sue-{version}-linux-{other}.AppImage")).map(|asset| Plan::AppImage {
                asset,
                path: path.clone(),
            })
        }
        Install::Flatpak => package(
            format!("Sue-{version}-linux-{other}.flatpak"),
            "flatpak install --reinstall",
        ),
        Install::Deb => package(
            format!("Sue-{version}-linux-{debian}.deb"),
            "sudo apt install",
        ),
        Install::Rpm => package(
            format!("Sue-{version}-linux-{other}.rpm"),
            "sudo dnf install",
        ),
        Install::Arch => package(
            format!("Sue-{version}-linux-{other}.pkg.tar.zst"),
            "sudo pacman -U",
        ),
        Install::Android => file(format!("Sue-{version}-android.apk")).map(Plan::Run),
        Install::Browser => Some(Plan::Current),
        Install::WindowsPortable | Install::LinuxFiles | Install::Unpacked | Install::Ios => None,
    };
    chosen.unwrap_or(Plan::Page)
}

/// The command that installs a downloaded package: `command` and the
/// package's path, quoted for a shell.
#[must_use]
pub fn package_command(command: &str, path: &Path) -> String {
    let path = path.to_string_lossy().replace('\'', "'\\''");
    format!("{command} '{path}'")
}

/// The SHA-256 that `sums` (a `SHA256SUMS.txt`, lines of a hex digest and
/// a file name) gives for `name`, in lower case.
#[must_use]
pub fn sum_for(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (digest, file) = line.trim().split_once(char::is_whitespace)?;
        let file = file.trim_start().trim_start_matches('*');
        (file == name && digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| digest.to_ascii_lowercase())
    })
}

/// Work done on a thread, whose answer is taken once.
pub struct Job<T> {
    answer: Arc<Mutex<Option<T>>>,
}

impl<T> Job<T> {
    /// The answer, once there is one (it is taken: later calls are `None`).
    #[must_use]
    pub fn take(&self) -> Option<T> {
        self.answer.lock().ok()?.take()
    }

    #[cfg_attr(
        not(target_arch = "wasm32"),
        allow(dead_code, reason = "only the browser answers at once")
    )]
    fn done(answer: T) -> Self {
        Self {
            answer: Arc::new(Mutex::new(Some(answer))),
        }
    }
}

impl<U: Send + 'static> Job<Result<U, String>> {
    #[cfg_attr(
        target_arch = "wasm32",
        allow(dead_code, reason = "the browser downloads nothing")
    )]
    fn spawn(work: impl FnOnce() -> Result<U, String> + Send + 'static) -> Self {
        let answer = Arc::new(Mutex::new(None));
        let slot = Arc::clone(&answer);
        let started = std::thread::Builder::new()
            .name("sue-update".to_owned())
            .spawn(move || {
                let value = work();
                if let Ok(mut slot) = slot.lock() {
                    *slot = Some(value);
                }
            });
        if let Err(error) = started
            && let Ok(mut slot) = answer.lock()
        {
            *slot = Some(Err(error.to_string()));
        }
        Self { answer }
    }
}

/// Asks GitHub for the latest release, on a thread. `user_agent` names
/// the asker, such as `Sue/0.0.4`.
#[must_use]
pub fn check_latest(user_agent: &str) -> Job<Result<Release, String>> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let user_agent = user_agent.to_owned();
        Job::spawn(move || {
            let text = net::get_text(&user_agent, LATEST_RELEASE_API)?;
            Release::parse(&text)
        })
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = user_agent;
        Job::done(Err("the browser always runs the newest Sue".to_owned()))
    }
}

/// A download running on a thread.
pub struct Download {
    received: Arc<AtomicU64>,
    total: Arc<AtomicU64>,
    answer: Job<Result<PathBuf, String>>,
}

impl Download {
    /// Bytes received so far, and of how many (0 while unknown).
    #[must_use]
    pub fn progress(&self) -> (u64, u64) {
        (
            self.received.load(Ordering::Relaxed),
            self.total.load(Ordering::Relaxed),
        )
    }

    /// Where the checked file is, or why there is none, once it is done.
    #[must_use]
    pub fn take(&self) -> Option<Result<PathBuf, String>> {
        self.answer.take()
    }
}

/// Downloads `asset` of `release` into `dir`, on a thread, and checks it
/// against the release's `SHA256SUMS.txt`: a file it does not list, or
/// whose digest differs, is thrown away.
#[must_use]
pub fn download(release: &Release, asset: &Asset, dir: &Path, user_agent: &str) -> Download {
    let received = Arc::new(AtomicU64::new(0));
    let total = Arc::new(AtomicU64::new(asset.size));
    #[cfg(not(target_arch = "wasm32"))]
    let answer = {
        let sums = release.asset(SUMS).cloned();
        let asset = asset.clone();
        let dir = dir.to_path_buf();
        let user_agent = user_agent.to_owned();
        let (received, total) = (Arc::clone(&received), Arc::clone(&total));
        Job::spawn(move || {
            let sums = sums.ok_or_else(|| format!("the release has no {SUMS}"))?;
            let sums = net::get_text(&user_agent, &sums.url)?;
            let expected = sum_for(&sums, &asset.name)
                .ok_or_else(|| format!("{SUMS} does not list {}", asset.name))?;
            net::get_file(&user_agent, &asset, &dir, &expected, &received, &total)
        })
    };
    #[cfg(target_arch = "wasm32")]
    let answer = {
        let _ = (release, dir, user_agent);
        Job::done(Err("the browser always runs the newest Sue".to_owned()))
    };
    Download {
        received,
        total,
        answer,
    }
}

/// Hands a downloaded update to the system: runs the setup program on
/// Windows, opens the disk image on macOS and gives the APK to the
/// package installer on Android.
///
/// # Errors
/// When the system cannot start it, or on systems that update otherwise.
pub fn run_update(path: &Path) -> io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new(path).spawn().map(|_| ())
    }
    #[cfg(target_os = "macos")]
    {
        crate::os::open_detached(path.as_os_str())
    }
    #[cfg(target_os = "android")]
    {
        crate::android::install_package(path).map_err(io::Error::other)
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "android")))]
    {
        let _ = path;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "this system updates Sue another way",
        ))
    }
}

/// Puts a downloaded AppImage (`new`, beside the old one) in place of the
/// running one at `path` and starts it.
///
/// # Errors
/// When it cannot be made runnable, moved or started.
pub fn replace_appimage(new: &Path, path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(new, std::fs::Permissions::from_mode(0o755))?;
        std::fs::rename(new, path)?;
        std::process::Command::new(path).spawn().map(|_| ())
    }
    #[cfg(not(unix))]
    {
        let _ = (new, path);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "AppImages are for Linux",
        ))
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod net {
    //! HTTPS through `ureq` (rustls).

    use std::io::{Read, Write};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    use sha2::{Digest, Sha256};

    use super::Asset;

    fn agent(user_agent: &str) -> ureq::Agent {
        ureq::Agent::config_builder()
            .user_agent(user_agent)
            .timeout_connect(Some(Duration::from_secs(20)))
            .timeout_recv_response(Some(Duration::from_secs(30)))
            .timeout_per_read(Some(Duration::from_secs(60)))
            .build()
            .into()
    }

    /// A text file (at most a few MiB).
    pub(super) fn get_text(user_agent: &str, url: &str) -> Result<String, String> {
        agent(user_agent)
            .get(url)
            .header("Accept", "application/vnd.github+json")
            .call()
            .map_err(|error| error.to_string())?
            .body_mut()
            .read_to_string()
            .map_err(|error| error.to_string())
    }

    /// Downloads `asset` into `dir` as a `.part` file, renamed to its own
    /// name once its SHA-256 is `expected`.
    pub(super) fn get_file(
        user_agent: &str,
        asset: &Asset,
        dir: &Path,
        expected: &str,
        received: &AtomicU64,
        total: &AtomicU64,
    ) -> Result<PathBuf, String> {
        let fail = |error: &dyn std::fmt::Display| error.to_string();
        std::fs::create_dir_all(dir).map_err(|error| fail(&error))?;
        let mut response = agent(user_agent)
            .get(&asset.url)
            .call()
            .map_err(|error| fail(&error))?;
        if let Some(length) = response.body().content_length() {
            total.store(length, Ordering::Relaxed);
        }
        let part = dir.join(format!("{}.part", asset.name));
        let mut file = std::fs::File::create(&part).map_err(|error| fail(&error))?;
        let mut reader = response.body_mut().as_reader();
        let mut hasher = Sha256::new();
        let mut buffer = vec![0_u8; 64 * 1024];
        let copied = loop {
            let read = match reader.read(&mut buffer) {
                Ok(0) => break Ok(()),
                Ok(read) => read,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => break Err(fail(&error)),
            };
            hasher.update(&buffer[..read]);
            if let Err(error) = file.write_all(&buffer[..read]) {
                break Err(fail(&error));
            }
            received.fetch_add(read as u64, Ordering::Relaxed);
        };
        let flushed = copied.and_then(|()| file.sync_all().map_err(|error| fail(&error)));
        drop(file);
        let digest: String = hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let checked = flushed.and_then(|()| {
            if digest == expected {
                Ok(())
            } else {
                Err(format!(
                    "{} does not match {} (its SHA-256 is {digest})",
                    asset.name,
                    super::SUMS
                ))
            }
        });
        if let Err(error) = checked {
            let _ = std::fs::remove_file(&part);
            return Err(error);
        }
        let path = dir.join(&asset.name);
        let _ = std::fs::remove_file(&path);
        std::fs::rename(&part, &path).map_err(|error| fail(&error))?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The files of v0.0.4, as GitHub listed them.
    const V004: &[&str] = &[
        "PKGBUILD",
        "SHA256SUMS.txt",
        "Sue-0.0.4-android.apk",
        "Sue-0.0.4-freebsd-x86_64.tar.gz",
        "Sue-0.0.4-ios-simulator.zip",
        "Sue-0.0.4-ios-unsigned.ipa",
        "Sue-0.0.4-linux-aarch64.AppImage",
        "Sue-0.0.4-linux-aarch64.AppImage.zsync",
        "Sue-0.0.4-linux-aarch64.flatpak",
        "Sue-0.0.4-linux-aarch64.pkg.tar.zst",
        "Sue-0.0.4-linux-aarch64.rpm",
        "Sue-0.0.4-linux-aarch64.tar.gz",
        "Sue-0.0.4-linux-amd64.deb",
        "Sue-0.0.4-linux-arm64.deb",
        "Sue-0.0.4-linux-riscv64.deb",
        "Sue-0.0.4-linux-riscv64.rpm",
        "Sue-0.0.4-linux-riscv64.tar.gz",
        "Sue-0.0.4-linux-x86_64.AppImage",
        "Sue-0.0.4-linux-x86_64.AppImage.zsync",
        "Sue-0.0.4-linux-x86_64.flatpak",
        "Sue-0.0.4-linux-x86_64.pkg.tar.zst",
        "Sue-0.0.4-linux-x86_64.rpm",
        "Sue-0.0.4-linux-x86_64.tar.gz",
        "Sue-0.0.4-macos-universal.dmg",
        "Sue-0.0.4-windows-arm64-portable.zip",
        "Sue-0.0.4-windows-arm64-setup.exe",
        "Sue-0.0.4-windows-arm64.msi",
        "Sue-0.0.4-windows-x64-portable.zip",
        "Sue-0.0.4-windows-x64-setup.exe",
        "Sue-0.0.4-windows-x64.msi",
        "Sue-0.0.4-windows-x86-portable.zip",
        "Sue-0.0.4-windows-x86-setup.exe",
        "Sue-0.0.4-windows-x86.msi",
        "Sue-cli-0.0.4-macos-universal.zip",
        "sue.json",
    ];

    fn v004() -> Release {
        let assets: Vec<String> = V004
            .iter()
            .map(|name| {
                format!(
                    r#"{{"name":"{name}","size":7,"browser_download_url":"https://github.com/MagiGun/sue/releases/download/v0.0.4/{name}"}}"#
                )
            })
            .collect();
        let json = format!(
            r#"{{"tag_name":"v0.0.4","name":"Sue 0.0.4","body":"Notes","html_url":"https://github.com/MagiGun/sue/releases/tag/v0.0.4","assets":[{}]}}"#,
            assets.join(",")
        );
        Release::parse(&json).expect("a release")
    }

    fn chosen(install: &Install, arch: &str) -> Option<String> {
        match plan(&v004(), install, arch) {
            Plan::Run(asset) | Plan::AppImage { asset, .. } | Plan::Package { asset, .. } => {
                Some(asset.name)
            }
            Plan::Command(command) => Some(command),
            Plan::Page => None,
            Plan::Current => Some("current".to_owned()),
        }
    }

    #[test]
    fn reads_a_release() {
        let release = v004();
        assert_eq!(release.tag, "v0.0.4");
        assert_eq!(release.version(), "0.0.4");
        assert_eq!(release.name, "Sue 0.0.4");
        assert_eq!(release.notes, "Notes");
        assert_eq!(release.assets.len(), 35);
        assert_eq!(release.asset("sue.json").map(|asset| asset.size), Some(7));
        assert!(Release::parse("{}").is_err());
        assert!(Release::parse("not json").is_err());
    }

    #[test]
    fn compares_versions() {
        assert!(is_newer("v0.0.5", "0.0.4"));
        assert!(is_newer("0.1.0", "0.0.9"));
        assert!(is_newer("v1.0.0", "0.9.9"));
        assert!(!is_newer("v0.0.4", "0.0.4"));
        assert!(!is_newer("v0.0.3", "0.0.4"));
        // A development build after a release counts as newer than it.
        assert!(!is_newer("v0.0.4", "0.0.4+dev.3"));
        assert!(is_newer("v0.0.5", "0.0.4+dev.3"));
        // A prerelease comes before its release.
        assert!(is_newer("v0.0.5", "0.0.5-rc.1"));
        assert!(!is_newer("v0.0.5-rc.1", "0.0.5"));
        // Nothing unreadable is newer.
        assert!(!is_newer("nightly", "0.0.4"));
        assert!(!is_newer("v0.0.5", "dev"));
    }

    #[test]
    fn windows_runs_its_own_setup() {
        let setup = Install::WindowsSetup;
        assert_eq!(
            chosen(&setup, "x86_64").as_deref(),
            Some("Sue-0.0.4-windows-x64-setup.exe")
        );
        assert_eq!(
            chosen(&setup, "x86").as_deref(),
            Some("Sue-0.0.4-windows-x86-setup.exe")
        );
        assert_eq!(
            chosen(&setup, "aarch64").as_deref(),
            Some("Sue-0.0.4-windows-arm64-setup.exe")
        );
        assert!(matches!(plan(&v004(), &setup, "x86_64"), Plan::Run(_)));
        assert_eq!(chosen(&Install::WindowsPortable, "x86_64"), None);
        assert_eq!(
            chosen(&Install::Scoop, "x86_64").as_deref(),
            Some("scoop update sue")
        );
    }

    #[test]
    fn macos_opens_the_disk_image() {
        for arch in ["x86_64", "aarch64"] {
            assert_eq!(
                chosen(&Install::MacApp, arch).as_deref(),
                Some("Sue-0.0.4-macos-universal.dmg")
            );
        }
    }

    #[test]
    fn linux_follows_how_sue_was_installed() {
        let appimage = Install::AppImage(PathBuf::from("/home/me/Sue.AppImage"));
        assert_eq!(
            plan(&v004(), &appimage, "x86_64"),
            Plan::AppImage {
                asset: v004()
                    .asset("Sue-0.0.4-linux-x86_64.AppImage")
                    .cloned()
                    .expect("asset"),
                path: PathBuf::from("/home/me/Sue.AppImage"),
            }
        );
        assert_eq!(
            chosen(&appimage, "aarch64").as_deref(),
            Some("Sue-0.0.4-linux-aarch64.AppImage")
        );
        // No AppImage for RISC-V: the page.
        assert_eq!(chosen(&appimage, "riscv64"), None);
        assert_eq!(
            chosen(&Install::Deb, "x86_64").as_deref(),
            Some("Sue-0.0.4-linux-amd64.deb")
        );
        assert_eq!(
            chosen(&Install::Deb, "aarch64").as_deref(),
            Some("Sue-0.0.4-linux-arm64.deb")
        );
        assert_eq!(
            chosen(&Install::Deb, "riscv64").as_deref(),
            Some("Sue-0.0.4-linux-riscv64.deb")
        );
        assert_eq!(
            chosen(&Install::Rpm, "x86_64").as_deref(),
            Some("Sue-0.0.4-linux-x86_64.rpm")
        );
        assert_eq!(
            chosen(&Install::Rpm, "riscv64").as_deref(),
            Some("Sue-0.0.4-linux-riscv64.rpm")
        );
        assert_eq!(
            chosen(&Install::Arch, "aarch64").as_deref(),
            Some("Sue-0.0.4-linux-aarch64.pkg.tar.zst")
        );
        assert_eq!(
            chosen(&Install::Flatpak, "x86_64").as_deref(),
            Some("Sue-0.0.4-linux-x86_64.flatpak")
        );
        match plan(&v004(), &Install::Deb, "x86_64") {
            Plan::Package { command, .. } => assert_eq!(command, "sudo apt install"),
            other => panic!("a package, not {other:?}"),
        }
        assert_eq!(chosen(&Install::LinuxFiles, "x86_64"), None);
        assert_eq!(chosen(&Install::Unpacked, "x86_64"), None);
    }

    #[test]
    fn phones_and_the_browser() {
        for arch in ["aarch64", "x86_64"] {
            assert_eq!(
                chosen(&Install::Android, arch).as_deref(),
                Some("Sue-0.0.4-android.apk")
            );
        }
        assert_eq!(chosen(&Install::Ios, "aarch64"), None);
        assert_eq!(plan(&v004(), &Install::Browser, "wasm32"), Plan::Current);
    }

    #[test]
    fn a_missing_file_opens_the_page() {
        let mut release = v004();
        release.assets.retain(|asset| !asset.name.ends_with(".exe"));
        assert_eq!(plan(&release, &Install::WindowsSetup, "x86_64"), Plan::Page);
        assert_eq!(plan(&release, &Install::WindowsSetup, "sparc"), Plan::Page);
    }

    #[test]
    fn reads_the_sums() {
        let sums = "\
0123456789abcdef0123456789abcdef0123456789abcdef0123456789ABCDEF  Sue-0.0.4-android.apk
fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210 *sue.json
short  PKGBUILD
";
        assert_eq!(
            sum_for(sums, "Sue-0.0.4-android.apk").as_deref(),
            Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
        );
        assert!(sum_for(sums, "sue.json").is_some());
        assert_eq!(sum_for(sums, "PKGBUILD"), None);
        assert_eq!(sum_for(sums, "missing"), None);
    }

    #[test]
    fn names_installs() {
        assert_eq!(Install::from_name("Setup"), Some(Install::WindowsSetup));
        assert_eq!(Install::from_name("scoop"), Some(Install::Scoop));
        assert_eq!(Install::from_name("deb"), Some(Install::Deb));
        assert_eq!(Install::from_name("nonsense"), None);
        assert_eq!(
            package_command("sudo apt install", Path::new("/tmp/it's.deb")),
            "sudo apt install '/tmp/it'\\''s.deb'"
        );
    }

    /// Waits for a job's answer.
    #[cfg(not(target_arch = "wasm32"))]
    fn wait<T>(job: impl Fn() -> Option<T>) -> T {
        loop {
            if let Some(answer) = job() {
                return answer;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }

    /// Asks GitHub for the real latest release and downloads its smallest
    /// listed file, checked against its sums. Needs the network:
    /// `cargo test -p sue-platform -- --ignored real_release`.
    #[test]
    #[ignore = "needs the network"]
    #[cfg(not(target_arch = "wasm32"))]
    fn real_release() {
        let job = check_latest("Sue/0.0.3");
        let release = wait(|| job.take()).expect("the latest release");
        assert!(is_newer(&release.tag, "0.0.3"), "{}", release.tag);
        assert!(release.asset(SUMS).is_some());
        for install in [
            Install::WindowsSetup,
            Install::MacApp,
            Install::Deb,
            Install::Android,
        ] {
            let chosen = plan(&release, &install, "x86_64");
            assert!(
                !matches!(chosen, Plan::Page),
                "{install:?} found no file in {}",
                release.tag
            );
        }
        let asset = release
            .assets
            .iter()
            .filter(|asset| asset.name != SUMS)
            .min_by_key(|asset| asset.size)
            .expect("a file")
            .clone();
        let dir = std::env::temp_dir().join(format!("sue-update-test-{}", std::process::id()));
        let download = download(&release, &asset, &dir, "Sue/0.0.3");
        let path = wait(|| download.take()).expect("a checked download");
        assert_eq!(
            std::fs::metadata(&path).map(|meta| meta.len()).ok(),
            Some(asset.size)
        );
        assert_eq!(download.progress().0, asset.size);
        // A file the sums do not list is refused.
        let mut stranger = asset.clone();
        stranger.name = "not-in-the-sums.txt".to_owned();
        let refused = download_named(&release, &stranger, &dir);
        assert!(refused.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn download_named(release: &Release, asset: &Asset, dir: &Path) -> Result<PathBuf, String> {
        let download = download(release, asset, dir, "Sue/0.0.3");
        wait(|| download.take())
    }
}
