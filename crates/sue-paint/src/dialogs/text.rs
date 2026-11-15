//! The Text tool's dialog.

use egui::Ui;
use sue_wm::Theme;
use sue_wm::ui::{Space, checkbox, form_grid, segmented, slider};

use super::{Outcome, bare, note, tip};

/// Where the words' color comes from: the main or the sub drawing color,
/// or one of their own.
fn ink_of(rgb: [u8; 3], main: [u8; 3], sub: [u8; 3]) -> usize {
    if rgb == main {
        0
    } else if rgb == sub {
        1
    } else {
        2
    }
}

/// The Text tool's words, font, size, ink and placement. `main` and
/// `sub` are the drawing colors the words can take.
pub(super) fn text_ui(
    ui: &mut Ui,
    lettering: &mut crate::lettering::Lettering,
    main: [u8; 3],
    sub: [u8; 3],
    theme: &Theme,
) -> Outcome {
    let mut outcome = Outcome::Open;
    let edit = ui.add(
        egui::TextEdit::multiline(&mut lettering.content)
            .desired_rows(5)
            .desired_width(f32::INFINITY)
            .hint_text(tr!("type-here")),
    );
    // Its name for assistive technology and in its tooltip, as every
    // other control of the dialog has.
    let words = tr!("text");
    edit.widget_info(|| egui::WidgetInfo::labeled(egui::Role::MultilineTextInput, true, &words));
    let edit = tip(edit, &words, "", theme);
    // The words take the keys while nothing else does.
    if ui.ctx().memory(|memory| memory.focused().is_none()) {
        edit.request_focus();
    }
    let default_font = tr!("default-font-ubuntu-light");
    form_grid(ui, "sue-paint-text", theme, |form| {
        let name = bare(tr!("font"));
        form.row(&name, |ui| {
            let shown = if lettering.font.is_empty() {
                default_font.clone()
            } else {
                lettering.font.clone()
            };
            let response = egui::ComboBox::from_id_salt("sue-paint-text-font")
                .selected_text(shown)
                .width(ui.available_width())
                .height(theme.space(Space::S9) * 8.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut lettering.font, String::new(), &default_font);
                    let selected = lettering.font.clone();
                    if let Some(family) = sue_wm::fonts::list_ui(ui, &selected, "") {
                        lettering.font = family;
                    }
                })
                .response;
            tip(response, &name, "", theme);
        });
        let name = bare(tr!("size"));
        form.row(&name, |ui| {
            let mut size = f64::from(lettering.size);
            let px = tr!("px");
            let response = slider("sue-paint-text-size", &mut size, 6.0..=400.0)
                .name(&name)
                .logarithmic()
                .suffix(&px)
                .show(ui, theme);
            if response.changed() {
                lettering.size = size as f32;
            }
            tip(response, &name, "", theme);
        });
        let name = bare(tr!("color"));
        form.row(&name, |ui| {
            let inks = [tr!("main-color"), tr!("sub-color"), tr!("text-own-color")];
            let chosen = ink_of(lettering.rgb, main, sub);
            let picked = segmented("sue-paint-text-ink", &inks, Some(chosen)).show(ui, theme);
            match picked.picked {
                Some(0) => lettering.rgb = main,
                Some(1) => lettering.rgb = sub,
                _ => {}
            }
            let [r, g, b] = lettering.rgb;
            let mut rgba = [r, g, b, 255];
            if sue_wm::color_button(ui, "sue-paint-text-color", &mut rgba, false, theme).changed {
                lettering.rgb = [rgba[0], rgba[1], rgba[2]];
            }
        });
        form.wide(|ui| {
            for (salt, name, on) in [
                (
                    "sue-paint-text-center",
                    tr!("centered"),
                    &mut lettering.center,
                ),
                (
                    "sue-paint-text-vertical",
                    tr!("vertical-columns-right-to-left"),
                    &mut lettering.vertical,
                ),
            ] {
                let response = checkbox(salt, &name, *on).show(ui, theme);
                if response.clicked() {
                    *on = !*on;
                }
                tip(response, &name, "", theme);
            }
        });
    });
    note(ui, &tr!("enter-starts-new-line-ctrl-enter"), theme);
    let commit = ui.input(|input| {
        input.events.iter().any(|event| {
            matches!(
                event,
                egui::Event::Key {
                    key: egui::Key::Enter,
                    pressed: true,
                    modifiers,
                    ..
                } if modifiers.command
            )
        })
    });
    if commit {
        outcome = Outcome::Ok;
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ink_is_told_from_the_drawing_colors() {
        let (main, sub) = ([0, 0, 0], [255, 255, 255]);
        assert_eq!(ink_of(main, main, sub), 0);
        assert_eq!(ink_of(sub, main, sub), 1);
        assert_eq!(ink_of([200, 10, 10], main, sub), 2);
    }
}
