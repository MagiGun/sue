//! Optional packages, as people see them: one card per package with what
//! it is for, how large it is, its licence and where it comes from, and
//! the buttons to install it (with progress, speed and Cancel), to try
//! again after a failure and to uninstall it.
//!
//! Sue owns this: Settings, Packages lists every package with these
//! cards, and a studio that needs a package (Sue Paint's Hand Scanner
//! needs Hand tracking) shows the same card in its own dialog. Both look
//! at the one [`PackageCenter`], so an install started in one place shows
//! and can be cancelled in the other.

use egui::{Label, RichText, Ui};
use sue_platform::packages::{OptionalPackage, PackageCenter, PackageState, PkgError};

use crate::Theme;
use crate::ui::{self, Space, TextRole, Tip};

/// `1.4 MiB`, `812.0 KiB`, `96 B`.
#[must_use]
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Why an install failed, in words for people.
#[must_use]
pub fn failure(error: &PkgError) -> String {
    match error {
        PkgError::Offline(_) => tr!("package-offline"),
        PkgError::HashMismatch { .. } | PkgError::SizeMismatch { .. } => tr!("package-damaged"),
        PkgError::Io(why) => tr!("package-disk", why = why),
        other => tr!("package-failed", why = other.to_string()),
    }
}

/// What a package's state reads as, in a word or two.
#[must_use]
pub fn state_word(state: &PackageState) -> String {
    match state {
        PackageState::Available | PackageState::Failed(_) => tr!("package-not-installed"),
        PackageState::Installing { .. } => tr!("package-installing"),
        PackageState::Installed => tr!("package-installed"),
    }
}

/// The line under an install's progress bar: how much of how much, and
/// how fast.
#[must_use]
pub fn progress_line(done: u64, total: u64, speed: f64) -> String {
    let amount = tr!(
        "package-progress",
        done = human_bytes(done),
        total = human_bytes(total)
    );
    if speed >= 1.0 {
        tr!(
            "package-progress-speed",
            amount = amount,
            speed = human_bytes(speed as u64)
        )
    } else {
        amount
    }
}

fn words(ui: &mut Ui, text: &str, role: TextRole, color: egui::Color32, theme: &Theme) {
    ui.add(Label::new(RichText::new(text).font(theme.text_font(role)).color(color)).wrap());
}

/// Draws the card of `package`: what it is and the buttons its state
/// calls for. A click acts on `center` at once (install, cancel, try
/// again, uninstall). Gives the state the package is in after that.
///
/// With `removable` off the card has no Uninstall button (a dialog that
/// only needs the package installed).
pub fn package_card(
    ui: &mut Ui,
    theme: &Theme,
    center: &PackageCenter,
    package: &OptionalPackage,
    removable: bool,
) -> PackageState {
    let id = package.id.as_str();
    let mut state = center.state(id);
    ui::Card::new().show(ui, theme, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            words(ui, &package.name, TextRole::Title, theme.text, theme);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let color = if state == PackageState::Installed {
                    theme.success()
                } else {
                    theme.text_muted()
                };
                words(ui, &state_word(&state), TextRole::Caption, color, theme);
            });
        });
        if !package.summary.is_empty() {
            words(ui, &package.summary, TextRole::Body, theme.text, theme);
        }
        ui.add_space(theme.space(Space::S1));
        let mut facts = vec![
            tr!("package-size", size = human_bytes(package.size())),
            tr!("package-version", version = package.version.to_string()),
        ];
        if !package.licence.is_empty() {
            facts.push(tr!("package-licence", licence = package.licence.as_str()));
        }
        words(
            ui,
            &facts.join(", "),
            TextRole::Caption,
            theme.text_muted(),
            theme,
        );
        if !package.author.is_empty() {
            words(
                ui,
                &tr!("package-from", author = package.author.as_str()),
                TextRole::Caption,
                theme.text_muted(),
                theme,
            );
        }
        if !package.homepage.is_empty() {
            words(
                ui,
                &package.homepage,
                TextRole::Caption,
                theme.text_muted(),
                theme,
            );
        }
        ui.add_space(theme.space(Space::S3));
        match &state {
            PackageState::Installing { done, total, speed } => {
                let said = tr!("package-installing-name", name = package.name.as_str());
                let bar = if *total > 0 {
                    ui::ProgressBar::new((*done as f32 / *total as f32).clamp(0.0, 1.0))
                } else {
                    ui::ProgressBar::indeterminate()
                };
                bar.label(&said).show(ui, theme);
                ui.add_space(theme.space(Space::S1));
                ui.horizontal(|ui| {
                    let cancel = tr!("package-cancel");
                    let response = ui::button(("sue-package-cancel", id), &cancel).show(ui, theme);
                    if ui::tooltip(response, &Tip::new(&cancel), theme).clicked() {
                        center.cancel(id);
                    }
                    words(
                        ui,
                        &progress_line(*done, *total, *speed),
                        TextRole::Caption,
                        theme.text_muted(),
                        theme,
                    );
                });
                // The bar moves by itself.
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(100));
            }
            PackageState::Installed => {
                if removable {
                    let label = tr!("package-uninstall");
                    let response =
                        ui::button(("sue-package-uninstall", id), &label).show(ui, theme);
                    let help = tr!("package-uninstall-hint");
                    if ui::tooltip(response, &Tip::new(&label).help(&help), theme).clicked() {
                        let _ = center.uninstall(id);
                    }
                }
            }
            PackageState::Available | PackageState::Failed(_) => {
                if let PackageState::Failed(error) = &state {
                    words(ui, &failure(error), TextRole::Body, theme.danger(), theme);
                    ui.add_space(theme.space(Space::S1));
                }
                let label = if matches!(state, PackageState::Failed(_)) {
                    tr!("package-try-again")
                } else {
                    tr!("package-install", size = human_bytes(package.size()))
                };
                let response = ui::button(("sue-package-install", id), &label)
                    .primary()
                    .show(ui, theme);
                let help = tr!("package-install-hint");
                if ui::tooltip(response, &Tip::new(&label).help(&help), theme).clicked() {
                    center.install(id);
                    ui.ctx().request_repaint();
                }
            }
        }
    });
    let after = center.state(id);
    if after != state {
        ui.ctx().request_repaint();
        state = after;
    }
    state
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_and_progress_read_plainly() {
        assert_eq!(human_bytes(96), "96 B");
        assert_eq!(human_bytes(8_005_355), "7.6 MiB");
        assert_eq!(
            progress_line(1_048_576, 8_388_608, 0.0),
            "1.0 MiB of 8.0 MiB"
        );
        assert_eq!(
            progress_line(1_048_576, 8_388_608, 2_097_152.0),
            "1.0 MiB of 8.0 MiB, 2.0 MiB a second"
        );
    }

    #[test]
    fn failures_say_what_to_do() {
        assert!(failure(&PkgError::Offline("dns".to_owned())).contains("internet"));
        assert!(failure(&PkgError::Io("disk full".to_owned())).contains("disk full"));
        assert!(failure(&PkgError::Fetch("404".to_owned())).contains("404"));
        assert_eq!(state_word(&PackageState::Installed), "Installed");
        assert_eq!(state_word(&PackageState::Available), "Not installed");
    }
}
