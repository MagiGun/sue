//! Settings, Packages: the optional downloads. Files Sue can use but does
//! not ship (the Hand Scanner's hand tracking models, and whatever comes
//! later), each a card with what it is for, its size, its licence and
//! where it comes from, Install with progress, speed and Cancel, and
//! Uninstall. Install from a File adds a package someone shared: a
//! manifest file with its files next to it (format in
//! `docs/packages.md`).
//!
//! The cards are Sue's shared ones (`sue_wm::packages`) over the one
//! package center of the packages folder, so an install started in a
//! studio's dialog shows here too and can be cancelled here.

/// Text from the language database's `settings` namespace.
macro_rules! tr {
    ($($arg:tt)*) => { sue_lang::tr!("settings", $($arg)*) };
}

use std::path::Path;
use std::sync::Arc;

use egui::{Label, RichText, Ui};
use sue_platform::packages::PackageCenter;
use sue_wm::icons::Icon;
use sue_wm::ui::{self, Space, TextRole};
use sue_wm::{AppletContext, AppletId, FilePick, ShellRequest};

use super::{ID, Settings};

/// What the pick of a manifest file is for.
const PICK_MANIFEST: &str = "package-manifest";

/// The Packages rows between frames.
#[derive(Debug, Default)]
pub(super) struct PackagesUi {
    /// What the last Install from a File said: added, or what was wrong
    /// with the file.
    message: Option<(String, bool)>,
}

/// The package center of the packages folder Sue runs with.
pub(crate) fn center(cx: &AppletContext<'_>) -> Option<Arc<PackageCenter>> {
    cx.catalog
        .folders
        .get("packages")
        .map(|folder| PackageCenter::at(folder))
}

impl PackagesUi {
    /// A manifest file picked for Install from a File.
    pub(super) fn picked(&mut self, pick: &FilePick, path: &Path, cx: &mut AppletContext<'_>) {
        if pick.purpose != PICK_MANIFEST {
            return;
        }
        let Some(center) = center(cx) else {
            return;
        };
        // A manifest file carries no signature: while Trust asks for
        // signed packages only, it is refused, and the page says why.
        if cx
            .rice()
            .get_bool("packages.require_signatures")
            .unwrap_or(true)
        {
            self.message = Some((tr!("packages-file-unsigned"), true));
            return;
        }
        self.message = Some(match center.add_file(path) {
            Ok(id) => {
                let name = center.package(&id).map_or(id, |package| package.name);
                (tr!("packages-added", name = name), false)
            }
            Err(error) => (tr!("packages-file-refused", why = error.to_string()), true),
        });
    }
}

impl Settings {
    /// Packages, Optional downloads.
    pub(super) fn packages_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        let Some(center) = center(cx) else {
            return;
        };
        let theme = cx.theme.clone();
        let words = |ui: &mut Ui, text: &str, color: egui::Color32| {
            ui.add(
                Label::new(
                    RichText::new(text)
                        .font(theme.text_font(TextRole::Body))
                        .color(color),
                )
                .wrap(),
            );
        };
        words(ui, &tr!("packages-note"), theme.text_muted());
        ui.add_space(theme.space(Space::S3));
        for package in center.packages() {
            sue_wm::packages::package_card(ui, &theme, &center, &package, true);
            ui.add_space(theme.space(Space::S3));
        }
        ui.horizontal(|ui| {
            let help = tr!("packages-install-file-hint");
            if ui::button("sue-packages-install-file", &tr!("packages-install-file"))
                .icon(Icon::FolderOpen)
                .hint(&help)
                .show(ui, &theme)
                .clicked()
            {
                let pick = FilePick::open(
                    AppletId::new(ID).ok(),
                    PICK_MANIFEST,
                    &tr!("packages-install-file"),
                )
                .extensions(&["suepkg"]);
                cx.request(ShellRequest::PickFile(pick));
            }
        });
        if let Some((message, failed)) = &self.packages.message {
            ui.add_space(theme.space(Space::S2));
            let color = if *failed { theme.danger() } else { theme.text };
            words(ui, message, color);
        }
        words(
            ui,
            &tr!(
                "packages-folder",
                path = center.home().root().display().to_string()
            ),
            theme.text_muted(),
        );
        ui.add_space(theme.space(Space::S3));
    }
}
