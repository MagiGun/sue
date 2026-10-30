//! Sue's own updates in the window: the check at startup (at most once a
//! day, unless `updates.check_automatically` is off), Check for Updates in
//! the Sue menu and in Settings, and a small window in the corner that
//! names the new release, shows its notes and updates the way this copy
//! was installed ([`sue_platform::update`] does the asking, choosing,
//! downloading and handing over). Nothing here blocks the interface: the
//! check and the download run on threads and are read between frames.

use std::path::{Path, PathBuf};
use std::time::Duration;

use egui::{Align2, Id, Order, RichText, ScrollArea, Sense, vec2};
use sue_core::SUE_VERSION;
use sue_platform::PlatformPaths;
use sue_platform::update::{self, Download, Install, Job, Plan, Release};
use sue_wm::{Theme, widgets};

use crate::util::human_bytes;

/// How long Sue waits between automatic checks.
const DAY: u64 = 24 * 60 * 60;

/// How often a running check or download is looked at.
pub const POLL: Duration = Duration::from_millis(250);

/// What the window asks the runtime to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Open a web page.
    OpenUrl(String),
    /// Put text on the clipboard.
    Copy(String),
    /// Close Sue (the update takes over).
    Quit,
}

enum State {
    Idle,
    Checking {
        job: Job<Result<Release, String>>,
        quiet: bool,
    },
    UpToDate,
    Available {
        release: Release,
        plan: Plan,
    },
    Downloading {
        release: Release,
        plan: Plan,
        download: Download,
    },
    /// Handed to the system, which finishes it (macOS's disk image,
    /// Android's installer), or a package waiting for its command.
    Handed {
        release: Release,
        note: String,
        command: Option<String>,
    },
    Failed {
        release: Option<Release>,
        error: String,
    },
}

/// Sue's updates, between frames.
pub struct Updater {
    state: State,
    shown: bool,
    install: Install,
    /// When the last automatic check ran (seconds since 1970).
    stamp: PathBuf,
    /// Where setup programs, disk images and APKs are downloaded.
    downloads: PathBuf,
    /// Where packages for a package manager are downloaded.
    packages: PathBuf,
}

impl Updater {
    /// An updater for this copy of Sue, keeping its notes in `paths`.
    #[must_use]
    pub fn new(paths: &PlatformPaths) -> Self {
        let downloads = paths.home_dir.join("Downloads");
        Self {
            state: State::Idle,
            shown: false,
            install: Install::detect(),
            stamp: paths.data_dir.join("updates").join("checked.txt"),
            downloads: paths.cache_dir.join("updates"),
            packages: if downloads.is_dir() {
                downloads
            } else {
                paths.cache_dir.join("updates")
            },
        }
    }

    /// Checks now when the last automatic check was a day ago or more
    /// (and notes the time).
    pub fn check_if_due(&mut self, now: u64) {
        if self.install == Install::Browser || !matches!(self.state, State::Idle) {
            return;
        }
        let last = sue_platform::fs::read_to_string(&self.stamp)
            .ok()
            .and_then(|text| text.trim().parse::<u64>().ok())
            .unwrap_or(0);
        if now.saturating_sub(last) < DAY {
            return;
        }
        if let Some(dir) = self.stamp.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = sue_platform::write_atomic(&self.stamp, now.to_string().as_bytes());
        self.check(true);
    }

    /// Asks for the latest release. A quiet check (at startup) shows the
    /// window only when there is a newer one.
    pub fn check(&mut self, quiet: bool) {
        if matches!(
            self.state,
            State::Checking { .. } | State::Downloading { .. }
        ) {
            self.shown |= !quiet;
            return;
        }
        if self.install == Install::Browser {
            self.state = State::UpToDate;
            self.shown = !quiet;
            return;
        }
        self.state = State::Checking {
            job: update::check_latest(&user_agent()),
            quiet,
        };
        self.shown = !quiet;
    }

    /// `true` while a check or a download runs, so the runtime wakes up to
    /// read it.
    #[must_use]
    pub fn busy(&self) -> bool {
        matches!(
            self.state,
            State::Checking { .. } | State::Downloading { .. }
        )
    }

    /// Reads a running check or download.
    pub fn poll(&mut self) -> Option<Action> {
        match &self.state {
            State::Checking { job, quiet } => {
                let quiet = *quiet;
                let answer = job.take()?;
                self.state = match answer {
                    Ok(release) if update::is_newer(&release.tag, SUE_VERSION) => {
                        let plan = update::plan(&release, &self.install, std::env::consts::ARCH);
                        self.shown = true;
                        State::Available { release, plan }
                    }
                    Ok(_) => State::UpToDate,
                    Err(error) => {
                        if quiet {
                            eprintln!("sue: could not check for updates: {error}");
                        }
                        State::Failed {
                            release: None,
                            error,
                        }
                    }
                };
                None
            }
            State::Downloading { download, .. } => {
                let answer = download.take()?;
                let State::Downloading { release, plan, .. } =
                    std::mem::replace(&mut self.state, State::Idle)
                else {
                    return None;
                };
                match answer {
                    Ok(path) => self.downloaded(release, &plan, &path),
                    Err(error) => {
                        self.state = State::Failed {
                            release: Some(release),
                            error,
                        };
                        self.shown = true;
                        None
                    }
                }
            }
            _ => None,
        }
    }

    /// Hands a checked download to the system.
    fn downloaded(&mut self, release: Release, plan: &Plan, path: &Path) -> Option<Action> {
        let handed = match plan {
            Plan::Run(_) => update::run_update(path).map(|()| match self.install {
                Install::WindowsSetup => None,
                Install::MacApp => Some((tr!("update-dmg-opened"), None)),
                _ => Some((tr!("update-installer-opened"), None)),
            }),
            Plan::AppImage { path: running, .. } => {
                update::replace_appimage(path, running).map(|()| None)
            }
            Plan::Package { command, .. } => Ok(Some((
                tr!("update-package-downloaded", path = path.display()),
                Some(update::package_command(command, path)),
            ))),
            Plan::Command(_) | Plan::Page | Plan::Current => Ok(None),
        };
        match handed {
            // The update takes over: Sue closes.
            Ok(None) => {
                self.state = State::Idle;
                self.shown = false;
                Some(Action::Quit)
            }
            Ok(Some((note, command))) => {
                self.state = State::Handed {
                    release,
                    note,
                    command,
                };
                self.shown = true;
                None
            }
            Err(error) => {
                self.state = State::Failed {
                    release: Some(release),
                    error: error.to_string(),
                };
                self.shown = true;
                None
            }
        }
    }

    fn start_download(&mut self) {
        let State::Available { release, plan } = std::mem::replace(&mut self.state, State::Idle)
        else {
            return;
        };
        let (asset, dir) = match &plan {
            Plan::Run(asset) => (asset.clone(), self.downloads.clone()),
            // Beside the running AppImage, so it can take its place.
            Plan::AppImage { asset, path } => (
                asset.clone(),
                path.parent()
                    .map_or_else(|| self.downloads.clone(), Path::to_path_buf),
            ),
            Plan::Package { asset, .. } => (asset.clone(), self.packages.clone()),
            Plan::Command(_) | Plan::Page | Plan::Current => {
                self.state = State::Available { release, plan };
                return;
            }
        };
        let download = update::download(&release, &asset, &dir, &user_agent());
        self.state = State::Downloading {
            release,
            plan,
            download,
        };
    }

    /// Draws the window in the corner, when it shows.
    pub fn ui(&mut self, ctx: &egui::Context, theme: &Theme) -> Option<Action> {
        if !self.shown {
            return None;
        }
        let mut action = None;
        let mut close = false;
        let mut download = false;
        let screen = ctx.content_rect();
        let width = (screen.width() - 32.0).clamp(200.0, 380.0);
        egui::Area::new(Id::unique("sue-update"))
            .order(Order::Foreground)
            .pivot(Align2::RIGHT_BOTTOM)
            .fixed_pos(screen.right_bottom() - vec2(16.0, 48.0))
            .show(ctx, |ui| {
                widgets::window_frame(theme).show(ui, |ui| {
                    egui::Frame::new()
                        .inner_margin(egui::Margin::same(16))
                        .show(ui, |ui| {
                            ui.set_width(width);
                            self.body(ui, theme, &mut action, &mut close, &mut download);
                        });
                });
            });
        if download {
            self.start_download();
        }
        if close {
            self.shown = false;
            if !self.busy() {
                self.state = State::Idle;
            }
        }
        action
    }

    fn body(
        &self,
        ui: &mut egui::Ui,
        theme: &Theme,
        action: &mut Option<Action>,
        close: &mut bool,
        download: &mut bool,
    ) {
        let heading = |ui: &mut egui::Ui, text: String| {
            ui.label(
                RichText::new(text)
                    .size(theme.heading_size)
                    .color(theme.text),
            );
        };
        let muted = |ui: &mut egui::Ui, text: String| {
            ui.label(RichText::new(text).color(theme.muted));
        };
        let page = |release: Option<&Release>| {
            release.map_or_else(
                || update::LATEST_RELEASE_PAGE.to_owned(),
                |release| release.page.clone(),
            )
        };
        match &self.state {
            State::Idle => {}
            State::Checking { .. } => {
                heading(ui, tr!("checking-for-updates"));
                muted(ui, tr!("update-current-version", version = SUE_VERSION));
                ui.add_space(theme.padding);
                if widgets::pill_button(ui, &tr!("update-hide"), false, theme).clicked() {
                    *close = true;
                }
            }
            State::UpToDate => {
                heading(ui, tr!("sue-is-up-to-date"));
                muted(
                    ui,
                    if self.install == Install::Browser {
                        tr!("update-browser-is-newest")
                    } else {
                        tr!("update-current-version", version = SUE_VERSION)
                    },
                );
                ui.add_space(theme.padding);
                if widgets::pill_button(ui, &tr!("update-close"), false, theme).clicked() {
                    *close = true;
                }
            }
            State::Available { release, plan } => {
                heading(ui, tr!("update-available", version = release.version()));
                muted(ui, tr!("update-current-version", version = SUE_VERSION));
                notes(ui, theme, release);
                if let Plan::Command(command) = plan {
                    muted(ui, tr!("update-run-command"));
                    ui.label(RichText::new(command).monospace().color(theme.text));
                }
                ui.add_space(theme.padding);
                ui.horizontal_wrapped(|ui| {
                    let primary = match plan {
                        Plan::Run(_) if self.install == Install::WindowsSetup => {
                            Some(tr!("update-install-and-restart"))
                        }
                        Plan::Run(_) => Some(tr!("update-download-and-install")),
                        Plan::AppImage { .. } => Some(tr!("update-install-and-restart")),
                        Plan::Package { .. } => Some(tr!("update-download")),
                        Plan::Command(_) | Plan::Page | Plan::Current => None,
                    };
                    if let Some(label) = primary
                        && widgets::pill_button(ui, &label, true, theme).clicked()
                    {
                        *download = true;
                    }
                    if let Plan::Command(command) = plan
                        && widgets::pill_button(ui, &tr!("update-copy-command"), true, theme)
                            .clicked()
                    {
                        *action = Some(Action::Copy(command.clone()));
                    }
                    let open_page = matches!(plan, Plan::Page);
                    if widgets::pill_button(ui, &tr!("update-release-page"), open_page, theme)
                        .clicked()
                    {
                        *action = Some(Action::OpenUrl(release.page.clone()));
                    }
                    if widgets::pill_button(ui, &tr!("update-later"), false, theme).clicked() {
                        *close = true;
                    }
                });
            }
            State::Downloading {
                release, download, ..
            } => {
                heading(ui, tr!("update-downloading", version = release.version()));
                let (received, total) = download.progress();
                progress(ui, theme, received, total);
                muted(
                    ui,
                    if total > 0 {
                        tr!(
                            "update-progress",
                            received = human_bytes(received),
                            total = human_bytes(total)
                        )
                    } else {
                        human_bytes(received)
                    },
                );
                ui.add_space(theme.padding);
                if widgets::pill_button(ui, &tr!("update-hide"), false, theme).clicked() {
                    *close = true;
                }
            }
            State::Handed {
                release,
                note,
                command,
            } => {
                heading(ui, tr!("update-available", version = release.version()));
                muted(ui, note.clone());
                if let Some(command) = command {
                    ui.label(RichText::new(command).monospace().color(theme.text));
                }
                ui.add_space(theme.padding);
                ui.horizontal_wrapped(|ui| {
                    if let Some(command) = command
                        && widgets::pill_button(ui, &tr!("update-copy-command"), true, theme)
                            .clicked()
                    {
                        *action = Some(Action::Copy(command.clone()));
                    }
                    if self.install == Install::MacApp
                        && widgets::pill_button(ui, &tr!("update-quit-sue"), true, theme).clicked()
                    {
                        *action = Some(Action::Quit);
                    }
                    if widgets::pill_button(ui, &tr!("update-close"), false, theme).clicked() {
                        *close = true;
                    }
                });
            }
            State::Failed { release, error } => {
                heading(ui, tr!("update-failed"));
                muted(ui, error.clone());
                ui.add_space(theme.padding);
                ui.horizontal_wrapped(|ui| {
                    if widgets::pill_button(ui, &tr!("update-release-page"), true, theme).clicked()
                    {
                        *action = Some(Action::OpenUrl(page(release.as_ref())));
                    }
                    if widgets::pill_button(ui, &tr!("update-close"), false, theme).clicked() {
                        *close = true;
                    }
                });
            }
        }
    }
}

/// The release's notes, scrolling when they are long.
fn notes(ui: &mut egui::Ui, theme: &Theme, release: &Release) {
    let text = release.notes.trim();
    if text.is_empty() {
        return;
    }
    ui.add_space(theme.padding * 0.5);
    ScrollArea::vertical()
        .max_height(220.0)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(theme.small_size).color(theme.text));
        });
}

/// A thin bar filled as far as the download got.
fn progress(ui: &mut egui::Ui, theme: &Theme, received: u64, total: u64) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, 3.0, theme.active);
    if total > 0 {
        let done = (received as f32 / total as f32).clamp(0.0, 1.0);
        let mut filled = rect;
        filled.set_width(rect.width() * done);
        painter.rect_filled(filled, 3.0, theme.text);
    }
}

/// How Sue names itself to GitHub.
fn user_agent() -> String {
    format!("Sue/{SUE_VERSION}")
}
