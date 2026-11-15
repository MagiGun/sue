//! The Animation Cels palette: the light tables of the cel being drawn.

use egui::{Rect, Ui, pos2};
use sue_core::EntityId;
use sue_wm::ui::{self as wm_ui, Density, Space, TextRole};
use sue_wm::{AppletContext, Theme};

use crate::panels::child;
use crate::panels::timeline::icon;
use crate::studio::SuePaint;

/// A button of the palette: its row and column, its icon's stable name,
/// its command, whether it can be used and whether it is lit.
type Button = (usize, usize, &'static str, &'static str, bool, bool);

impl SuePaint {
    /// The Animation Cels palette, as Clip Studio Paint's: three rows of
    /// light table and cel buttons with the opacity and color mode on the
    /// first, then an entry for each layer on the cel's light table and on
    /// the general one, each with its picture, its name and a button that
    /// takes it off.
    pub(super) fn cels_tab(
        &mut self,
        ui: &mut Ui,
        body: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let cel = self.editing_cel(&cx.world.arena);
        let has = cel.is_some();
        let registered = cel.zip(self.layer()).is_some_and(|(cel, layer)| {
            crate::anim::light_table(&cx.world.arena, cel).contains(&layer)
        });
        let side = Density::of(ui).row();
        let gap = theme.space(Space::S1);
        let edge = theme.space(Space::S3);
        let at = |row: usize, column: usize| {
            Rect::from_min_size(
                pos2(
                    body.left() + edge + column as f32 * (side + gap),
                    body.top() + edge + row as f32 * (side + gap),
                ),
                egui::Vec2::splat(side),
            )
        };
        let buttons: [Button; 14] = [
            (
                0,
                0,
                "reset",
                "anim.light_table.reset",
                has || self.light_general,
                false,
            ),
            (0, 1, "reverse", "anim.light_table.reverse", has, false),
            (
                0,
                2,
                "layer-opacity-target",
                "anim.light_table.opacity_target",
                true,
                self.opacity_general,
            ),
            (
                1,
                0,
                "light-table",
                "anim.light_table",
                true,
                self.light_table,
            ),
            (
                1,
                1,
                "cel-lock",
                "anim.cel.lock",
                has || self.cel_lock.is_some(),
                self.cel_lock.is_some(),
            ),
            (1, 2, "chevron-left", "anim.cel.prev", true, false),
            (1, 3, "chevron-right", "anim.cel.next", true, false),
            (1, 4, "cel-new", "anim.cel.new", true, false),
            (
                1,
                6,
                "light-table-file",
                "anim.light_table.file",
                has,
                false,
            ),
            (
                1,
                7,
                "light-table-add",
                "anim.light_table.register",
                has,
                false,
            ),
            (
                1,
                8,
                "light-table-remove",
                "anim.light_table.deregister",
                registered,
                false,
            ),
            (
                2,
                0,
                "light-table-tool",
                "anim.light_table.tool",
                true,
                self.light_tool,
            ),
            (2, 1, "cel", "anim.light_table.cel", true, self.light_cel),
            (
                2,
                2,
                "layers",
                "anim.light_table.general",
                true,
                self.light_general,
            ),
        ];
        let mut run = None;
        for (row, column, name, command, enabled, lit) in buttons {
            let tip = self.button_name(cx, command);
            let mut button = wm_ui::icon_button(("cels-button", command), icon(name), &tip)
                .enabled(enabled)
                .quiet();
            if lit {
                button = button.selected(true);
            }
            if button.show_at(ui, at(row, column), theme).clicked() {
                run = Some(command);
            }
        }
        // Opacity and Color Mode, on the first row after its buttons. The
        // slider sets the cel's light table, or the general one (Switch
        // Opacity Target).
        let color_rect = Rect::from_min_size(
            pos2(body.right() - edge - side, at(0, 0).top()),
            egui::Vec2::splat(side),
        );
        let opacity_rect = Rect::from_min_max(
            pos2(at(0, 3).left() + gap, at(0, 3).top()),
            pos2(color_rect.left() - gap, at(0, 3).bottom()),
        );
        let mut opacity = f64::from(if self.opacity_general {
            self.general_opacity
        } else {
            self.light_opacity
        }) * 100.0;
        if opacity_rect.width() > side
            && wm_ui::wedge("cels-opacity", &mut opacity, 0.0..=100.0)
                .label(&tr!("opacity"))
                .show_at(ui, opacity_rect, theme)
                .changed()
        {
            let opacity = (opacity / 100.0) as f32;
            if self.opacity_general {
                self.general_opacity = opacity;
            } else {
                self.light_opacity = opacity;
            }
        }
        let tip = self.button_name(cx, "anim.light_table.color");
        let mut color =
            wm_ui::icon_button("cels-color-mode", icon("layer-color-mode"), &tip).quiet();
        if self.light_color {
            color = color.selected(true);
        }
        if color.show_at(ui, color_rect, theme).clicked() {
            run = Some("anim.light_table.color");
        }

        // The entries.
        let list = Rect::from_min_max(
            pos2(body.left() + edge, at(2, 0).bottom() + edge),
            pos2(body.right() - edge, body.bottom() - edge),
        );
        let mut inner = child(ui, list, "cels-light-table");
        let caption = |ui: &mut Ui, text: &str| {
            ui.label(
                egui::RichText::new(text)
                    .font(theme.text_font(TextRole::Caption))
                    .color(theme.text_muted()),
            );
        };
        let mut off: Option<(Option<EntityId>, EntityId)> = None;
        match cel {
            None => caption(&mut inner, &tr!("go-frame-cel-see-light-table")),
            Some(cel) => {
                let name = self.entity_name(cx, cel);
                caption(&mut inner, &tr!("light-table-of-cel", value = name));
                let layers = crate::anim::light_table(&cx.world.arena, cel);
                if layers.is_empty() {
                    caption(&mut inner, &tr!("pick-layer-trace-then-register-last"));
                }
                for layer in layers {
                    if self.light_table_entry(&mut inner, layer, cx, theme) {
                        off = Some((Some(cel), layer));
                    }
                }
            }
        }
        // The general light table: shown on every cel.
        if let Some(picture) = self.picture() {
            inner.add_space(theme.space(Space::S4));
            caption(&mut inner, &tr!("general-light-table"));
            for layer in crate::anim::general_light_table(&cx.world.arena, picture) {
                if self.light_table_entry(&mut inner, layer, cx, theme) {
                    off = Some((None, layer));
                }
            }
            let register = tr!("register-layer-general-light-table");
            if wm_ui::button("cels-register-general", &register)
                .icon(icon("light-table-add"))
                .show(&mut inner, theme)
                .clicked()
            {
                run = Some("anim.light_table.register_general");
            }
            match off {
                Some((Some(cel), layer)) => {
                    Self::apply(cx, crate::anim::deregister(cel, &[layer]));
                }
                Some((None, layer)) => {
                    Self::apply(cx, crate::anim::deregister_general(picture, &[layer]));
                }
                None => {}
            }
        }
        if let Some(command) = run {
            self.run(command, cx);
        }
    }

    /// The name of `id`, or "Layer" when it is gone.
    fn entity_name(&self, cx: &AppletContext<'_>, id: EntityId) -> String {
        cx.world
            .arena
            .get_by_id(id)
            .map_or_else(|_| tr!("layer-2"), |entity| entity.name().to_owned())
    }

    /// One layer on a light table: its picture, its name and a button
    /// that takes it off. `true` when that button was pressed.
    fn light_table_entry(
        &mut self,
        ui: &mut Ui,
        layer: EntityId,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) -> bool {
        let side = Density::of(ui).row();
        let name = self.entity_name(cx, layer);
        let mut off = false;
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(side), egui::Sense::hover());
            self.cel_thumb(ui.painter(), rect, layer, cx, theme);
            ui.label(
                egui::RichText::new(&name)
                    .font(theme.text_font(TextRole::Body))
                    .color(theme.text),
            );
            off = wm_ui::icon_button(
                ("cels-off", layer),
                icon("close"),
                &tr!("deregister-from-the-light-table"),
            )
            .quiet()
            .show(ui, theme)
            .clicked();
        });
        off
    }
}
