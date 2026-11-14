//! The View menu: the text, the corkboard and the outliner, pages, panels,
//! composition mode and zoom.

use sue_wm::{AppletContext, Call, Command, Facts, Flags, ItemState};

use crate::studio::{SueWriter, View};

const fn c(name: &'static str, label: &'static str) -> Command<SueWriter> {
    Command::new(name, label, "view", run).state(state)
}

pub(super) const COMMANDS: &[Command<SueWriter>] = &[
    c("view.document", "text").keys("Ctrl+1"),
    c("view.corkboard", "corkboard").keys("Ctrl+2"),
    c("view.outliner", "outliner").keys("Ctrl+3"),
    c("view.page", "page-view"),
    c("view.draft", "draft-view"),
    c("view.binder", "binder"),
    c("view.inspector", "inspector"),
    c("view.composition", "composition-mode")
        .keys("F11")
        .with(Flags::PALETTE),
    c("view.typewriter", "typewriter-scrolling"),
    c("view.zoom_in", "zoom-in"),
    c("view.zoom_out", "zoom-out"),
    c("view.zoom_reset", "actual-size"),
    c("view.zoom_fit", "fit-to-window"),
];

/// Does what a View menu command does.
fn run(writer: &mut SueWriter, _: &mut AppletContext<'_>, call: Call<'_>) {
    let command = call.name;
    match command {
        "view.document" => writer.view = View::Document,
        "view.corkboard" => writer.view = View::Corkboard,
        "view.outliner" => writer.view = View::Outliner,
        "view.page" | "view.draft" => {
            let pages = command == "view.page";
            if writer.phone {
                writer.phone_pages = pages;
            } else {
                writer.paged = pages;
            }
            writer.view = View::Document;
        }
        "view.composition" => {
            writer.composition = !writer.composition;
            writer.view = View::Document;
            writer.editor.focus();
        }
        "view.typewriter" => writer.typewriter = !writer.typewriter,
        "view.binder" => {
            if writer.phone {
                writer.phone_binder = !writer.phone_binder;
            } else {
                writer.binder = !writer.binder;
            }
        }
        "view.inspector" => writer.inspector = !writer.inspector,
        "view.zoom_in" => {
            writer.fit = false;
            writer.zoom = (writer.shown_zoom * 1.1).min(4.0);
        }
        "view.zoom_out" => {
            writer.fit = false;
            writer.zoom = (writer.shown_zoom / 1.1).max(0.25);
        }
        "view.zoom_reset" => {
            writer.fit = false;
            writer.zoom = 1.0;
        }
        "view.zoom_fit" => writer.fit = !writer.fit,
        _ => {}
    }
}

/// Whether a View menu command works now, and its check mark.
fn state(writer: &SueWriter, _: &Facts<'_>, call: Call<'_>) -> ItemState {
    let has_project = writer.project.is_some();
    match call.name {
        "view.page" => ItemState::check(writer.pages_shown() && writer.view == View::Document),
        "view.draft" => ItemState::check(!writer.pages_shown() && writer.view == View::Document),
        "view.document" => ItemState::check(writer.view == View::Document),
        "view.corkboard" => ItemState {
            enabled: has_project,
            checked: Some(writer.view == View::Corkboard),
        },
        "view.outliner" => ItemState {
            enabled: has_project,
            checked: Some(writer.view == View::Outliner),
        },
        "view.composition" => ItemState::check(writer.composition),
        "view.typewriter" => ItemState::check(writer.typewriter),
        "view.binder" => ItemState {
            enabled: has_project,
            checked: Some(writer.binder_shown()),
        },
        "view.inspector" => ItemState::check(writer.inspector && !writer.comments),
        _ => ItemState::on(),
    }
}
