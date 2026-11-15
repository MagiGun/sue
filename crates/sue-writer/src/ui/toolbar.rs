//! The formatting toolbar: styles, fonts, colors and the font picker.

use egui::{Id, Rect, Ui, UiBuilder, Vec2};
use sue_core::Value;
use sue_model::mark_kind as kind;
use sue_wm::icons::Icon;
use sue_wm::ui::{Density, Space, dropdown, icon_button, picker_box, popover, toggle_button};
use sue_wm::{AppletContext, Theme, widgets};

use crate::studio::{SueWriter, View};

use super::widgets::{align_icon, tool, tool_toggle};
use super::{THUMB, TOOLBAR_HEIGHT};

/// The sizes the size list offers; the text's own size joins them when it
/// is none of these.
const SIZES: [f32; 15] = [
    8.0, 9.0, 10.0, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 24.0, 28.0, 32.0, 36.0, 48.0, 72.0,
];

/// The font box's width, in control heights.
const FONT_BOX: f32 = 6.0;

/// The font search field's width, in control heights.
const FONT_SEARCH: f32 = 8.0;

/// The grid an icon is drawn on, and how much of a button its square
/// takes at most (as `sue_wm::ui::icon_button` draws it).
const ICON_GRID: f32 = 24.0;
const ICON_SHARE: f32 = 0.75;

/// The bar of the text color icon on that grid: left, top, right, bottom.
const COLOR_BAR: (f32, f32, f32, f32) = (3.0, 17.5, 21.0, 21.0);

/// How wide the text color's wheel is, in points.
const COLOR_WHEEL: f32 = 180.0;

/// The highlight colors, by the name the command takes.
const HIGHLIGHTS: [&str; 5] = ["yellow", "green", "blue", "pink", "none"];

/// The density of bar buttons `size` points tall: a desk's compact
/// buttons, a tablet's regular ones, a thumb's on a phone.
fn density_for(size: f32) -> Density {
    if size >= THUMB {
        Density::Touch
    } else if size > Density::Compact.icon_button() {
        Density::Regular
    } else {
        Density::Compact
    }
}

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
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::ZERO, theme.panel);
        ui.painter()
            .hline(rect.x_range(), rect.bottom(), theme.hairline());
        let mut run: Option<String> = None;
        let pad = theme.space(Space::S2);
        let side = theme.space(Space::S4);
        let inner = Rect::from_min_max(
            rect.min + Vec2::new(side, pad),
            rect.max - Vec2::new(side, pad),
        );
        let first_row = Rect::from_min_size(
            inner.min,
            Vec2::new(
                inner.width(),
                (self.toolbar_height - 2.0 * pad).min(TOOLBAR_HEIGHT),
            ),
        );
        // A tablet's buttons are bigger, for fingers and pens.
        let tablet = cx.view.form_factor == sue_platform::FormFactor::Tablet;
        let density = if tablet {
            Density::Regular
        } else {
            Density::Compact
        };
        let mut right = ui.new_child(
            UiBuilder::new()
                .max_rect(first_row)
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        right.set_clip_rect(rect.intersect(ui.clip_rect()));
        let (name, keys) = self.tip_of("edit.find");
        let find = density.scope(&mut right, |ui| {
            tool_toggle(
                ui,
                theme,
                "writer-find-toggle",
                Icon::Search,
                (&name, &keys),
                self.find.is_some(),
            )
        });
        if find.clicked() {
            if self.find.is_some() {
                self.find = None;
            } else {
                run = Some("edit.find".to_owned());
            }
        }
        let taken = right.min_rect().width() + theme.space(Space::S5);
        let left = Rect::from_min_max(
            inner.min,
            egui::pos2((inner.right() - taken).max(inner.left()), inner.bottom()),
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
        row.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(theme.space(Space::S2), pad);
            let size = density.icon_button();
            self.format_controls(ui, cx, theme, size, &mut run);
        });
        self.toolbar_height = (row.min_rect().bottom() - rect.top() + pad).max(TOOLBAR_HEIGHT);
        if let Some(command) = run {
            self.run_typing(&command, cx);
        }
    }

    /// A bar button for `command` showing `icon`, lit while `on` (`None`:
    /// a plain button). Its tooltip is the command's name and keys.
    fn command_button(
        &self,
        ui: &mut Ui,
        theme: &Theme,
        command: &str,
        icon: Icon,
        on: Option<bool>,
        run: &mut Option<String>,
    ) {
        let (name, keys) = self.tip_of(command);
        let salt = format!("writer-tool-{command}");
        let response = match on {
            Some(on) => tool_toggle(ui, theme, &salt, icon, (&name, &keys), on),
            None => tool(ui, theme, &salt, icon, &name, &keys),
        };
        if response.clicked() {
            *run = Some(command.to_owned());
        }
    }

    /// The formatting controls, `size` points tall: a desk's compact
    /// buttons, a tablet's regular ones, a thumb's 44 on a phone. A phone
    /// has undo and redo in the top bar, starts with what it reaches for
    /// most and steps through the alignments with one button.
    pub(super) fn format_controls(
        &mut self,
        ui: &mut Ui,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
        size: f32,
        run: &mut Option<String>,
    ) {
        density_for(size).scope(ui, |ui| self.format_row(ui, cx, theme, run));
    }

    #[allow(clippy::too_many_lines, reason = "one row of controls")]
    fn format_row(
        &mut self,
        ui: &mut Ui,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
        run: &mut Option<String>,
    ) {
        let big = self.phone;
        let sheet = self.sheet(cx);
        let (para, chars) = self.editor.look_here(&sheet);
        let text_on = self.editor.current().is_some() && self.view == View::Document;
        if !big {
            let history = (cx.history.can_undo(), cx.history.can_redo());
            for (enabled, icon, command) in [
                (history.0, Icon::Undo, "edit.undo"),
                (history.1, Icon::Redo, "edit.redo"),
            ] {
                ui.add_enabled_ui(enabled, |ui| {
                    self.command_button(ui, theme, command, icon, None, run);
                });
            }
            widgets::pill_divider(ui, theme);
        }
        ui.add_enabled_ui(text_on, |ui| {
            // Paragraph style.
            let names: Vec<&str> = sheet
                .styles
                .iter()
                .map(|style| style.name.as_str())
                .collect();
            let now = sheet
                .styles
                .iter()
                .position(|style| style.key == para.style)
                .unwrap_or(0);
            let mut chosen = now;
            let style_name = tr!("paragraph-style");
            dropdown("writer-style", &mut chosen, &names)
                .label(&style_name)
                .show(ui, theme)
                .on_hover_text(&style_name);
            if chosen != now
                && let Some(style) = sheet.styles.get(chosen)
            {
                *run = Some(format!("style.{}", style.key));
            }
            if !big {
                self.font_controls(ui, cx, theme, &para, &chars);
            }
            widgets::pill_divider(ui, theme);
            for (icon, command, on) in [
                (Icon::Bold, "format.bold", chars.bold || para.bold),
                (Icon::Italic, "format.italic", chars.italic || para.italic),
                (Icon::Underline, "format.underline", chars.underline),
            ] {
                self.command_button(ui, theme, command, icon, Some(on), run);
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
                let name = tr!("alignment-next", next = next.label());
                if tool(
                    ui,
                    theme,
                    "writer-align-next",
                    align_icon(para.align),
                    &name,
                    "",
                )
                .clicked()
                {
                    *run = Some(command.to_owned());
                }
            } else {
                for (align, command) in [
                    (sue_type::Align::Left, "align.left"),
                    (sue_type::Align::Center, "align.center"),
                    (sue_type::Align::Right, "align.right"),
                    (sue_type::Align::Justify, "align.justify"),
                ] {
                    let on = Some(para.align == align);
                    self.command_button(ui, theme, command, align_icon(align), on, run);
                }
                widgets::pill_divider(ui, theme);
            }
            let bullet = para.list == Some(sue_type::ListKind::Bullet);
            let number = para.list == Some(sue_type::ListKind::Number);
            self.command_button(
                ui,
                theme,
                "list.bullet",
                Icon::ListBullets,
                Some(bullet),
                run,
            );
            self.command_button(
                ui,
                theme,
                "list.number",
                Icon::ListNumbers,
                Some(number),
                run,
            );
            self.command_button(ui, theme, "indent.less", Icon::IndentLess, None, run);
            self.command_button(ui, theme, "indent.more", Icon::IndentMore, None, run);
            if big {
                widgets::pill_divider(ui, theme);
                self.font_controls(ui, cx, theme, &para, &chars);
                self.color_controls(ui, cx, theme, &chars, run);
                // Then what the menus hold that a phone reaches for: more
                // formatting, and what goes into the text.
                widgets::pill_divider(ui, theme);
                let strike = Some(chars.strike);
                self.command_button(ui, theme, "format.strike", Icon::Strikethrough, strike, run);
                for (label, command) in [
                    (tr!("quick-clear"), "format.clear"),
                    (tr!("quick-link"), "insert.link"),
                    (tr!("quick-picture"), "insert.picture"),
                    (tr!("quick-table"), "table.insert"),
                    (tr!("quick-footnote"), "insert.footnote"),
                    (tr!("quick-comment"), "review.comment"),
                    (tr!("quick-page-break"), "insert.page_break"),
                ] {
                    let salt = format!("writer-quick-{command}");
                    if sue_wm::ui::button(salt.as_str(), &label)
                        .ghost()
                        .show(ui, theme)
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
        // The installed fonts, each in its own face, under a search field:
        // a box like the style's and the size's opens them in a popover.
        let open_id = Id::unique("writer-font-open");
        let filter_id = Id::unique("writer-font-filter");
        let mut open = ui.data(|data| data.get_temp::<bool>(open_id).unwrap_or(false));
        let line = Density::of(ui).control_height();
        let name = tr!("font");
        let font_box = picker_box(
            ui,
            "writer-font",
            &font,
            &name,
            open,
            line * FONT_BOX,
            theme,
        );
        let opened = font_box.clicked() && !open;
        if font_box.clicked() {
            open = !open;
        }
        let picked = popover(
            ui.ctx(),
            open_id.with("list"),
            font_box.rect,
            &mut open,
            theme,
            |ui| {
                ui.set_width(line * FONT_SEARCH);
                let mut filter: String =
                    ui.data(|data| data.get_temp(filter_id)).unwrap_or_default();
                let search = sue_wm::ui::search_field("writer-font-search", &mut filter)
                    .hint(&tr!("search-fonts"))
                    .width(line * FONT_SEARCH)
                    .show(ui, theme);
                if opened {
                    search.request_focus();
                }
                ui.data_mut(|data| data.insert_temp(filter_id, filter.clone()));
                if !sue_wm::fonts::ready() {
                    ui.label(
                        egui::RichText::new(tr!("finding-installed-fonts-ellipsis"))
                            .color(theme.text_muted()),
                    );
                }
                let mut picked = None;
                let styles = tr!("styles-font");
                if toggle_button("writer-font-style", &styles, chars.font.is_none())
                    .ghost()
                    .full_width()
                    .show(ui, theme)
                    .clicked()
                {
                    picked = Some(None);
                }
                if let Some(family) = sue_wm::fonts::list_ui(ui, &font, &filter) {
                    picked = Some(Some(family));
                }
                picked
            },
        )
        .flatten();
        if let Some(family) = picked {
            self.editor
                .set_char(cx, kind::FONT, family.map(Value::Text), &name);
            open = false;
            // Writing goes on in the face just picked, as after Bold.
            self.return_to_text();
        }
        ui.data_mut(|data| data.insert_temp(open_id, open));
        let size = chars.size.unwrap_or(para.size);
        let mut sizes: Vec<f32> = SIZES.to_vec();
        if !sizes.iter().any(|option| (option - size).abs() < 0.1) {
            sizes.push(size);
            sizes.sort_by(f32::total_cmp);
        }
        let labels: Vec<String> = sizes.iter().map(|option| format!("{option:.0}")).collect();
        let now = sizes
            .iter()
            .position(|option| (option - size).abs() < 0.1)
            .unwrap_or(0);
        let mut chosen = now;
        let size_name = tr!("size");
        dropdown("writer-size", &mut chosen, &labels)
            .label(&size_name)
            .show(ui, theme)
            .on_hover_text(&size_name);
        if chosen != now
            && let Some(option) = sizes.get(chosen)
        {
            self.editor.set_char(
                cx,
                kind::SIZE,
                Some(Value::Float(f64::from(*option))),
                &tr!("size"),
            );
            self.return_to_text();
        }
    }

    /// Gives the keyboard back to the text after a pick in a box of the
    /// toolbar, so the caret is where it was.
    fn return_to_text(&mut self) {
        if self.view == View::Document {
            self.editor.focus();
        }
    }

    /// The text color: a button with the letter A over a stripe of the
    /// color, which opens a color wheel. A change is applied when a drag
    /// on the wheel ends, so one pick is one undo step.
    fn text_color(
        &mut self,
        ui: &mut Ui,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
        chars: &sue_type::CharFormat,
    ) {
        // Text with no color of its own is drawn in the paper's ink.
        let ink = sue_type::Ink::paper().text.to_array();
        let mut rgba = chars.color.map_or(ink, |[r, g, b]| [r, g, b, u8::MAX]);
        let open_id = Id::unique("writer-color-open");
        let mut open = ui.data(|data| data.get_temp::<bool>(open_id).unwrap_or(false));
        let name = tr!("text-color");
        let button = icon_button("writer-color", Icon::TextColor, &name)
            .quiet()
            .selected(open)
            .show(ui, theme);
        // The icon's bar takes the color itself, outlined so that black
        // reads on a dark bar and white on a light one. The bar lies where
        // the icon's description puts it, along the foot of its square.
        let side = Density::of(ui)
            .icon()
            .min(button.rect.width().min(button.rect.height()) * ICON_SHARE)
            .round();
        let unit = side / ICON_GRID;
        let square = Rect::from_center_size(button.rect.center(), Vec2::splat(side));
        let bar = Rect::from_min_max(
            square.min + Vec2::new(COLOR_BAR.0, COLOR_BAR.1) * unit,
            square.min + Vec2::new(COLOR_BAR.2, COLOR_BAR.3) * unit,
        );
        let [r, g, b, _] = rgba;
        ui.painter().rect(
            bar,
            egui::CornerRadius::same(unit.round().clamp(0.0, 255.0) as u8),
            egui::Color32::from_rgb(r, g, b),
            egui::Stroke::new(theme.border_width, theme.border_strong()),
            egui::StrokeKind::Inside,
        );
        if button.clicked() {
            open = !open;
        }
        let pick = popover(
            ui.ctx(),
            open_id.with("wheel"),
            button.rect,
            &mut open,
            theme,
            |ui| sue_wm::color_wheel(ui, open_id.with("hue"), &mut rgba, COLOR_WHEEL),
        );
        if let Some(pick) = pick
            && (pick.released || (pick.changed && !pick.dragging))
        {
            let hex = format!(
                "#{r:02x}{g:02x}{b:02x}",
                r = rgba[0],
                g = rgba[1],
                b = rgba[2]
            );
            self.editor
                .set_char(cx, kind::COLOR, Some(Value::Text(hex)), &name);
        }
        ui.data_mut(|data| data.insert_temp(open_id, open));
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
        self.text_color(ui, cx, theme, chars);
        let open_id = Id::unique("writer-highlight-open");
        let mut open = ui.data(|data| data.get_temp::<bool>(open_id).unwrap_or(false));
        let name = tr!("highlight");
        let button = icon_button("writer-highlight", Icon::Highlight, &name)
            .quiet()
            .selected(open)
            .show(ui, theme);
        if button.clicked() {
            open = !open;
        }
        let names = [
            tr!("yellow"),
            tr!("green"),
            tr!("blue"),
            tr!("pink"),
            tr!("none"),
        ];
        let picked = popover(
            ui.ctx(),
            open_id.with("list"),
            button.rect,
            &mut open,
            theme,
            |ui| {
                let mut picked = None;
                for (label, color) in names.iter().zip(HIGHLIGHTS) {
                    if sue_wm::ui::button(color, label)
                        .ghost()
                        .full_width()
                        .show(ui, theme)
                        .clicked()
                    {
                        picked = Some(color);
                    }
                }
                picked
            },
        )
        .flatten();
        if let Some(color) = picked {
            *run = Some(format!("format.highlight.{color}"));
            open = false;
        }
        ui.data_mut(|data| data.insert_temp(open_id, open));
    }
}
