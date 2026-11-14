//! The formatting toolbar: styles, fonts, colors and the font picker.

use egui::{Id, Rect, RichText, Ui, UiBuilder, pos2, vec2};
use sue_core::Value;
use sue_model::mark_kind as kind;
use sue_wm::{AppletContext, Theme, widgets};

use crate::studio::{SueWriter, View};

use super::widgets::{align_icon, icon, indent_icon, list_icon, toggle, undo_icon};
use super::{TOOL, TOOLBAR_HEIGHT};

impl SueWriter {
    /// The formatting toolbar: what Word's mini toolbar and LibreOffice
    /// Writer's single toolbar keep at hand (undo and redo, the paragraph
    /// style, the font and its size, bold, italic and underline, the
    /// colors, alignment, lists and indents), with Find at the right. The
    /// rest is in the Format menu; the views are in the status bar.
    pub(super) fn toolbar(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        ui.painter().rect_filled(rect, 0.0, theme.panel);
        ui.painter()
            .hline(rect.x_range(), rect.bottom(), theme.hairline());
        let mut run: Option<String> = None;
        let inner = rect.shrink2(vec2(8.0, 4.0));
        let first_row = Rect::from_min_size(
            inner.min,
            vec2(
                inner.width(),
                (self.toolbar_height - 8.0).min(TOOLBAR_HEIGHT),
            ),
        );
        let mut right = ui.new_child(
            UiBuilder::new()
                .max_rect(first_row)
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        right.set_clip_rect(rect.intersect(ui.clip_rect()));
        if widgets::pill_button(&mut right, &tr!("find"), self.find.is_some(), theme)
            .on_hover_text(tr!("find-and-replace-ctrl-f"))
            .clicked()
        {
            if self.find.is_some() {
                self.find = None;
            } else {
                run = Some("edit.find".to_owned());
            }
        }
        let taken = right.min_rect().width() + 12.0;
        let left = Rect::from_min_max(
            inner.min,
            pos2((inner.right() - taken).max(inner.left()), inner.bottom()),
        );
        let mut row = ui.new_child(
            UiBuilder::new()
                .max_rect(left)
                .layout(egui::Layout::left_to_right(egui::Align::Min)),
        );
        row.set_clip_rect(
            Rect::from_x_y_ranges(left.x_range(), rect.y_range()).intersect(ui.clip_rect()),
        );
        // The formatting wraps onto a second row when the studio is narrow.
        // A tablet's buttons are bigger, for fingers and pens.
        let tablet = cx.view.form_factor == sue_platform::FormFactor::Tablet;
        row.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 6.0);
            if tablet {
                ui.spacing_mut().interact_size.y = 32.0;
                ui.spacing_mut().button_padding = vec2(8.0, 6.0);
            }
            let size = if tablet { 32.0 } else { 24.0 };
            self.format_controls(ui, cx, theme, size, &mut run);
        });
        self.toolbar_height = (row.min_rect().bottom() - rect.top() + 5.0).max(TOOLBAR_HEIGHT);
        if let Some(command) = run {
            self.run_typing(&command, cx);
        }
    }

    /// The formatting controls, `size` points tall: 24 at a desk, 32 on a
    /// tablet, a thumb's 44 on a phone. A phone has undo and redo in the
    /// top bar, starts with what it reaches for most and steps through the
    /// alignments with one button.
    #[allow(clippy::too_many_lines, reason = "one row of controls")]
    pub(super) fn format_controls(
        &mut self,
        ui: &mut Ui,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
        size: f32,
        run: &mut Option<String>,
    ) {
        let big = self.phone;
        let sheet = self.sheet(cx);
        let (para, chars) = self.editor.look_here(&sheet);
        let text_on = self.editor.current().is_some() && self.view == View::Document;
        if !big {
            let history = (cx.history.can_undo(), cx.history.can_redo());
            for (redo, enabled, hint, command) in [
                (false, history.0, tr!("undo-ctrl-z"), "edit.undo"),
                (true, history.1, tr!("redo-ctrl-shift-z"), "edit.redo"),
            ] {
                if ui
                    .add_enabled_ui(enabled, |ui| {
                        icon(ui, size, false, theme, |painter, rect, color| {
                            undo_icon(painter, rect, color, redo);
                        })
                    })
                    .inner
                    .on_hover_text(hint)
                    .clicked()
                {
                    *run = Some(command.to_owned());
                }
            }
            widgets::pill_divider(ui, theme);
        }
        ui.add_enabled_ui(text_on, |ui| {
            if big {
                ui.spacing_mut().interact_size.y = size;
                ui.spacing_mut().button_padding = vec2(10.0, 8.0);
            }
            // Paragraph style.
            egui::ComboBox::from_id_salt("writer-style")
                .selected_text(sheet.get(&para.style).name.clone())
                .width(if big { 140.0 } else { 132.0 })
                .show_ui(ui, |ui| {
                    for style in &sheet.styles {
                        let label = RichText::new(&style.name)
                            .size(theme.ui_size + if style.outline.is_some() { 1.5 } else { 0.0 });
                        if ui
                            .selectable_label(style.key == para.style, label)
                            .clicked()
                        {
                            *run = Some(format!("style.{}", style.key));
                        }
                    }
                });
            if !big {
                self.font_controls(ui, cx, theme, &para, &chars);
            }
            widgets::pill_divider(ui, theme);
            for (label, command, on, hint) in [
                (
                    "B",
                    "format.bold",
                    chars.bold || para.bold,
                    tr!("bold-ctrl-b"),
                ),
                (
                    "I",
                    "format.italic",
                    chars.italic || para.italic,
                    tr!("italic-ctrl-i"),
                ),
                (
                    "U",
                    "format.underline",
                    chars.underline,
                    tr!("underline-ctrl-u"),
                ),
            ] {
                let text = match label {
                    "B" => RichText::new(label).strong(),
                    "I" => RichText::new(label).italics(),
                    _ => RichText::new(label).underline(),
                };
                if toggle(ui, size, text, on, theme)
                    .on_hover_text(hint)
                    .clicked()
                {
                    *run = Some(command.to_owned());
                }
            }
            if !big {
                self.color_controls(ui, cx, theme, &chars, run);
            }
            widgets::pill_divider(ui, theme);
            if big {
                // One button steps through the alignments.
                let (next, command) = match para.align {
                    sue_type::Align::Left => (sue_type::Align::Center, "align.center"),
                    sue_type::Align::Center => (sue_type::Align::Right, "align.right"),
                    sue_type::Align::Right => (sue_type::Align::Justify, "align.justify"),
                    sue_type::Align::Justify => (sue_type::Align::Left, "align.left"),
                };
                let shown = para.align;
                if icon(ui, size, false, theme, |painter, rect, color| {
                    align_icon(painter, rect, color, shown);
                })
                .on_hover_text(tr!("alignment-next", next = next.label()))
                .clicked()
                {
                    *run = Some(command.to_owned());
                }
            } else {
                for (align, command, hint) in [
                    (
                        sue_type::Align::Left,
                        "align.left",
                        tr!("align-left-ctrl-l"),
                    ),
                    (
                        sue_type::Align::Center,
                        "align.center",
                        tr!("center-ctrl-e"),
                    ),
                    (
                        sue_type::Align::Right,
                        "align.right",
                        tr!("align-right-ctrl-r"),
                    ),
                    (
                        sue_type::Align::Justify,
                        "align.justify",
                        tr!("justify-ctrl-j"),
                    ),
                ] {
                    if icon(
                        ui,
                        size,
                        para.align == align,
                        theme,
                        |painter, rect, color| {
                            align_icon(painter, rect, color, align);
                        },
                    )
                    .on_hover_text(hint)
                    .clicked()
                    {
                        *run = Some(command.to_owned());
                    }
                }
                widgets::pill_divider(ui, theme);
            }
            let bullet = para.list == Some(sue_type::ListKind::Bullet);
            let number = para.list == Some(sue_type::ListKind::Number);
            for (numbered, on, hint, command) in [
                (
                    false,
                    bullet,
                    tr!("bulleted-list-ctrl-shift-l"),
                    "list.bullet",
                ),
                (true, number, tr!("numbered-list-2"), "list.number"),
            ] {
                if icon(ui, size, on, theme, |painter, rect, color| {
                    list_icon(painter, rect, color, numbered);
                })
                .on_hover_text(hint)
                .clicked()
                {
                    *run = Some(command.to_owned());
                }
            }
            for (more, hint, command) in [
                (false, tr!("decrease-indent-2"), "indent.less"),
                (true, tr!("increase-indent-2"), "indent.more"),
            ] {
                if icon(ui, size, false, theme, |painter, rect, color| {
                    indent_icon(painter, rect, color, more);
                })
                .on_hover_text(hint)
                .clicked()
                {
                    *run = Some(command.to_owned());
                }
            }
            if big {
                widgets::pill_divider(ui, theme);
                self.font_controls(ui, cx, theme, &para, &chars);
                self.color_controls(ui, cx, theme, &chars, run);
                // Then what the menus hold that a phone reaches for: more
                // formatting, and what goes into the text.
                widgets::pill_divider(ui, theme);
                let strike = RichText::new("S").strikethrough();
                if ui
                    .add_sized(
                        vec2(size, size),
                        egui::Button::new(strike).selected(chars.strike).sense(TOOL),
                    )
                    .on_hover_text(tr!("strikethrough"))
                    .clicked()
                {
                    *run = Some("format.strike".to_owned());
                }
                for (label, command) in [
                    (tr!("quick-clear"), "format.clear"),
                    (tr!("quick-link"), "insert.link"),
                    (tr!("quick-picture"), "insert.picture"),
                    (tr!("quick-table"), "table.insert"),
                    (tr!("quick-footnote"), "insert.footnote"),
                    (tr!("quick-comment"), "review.comment"),
                    (tr!("quick-page-break"), "insert.page_break"),
                ] {
                    if ui
                        .add(
                            egui::Button::new(label)
                                .min_size(vec2(size, size))
                                .sense(TOOL),
                        )
                        .clicked()
                    {
                        *run = Some(command.to_owned());
                    }
                }
            }
        });
    }

    /// The font and its size.
    fn font_controls(
        &mut self,
        ui: &mut Ui,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
        para: &sue_type::ParagraphLook,
        chars: &sue_type::CharFormat,
    ) {
        let font = chars.font.clone().unwrap_or_else(|| para.font.clone());
        let font_id = Id::unique("writer-font-filter");
        egui::ComboBox::from_id_salt("writer-font")
            .selected_text(font.clone())
            .width(150.0)
            .height(420.0)
            .show_ui(ui, |ui| {
                let mut filter: String = ui.data(|data| data.get_temp(font_id)).unwrap_or_default();
                ui.add(
                    egui::TextEdit::singleline(&mut filter)
                        .hint_text(tr!("search-fonts"))
                        .desired_width(200.0),
                );
                ui.data_mut(|data| data.insert_temp(font_id, filter.clone()));
                if !sue_wm::fonts::ready() {
                    ui.label(
                        RichText::new(tr!("finding-installed-fonts-ellipsis")).color(theme.muted),
                    );
                }
                if ui
                    .selectable_label(chars.font.is_none(), tr!("styles-font"))
                    .clicked()
                {
                    self.editor.set_char(cx, kind::FONT, None, &tr!("font"));
                }
                if let Some(family) = sue_wm::fonts::list_ui(ui, &font, &filter) {
                    self.editor
                        .set_char(cx, kind::FONT, Some(Value::Text(family)), &tr!("font"));
                }
            });
        let size = chars.size.unwrap_or(para.size);
        egui::ComboBox::from_id_salt("writer-size")
            .selected_text(format!("{size:.0}"))
            .width(48.0)
            .show_ui(ui, |ui| {
                for option in [
                    8.0_f32, 9.0, 10.0, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 24.0, 28.0, 32.0, 36.0,
                    48.0, 72.0,
                ] {
                    if ui
                        .selectable_label((option - size).abs() < 0.1, format!("{option:.0}"))
                        .clicked()
                    {
                        self.editor.set_char(
                            cx,
                            kind::SIZE,
                            Some(Value::Float(f64::from(option))),
                            &tr!("size"),
                        );
                    }
                }
            });
    }

    /// The text color and the highlight.
    fn color_controls(
        &mut self,
        ui: &mut Ui,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
        chars: &sue_type::CharFormat,
        run: &mut Option<String>,
    ) {
        let mut rgba = chars
            .color
            .map_or([0x18, 0x18, 0x18, 255], |[r, g, b]| [r, g, b, 255]);
        let pick = sue_wm::color_button(ui, "writer-color", &mut rgba, false, theme);
        if pick.released || (pick.changed && !pick.dragging) {
            let hex = format!("#{:02x}{:02x}{:02x}", rgba[0], rgba[1], rgba[2]);
            self.editor
                .set_char(cx, kind::COLOR, Some(Value::Text(hex)), &tr!("text-color"));
        }
        ui.menu_button(
            RichText::new(tr!("highlight")).size(theme.small_size + 1.0),
            |ui| {
                for (name, color) in [
                    (tr!("yellow"), "yellow"),
                    (tr!("green"), "green"),
                    (tr!("blue"), "blue"),
                    (tr!("pink"), "pink"),
                    (tr!("none"), "none"),
                ] {
                    if ui.button(name).clicked() {
                        *run = Some(format!("format.highlight.{color}"));
                        ui.close();
                    }
                }
            },
        );
    }

    // ----- Binder ----------------------------------------------------------------
}

pub(super) fn font_picker(ui: &mut Ui, salt: &str, font: &mut String, filter: &mut String) {
    egui::ComboBox::from_id_salt(salt.to_owned())
        .selected_text(font.clone())
        .height(360.0)
        .width(180.0)
        .show_ui(ui, |ui| {
            ui.add(egui::TextEdit::singleline(filter).hint_text(tr!("search-fonts")));
            if let Some(family) = sue_wm::fonts::list_ui(ui, font, filter) {
                *font = family;
            }
        });
}
