//! A phone's and a tablet's bars: the top bar, the quick tools and the
//! formatting bar above the keyboard.

use egui::{Color32, Painter, Rect, Ui, UiBuilder, pos2, vec2};
use sue_wm::{AppletContext, Theme};

use crate::studio::SueWriter;

use super::THUMB;
use super::widgets::{
    book_icon, comment_icon, export_icon, find_icon, menu_icon, page_icon, picture_icon,
    table_icon, thumb, undo_icon,
};

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
        let slot = |index: usize, from_right: bool| {
            let x = if from_right {
                bar.right() - 4.0 - (index as f32 + 1.0) * (THUMB + 2.0)
            } else {
                bar.left() + 4.0 + index as f32 * (THUMB + 2.0)
            };
            Rect::from_min_size(pos2(x, bar.center().y - THUMB / 2.0), vec2(THUMB, THUMB))
        };
        let mut run = None;
        let menu = slot(0, false);
        let open = self.menubar.is_open();
        let toggle = thumb(ui, menu, open, true, theme, menu_icon)
            .on_hover_text(tr!("menus"))
            .clicked();
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
        let history = (cx.history.can_undo(), cx.history.can_redo());
        let undo = thumb(
            ui,
            slot(1, false),
            false,
            history.0,
            theme,
            |painter, rect, color| {
                undo_icon(painter, rect, color, false);
            },
        );
        if undo.on_hover_text(tr!("undo")).clicked() {
            run = Some("edit.undo".to_owned());
        }
        let redo = thumb(
            ui,
            slot(2, false),
            false,
            history.1,
            theme,
            |painter, rect, color| {
                undo_icon(painter, rect, color, true);
            },
        );
        if redo.on_hover_text(tr!("redo")).clicked() {
            run = Some("edit.redo".to_owned());
        }
        let finding = self.find.is_some();
        if thumb(ui, slot(0, true), finding, true, theme, find_icon)
            .on_hover_text(tr!("find"))
            .clicked()
        {
            if finding {
                self.find = None;
            } else {
                run = Some("edit.find".to_owned());
            }
        }
        let mut right = 1;
        if self.project.is_some()
            && !self.simple(cx)
            && thumb(ui, slot(1, true), self.phone_binder, true, theme, book_icon)
                .on_hover_text(tr!("binder"))
                .clicked()
        {
            run = Some("view.binder".to_owned());
        }
        if self.project.is_some() && !self.simple(cx) {
            right += 1;
        }
        // The room between: pages or flowing text, a picture, the
        // comments and a PDF, as many as fit.
        if self.project.is_some() {
            let room = slot(2, false).right() + 4.0;
            for (tool, hint) in self.quick_tools() {
                let place = slot(right, true);
                if place.left() < room {
                    break;
                }
                right += 1;
                if thumb(ui, place, tool.on(self), true, theme, |p, r, c| {
                    tool.draw(p, r, c)
                })
                .on_hover_text(hint)
                .clicked()
                {
                    run = Some(tool.command(self).to_owned());
                }
            }
        }
        run
    }

    /// The tools a phone's or a tablet's top bar adds in its spare room,
    /// most wanted first.
    fn quick_tools(&self) -> Vec<(QuickTool, String)> {
        vec![
            (
                QuickTool::Pages,
                if self.pages_shown() {
                    tr!("quick-draft-view")
                } else {
                    tr!("quick-page-view")
                },
            ),
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
        let size = rect.height().min(THUMB) - 4.0;
        let mut x = rect.right() - 4.0;
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
            x -= size + 2.0;
            if thumb(ui, place, tool.on(self), true, theme, |p, r, c| {
                tool.draw(p, r, c)
            })
            .on_hover_text(hint)
            .clicked()
            {
                run = Some(tool.command(self).to_owned());
            }
        }
        let _ = cx;
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
        ui.painter().rect_filled(rect, 0.0, theme.panel);
        ui.painter()
            .hline(rect.x_range(), rect.top(), theme.hairline());
        let mut run: Option<String> = None;
        let mut row = ui.new_child(
            UiBuilder::new()
                .max_rect(rect.shrink2(vec2(4.0, 4.0)))
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
                    ui.spacing_mut().item_spacing = vec2(2.0, 0.0);
                    self.format_controls(ui, cx, theme, THUMB, &mut run);
                });
            });
        if let Some(command) = run {
            self.run_typing(&command, cx);
        }
    }
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

    fn draw(self, painter: &Painter, rect: Rect, color: Color32) {
        match self {
            Self::Find => find_icon(painter, rect, color),
            Self::Pages => page_icon(painter, rect, color, false),
            Self::Picture => picture_icon(painter, rect, color),
            Self::Comments => comment_icon(painter, rect, color),
            Self::Pdf => export_icon(painter, rect, color),
            Self::Table => table_icon(painter, rect, color),
        }
    }
}
