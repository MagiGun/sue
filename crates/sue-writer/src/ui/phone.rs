//! A phone's and a tablet's bars: the top bar, the quick tools and the
//! formatting bar above the keyboard.
//!
//! Every button is one of Sue's icon buttons with an icon from
//! `sue_wm::icons` by its stable name, so an icon pack replaces them, and
//! each names what it does and its keys when it is pressed and held (or
//! when the pointer rests on it).

use egui::{Rect, Ui, UiBuilder, pos2, vec2};
use sue_wm::icons::Icon;
use sue_wm::ui::{self, Space};
use sue_wm::{AppletContext, Theme};

use crate::studio::SueWriter;

use super::THUMB;

/// Room between two buttons of a bar, and around the bar's ends.
const GAP: Space = Space::S1;
/// Room around the formatting bar's row.
const EDGE: Space = Space::S2;

impl SueWriter {
    /// Sue Writer's row in Sue's top bar on a phone: the menus under one
    /// button, undo and redo, and at the right Find and the binder.
    pub(super) fn phone_top_bar(
        &mut self,
        ui: &mut Ui,
        bar: Rect,
        cx: &AppletContext<'_>,
        theme: &Theme,
    ) -> Option<String> {
        let edge = theme.space(EDGE);
        let step = THUMB + theme.space(GAP);
        let slot = |index: usize, from_right: bool| {
            let x = if from_right {
                bar.right() - edge - (index as f32 + 1.0) * step
            } else {
                bar.left() + edge + index as f32 * step
            };
            Rect::from_min_size(pos2(x, bar.center().y - THUMB / 2.0), vec2(THUMB, THUMB))
        };
        let mut run = None;
        let menu = slot(0, false);
        let open = self.menubar.is_open();
        let menus = tr!("menus");
        let toggle = BarButton::new(menu, Icon::Menu, &menus, "")
            .on(open)
            .show(ui, theme, cx);
        let items = crate::menus::phone_menu(&self.menus);
        let picked = {
            let mut menubar = std::mem::take(&mut self.menubar);
            let state = |command: &str| self.item_state(command, cx);
            let picked = menubar.dropdown(ui, menu, toggle, &items, &state, theme);
            self.menubar = menubar;
            picked
        };
        if picked.is_some() {
            run = picked;
        }
        let (can_undo, can_redo) = (cx.history.can_undo(), cx.history.can_redo());
        for (index, icon, name, command, enabled) in [
            (1, Icon::Undo, tr!("undo"), "edit.undo", can_undo),
            (2, Icon::Redo, tr!("redo"), "edit.redo", can_redo),
        ] {
            if BarButton::new(slot(index, false), icon, &name, command)
                .enabled(enabled)
                .show(ui, theme, cx)
            {
                run = Some(command.to_owned());
            }
        }
        let finding = self.find.is_some();
        let find = tr!("find");
        if BarButton::new(slot(0, true), Icon::Search, &find, "edit.find")
            .on(finding)
            .show(ui, theme, cx)
        {
            if finding {
                self.find = None;
            } else {
                run = Some("edit.find".to_owned());
            }
        }
        let mut right = 1;
        if self.project.is_some() && !self.simple(cx) {
            let binder = tr!("binder");
            if BarButton::new(slot(1, true), Icon::Binder, &binder, "view.binder")
                .on(self.phone_binder)
                .show(ui, theme, cx)
            {
                run = Some("view.binder".to_owned());
            }
            right += 1;
        }
        // The room between: pages or flowing text, a picture, the
        // comments and a PDF, as many as fit.
        if self.project.is_some() {
            let room = slot(2, false).right() + edge;
            for (tool, hint) in self.quick_tools() {
                let place = slot(right, true);
                if place.left() < room {
                    break;
                }
                right += 1;
                let command = tool.command(self);
                if BarButton::new(place, tool.icon(), &hint, command)
                    .on(tool.on(self))
                    .show(ui, theme, cx)
                {
                    run = Some(command.to_owned());
                }
            }
        }
        run
    }

    /// The tools a phone's or a tablet's top bar adds in its spare room,
    /// most wanted first.
    fn quick_tools(&self) -> Vec<(QuickTool, String)> {
        vec![
            // One name whether lit or not: lit says the pages show, and
            // pressing it again goes back to the flowing draft.
            (QuickTool::Pages, tr!("quick-page-view")),
            (QuickTool::Picture, tr!("quick-picture")),
            (QuickTool::Comments, tr!("comments")),
            (QuickTool::Pdf, tr!("quick-export-pdf")),
            (QuickTool::Table, tr!("quick-table")),
        ]
    }

    /// A tablet's quick tools at the right of the menus: Find, then the
    /// phone's.
    pub(super) fn tablet_tools(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &AppletContext<'_>,
        theme: &Theme,
    ) {
        if self.project.is_none() {
            return;
        }
        let gap = theme.space(GAP);
        let size = rect.height().min(THUMB) - theme.space(EDGE);
        let mut x = rect.right() - theme.space(EDGE);
        let mut run = None;
        let mut tools: Vec<(QuickTool, String)> = vec![(QuickTool::Find, tr!("find"))];
        tools.extend(self.quick_tools());
        for (tool, hint) in tools {
            let place = Rect::from_min_size(
                pos2(x - size, rect.center().y - size / 2.0),
                vec2(size, size),
            );
            if place.left() < rect.left() {
                break;
            }
            x -= size + gap;
            let command = tool.command(self);
            if BarButton::new(place, tool.icon(), &hint, command)
                .on(tool.on(self))
                .show(ui, theme, cx)
            {
                run = Some(command.to_owned());
            }
        }
        if let Some(command) = run {
            self.pending_command = Some(command);
        }
    }

    /// A phone's formatting bar: the same controls as thumb-sized buttons
    /// in one row that scrolls sideways, at the bottom of the page, so that
    /// it sits right above the on-screen keyboard.
    pub(super) fn phone_toolbar(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::ZERO, theme.panel);
        ui.painter()
            .hline(rect.x_range(), rect.top(), theme.hairline());
        let mut run: Option<String> = None;
        let edge = theme.space(EDGE);
        let mut row = ui.new_child(
            UiBuilder::new()
                .max_rect(rect.shrink(edge))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        row.set_clip_rect(rect.intersect(ui.clip_rect()));
        // A finger slides the row sideways, over its buttons too.
        egui::ScrollArea::horizontal()
            .id_salt("writer-phone-toolbar")
            .scroll_source(egui::scroll_area::ScrollSource {
                drag: egui::scroll_area::DragScroll::Always,
                ..egui::scroll_area::ScrollSource::default()
            })
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(&mut row, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing.x = theme.space(GAP);
                    self.format_controls(ui, cx, theme, THUMB, &mut run);
                });
            });
        if let Some(command) = run {
            self.run_typing(&command, cx);
        }
    }
}

/// One button of a phone's or a tablet's bar.
struct BarButton<'a> {
    /// Where it is drawn.
    rect: Rect,
    icon: Icon,
    /// What it does, shown in its tip and read out.
    name: &'a str,
    /// The command whose keys its tip shows.
    command: &'a str,
    /// Lit, for what is on now.
    on: bool,
    /// Whether it can be pressed now.
    enabled: bool,
}

impl<'a> BarButton<'a> {
    fn new(rect: Rect, icon: Icon, name: &'a str, command: &'a str) -> Self {
        Self {
            rect,
            icon,
            name,
            command,
            on: false,
            enabled: true,
        }
    }

    const fn on(mut self, on: bool) -> Self {
        self.on = on;
        self
    }

    const fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Draws it; `true` when it was pressed.
    fn show(self, ui: &Ui, theme: &Theme, cx: &AppletContext<'_>) -> bool {
        // The keys are looked up only while the button may show its tip.
        let keys = if ui.rect_contains_pointer(self.rect) {
            keys_of(self.command, cx)
        } else {
            String::new()
        };
        ui::icon_button(("writer-bar", self.icon.name()), self.icon, self.name)
            .selected(self.on)
            .enabled(self.enabled)
            .shortcut(&keys)
            .show_at(ui, self.rect, theme)
            .clicked()
    }
}

/// The keys that run `command` as set now, such as `Ctrl+F`; empty for
/// none.
fn keys_of(command: &str, cx: &AppletContext<'_>) -> String {
    let Some(shortcut) = crate::commands::registry()
        .shortcuts()
        .into_iter()
        .find(|shortcut| shortcut.command == command)
    else {
        return String::new();
    };
    sue_wm::shortcuts::chords(cx.rice(), crate::studio::KEYS_SECTION, &shortcut)
        .chords()
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// A tool in a phone's or a tablet's top bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QuickTool {
    Find,
    Pages,
    Picture,
    Comments,
    Pdf,
    Table,
}

impl QuickTool {
    fn command(self, writer: &SueWriter) -> &'static str {
        match self {
            Self::Find => "edit.find",
            Self::Pages if writer.pages_shown() => "view.draft",
            Self::Pages => "view.page",
            Self::Picture => "insert.picture",
            Self::Comments => "review.comments",
            Self::Pdf => "compile.pdf",
            Self::Table => "table.insert",
        }
    }

    fn on(self, writer: &SueWriter) -> bool {
        match self {
            Self::Find => writer.find.is_some(),
            Self::Pages => writer.pages_shown(),
            Self::Comments => writer.comments,
            _ => false,
        }
    }

    /// Its icon, by the stable name an icon pack replaces.
    const fn icon(self) -> Icon {
        match self {
            Self::Find => Icon::Search,
            Self::Pages => Icon::Page,
            Self::Picture => Icon::Picture,
            Self::Comments => Icon::Comment,
            Self::Pdf => Icon::ExportPdf,
            Self::Table => Icon::Table,
        }
    }
}
