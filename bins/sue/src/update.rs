//! Sue's own updates in the window: the check at startup (at most once a
//! day, unless `updates.check_automatically` is off), Check for Updates in
//! the Sue menu and in Settings, and a small window in the corner that
//! names the new release, shows its notes and updates the way this copy
//! was installed ([`sue_platform::update`] does the asking, choosing,
//! downloading and handing over). Nothing here blocks the interface: the
//! check and the download run on threads and are read between frames.

use std::path::{Path, PathBuf};
use std::time::Duration;

use egui::{Align2, Id, Label, RichText, Sense, Vec2, vec2};
use sue_core::SUE_VERSION;
use sue_platform::PlatformPaths;
use sue_platform::update::{self, Download, Install, Job, Plan, Release};
use sue_wm::Theme;
use sue_wm::icons::Icon;
use sue_wm::ui::{self, Answer, Dialog, Elevation, Layer, ScrollView, Space, TextRole, Tip};

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
    /// Check for updates again.
    CheckAgain,
}

/// What was pressed in the window this frame.
#[derive(Default)]
struct Press {
    download: bool,
    /// Cancel the download that runs.
    cancel: bool,
    again: bool,
    read: Option<Release>,
}

enum State {
    Idle,
    Checking {
        job: Job<Result<Release, String>>,
        quiet: bool,
    },
    /// The latest release is this one (kept for its notes, when known).
    UpToDate(Option<Release>),
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
        /// The download, to hand over again (Android, after the person
        /// cancelled or had to let Sue install apps first).
        again: Option<PathBuf>,
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
    /// The release whose whole notes are open, over everything.
    reading: Option<Release>,
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
            reading: None,
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

    /// Shows `release` as the newer one, as a check that found it would,
    /// without asking the network: for tests and the snapshot tour.
    pub(crate) fn offer(&mut self, release: Release) {
        let plan = update::plan(&release, &self.install, std::env::consts::ARCH);
        self.state = State::Available { release, plan };
        self.shown = true;
    }

    /// Shows a check that could not reach the releases, with `error`.
    pub(crate) fn fail(&mut self, error: String) {
        self.state = State::Failed {
            release: None,
            error,
        };
        self.shown = true;
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
            self.state = State::UpToDate(None);
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
        ) || (self.install == Install::Android && matches!(self.state, State::Handed { .. }))
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
                    Ok(release) => State::UpToDate(Some(release)),
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
            // Android's installer tells how it went.
            State::Handed { .. } => {
                let answer = update::installed()?;
                let State::Handed { release, again, .. } =
                    std::mem::replace(&mut self.state, State::Idle)
                else {
                    return None;
                };
                self.shown = true;
                self.state = match answer {
                    update::Installed::Cancelled => State::Handed {
                        release,
                        note: tr!("update-android-cancelled"),
                        command: None,
                        again,
                    },
                    update::Installed::Conflict => State::Failed {
                        release: Some(release),
                        error: tr!("update-android-conflict"),
                    },
                    update::Installed::Failed(error) => State::Failed {
                        release: Some(release),
                        error,
                    },
                };
                None
            }
            _ => None,
        }
    }

    /// Hands the downloaded update to the system again.
    fn hand_again(&mut self) {
        let State::Handed {
            release,
            again: Some(path),
            ..
        } = &self.state
        else {
            return;
        };
        let (release, path) = (release.clone(), path.clone());
        self.state = match update::run_update(&path) {
            Ok(()) => State::Handed {
                release,
                note: tr!("update-installer-opened"),
                command: None,
                again: Some(path),
            },
            Err(error) => State::Failed {
                release: Some(release),
                error: error.to_string(),
            },
        };
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
                let again = (self.install == Install::Android).then(|| path.to_path_buf());
                self.state = State::Handed {
                    release,
                    note,
                    command,
                    again,
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

    /// Stops the download that runs and goes back to offering the
    /// update. The part it had is removed by the download itself.
    fn cancel_download(&mut self) {
        match std::mem::replace(&mut self.state, State::Idle) {
            State::Downloading {
                release,
                plan,
                download,
            } => {
                download.cancel();
                self.state = State::Available { release, plan };
            }
            other => self.state = other,
        }
    }

    /// `true` while an update downloads (for tests).
    #[cfg(test)]
    fn downloading(&self) -> bool {
        matches!(self.state, State::Downloading { .. })
    }

    /// Draws the window in the corner, when it shows.
    pub fn ui(&mut self, ctx: &egui::Context, theme: &Theme) -> Option<Action> {
        let mut action = None;
        if let Some(release) = &self.reading {
            let mut close = false;
            read_notes(ctx, theme, release, &mut close, &mut action);
            if close {
                self.reading = None;
            }
        }
        if !self.shown {
            return action;
        }
        let mut close = false;
        let mut press = Press::default();
        let screen = ctx.content_rect();
        let edge = theme.space(Space::S6);
        // On a phone a card across the bottom, on a computer a card in
        // the corner, above the status bar.
        let phone = screen.width() < theme.space(Space::S9) * 14.0;
        let width = if phone {
            screen.width() - edge * 2.0
        } else {
            (screen.width() - edge * 2.0)
                .clamp(theme.space(Space::S9) * 6.0, theme.space(Space::S9) * 10.5)
        };
        let lift = theme.space(Space::S9) + theme.space(Space::S4);
        let (pivot, at) = if phone {
            (
                Align2::CENTER_BOTTOM,
                screen.center_bottom() - Vec2::Y * edge,
            )
        } else {
            (
                Align2::RIGHT_BOTTOM,
                screen.right_bottom() - vec2(edge, lift),
            )
        };
        egui::Area::new(Id::unique("sue-update"))
            .order(Layer::Toast.order())
            .pivot(pivot)
            .fixed_pos(at)
            .show(ctx, |ui| {
                ui::Card::new()
                    .surface(3)
                    .elevation(Elevation::Floating)
                    .padding(Space::S6)
                    .show(ui, theme, |ui| {
                        ui.set_width(width - theme.space(Space::S6) * 2.0);
                        self.body(ui, theme, &mut action, &mut close, &mut press);
                    });
            });
        if press.download {
            self.start_download();
        }
        if press.cancel {
            self.cancel_download();
        }
        if press.again {
            self.hand_again();
        }
        if let Some(release) = press.read {
            self.reading = Some(release);
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
        press: &mut Press,
    ) {
        let page = |release: Option<&Release>| {
            release.map_or_else(
                || update::LATEST_RELEASE_PAGE.to_owned(),
                |release| release.page.clone(),
            )
        };
        match &self.state {
            State::Idle => {}
            State::Checking { .. } => {
                ui.horizontal(|ui| {
                    ui::spinner()
                        .label(&tr!("checking-for-updates"))
                        .show(ui, theme);
                    heading(ui, theme, &tr!("checking-for-updates"));
                });
                muted(
                    ui,
                    theme,
                    &tr!("update-current-version", version = SUE_VERSION),
                );
                ui.add_space(theme.space(Space::S4));
                if button(
                    ui,
                    theme,
                    "sue-update-hide",
                    &tr!("update-hide"),
                    Look::Quiet,
                ) {
                    *close = true;
                }
            }
            State::UpToDate(release) => {
                heading(ui, theme, &tr!("sue-is-up-to-date"));
                muted(
                    ui,
                    theme,
                    &if self.install == Install::Browser {
                        tr!("update-browser-is-newest")
                    } else {
                        tr!("update-current-version", version = SUE_VERSION)
                    },
                );
                ui.add_space(theme.space(Space::S4));
                ui.horizontal_wrapped(|ui| {
                    if button(
                        ui,
                        theme,
                        "sue-update-close",
                        &tr!("update-close"),
                        Look::Main,
                    ) {
                        *close = true;
                    }
                    if let Some(release) = release {
                        read_button(ui, theme, release, press);
                    }
                });
            }
            State::Available { release, plan } => {
                heading(
                    ui,
                    theme,
                    &tr!("update-available", version = release.version()),
                );
                muted(
                    ui,
                    theme,
                    &tr!("update-current-version", version = SUE_VERSION),
                );
                notes(ui, theme, release);
                if let Plan::Command(command) = plan {
                    muted(ui, theme, &tr!("update-run-command"));
                    code(ui, theme, command);
                }
                ui.add_space(theme.space(Space::S4));
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
                        && button(ui, theme, "sue-update-install", &label, Look::Main)
                    {
                        press.download = true;
                    }
                    if let Plan::Command(command) = plan
                        && button(
                            ui,
                            theme,
                            "sue-update-copy",
                            &tr!("update-copy-command"),
                            Look::Main,
                        )
                    {
                        *action = Some(Action::Copy(command.clone()));
                    }
                    // Where Sue updates in place, the page is not needed.
                    if matches!(plan, Plan::Page | Plan::Current)
                        && button(
                            ui,
                            theme,
                            "sue-update-page",
                            &tr!("update-release-page"),
                            Look::Main,
                        )
                    {
                        *action = Some(Action::OpenUrl(release.page.clone()));
                    }
                    read_button(ui, theme, release, press);
                    if button(
                        ui,
                        theme,
                        "sue-update-later",
                        &tr!("update-later"),
                        Look::Quiet,
                    ) {
                        *close = true;
                    }
                });
            }
            State::Downloading {
                release, download, ..
            } => {
                heading(
                    ui,
                    theme,
                    &tr!("update-downloading", version = release.version()),
                );
                let (received, total) = download.progress();
                ui.add_space(theme.space(Space::S2));
                let bar = if total > 0 {
                    ui::progress_bar((received as f32 / total as f32).clamp(0.0, 1.0))
                } else {
                    ui::ProgressBar::indeterminate()
                };
                let said = tr!("update-downloading", version = release.version());
                bar.label(&said).show(ui, theme);
                muted(
                    ui,
                    theme,
                    &if total > 0 {
                        tr!(
                            "update-progress",
                            received = human_bytes(received),
                            total = human_bytes(total)
                        )
                    } else {
                        human_bytes(received)
                    },
                );
                ui.add_space(theme.space(Space::S4));
                ui.horizontal(|ui| {
                    if button(
                        ui,
                        theme,
                        "sue-update-cancel",
                        &tr!("update-cancel"),
                        Look::Plain,
                    ) {
                        press.cancel = true;
                    }
                    if button(
                        ui,
                        theme,
                        "sue-update-hide",
                        &tr!("update-hide"),
                        Look::Quiet,
                    ) {
                        *close = true;
                    }
                });
            }
            State::Handed {
                release,
                note,
                command,
                again,
            } => {
                heading(
                    ui,
                    theme,
                    &tr!("update-available", version = release.version()),
                );
                muted(ui, theme, note);
                if let Some(command) = command {
                    code(ui, theme, command);
                }
                ui.add_space(theme.space(Space::S4));
                ui.horizontal_wrapped(|ui| {
                    if again.is_some()
                        && button(
                            ui,
                            theme,
                            "sue-update-again",
                            &tr!("update-install"),
                            Look::Main,
                        )
                    {
                        press.again = true;
                    }
                    if let Some(command) = command
                        && button(
                            ui,
                            theme,
                            "sue-update-copy",
                            &tr!("update-copy-command"),
                            Look::Main,
                        )
                    {
                        *action = Some(Action::Copy(command.clone()));
                    }
                    if self.install == Install::MacApp
                        && button(
                            ui,
                            theme,
                            "sue-update-quit",
                            &tr!("update-quit-sue"),
                            Look::Main,
                        )
                    {
                        *action = Some(Action::Quit);
                    }
                    read_button(ui, theme, release, press);
                    if button(
                        ui,
                        theme,
                        "sue-update-close",
                        &tr!("update-close"),
                        Look::Quiet,
                    ) {
                        *close = true;
                    }
                });
            }
            State::Failed { release, error } => {
                ui.horizontal(|ui| {
                    let side = theme.text_size(TextRole::Heading);
                    let (mark, _) = ui.allocate_exact_size(Vec2::splat(side), Sense::hover());
                    sue_wm::icons::paint(ui.painter(), mark, Icon::Warning, theme.text);
                    heading(ui, theme, &tr!("update-failed"));
                });
                muted(ui, theme, error);
                ui.add_space(theme.space(Space::S4));
                ui.horizontal_wrapped(|ui| {
                    if button(
                        ui,
                        theme,
                        "sue-update-retry",
                        &tr!("update-try-again"),
                        Look::Main,
                    ) {
                        *action = Some(Action::CheckAgain);
                    }
                    if let Some(release) = release {
                        read_button(ui, theme, release, press);
                    }
                    if button(
                        ui,
                        theme,
                        "sue-update-page",
                        &tr!("update-release-page"),
                        Look::Plain,
                    ) {
                        *action = Some(Action::OpenUrl(page(release.as_ref())));
                    }
                    if button(
                        ui,
                        theme,
                        "sue-update-close",
                        &tr!("update-close"),
                        Look::Quiet,
                    ) {
                        *close = true;
                    }
                });
            }
        }
    }
}

/// How a button of the update window looks.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Look {
    /// What the window is there for.
    Main,
    /// Another answer.
    Plain,
    /// Away for now.
    Quiet,
}

/// A button of the update window, with its name and what it does as its
/// tooltip; `true` when pressed.
fn button(ui: &mut egui::Ui, theme: &Theme, salt: &str, label: &str, look: Look) -> bool {
    let button = ui::button(salt, label);
    let button = match look {
        Look::Main => button.primary(),
        Look::Plain => button,
        Look::Quiet => button.ghost(),
    };
    let response = button.show(ui, theme);
    ui::tooltip(response, &Tip::new(label), theme).clicked()
}

/// Release Notes: the whole notes, read in Sue (when there are any).
fn read_button(ui: &mut egui::Ui, theme: &Theme, release: &Release, press: &mut Press) {
    if !release.notes.trim().is_empty()
        && button(
            ui,
            theme,
            "sue-update-notes",
            &tr!("update-release-notes"),
            Look::Plain,
        )
    {
        press.read = Some(release.clone());
    }
}

/// The window's title line.
fn heading(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.add(
        Label::new(
            RichText::new(text)
                .font(theme.text_font(TextRole::Title))
                .color(theme.text),
        )
        .wrap(),
    );
}

/// A line in the secondary text color.
fn muted(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.add(
        Label::new(
            RichText::new(text)
                .font(theme.text_font(TextRole::Body))
                .color(theme.text_muted()),
        )
        .wrap(),
    );
}

/// A command to type, in the monospace face.
fn code(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    ui.add(
        Label::new(
            RichText::new(text)
                .font(theme.text_font(TextRole::Mono))
                .color(theme.text),
        )
        .wrap(),
    );
}

/// The release's notes, scrolling when they are long.
fn notes(ui: &mut egui::Ui, theme: &Theme, release: &Release) {
    let text = release.notes.trim();
    if text.is_empty() {
        return;
    }
    ui.add_space(theme.space(Space::S4));
    ScrollView::vertical("sue-update-notes")
        .max_height(theme.space(Space::S9) * 5.5)
        .auto_shrink(false, true)
        .show(ui, theme, |ui| markdown(ui, theme, text));
}

/// A release's whole notes over everything, in Sue's dialog: the width of
/// a phone, or a column in the middle of a computer's window.
fn read_notes(
    ctx: &egui::Context,
    theme: &Theme,
    release: &Release,
    close: &mut bool,
    action: &mut Option<Action>,
) {
    let screen = ctx.content_rect();
    let room = theme.space(Space::S9) * 18.0;
    let width = (screen.width() - theme.space(Space::S8)).min(room);
    let height = (screen.height() - theme.space(Space::S9) * 2.5).max(theme.space(Space::S9) * 5.0);
    let title = tr!("update-notes-title", version = release.version());
    let shown = Dialog::new("sue-release-notes", &title, theme)
        .width(width)
        .height(height)
        .accept(tr!("update-close"))
        .extra(tr!("update-release-page"))
        .scrim_cancels()
        .show(ctx, |ui| {
            ScrollView::vertical("sue-release-notes-text")
                .show(ui, theme, |ui| markdown(ui, theme, &release.notes));
        });
    match shown.answer {
        Some(Answer::Accept | Answer::Cancel) => *close = true,
        Some(Answer::Extra) => *action = Some(Action::OpenUrl(release.page.clone())),
        Some(Answer::Alternate) | None => {}
    }
}

/// Release notes as GitHub writes them, read plainly: headings, lists,
/// paragraphs and code, with the marks of bold, code and links taken off.
fn markdown(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    let mut fenced = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            code(ui, theme, line);
            continue;
        }
        if trimmed.is_empty() {
            ui.add_space(theme.space(Space::S4));
            continue;
        }
        let hashes = trimmed.chars().take_while(|c| *c == '#').count();
        if hashes > 0 && trimmed[hashes..].starts_with(' ') {
            let role = if hashes <= 2 {
                TextRole::Heading
            } else {
                TextRole::Title
            };
            ui.add_space(theme.space(Space::S4));
            ui.add(
                Label::new(
                    RichText::new(plain(trimmed[hashes..].trim()))
                        .font(theme.text_font(role))
                        .color(theme.text),
                )
                .wrap(),
            );
            continue;
        }
        let item = ["- ", "* ", "+ "]
            .iter()
            .find_map(|mark| trimmed.strip_prefix(mark));
        let body = |ui: &mut egui::Ui, words: String| {
            ui.add(
                Label::new(
                    RichText::new(words)
                        .font(theme.text_font(TextRole::Body))
                        .color(theme.text),
                )
                .wrap(),
            );
        };
        if let Some(rest) = item {
            let depth = (line.len() - line.trim_start().len()) as f32;
            ui.horizontal_top(|ui| {
                ui.add_space(depth * theme.space(Space::S3));
                let size = vec2(
                    theme.space(Space::S5),
                    theme.text_size(TextRole::Body) + theme.space(Space::S2),
                );
                let (dot, _) = ui.allocate_exact_size(size, Sense::hover());
                ui.painter().circle_filled(
                    dot.center(),
                    theme.space(Space::S1),
                    theme.text_muted(),
                );
                body(ui, plain(rest));
            });
        } else {
            body(ui, plain(trimmed));
        }
    }
}

/// `text` without Markdown's inline marks: `**bold**`, `code` and
/// `[links](to)` read as their words.
fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('[') {
        let after = &rest[start + 1..];
        let link = after.find("](").and_then(|middle| {
            after[middle + 2..]
                .find(')')
                .map(|end| (middle, middle + 2 + end))
        });
        if let Some((middle, end)) = link {
            out.push_str(&rest[..start]);
            out.push_str(&after[..middle]);
            rest = &after[end + 1..];
        } else {
            out.push_str(&rest[..=start]);
            rest = after;
        }
    }
    out.push_str(rest);
    out.replace("**", "").replace('`', "")
}

/// How Sue names itself to GitHub.
fn user_agent() -> String {
    format!("Sue/{SUE_VERSION}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_read_without_their_marks() {
        assert_eq!(
            plain("**Updates itself.** See [the page](https://example.com) and `sue`."),
            "Updates itself. See the page and sue."
        );
        assert_eq!(plain("a [b] c"), "a [b] c");
    }
}
