//! Tool Property for the Text tool.

use egui::Ui;
use sue_wm::Theme;
use sue_wm::ui::TextRole;

use super::{Category, Rows};
use crate::lettering::Typing;
use crate::tools::SubTool;

/// Clip Studio Paint's Text tool property: the font, its size,
/// the direction and the justification, also for the words
/// being typed.
pub(super) fn text_properties(
    ui: &mut Ui,
    rows: &mut Rows<'_>,
    sub: &mut SubTool,
    typing: Option<&mut Typing>,
    theme: &Theme,
) {
    rows.row(ui, "font", Category::Text, |ui, theme, enabled| {
        ui.add_enabled_ui(enabled, |ui| font_row(ui, sub, theme));
    });
    rows.slider(
        ui,
        "text-size",
        Category::Text,
        &tr!("size-2"),
        &mut sub.tip.size,
        6.0..=400.0,
        &tr!("px"),
        true,
    );
    // Bold, italic, underline and strike through: each a switch.
    rows.row(ui, "text-style", Category::Text, |ui, theme, enabled| {
        style_row(ui, &mut sub.text_style, theme, enabled);
    });
    rows.choice_u8(
        ui,
        "text-direction",
        Category::Text,
        &tr!("direction"),
        &mut sub.kind,
        &[tr!("pref-horizontal"), tr!("pref-vertical")],
    );
    rows.check(
        ui,
        "text-centered",
        Category::Text,
        &tr!("centered"),
        None,
        &mut sub.whole,
    );
    // Clip Studio Paint's letter spacing, line spacing and justification.
    rows.slider(
        ui,
        "letter-spacing",
        Category::Text,
        &tr!("letter-spacing"),
        &mut sub.tracking,
        -0.3..=1.5,
        &tr!("em"),
        false,
    );
    let mut leading = if sub.leading > 0.0 { sub.leading } else { 1.0 };
    if rows.slider(
        ui,
        "line-spacing",
        Category::Text,
        &tr!("line-spacing"),
        &mut leading,
        0.5..=3.0,
        "x",
        false,
    ) {
        sub.leading = leading;
    }
    rows.choice_u8(
        ui,
        "justify",
        Category::Text,
        &tr!("justify"),
        &mut sub.justify,
        &[
            tr!("justify-start"),
            tr!("justify-middle"),
            tr!("justify-end"),
        ],
    );
    // Ruby (furigana): a reading over the selected words.
    if let Some(typing) = typing {
        rows.row(ui, "ruby", Category::Text, |ui, _, _| {
            ruby_rows(ui, typing, theme)
        });
    }
}

/// The Style row: four switches, lit while on.
fn style_row(ui: &mut Ui, bits: &mut u8, theme: &Theme, enabled: bool) {
    use crate::tools::{STYLE_BOLD, STYLE_ITALIC, STYLE_STRIKE, STYLE_UNDERLINE};
    use sue_wm::icons::Icon;
    ui.horizontal(|ui| {
        super::name_cell(ui, &tr!("text-style"), super::name_width(theme), theme);
        let side = sue_wm::ui::Density::of(ui).icon_button();
        for (bit, icon, name) in [
            (STYLE_BOLD, Icon::Bold, tr!("bold")),
            (STYLE_ITALIC, Icon::Italic, tr!("italic")),
            (STYLE_UNDERLINE, Icon::Underline, tr!("underline")),
            (STYLE_STRIKE, Icon::Strikethrough, tr!("strike-through")),
        ] {
            let on = *bits & bit != 0;
            let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(side), egui::Sense::hover());
            let clicked = sue_wm::ui::icon_button(("sue-paint-text-style", bit), icon, &name)
                .selected(on)
                .enabled(enabled)
                .show_at(ui, rect, theme)
                .clicked();
            if clicked {
                *bits ^= bit;
            }
        }
    });
}

/// The font: Sue's own, or any installed family from Sue's font list.
fn font_row(ui: &mut Ui, sub: &mut SubTool, theme: &Theme) {
    ui.horizontal(|ui| {
        ui.label(super::small_text(theme)(&tr!("font")));
        let shown = if sub.font.is_empty() {
            tr!("default-font-ubuntu-light")
        } else {
            sub.font.clone()
        };
        egui::ComboBox::from_id_salt("sue-paint-text-tool-font")
            .selected_text(shown)
            .width(ui.available_width())
            .height(theme.text_size(TextRole::Body) * 24.0)
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut sub.font,
                    String::new(),
                    tr!("default-font-ubuntu-light"),
                );
                let selected = sub.font.clone();
                if let Some(family) = sue_wm::fonts::list_ui(ui, &selected, "") {
                    sub.font = family;
                }
            })
            .response
            .on_hover_text(tr!("font"));
    });
}

/// The reading of the words picked, and the buttons that set and remove
/// it.
fn ruby_rows(ui: &mut Ui, typing: &mut Typing, theme: &Theme) {
    let label = tr!("ruby");
    let field = sue_wm::ui::input::text_field("sue-paint-ruby", &mut typing.reading)
        .label(&label)
        .hint(&tr!("ruby-hint"))
        .show(ui, theme)
        .on_hover_text(&label);
    if field.gained_focus() || field.clicked() {
        typing.reading_focus = true;
    }
    if field.lost_focus() {
        if ui.input(|input| input.key_pressed(egui::Key::Enter)) && !typing.selected.is_empty() {
            typing.add_ruby();
        }
        typing.reading_focus = false;
    }
    ui.horizontal(|ui| {
        let chosen = !typing.selected.is_empty();
        let set = tr!("set-ruby");
        let can_set = chosen && !typing.reading.trim().is_empty();
        let response = sue_wm::ui::button("sue-paint-set-ruby", &set)
            .enabled(can_set)
            .show(ui, theme);
        let response = if can_set {
            response.on_hover_text(&set)
        } else {
            response.on_hover_text(tr!("ruby-needs-selection"))
        };
        if response.clicked() {
            typing.add_ruby();
        }
        let remove = tr!("remove-ruby");
        if sue_wm::ui::button("sue-paint-remove-ruby", &remove)
            .enabled(chosen && typing.has_ruby())
            .show(ui, theme)
            .on_hover_text(&remove)
            .clicked()
        {
            typing.remove_ruby();
        }
    });
}
