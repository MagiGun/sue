//! Tool Property for the Text tool.

use egui::Ui;
use sue_wm::Theme;

use super::small_text;
use crate::lettering::Typing;
use crate::panels::slider_row;
use crate::tools::SubTool;

/// Clip Studio Paint's Text tool property: the font, its size,
/// the direction and the justification, also for the words
/// being typed.
pub(super) fn text_properties(
    ui: &mut Ui,
    sub: &mut SubTool,
    typing: Option<&mut Typing>,
    theme: &Theme,
) {
    let small = small_text(theme);
    ui.horizontal(|ui| {
        ui.label(small(&tr!("font")));
        let shown = if sub.font.is_empty() {
            tr!("default-font-ubuntu-light")
        } else {
            sub.font.clone()
        };
        egui::ComboBox::from_id_salt("sue-paint-text-tool-font")
            .selected_text(shown)
            .width(ui.available_width().max(80.0))
            .height(320.0)
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
            });
    });
    slider_row(
        ui,
        &tr!("size-2"),
        &mut sub.tip.size,
        6.0..=400.0,
        &tr!("px"),
        true,
        theme,
    );
    ui.horizontal(|ui| {
        ui.radio_value(&mut sub.kind, 0, small(&tr!("pref-horizontal")));
        ui.radio_value(&mut sub.kind, 1, small(&tr!("pref-vertical")));
    });
    ui.checkbox(&mut sub.whole, small(&tr!("centered")));
    // Clip Studio Paint's letter spacing, line spacing and
    // justification.
    slider_row(
        ui,
        &tr!("letter-spacing"),
        &mut sub.tracking,
        -0.3..=1.5,
        &tr!("em"),
        false,
        theme,
    );
    let mut leading = if sub.leading > 0.0 { sub.leading } else { 1.0 };
    if slider_row(
        ui,
        &tr!("line-spacing"),
        &mut leading,
        0.5..=3.0,
        "x",
        false,
        theme,
    ) {
        sub.leading = leading;
    }
    ui.horizontal(|ui| {
        ui.label(small(&tr!("justify")));
        ui.radio_value(&mut sub.justify, 0, small(&tr!("justify-start")));
        ui.radio_value(&mut sub.justify, 1, small(&tr!("justify-middle")));
        ui.radio_value(&mut sub.justify, 2, small(&tr!("justify-end")));
    });
    // Ruby (furigana): a reading over the selected words.
    if let Some(typing) = typing {
        ui.separator();
        ui.horizontal(|ui| {
            ui.label(small(&tr!("ruby")));
            let field = ui.add(
                egui::TextEdit::singleline(&mut typing.reading)
                    .hint_text(tr!("ruby-hint"))
                    .desired_width(ui.available_width()),
            );
            if field.gained_focus() || field.clicked() {
                typing.reading_focus = true;
            }
            if field.lost_focus() {
                if ui.input(|input| input.key_pressed(egui::Key::Enter))
                    && !typing.selected.is_empty()
                {
                    typing.add_ruby();
                }
                typing.reading_focus = false;
            }
        });
        ui.horizontal(|ui| {
            let chosen = !typing.selected.is_empty();
            if ui
                .add_enabled(
                    chosen && !typing.reading.trim().is_empty(),
                    egui::Button::new(small(&tr!("set-ruby"))),
                )
                .on_disabled_hover_text(tr!("ruby-needs-selection"))
                .clicked()
            {
                typing.add_ruby();
            }
            if ui
                .add_enabled(
                    chosen && typing.has_ruby(),
                    egui::Button::new(small(&tr!("remove-ruby"))),
                )
                .clicked()
            {
                typing.remove_ruby();
            }
        });
    }
}
