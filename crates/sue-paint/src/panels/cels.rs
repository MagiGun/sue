//! The cels tab.

use egui::{Rect, RichText, Ui, pos2, vec2};
use sue_wm::{AppletContext, Theme};

use crate::icons::Icon;
use crate::panels::{Wedge, child, icon_button};
use crate::studio::SuePaint;

impl SuePaint {
    /// The Animation Cels tab, as Clip Studio Paint's Animation cels
    /// palette: its three rows of light table and cel buttons with the
    /// opacity between them, then the layers on the cel's light table.
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
        let side = 24.0;
        let at = |row: usize, column: usize| {
            Rect::from_min_size(
                body.min
                    + vec2(
                        6.0 + column as f32 * (side + 2.0),
                        6.0 + row as f32 * (side + 4.0),
                    ),
                egui::Vec2::splat(side),
            )
        };
        // (row, column, icon, tip, command, enabled, active).
        let buttons = [
            (
                0,
                0,
                Icon::Reset,
                tr!("reset-position-layers-light-table"),
                "anim.light_table.reset",
                has || self.light_general,
                false,
            ),
            (
                0,
                1,
                Icon::Reverse,
                tr!("reverse-layers-vertically-light-table"),
                "anim.light_table.reverse",
                has,
                false,
            ),
            (
                0,
                2,
                Icon::OpacityTarget,
                tr!("switch-opacity-target"),
                "anim.light_table.opacity_target",
                true,
                self.opacity_general,
            ),
            (
                1,
                0,
                Icon::LightTable,
                tr!("enable-light-table-2"),
                "anim.light_table",
                true,
                self.light_table,
            ),
            (
                1,
                1,
                Icon::Lock,
                tr!("lock-cel-editing-target"),
                "anim.cel.lock",
                has || self.cel_lock.is_some(),
                self.cel_lock.is_some(),
            ),
            (
                1,
                2,
                Icon::Prev,
                tr!("select-previous-cel"),
                "anim.cel.prev",
                true,
                false,
            ),
            (
                1,
                3,
                Icon::Next,
                tr!("select-next-cel"),
                "anim.cel.next",
                true,
                false,
            ),
            (
                1,
                4,
                Icon::Cel,
                tr!("new-animation-cel-2"),
                "anim.cel.new",
                true,
                false,
            ),
            (
                1,
                6,
                Icon::Import,
                tr!("select-and-register-file"),
                "anim.light_table.file",
                has,
                false,
            ),
            (
                1,
                7,
                Icon::Reference,
                tr!("register-selected-layer-light-table"),
                "anim.light_table.register",
                has,
                false,
            ),
            (
                1,
                8,
                Icon::Minus,
                tr!("deregister-selected-light-table"),
                "anim.light_table.deregister",
                registered,
                false,
            ),
            (
                2,
                0,
                Icon::Operation,
                tr!("light-table-tool"),
                "anim.light_table.tool",
                true,
                self.light_tool,
            ),
            (
                2,
                1,
                Icon::Onion,
                tr!("show-cel-light-table"),
                "anim.light_table.cel",
                true,
                self.light_cel,
            ),
            (
                2,
                2,
                Icon::Layers,
                tr!("show-general-light-table"),
                "anim.light_table.general",
                true,
                self.light_general,
            ),
        ];
        let mut run = None;
        for (row, column, icon, tip, command, enabled, active) in &buttons {
            if icon_button(ui, at(*row, *column), *icon, *enabled, *active, tip, theme) {
                run = Some(*command);
            }
        }
        // Opacity and Color mode, on the first row after its buttons.
        let opacity_rect = Rect::from_min_max(
            pos2(at(0, 3).left() + 4.0, at(0, 3).top()),
            pos2(body.right() - side - 12.0, at(0, 3).bottom()),
        );
        let mut inner = child(ui, opacity_rect, "cels-opacity");
        // The slider sets the cel's light table, or the general one
        // (Switch opacity target).
        let mut opacity = if self.opacity_general {
            self.general_opacity
        } else {
            self.light_opacity
        } * 100.0;
        let width = (opacity_rect.width() - 50.0).max(30.0);
        let changed = inner
            .horizontal(|ui| {
                let wedge = ui
                    .add(Wedge::new(&mut opacity, 0.0..=100.0, width, theme))
                    .on_hover_text(tr!("opacity"));
                let typed = ui.add(
                    egui::DragValue::new(&mut opacity)
                        .range(0.0..=100.0)
                        .max_decimals(0),
                );
                wedge.changed() || typed.changed()
            })
            .inner;
        if changed {
            if self.opacity_general {
                self.general_opacity = opacity / 100.0;
            } else {
                self.light_opacity = opacity / 100.0;
            }
        }
        let color_rect = Rect::from_min_size(
            pos2(body.right() - side - 6.0, at(0, 0).top()),
            egui::Vec2::splat(side),
        );
        if icon_button(
            ui,
            color_rect,
            Icon::ColorMode,
            true,
            self.light_color,
            &tr!("color-mode"),
            theme,
        ) {
            run = Some("anim.light_table.color");
        }
        let mut inner = child(
            ui,
            Rect::from_min_max(
                pos2(body.left() + 8.0, at(2, 0).bottom() + 8.0),
                body.max - vec2(8.0, 8.0),
            ),
            "cels-light-table",
        );
        let small = |text: &str| {
            RichText::new(text)
                .size(theme.small_size + 0.5)
                .color(theme.muted)
        };
        match cel {
            None => {
                inner.label(small(&tr!("go-frame-cel-see-light-table")));
            }
            Some(cel) => {
                let name = |id: sue_core::EntityId| {
                    cx.world
                        .arena
                        .get_by_id(id)
                        .map_or_else(|_| tr!("layer-2"), |entity| entity.name().to_owned())
                };
                inner.label(small(&tr!("light-table-of-cel", value = name(cel))));
                let layers = crate::anim::light_table(&cx.world.arena, cel);
                if layers.is_empty() {
                    inner.label(small(&tr!("pick-layer-trace-then-register-last")));
                }
                let mut off = None;
                for layer in layers {
                    inner.horizontal(|ui| {
                        if ui
                            .small_button("x")
                            .on_hover_text(tr!("deregister-from-the-light-table"))
                            .clicked()
                        {
                            off = Some(layer);
                        }
                        ui.label(name(layer));
                    });
                }
                if let Some(layer) = off {
                    Self::apply(cx, crate::anim::deregister(cel, &[layer]));
                }
            }
        }
        // The general light table: shown on every cel.
        if let Some(picture) = self.picture() {
            inner.add_space(8.0);
            inner.label(small(&tr!("general-light-table")));
            let layers = crate::anim::general_light_table(&cx.world.arena, picture);
            let name = |id: sue_core::EntityId| {
                cx.world
                    .arena
                    .get_by_id(id)
                    .map_or_else(|_| tr!("layer-2"), |entity| entity.name().to_owned())
            };
            let mut off = None;
            for layer in layers {
                inner.horizontal(|ui| {
                    if ui
                        .small_button("x")
                        .on_hover_text(tr!("deregister-from-the-light-table"))
                        .clicked()
                    {
                        off = Some(layer);
                    }
                    ui.label(name(layer));
                });
            }
            if inner
                .small_button(tr!("register-layer-general-light-table"))
                .clicked()
            {
                run = Some("anim.light_table.register_general");
            }
            if let Some(layer) = off {
                Self::apply(cx, crate::anim::deregister_general(picture, &[layer]));
            }
        }
        if let Some(command) = run.filter(|command| !command.is_empty()) {
            self.run(command, cx);
        }
    }
}
