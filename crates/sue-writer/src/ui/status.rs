//! The status bar, the find bar and the progress bar.

use egui::{Align2, CornerRadius, Rect, Ui, UiBuilder, Vec2, pos2};
use sue_core::Value;
use sue_model::manuscript;
use sue_wm::icons::Icon;
use sue_wm::ui::{
    Density, Space, TextRole, button, checkbox, icon_button, search_field, segmented,
    text_field,
};
use sue_wm::{AppletContext, Theme};

use crate::project;
use crate::studio::{SueWriter, View};

use super::widgets::tool;


/// The find field's width, in control heights.
const FIND_FIELD: f32 = 8.0;

/// The replace field's width, in control heights.
const REPLACE_FIELD: f32 = 7.0;

/// Takes room from the right end of a bar, a control at a time.
struct FromRight {
    /// Where the next control ends.
    right: f32,
    /// The bar's middle, top to bottom.
    middle: f32,
    /// How tall each control is.
    tall: f32,
    /// The room between two controls.
    gap: f32,
}

impl FromRight {
    /// The next `width` points, right to left.
    fn take(&mut self, width: f32) -> Rect {
        let rect = Rect::from_min_max(
            pos2(self.right - width, self.middle - self.tall / 2.0),
            pos2(self.right, self.middle + self.tall / 2.0),
        );
        self.right -= width + self.gap;
        rect
    }
}

impl SueWriter {
    pub(super) fn status_ui(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        ui.painter()
            .rect_filled(rect, CornerRadius::ZERO, theme.panel);
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
        let edge = theme.space(Space::S5);
        ui.painter().text(
            rect.left_center() + Vec2::X * edge,
            Align2::LEFT_CENTER,
            text,
            theme.text_font(TextRole::Caption),
            theme.text_muted(),
        );
        if self.phone {
            // Two fingers zoom on a phone.
            return;
        }
        // The views and the zoom at the right, where Word and LibreOffice
        // keep them: [Draft | Page]  |  Zoom Out  100%  Zoom In. Sized to
        // the bar, laid out from its right end.
        let mut bar = ui.new_child(UiBuilder::new().max_rect(rect));
        bar.set_clip_rect(rect.intersect(ui.clip_rect()));
        Density::Compact.scope(&mut bar, |ui| self.status_controls(ui, rect, cx, theme));
    }

    /// The zoom and the Draft and Page switch, at the right of the status
    /// bar `rect`.
    fn status_controls(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let mut at = FromRight {
            right: rect.right() - theme.space(Space::S4),
            middle: rect.center().y,
            tall: rect.height() - 2.0 * theme.space(Space::S1),
            gap: theme.space(Space::S2),
        };
        let mut run: Option<&str> = None;
        let (name, keys) = self.tip_of("view.zoom_in");
        let place = at.take(at.tall);
        if icon_button("writer-zoom-in", Icon::ZoomIn, &name)
            .quiet()
            .shortcut(&keys)
            .show_at(ui, place, theme)
            .clicked()
        {
            run = Some("view.zoom_in");
        }
        let label = if self.fit {
            tr!(
                "fit-percent",
                value = sue_lang::number(f64::from(self.shown_zoom * 100.0), 0)
            )
        } else {
            format!("{:.0}%", self.shown_zoom * 100.0)
        };
        let hint = tr!("fit-page-window-show-size");
        let zoom = button("writer-zoom-shown", &label).ghost().hint(&hint);
        let width = zoom.size(ui.painter(), theme, Density::of(ui)).x;
        let place = at.take(width);
        if zoom.show_at(ui, place, theme).clicked() {
            run = Some(if self.fit {
                "view.zoom_reset"
            } else {
                "view.zoom_fit"
            });
        }
        let (name, keys) = self.tip_of("view.zoom_out");
        let place = at.take(at.tall);
        if icon_button("writer-zoom-out", Icon::ZoomOut, &name)
            .quiet()
            .shortcut(&keys)
            .show_at(ui, place, theme)
            .clicked()
        {
            run = Some("view.zoom_out");
        }
        let line = at.take(theme.border_width.max(1.0));
        ui.painter().vline(
            line.center().x,
            line.shrink2(Vec2::Y * theme.space(Space::S2)).y_range(),
            theme.hairline(),
        );
        let options = [tr!("draft"), tr!("page")];
        let chosen = (self.view == View::Document).then_some(usize::from(self.paged));
        let views = segmented("writer-views", &options, chosen);
        let width = views.size(ui.painter(), theme, Density::of(ui)).x;
        let place = at.take(width);
        if let Some(index) = views.show_at(ui, place, theme).picked {
            run = Some(["view.draft", "view.page"][index]);
        }
        if let Some(command) = run {
            self.run(command, cx);
        }
    }

    pub(super) fn find_bar(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        ui.painter()
            .rect_filled(rect, CornerRadius::ZERO, theme.panel);
        ui.painter()
            .hline(rect.x_range(), rect.bottom(), theme.hairline());
        let Some(mut find) = self.find.clone() else {
            return;
        };
        let inner = Rect::from_min_max(
            rect.min + Vec2::new(theme.space(Space::S5), theme.space(Space::S2)),
            rect.max - Vec2::new(theme.space(Space::S5), theme.space(Space::S2)),
        );
        let mut child = ui.new_child(
            UiBuilder::new()
                .max_rect(inner)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let (action, close) = Density::Compact.scope(&mut child, |ui| {
            ui.spacing_mut().item_spacing.x = theme.space(Space::S3);
            self.find_controls(ui, &mut find, theme)
        });
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

    /// The find bar's fields and buttons; returns what to do and whether
    /// to close it.
    fn find_controls(
        &self,
        ui: &mut Ui,
        find: &mut crate::studio::Find,
        theme: &Theme,
    ) -> (Option<&'static str>, bool) {
        let mut action = None;
        let line = Density::of(ui).control_height();
        let hint = tr!("find");
        let query = search_field("writer-find", &mut find.query)
            .hint(&hint)
            .width(line * FIND_FIELD)
            .show(ui, theme);
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
        for (command, icon, what) in [
            ("edit.find_previous", Icon::ChevronUp, "previous"),
            ("edit.find_next", Icon::ChevronDown, "next"),
        ] {
            let (name, keys) = self.tip_of(command);
            if tool(ui, theme, command, icon, &name, &keys).clicked() {
                action = Some(what);
            }
        }
        if find.replacing {
            let hint = tr!("replace-with");
            text_field("writer-replace", &mut find.replace)
                .hint(&hint)
                .width(line * REPLACE_FIELD)
                .show(ui, theme);
            let one = tr!("replace");
            if button("writer-replace-one", &one)
                .hint(&one)
                .show(ui, theme)
                .clicked()
            {
                action = Some("replace");
            }
            let all = tr!("replace-all");
            if button("writer-replace-all", &all)
                .hint(&all)
                .show(ui, theme)
                .clicked()
            {
                action = Some("all");
            }
        } else {
            let open = tr!("replace-ellipsis");
            if button("writer-replace-open", &open)
                .ghost()
                .hint(&open)
                .show(ui, theme)
                .clicked()
            {
                find.replacing = true;
            }
        }
        let flip = |ui: &mut Ui, salt: &str, label: String, on: &mut bool, help: Option<String>| {
            let response = checkbox(salt, &label, *on).show(ui, theme);
            // The tooltip names the box, and says more where it helps.
            let response = response.on_hover_text(help.unwrap_or_else(|| label.clone()));
            if response.clicked() {
                *on = !*on;
            }
        };
        flip(
            ui,
            "writer-find-case",
            tr!("match-case"),
            &mut find.case,
            None,
        );
        flip(
            ui,
            "writer-find-words",
            tr!("whole-words"),
            &mut find.words,
            None,
        );
        flip(
            ui,
            "writer-find-regex",
            tr!("regular-expressions"),
            &mut find.regex,
            Some(tr!("patterns-such-as-d-chapter-1")),
        );
        if self.project.is_some() {
            flip(
                ui,
                "writer-find-project",
                tr!("whole-project"),
                &mut find.project,
                None,
            );
        }
        if !find.message.is_empty() {
            ui.label(
                egui::RichText::new(&find.message)
                    .font(theme.text_font(TextRole::Caption))
                    .color(theme.text_muted()),
            );
        }
        let close = ui
            .with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let name = tr!("close");
                // The key's name as the menus and Settings write it.
                let keys = sue_platform::NamedKey::Escape.to_string();
                icon_button("writer-find-close", Icon::Close, &name)
                    .quiet()
                    .shortcut(&keys)
                    .show(ui, theme)
                    .clicked()
            })
            .inner;
        (action, close)
    }
}

