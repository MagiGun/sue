//! Tool Property for picked vector lines (the Object tool on a vector
//! layer): their main and sub color, brush size and brush shape, and how
//! the box round them works. A change shows on the lines while its
//! control is dragged and is kept as one step when it is let go; a hex
//! code typed recolors them as soon as it is whole.

use egui::Ui;
use sue_wm::Theme;
use sue_wm::ui::{Tip, checkbox, dropdown, heading, segmented, slider, text_field, tooltip};

use crate::canvas::lines::LineChange;
use crate::studio::SuePaint;
use crate::tools::Tool;
use crate::vector::{self, Restyle};

/// The tools whose sub tools offer a brush shape for lines.
const SHAPES_FROM: [Tool; 4] = [Tool::Pen, Tool::Pencil, Tool::Brush, Tool::Airbrush];

impl SuePaint {
    /// The Object tool's rows on a vector layer: the picked lines'
    /// settings, or how to pick them. `false` when the current layer is
    /// not a vector layer or the sub tool is not Object (its rows are
    /// another's).
    pub(super) fn line_properties(
        &mut self,
        ui: &mut Ui,
        arena: &sue_core::Arena,
        theme: &Theme,
    ) -> bool {
        if self.toolbox.current().mode != 0 {
            return false;
        }
        let Some(layer) = self
            .layer()
            .filter(|layer| vector::is_vector(arena, *layer))
        else {
            return false;
        };
        heading(ui, &tr!("editing-vector"), theme);
        let lines = vector::lines(&vector::geometry(arena, layer));
        let picked = self.picked_on(layer).unwrap_or_default();
        // Mode and Adjust line thickness when scaling: how the box works.
        let modes = [tr!("scale"), tr!("free-transform")];
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(tr!("mode")).color(theme.muted));
            let strip = segmented(
                "sue-paint-lines-mode",
                &modes,
                Some(usize::from(self.line_free)),
            )
            .show(ui, theme);
            if let Some(chosen) = strip.picked {
                self.line_free = chosen == 1;
            }
            tooltip(
                strip.response,
                &Tip::new(&tr!("mode")).help(&tr!("lines-mode-hint")),
                theme,
            );
        });
        let thicken = checkbox(
            "sue-paint-lines-thicken",
            &tr!("adjust-line-thickness-when-scaling"),
            self.line_thicken,
        )
        .show(ui, theme);
        if thicken.clicked() {
            self.line_thicken = !self.line_thicken;
        }
        tooltip(
            thicken,
            &Tip::new(&tr!("adjust-line-thickness-when-scaling")),
            theme,
        );
        let shown: Vec<&vector::Line> = picked
            .iter()
            .filter_map(|index| lines.get(*index))
            .collect();
        let Some(first) = shown.first() else {
            ui.label(
                egui::RichText::new(tr!("pick-lines-hint"))
                    .size(theme.small_size)
                    .color(theme.muted),
            );
            return true;
        };
        ui.label(
            egui::RichText::new(tr!("lines-picked", count = shown.len()))
                .size(theme.small_size)
                .color(theme.muted),
        );
        let mut change: Option<LineChange> = None;
        // Main and sub color: a swatch with the wheel, and the hex code.
        for (slot, name) in [(0, tr!("main-color")), (1, tr!("sub-color"))] {
            let rgb = first.colors[slot].map(|value| (value.clamp(0.0, 1.0) * 255.0).round() as u8);
            // The code follows the lines unless it is being typed.
            let kept = egui::Id::unique(("sue-paint-lines-hex-field", slot));
            let typing = ui
                .data(|data| data.get_temp::<egui::Id>(kept))
                .is_some_and(|id| ui.memory(|memory| memory.has_focus(id)));
            if !typing && crate::color::parse_hex(&self.line_hex[slot]) != Some(rgb) {
                self.line_hex[slot] = crate::color::hex(rgb);
            }
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(&name).color(theme.muted));
                let mut rgba = [rgb[0], rgb[1], rgb[2], 255];
                let pick = sue_wm::color_button(
                    ui,
                    ("sue-paint-lines-color", slot),
                    &mut rgba,
                    false,
                    theme,
                );
                if pick.changed || pick.released {
                    let unit = crate::color::unit([rgba[0], rgba[1], rgba[2]]);
                    let restyle = if slot == 0 {
                        Restyle {
                            main: Some(unit),
                            ..Restyle::default()
                        }
                    } else {
                        Restyle {
                            sub: Some(unit),
                            ..Restyle::default()
                        }
                    };
                    change = Some(LineChange {
                        restyle,
                        keep: !pick.dragging,
                    });
                }
                let field = text_field(("sue-paint-lines-hex", slot), &mut self.line_hex[slot])
                    .label(&tr!("hex-code-of", name = name.clone()))
                    .show(ui, theme);
                ui.data_mut(|data| data.insert_temp(kept, field.id));
                // A whole code recolors the lines at once, as typed.
                if field.changed()
                    && let Some(code) = crate::color::parse_hex(&self.line_hex[slot])
                    && code != rgb
                {
                    let unit = crate::color::unit(code);
                    let restyle = if slot == 0 {
                        Restyle {
                            main: Some(unit),
                            ..Restyle::default()
                        }
                    } else {
                        Restyle {
                            sub: Some(unit),
                            ..Restyle::default()
                        }
                    };
                    change = Some(LineChange {
                        restyle,
                        keep: true,
                    });
                }
                tooltip(
                    field,
                    &Tip::new(&tr!("hex-code-of", name = name.clone())).help(&tr!("hex-code-hint")),
                    theme,
                );
            });
        }
        let use_drawing =
            sue_wm::ui::button("sue-paint-lines-drawing-color", &tr!("use-drawing-color"))
                .show(ui, theme);
        let use_drawing = tooltip(
            use_drawing,
            &Tip::new(&tr!("use-drawing-color")).help(&tr!("use-drawing-color-hint")),
            theme,
        );
        if use_drawing.clicked() {
            change = Some(LineChange {
                restyle: Restyle {
                    main: Some(crate::color::unit(self.drawing_color())),
                    ..Restyle::default()
                },
                keep: true,
            });
        }
        // Brush size: shown while dragged, kept when let go.
        let mut size = f64::from(first.tip.size);
        let response = slider("sue-paint-lines-size", &mut size, 0.5..=200.0)
            .label(&tr!("brush-size"))
            .logarithmic()
            .decimals(1)
            .show(ui, theme);
        if response.changed() || response.drag_stopped() {
            change = Some(LineChange {
                restyle: Restyle {
                    size: Some(size as f32),
                    ..Restyle::default()
                },
                keep: !response.dragged(),
            });
        }
        tooltip(response, &Tip::new(&tr!("brush-size")), theme);
        // Brush shape: a pen, pencil, brush or airbrush sub tool's brush,
        // each line keeping its size.
        let shapes: Vec<(String, crate::brush::Tip)> = SHAPES_FROM
            .iter()
            .flat_map(|tool| self.toolbox.subs(*tool).iter())
            .filter(|sub| !sub.tip.eraser)
            .map(|sub| (crate::localize(&sub.name), sub.tip))
            .collect();
        let mut names = vec![tr!("brush-shape-as-drawn")];
        names.extend(shapes.iter().map(|(name, _)| name.clone()));
        // The sub tool whose brush the first picked line has, if any.
        let mut chosen = shapes
            .iter()
            .position(|(_, tip)| {
                crate::brush::Tip {
                    size: first.tip.size,
                    eraser: first.tip.eraser,
                    ..*tip
                } == first.tip
            })
            .map_or(0, |at| at + 1);
        let was = chosen;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(tr!("brush-shape")).color(theme.muted));
            let response = dropdown("sue-paint-lines-shape", &mut chosen, &names)
                .label(&tr!("brush-shape"))
                .show(ui, theme);
            if response.changed()
                && chosen != was
                && let Some((_, tip)) = chosen.checked_sub(1).and_then(|at| shapes.get(at))
            {
                change = Some(LineChange {
                    restyle: Restyle {
                        shape: Some(*tip),
                        ..Restyle::default()
                    },
                    keep: true,
                });
            }
            tooltip(
                response,
                &Tip::new(&tr!("brush-shape")).help(&tr!("brush-shape-hint")),
                theme,
            );
        });
        if change.is_some() {
            self.pending_lines = change;
        }
        true
    }

    /// Fill up to vector path, for the Fill tool: a fill that looks at
    /// vector layers stops at their lines' middles, under the lines.
    /// It only matters when the fill looks at other layers, so it is
    /// greyed out while the sub tool refers to the editing layer alone.
    pub(super) fn vector_path_row(&mut self, ui: &mut Ui, theme: &Theme) {
        let others = self.toolbox.current().referred() != crate::fill::Refer::Editing;
        ui.add_enabled_ui(others, |ui| {
            let response = checkbox(
                "sue-paint-fill-vector-path",
                &tr!("fill-up-to-vector-path"),
                self.fill_vector_path,
            )
            .show(ui, theme);
            if response.clicked() {
                self.fill_vector_path = !self.fill_vector_path;
            }
            let hint = if others {
                tr!("fill-up-to-vector-path-hint")
            } else {
                tr!("fill-up-to-vector-path-off-hint")
            };
            tooltip(
                response,
                &Tip::new(&tr!("fill-up-to-vector-path")).help(&hint),
                theme,
            );
        });
    }

    /// Vector magnet, for the pen, pencil, brush and Figure tools: a line
    /// laid on a vector layer joins its ends to the ends of lines near
    /// them.
    pub(super) fn magnet_row(&mut self, ui: &mut Ui, theme: &Theme) {
        let response = checkbox(
            "sue-paint-vector-magnet",
            &tr!("vector-magnet"),
            self.vector_magnet,
        )
        .show(ui, theme);
        if response.clicked() {
            self.vector_magnet = !self.vector_magnet;
        }
        tooltip(
            response,
            &Tip::new(&tr!("vector-magnet")).help(&tr!("vector-magnet-hint")),
            theme,
        );
    }
}
