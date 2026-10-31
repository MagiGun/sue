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
    /// Check for updates again.
    CheckAgain,
}

/// What was pressed in the window this frame.
#[derive(Default)]
struct Press {
    download: bool,
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
        // On a phone a sheet across the bottom, on a computer a window in
        // the corner.
        let phone = screen.width() < 560.0;
        let width = if phone {
            screen.width() - 24.0
        } else {
            (screen.width() - 32.0).clamp(200.0, 380.0) + 32.0
        };
        let (pivot, at) = if phone {
            (
                Align2::CENTER_BOTTOM,
                screen.center_bottom() - vec2(0.0, 24.0),
            )
        } else {
            (
                Align2::RIGHT_BOTTOM,
                screen.right_bottom() - vec2(16.0, 48.0),
            )
        };
        egui::Area::new(Id::unique("sue-update"))
            .order(Order::Foreground)
            .pivot(pivot)
            .fixed_pos(at)
            .show(ctx, |ui| {
                widgets::window_frame(theme).show(ui, |ui| {
                    egui::Frame::new()
                        .inner_margin(egui::Margin::same(16))
                        .show(ui, |ui| {
                            ui.set_width(width - 32.0);
                            self.body(ui, theme, &mut action, &mut close, &mut press);
                        });
                });
            });
        if press.download {
            self.start_download();
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
        // Release Notes: the whole notes, read in Sue.
        let read = |ui: &mut egui::Ui, release: &Release, press: &mut Press| {
            if !release.notes.trim().is_empty()
                && widgets::pill_button(ui, &tr!("update-release-notes"), false, theme).clicked()
            {
                press.read = Some(release.clone());
            }
        };
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
            State::UpToDate(release) => {
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
                ui.horizontal_wrapped(|ui| {
                    if let Some(release) = release {
                        read(ui, release, press);
                    }
                    if widgets::pill_button(ui, &tr!("update-close"), false, theme).clicked() {
                        *close = true;
                    }
                });
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
                        press.download = true;
                    }
                    read(ui, release, press);
                    if let Plan::Command(command) = plan
                        && widgets::pill_button(ui, &tr!("update-copy-command"), true, theme)
                            .clicked()
                    {
                        *action = Some(Action::Copy(command.clone()));
                    }
                    // Where Sue updates in place, the page is not needed.
                    if matches!(plan, Plan::Page | Plan::Current)
                        && widgets::pill_button(ui, &tr!("update-release-page"), true, theme)
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
                again,
            } => {
                heading(ui, tr!("update-available", version = release.version()));
                muted(ui, note.clone());
                if let Some(command) = command {
                    ui.label(RichText::new(command).monospace().color(theme.text));
                }
                ui.add_space(theme.padding);
                ui.horizontal_wrapped(|ui| {
                    if again.is_some()
                        && widgets::pill_button(ui, &tr!("update-install"), true, theme).clicked()
                    {
                        press.again = true;
                    }
                    if let Some(command) = command
                        && widgets::pill_button(ui, &tr!("update-copy-command"), true, theme)
                            .clicked()
                    {
                        *action = Some(Action::Copy(command.clone()));
                    }
                    read(ui, release, press);
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
                    if widgets::pill_button(ui, &tr!("update-try-again"), true, theme).clicked() {
                        *action = Some(Action::CheckAgain);
                    }
                    if let Some(release) = release {
                        read(ui, release, press);
                    }
                    if widgets::pill_button(ui, &tr!("update-release-page"), false, theme).clicked()
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
        .show(ui, |ui| markdown(ui, theme, text));
}

/// A release's whole notes over everything, as a page: the width of a
/// phone, or a column in the middle of a computer's window.
fn read_notes(
    ctx: &egui::Context,
    theme: &Theme,
    release: &Release,
    close: &mut bool,
    action: &mut Option<Action>,
) {
    let screen = ctx.content_rect();
    let phone = screen.width() < 560.0;
    let size = if phone {
        screen.size() - vec2(16.0, 48.0)
    } else {
        vec2(
            (screen.width() - 64.0).min(720.0),
            (screen.height() - 96.0).max(200.0),
        )
    };
    egui::Area::new(Id::unique("sue-release-notes"))
        .order(Order::Foreground)
        .pivot(Align2::CENTER_CENTER)
        .fixed_pos(screen.center())
        .show(ctx, |ui| {
            widgets::window_frame(theme).show(ui, |ui| {
                egui::Frame::new()
                    .inner_margin(egui::Margin::same(16))
                    .show(ui, |ui| {
                        ui.set_width(size.x - 32.0);
                        ui.set_height(size.y - 32.0);
                        ui.label(
                            RichText::new(tr!("update-notes-title", version = release.version()))
                                .size(theme.heading_size)
                                .color(theme.text),
                        );
                        ui.add_space(theme.padding * 0.5);
                        let bottom = theme.pill_height + theme.padding * 2.0;
                        ScrollArea::vertical()
                            .max_height(ui.available_height() - bottom)
                            .auto_shrink([false, false])
                            .show(ui, |ui| markdown(ui, theme, &release.notes));
                        ui.add_space(theme.padding);
                        ui.horizontal_wrapped(|ui| {
                            if widgets::pill_button(ui, &tr!("update-close"), true, theme).clicked()
                            {
                                *close = true;
                            }
                            if widgets::pill_button(ui, &tr!("update-release-page"), false, theme)
                                .clicked()
                            {
                                *action = Some(Action::OpenUrl(release.page.clone()));
                            }
                        });
                    });
            });
        });
}

/// Release notes as GitHub writes them, read plainly: headings, lists,
/// paragraphs and code, with the marks of bold, code and links taken off.
fn markdown(ui: &mut egui::Ui, theme: &Theme, text: &str) {
    let mut code = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            code = !code;
            continue;
        }
        if code {
            ui.label(RichText::new(line).monospace().color(theme.text));
            continue;
        }
        if trimmed.is_empty() {
            ui.add_space(theme.padding * 0.5);
            continue;
        }
        let hashes = trimmed.chars().take_while(|c| *c == '#').count();
        if hashes > 0 && trimmed[hashes..].starts_with(' ') {
            let size = if hashes <= 2 {
                theme.heading_size
            } else {
                theme.ui_size * 1.1
            };
            ui.add_space(theme.padding * 0.5);
            ui.label(
                RichText::new(plain(trimmed[hashes..].trim()))
                    .size(size)
                    .color(theme.text),
            );
            continue;
        }
        let item = ["- ", "* ", "+ "]
            .iter()
            .find_map(|mark| trimmed.strip_prefix(mark));
        if let Some(rest) = item {
            let indent = (line.len() - line.trim_start().len()) as f32 * 6.0;
            ui.horizontal_top(|ui| {
                ui.add_space(indent);
                let (dot, _) = ui.allocate_exact_size(
                    vec2(theme.ui_size * 0.9, theme.ui_size * 1.3),
                    Sense::hover(),
                );
                ui.painter().circle_filled(dot.center(), 2.5, theme.muted);
                ui.add(egui::Label::new(RichText::new(plain(rest)).color(theme.text)).wrap());
            });
        } else {
            ui.add(egui::Label::new(RichText::new(plain(trimmed)).color(theme.text)).wrap());
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
