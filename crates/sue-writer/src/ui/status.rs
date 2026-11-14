//! The status bar, the find bar and the progress bar.

use egui::{Align2, Color32, Id, Rect, RichText, Sense, Ui, UiBuilder, pos2, vec2};
use sue_core::Value;
use sue_model::manuscript;
use sue_wm::{AppletContext, Theme, widgets};

use crate::project;
use crate::studio::{SueWriter, View};

impl SueWriter {
    pub(super) fn status_ui(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        ui.painter().rect_filled(rect, 0.0, theme.panel);
        ui.painter()
            .hline(rect.x_range(), rect.top(), theme.hairline());
        let (draft, session) = self.counts(cx);
        let doc_words = self
            .editor
            .current()
            .map_or(0, |doc| manuscript::words(cx.arena(), doc));
        let selected = sue_model::word_count(self.editor.selected_text());
        let mut parts = Vec::new();
        if selected > 0 {
            parts.push(tr!("of-words", selected = selected, doc_words = doc_words));
        } else {
            parts.push(tr!("word-count", count = doc_words));
        }
        // A phone keeps to the words and the page.
        if self.project.is_some() && !self.simple(cx) && !self.phone {
            let target = self
                .project_entity(cx)
                .and_then(|entity| entity.property(manuscript::TARGET))
                .and_then(Value::as_i64)
                .unwrap_or(0);
            if target > 0 {
                parts.push(tr!(
                    "draft-percent",
                    value = sue_lang::number(draft as f64 * 100.0 / target as f64, 0),
                    draft = draft,
                    target = target
                ));
            } else {
                parts.push(tr!("draft-words", draft = draft));
            }
            let goal = self
                .project_entity(cx)
                .and_then(|entity| entity.property(project::SESSION_TARGET))
                .and_then(Value::as_i64)
                .unwrap_or(0);
            if goal > 0 {
                parts.push(tr!(
                    "today",
                    session = format!("{:+}", session),
                    goal = goal
                ));
            } else {
                parts.push(tr!("today-2", session = format!("{:+}", session)));
            }
        }
        if self.view == View::Document && self.pages_shown() {
            let (page, pages) = self.page_info;
            parts.push(tr!("page-of", value = page + 1, value2 = pages.max(1)));
        }
        let text = parts.join("    |    ");
        ui.painter().text(
            rect.left_center() + vec2(12.0, 0.0),
            Align2::LEFT_CENTER,
            text,
            theme.font(theme.small_size + 0.5),
            theme.muted,
        );
        if self.phone {
            // Two fingers zoom on a phone.
            return;
        }
        // The views and the zoom at the right, where Word and LibreOffice
        // keep them.
        let zoom = Rect::from_min_max(
            pos2(rect.right() - 290.0, rect.top() + 1.0),
            pos2(rect.right() - 8.0, rect.bottom() - 1.0),
        );
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(zoom)
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        // Sized to the bar: Sue's buttons are taller than it, and the
        // Draft and Page switch hung into the bar below.
        child.set_clip_rect(rect.intersect(ui.clip_rect()));
        child.spacing_mut().item_spacing.x = 4.0;
        child.spacing_mut().interact_size.y = (zoom.height() - 6.0).max(12.0);
        child.spacing_mut().button_padding = vec2(6.0, 0.0);
        if child
            .small_button("+")
            .on_hover_text(tr!("zoom-in-2"))
            .clicked()
        {
            self.run("view.zoom_in", cx);
        }
        let label = if self.fit {
            tr!(
                "fit-percent",
                value = sue_lang::number(f64::from(self.shown_zoom * 100.0), 0)
            )
        } else {
            format!("{:.0}%", self.shown_zoom * 100.0)
        };
        if child
            .button(RichText::new(label).size(theme.small_size))
            .on_hover_text(tr!("fit-page-window-show-size"))
            .clicked()
        {
            self.run(
                if self.fit {
                    "view.zoom_reset"
                } else {
                    "view.zoom_fit"
                },
                cx,
            );
        }
        if child
            .small_button("\u{2212}")
            .on_hover_text(tr!("zoom-out-2"))
            .clicked()
        {
            self.run("view.zoom_out", cx);
        }
        widgets::pill_divider(&mut child, theme);
        let options = [tr!("draft"), tr!("page")].map(String::from);
        let chosen = (self.view == View::Document).then_some(usize::from(self.paged));
        if let Some(index) = widgets::segmented(&mut child, &options, chosen, theme) {
            self.run(["view.draft", "view.page"][index], cx);
        }
    }

    pub(super) fn find_bar(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        ui.painter().rect_filled(rect, 0.0, theme.panel);
        ui.painter()
            .hline(rect.x_range(), rect.bottom(), theme.hairline());
        let Some(mut find) = self.find.clone() else {
            return;
        };
        let mut action: Option<&str> = None;
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(rect.shrink2(vec2(10.0, 4.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let ui = &mut child;
        let query = ui.add(
            egui::TextEdit::singleline(&mut find.query)
                .hint_text(tr!("find"))
                .desired_width(200.0)
                .id(Id::unique("writer-find")),
        );
        if find.focus {
            query.request_focus();
            find.focus = false;
        }
        if query.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) {
            action = Some(if ui.input(|input| input.modifiers.shift) {
                "previous"
            } else {
                "next"
            });
            query.request_focus();
        }
        if ui
            .small_button("\u{2191}")
            .on_hover_text(tr!("previous-shift-f3"))
            .clicked()
        {
            action = Some("previous");
        }
        if ui
            .small_button("\u{2193}")
            .on_hover_text(tr!("next-f3"))
            .clicked()
        {
            action = Some("next");
        }
        if find.replacing {
            ui.add(
                egui::TextEdit::singleline(&mut find.replace)
                    .hint_text(tr!("replace-with"))
                    .desired_width(180.0),
            );
            if ui.button(tr!("replace")).clicked() {
                action = Some("replace");
            }
            if ui.button(tr!("replace-all")).clicked() {
                action = Some("all");
            }
        } else if ui.small_button(tr!("replace-ellipsis")).clicked() {
            find.replacing = true;
        }
        ui.checkbox(&mut find.case, tr!("match-case"));
        ui.checkbox(&mut find.words, tr!("whole-words"));
        ui.checkbox(&mut find.regex, tr!("regular-expressions"))
            .on_hover_text(tr!("patterns-such-as-d-chapter-1"));
        if self.project.is_some() {
            ui.checkbox(&mut find.project, tr!("whole-project"));
        }
        if !find.message.is_empty() {
            ui.label(RichText::new(&find.message).color(theme.muted));
        }
        let close = ui
            .with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.small_button("\u{2715}")
                    .on_hover_text(tr!("close-esc"))
                    .clicked()
            })
            .inner;
        self.find = Some(find);
        match action {
            Some("next") => self.find_next(cx, false),
            Some("previous") => self.find_next(cx, true),
            Some("replace") => self.replace_one(cx),
            Some("all") => self.replace_all(cx),
            _ => {}
        }
        if close {
            self.find = None;
            self.editor.focus();
        }
    }
}

/// A thin bar of `share` (0 to 1).
pub(super) fn progress(ui: &mut Ui, share: f32, theme: &Theme) {
    let (rect, _) =
        ui.allocate_exact_size(vec2(ui.available_width().min(380.0), 6.0), Sense::hover());
    ui.painter().rect_filled(rect, 3.0, theme.pill);
    let done = Rect::from_min_size(
        rect.min,
        vec2(rect.width() * share.clamp(0.0, 1.0), rect.height()),
    );
    ui.painter().rect_filled(
        done,
        3.0,
        if share >= 1.0 {
            Color32::from_rgb(0x50, 0xb0, 0x7a)
        } else {
            theme.text.gamma_multiply(0.75)
        },
    );
}
