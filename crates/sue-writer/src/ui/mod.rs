//! Drawing Sue Writer: the menu bar and toolbar, the binder, the text (or
//! the corkboard, or the outliner), the inspector, the status bar, the find
//! bar, composition mode, the start screen and the dialogs.

use egui::{Color32, Rect, Sense, Ui, UiBuilder, pos2, vec2};
use sue_model::manuscript::{self, role};
use sue_wm::{AppletContext, AppletId, ItemState};

use crate::project;
use crate::studio::{ID, SueWriter, View};

mod binder;
mod comments;
mod corkboard;
mod dialogs;
mod editor;
mod inspector;
mod outliner;
mod phone;
mod research;
mod settings;
mod start;
mod status;
mod toolbar;
mod widgets;

use editor::keeps_typing;
use widgets::splitter;

const TOOLBAR_HEIGHT: f32 = 36.0;
/// What a tool button senses: clicks, but never the keyboard's focus,
/// which stays with the text.
const TOOL: Sense = Sense::CLICK.union(Sense::HOVER);
const STATUS_HEIGHT: f32 = 24.0;
const FIND_HEIGHT: f32 = 34.0;
/// A thumb-sized button on a phone.
const THUMB: f32 = 44.0;
/// A phone's formatting bar: one row of thumb-sized buttons.
const PHONE_BAR: f32 = THUMB + 8.0;
/// The room beside a phone's lines, so that they get the screen's width.
const PHONE_GAP: f32 = 14.0;

fn rgb(color: [u8; 3]) -> Color32 {
    Color32::from_rgb(color[0], color[1], color[2])
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl SueWriter {
    /// Builds the menus again when the language changed, and keeps their
    /// keys in step with the settings.
    pub(crate) fn follow_language(&mut self, cx: &AppletContext<'_>) {
        if self.language.changed() {
            self.menus = crate::menus::menus();
            self.bound.invalidate();
        }
        self.bound.refresh(
            &mut self.menus,
            cx.rice(),
            crate::studio::KEYS_SECTION,
            crate::studio::shortcut_table(),
        );
    }

    /// Draws the studio.
    pub(crate) fn frame(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        self.follow_language(cx);
        let present = cx.view.page.as_ref().map(AppletId::as_str) == Some(ID);
        let reveal = cx.view.reveal.clamp(0.0, 1.0);
        self.present = present;
        self.ready = present && reveal >= 0.999;
        self.reveal = reveal;
        if present && !self.shown {
            self.shown = true;
            if let Some(root) = self
                .project
                .filter(|root| cx.arena().get_by_id(*root).is_ok())
                && self.session.is_none()
            {
                let words = manuscript::total_words(cx.arena(), root);
                self.session = Some((root, words, project::today()));
            }
        }
        // What was open may have been undone away.
        if self
            .project
            .is_some_and(|root| cx.arena().get_by_id(root).is_err())
        {
            self.project = None;
            self.open = None;
        }
        if self
            .open
            .is_some_and(|open| cx.arena().get_by_id(open).is_err())
        {
            self.open = self
                .project
                .and_then(|root| manuscript::folder(cx.arena(), root, role::DRAFT));
        }
        if self
            .selected
            .is_some_and(|id| cx.arena().get_by_id(id).is_err())
        {
            self.selected = self.open;
        }
        let ctx = ui.ctx().clone();
        self.phone = cx.view.form_factor == sue_platform::FormFactor::Mobile;
        if self.ready {
            self.keys(&ctx, cx);
        }
        // A press on the Writer's own bars leaves the keyboard with the
        // text: on a phone, tapping Bold keeps the on-screen keyboard up.
        let on_bar = ctx.input(|input| {
            (input.pointer.any_pressed() || input.pointer.any_released())
                && input
                    .pointer
                    .interact_pos()
                    .is_some_and(|pos| self.bars.iter().any(|bar| bar.contains(pos)))
        });
        if on_bar {
            self.editor.keep_focus();
        }
        let theme = cx.theme.clone();
        let full = ui.max_rect();
        // Coming in, the page turns opaque first; the bars and panels slide
        // in from the edges around it. Going out, the other way.
        let hide = 1.0 - smooth(reveal);
        let cover = smooth(reveal / 0.35);
        ui.painter()
            .rect_filled(full, 0.0, theme.void.gamma_multiply(cover));
        if self.composition {
            ui.set_opacity(cover.max(0.02));
            self.composition_ui(ui, full, cx, &theme);
            self.dialog_ui(&ctx, cx, &theme);
            return;
        }
        let simple = self.simple(cx);
        let places = self.places(full, simple, self.project.is_some(), self.open.is_some());
        // The text first, so the panels sliding in pass over it (on a
        // phone the binder or the inspector takes the text's place).
        if places.center.is_positive() {
            let mut center = ui.new_child(UiBuilder::new().max_rect(places.center));
            center.set_clip_rect(places.center.intersect(ui.clip_rect()));
            center.set_opacity(cover.max(0.02));
            self.center_ui(&mut center, places.center, cx, &theme);
        }
        // The menus are in Sue's top bar (`menu_bar`). The toolbar slides
        // down from the page's top; a phone's comes up from the bottom,
        // where it sits right above the on-screen keyboard.
        let toolbar = if self.phone {
            places
                .toolbar
                .translate(vec2(0.0, hide * (full.bottom() - places.toolbar.top())))
        } else {
            let lift = hide * (places.body.top() - full.top());
            places.toolbar.translate(vec2(0.0, -lift))
        };
        if self.phone {
            if toolbar.height() > 0.0 {
                self.phone_toolbar(ui, toolbar, cx, &theme);
            }
        } else {
            self.toolbar(ui, toolbar, cx, &theme);
        }
        self.bars.clear();
        self.bars.push(toolbar);
        self.bars.extend(self.top_bar);
        if self.find.is_some() {
            let top = if self.phone {
                full.top()
            } else {
                toolbar.bottom()
            };
            let find = Rect::from_min_size(pos2(full.left(), top), vec2(full.width(), FIND_HEIGHT));
            self.find_bar(ui, find, cx, &theme);
        }
        let body = places.body;
        if let Some(rect) = places.binder {
            let slid = rect.translate(vec2(-hide * (rect.width() + 2.0), 0.0));
            self.binder_ui(ui, slid, cx, &theme);
            if hide < 0.01
                && let Some(width) = splitter(
                    ui,
                    "writer-binder-edge",
                    rect.right(),
                    body.y_range(),
                    &theme,
                )
            {
                self.binder_width = (width - body.left()).clamp(150.0, 480.0);
            }
        }
        if let Some(rect) = places.inspector {
            let slid = rect.translate(vec2(hide * (rect.width() + 2.0), 0.0));
            self.inspector_ui(ui, slid, cx, &theme);
            if hide < 0.01
                && let Some(x) = splitter(
                    ui,
                    "writer-inspector-edge",
                    rect.left(),
                    body.y_range(),
                    &theme,
                )
            {
                self.inspector_width = (body.right() - x).clamp(190.0, 520.0);
            }
        }
        let status = places.status.translate(vec2(0.0, hide * STATUS_HEIGHT));
        self.status_ui(ui, status, cx, &theme);
        self.dialog_ui(&ctx, cx, &theme);
    }

    /// Sue Writer's menus, in Sue's top bar after the Sue button (none in
    /// composition mode, which keeps only the text).
    pub(crate) fn menu_bar(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        if self.composition {
            return;
        }
        let theme = cx.theme.clone();
        let rect = ui.max_rect();
        if cx.view.form_factor == sue_platform::FormFactor::Mobile {
            self.top_bar = Some(rect);
            if let Some(command) = self.phone_top_bar(ui, rect, cx, &theme) {
                self.run_typing(&command, cx);
            }
            return;
        }
        self.top_bar = None;
        let picked = {
            let mut menubar = std::mem::take(&mut self.menubar);
            let state = |command: &str| self.item_state(command, cx);
            let picked = menubar.ui(ui, rect, &self.menus, &state, &theme);
            self.menubar = menubar;
            picked
        };
        if let Some(command) = picked {
            self.run_typing(&command, cx);
        }
        // A tablet's top bar has room past the menus for quick tools.
        if cx.view.form_factor == sue_platform::FormFactor::Tablet {
            let tools = Rect::from_min_max(
                pos2(rect.left() + rect.width() * 0.55, rect.top()),
                rect.max,
            );
            self.tablet_tools(ui, tools, cx, &theme);
            if let Some(command) = self.pending_command.take() {
                self.run_typing(&command, cx);
            }
        }
    }

    /// Runs `command`, and gives the keyboard back to the text after one
    /// that changes how it looks, so that writing goes on (on a phone,
    /// with the on-screen keyboard up).
    pub(crate) fn run_typing(&mut self, command: &str, cx: &mut AppletContext<'_>) {
        self.run(command, cx);
        if keeps_typing(command) && self.view == View::Document {
            self.editor.focus();
        }
    }

    /// Where everything goes in `full`: the bars, the binder and inspector
    /// (when they fit), and the text between them.
    pub(crate) fn places(
        &self,
        full: Rect,
        simple: bool,
        has_project: bool,
        has_open: bool,
    ) -> Places {
        let find = if self.find.is_some() {
            FIND_HEIGHT
        } else {
            0.0
        };
        let status = Rect::from_min_max(pos2(full.left(), full.bottom() - STATUS_HEIGHT), full.max);
        if self.phone {
            // The formatting sits at the bottom, above the status bar and so
            // right above the on-screen keyboard; the binder or the
            // inspector, when asked for, takes the whole page in turn.
            // The start page (no project yet) has nothing to format.
            let bar = if has_project { PHONE_BAR } else { 0.0 };
            let toolbar = Rect::from_min_max(
                pos2(full.left(), status.top() - bar),
                pos2(full.right(), status.top()),
            );
            let body = Rect::from_min_max(
                pos2(full.left(), full.top() + find),
                pos2(full.right(), toolbar.top()),
            );
            let binder = (self.phone_binder && !simple && has_project).then_some(body);
            let inspector =
                (binder.is_none() && self.inspector && (has_project || has_open)).then_some(body);
            let center = if binder.is_some() || inspector.is_some() {
                Rect::NOTHING
            } else {
                body
            };
            return Places {
                body,
                toolbar,
                status,
                binder,
                inspector,
                center,
            };
        }
        let toolbar = Rect::from_min_size(full.min, vec2(full.width(), self.toolbar_height));
        let top = toolbar.bottom() + find;
        let body = Rect::from_min_max(pos2(full.left(), top), pos2(full.right(), status.top()));
        let mut center = body;
        let mut binder = None;
        if self.binder && !simple && has_project && body.width() > 520.0 {
            let width = self
                .binder_width
                .clamp(150.0, (body.width() * 0.4).max(150.0));
            let rect = Rect::from_min_max(body.min, pos2(body.left() + width, body.bottom()));
            center.min.x = rect.right() + 1.0;
            binder = Some(rect);
        }
        // The inspector folds away before the text gets too narrow to write in
        // (and a phone's narrow page never turns its bounds around).
        let width = self
            .inspector_width
            .clamp(190.0, (body.width() * 0.35).max(190.0));
        let mut inspector = None;
        if self.inspector && center.width() - width >= 520.0 && (has_project || has_open) {
            let rect = Rect::from_min_max(pos2(body.right() - width, body.top()), body.max);
            center.max.x = rect.left() - 1.0;
            inspector = Some(rect);
        }
        Places {
            body,
            toolbar,
            status,
            binder,
            inspector,
            center,
        }
    }

    /// Whether a menu item works, and its check mark.
    fn item_state(&self, command: &str, cx: &AppletContext<'_>) -> ItemState {
        let has_project = self.project.is_some();
        let has_text = self.editor.current().is_some();
        match command {
            "view.page" => ItemState::check(self.pages_shown() && self.view == View::Document),
            "view.draft" => ItemState::check(!self.pages_shown() && self.view == View::Document),
            "view.document" => ItemState::check(self.view == View::Document),
            "view.corkboard" => ItemState {
                enabled: has_project,
                checked: Some(self.view == View::Corkboard),
            },
            "view.outliner" => ItemState {
                enabled: has_project,
                checked: Some(self.view == View::Outliner),
            },
            "view.composition" => ItemState::check(self.composition),
            "view.typewriter" => ItemState::check(self.typewriter),
            "view.binder" => ItemState {
                enabled: has_project,
                checked: Some(self.binder_shown()),
            },
            "view.inspector" => ItemState::check(self.inspector && !self.comments),
            "review.comments" => ItemState::check(self.inspector && self.comments),
            "review.track" => ItemState::check(self.tracking(cx)),
            "review.spelling" => ItemState::check(self.spell_check(cx)),
            "project.include" => ItemState {
                enabled: self.selected.is_some(),
                checked: self
                    .selected
                    .and_then(|id| cx.arena().get_by_id(id).ok())
                    .map(manuscript::included),
            },
            "binder.new_doc"
            | "binder.new_folder"
            | "project.duplicate"
            | "project.trash"
            | "project.empty_trash"
            | "project.merge"
            | "project.statistics"
            | "project.settings" => {
                if has_project {
                    ItemState::on()
                } else {
                    ItemState::off()
                }
            }
            command if command.starts_with("status.") || command.starts_with("label.") => {
                if has_project {
                    ItemState::on()
                } else {
                    ItemState::off()
                }
            }
            command
                if command.starts_with("format.")
                    || command.starts_with("align.")
                    || command.starts_with("insert.")
                    || command.starts_with("style.")
                    || command.starts_with("list.")
                    || command.starts_with("indent.")
                    || command.starts_with("spacing.")
                    || command == "project.split"
                    || command == "project.snapshot" =>
            {
                if has_text {
                    ItemState::on()
                } else {
                    ItemState::off()
                }
            }
            command if command.starts_with("compile.") => {
                if has_project || has_text {
                    ItemState::on()
                } else {
                    ItemState::off()
                }
            }
            _ => ItemState::on(),
        }
    }

    // ----- Toolbar -------------------------------------------------------------
}

/// Where the studio's parts go.
pub(crate) struct Places {
    /// Between the bars.
    pub body: Rect,
    /// The formatting toolbar: at the top, or a phone's at the bottom.
    pub toolbar: Rect,
    /// The status bar.
    pub status: Rect,
    /// The binder, when it shows.
    pub binder: Option<Rect>,
    /// The inspector, when it shows.
    pub inspector: Option<Rect>,
    /// The text, or the corkboard or outliner.
    pub center: Rect,
}

/// What a dialog's button does once the dialog is drawn.
type After = Box<dyn FnOnce(&mut SueWriter, &mut AppletContext<'_>)>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_lays_out_on_a_phone_and_on_a_desktop() {
        let mut writer = SueWriter::default();
        // A narrow window on a desk: the text takes it all, the panels
        // fold away.
        let narrow = Rect::from_min_size(pos2(0.0, 52.0), vec2(390.0, 760.0));
        let places = writer.places(narrow, false, true, true);
        assert!(places.binder.is_none() && places.inspector.is_none());
        assert_eq!(places.center.width(), 390.0);
        // A desktop's page has room for the binder, and for the inspector
        // once it is asked for (it starts hidden, as a word processor's
        // side panels do).
        let desktop = Rect::from_min_size(pos2(0.0, 28.0), vec2(1440.0, 850.0));
        let places = writer.places(desktop, false, true, true);
        assert!(places.binder.is_some() && places.inspector.is_none());
        writer.inspector = true;
        let places = writer.places(desktop, false, true, true);
        assert!(places.binder.is_some() && places.inspector.is_some());
        assert!(places.center.width() >= 520.0);
        assert_eq!(places.toolbar.top(), desktop.top());
    }

    #[test]
    fn a_phone_writes_above_its_keyboard() {
        let mut writer = SueWriter {
            phone: true,
            ..SueWriter::default()
        };
        // The window above an on-screen keyboard: the formatting bar sits
        // at its bottom, right above the keyboard, the text fills the rest.
        let above = Rect::from_min_size(pos2(0.0, 52.0), vec2(390.0, 380.0));
        let places = writer.places(above, false, true, true);
        assert_eq!(places.toolbar.bottom(), places.status.top());
        assert_eq!(places.status.bottom(), above.bottom());
        assert_eq!(places.center, places.body);
        assert_eq!(places.center.bottom(), places.toolbar.top());
        assert!(places.center.height() > 250.0);
        assert!(places.toolbar.height() >= THUMB);
        // The binder starts closed; asked for, it takes the whole page in
        // turn, and the text hides.
        writer.phone_binder = true;
        let places = writer.places(above, false, true, true);
        assert_eq!(places.binder, Some(places.body));
        assert!(!places.center.is_positive());
        // A phone shows whole pages until asked to reflow the text.
        assert!(writer.pages_shown());
        writer.phone_pages = false;
        assert!(!writer.pages_shown());
    }

    #[test]
    fn typing_goes_on_after_formatting() {
        for command in [
            "format.bold",
            "align.center",
            "list.bullet",
            "style.heading1",
            "edit.undo",
        ] {
            assert!(keeps_typing(command), "{command}");
        }
        // These ask something first, and keep the keyboard for that.
        for command in ["insert.link", "edit.find", "compile.open", "view.binder"] {
            assert!(!keeps_typing(command), "{command}");
        }
    }
}
