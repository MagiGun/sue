//! The tool strip, its color circles and the icon of each tool.

use egui::{Color32, Id, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use sue_wm::{AppletContext, Theme};

use crate::color;
use crate::icons::Icon;
use crate::panels::tool_property::is_liquify_mode;
use crate::panels::{Flow, icon_button};
use crate::studio::SuePaint;
use crate::tools::Tool;

/// The icon of a tool.
pub(crate) fn tool_icon(tool: Tool) -> Icon {
    match tool {
        Tool::Zoom => Icon::Zoom,
        Tool::Hand => Icon::Hand,
        Tool::MoveLayer => Icon::MoveLayer,
        Tool::Select => Icon::Select,
        Tool::AutoSelect => Icon::AutoSelect,
        Tool::Eyedropper => Icon::Eyedropper,
        Tool::Pen => Icon::Pen,
        Tool::Pencil => Icon::Pencil,
        Tool::Brush => Icon::Brush,
        Tool::Airbrush => Icon::Airbrush,
        Tool::Decoration => Icon::Decoration,
        Tool::Eraser => Icon::Eraser,
        Tool::Blend => Icon::Blend,
        Tool::Fill => Icon::Fill,
        Tool::Gradient => Icon::Gradient,
        Tool::Figure => Icon::Figure,
        Tool::Text => Icon::Text,
        Tool::Balloon => Icon::Balloon,
        Tool::Frame => Icon::Frame,
        Tool::Ruler => Icon::Ruler,
        Tool::CorrectLine => Icon::CorrectLine,
        Tool::Operation => Icon::Operation,
    }
}

impl SuePaint {
    /// Clip Studio Paint's tool palette: its tools in its order (with
    /// Frame border and Balloon as Comic, and Hand, Zoom and Move layer as
    /// Move; clicking one again goes to the next), then the main and sub
    /// colors as overlapping circles with their swap, and the transparent
    /// color.
    pub(crate) fn tool_strip(
        &mut self,
        ui: &mut Ui,
        strip: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let _ = cx;
        ui.painter().rect_filled(strip, 0.0, theme.panel);
        ui.painter()
            .vline(strip.right(), strip.y_range(), theme.hairline());
        // (icon, name, tools), and None between groups.
        let entries: [Option<(Icon, String, &[Tool])>; 22] = [
            Some((Icon::Pen, tr!("pen"), &[Tool::Pen])),
            Some((Icon::Pencil, tr!("sketch"), &[Tool::Pencil])),
            Some((Icon::Brush, tr!("brush"), &[Tool::Brush])),
            Some((Icon::Eraser, tr!("eraser"), &[Tool::Eraser])),
            Some((Icon::Airbrush, tr!("airbrush"), &[Tool::Airbrush])),
            Some((Icon::Decoration, tr!("decoration"), &[Tool::Decoration])),
            Some((Icon::Blend, tr!("blend"), &[Tool::Blend])),
            Some((Icon::Replace, tr!("liquify"), &[Tool::Blend])),
            None,
            Some((Icon::Select, tr!("selection-area"), &[Tool::Select])),
            Some((Icon::AutoSelect, tr!("auto-select"), &[Tool::AutoSelect])),
            Some((Icon::Fill, tr!("fill"), &[Tool::Fill])),
            Some((Icon::Gradient, tr!("gradient"), &[Tool::Gradient])),
            None,
            Some((Icon::Operation, tr!("operation"), &[Tool::Operation])),
            Some((Icon::Figure, tr!("figure"), &[Tool::Figure])),
            Some((Icon::Text, tr!("text"), &[Tool::Text])),
            Some((Icon::Frame, tr!("comic"), &[Tool::Frame, Tool::Balloon])),
            Some((Icon::Ruler, tr!("ruler"), &[Tool::Ruler])),
            Some((Icon::CorrectLine, tr!("correct-line"), &[Tool::CorrectLine])),
            Some((
                Icon::Hand,
                tr!("move"),
                &[Tool::Hand, Tool::Zoom, Tool::MoveLayer],
            )),
            Some((Icon::Eyedropper, tr!("eyedropper"), &[Tool::Eyedropper])),
        ];
        let current = self.toolbox.tool();
        // Liquify is Blend's liquify sub tools; Blend the others.
        let liquify = current == Tool::Blend && is_liquify_mode(self.toolbox.of(Tool::Blend).mode);
        // Where each tool's button goes: in a line along a row (the top or
        // the bottom), in one column, or in several once people widen the
        // palette's column, as Clip Studio Paint's tool palette does.
        let horizontal = strip.width() > strip.height() * 1.3;
        let colors = if horizontal {
            strip.width() >= 220.0 && strip.height() >= 64.0
        } else {
            strip.height() >= 200.0
        };
        let columns = (((strip.width() - 8.0) / 36.0).floor() as usize).max(1);
        let mut places: Vec<Option<Rect>> = Vec::with_capacity(entries.len());
        if horizontal {
            let reserve = if colors { 62.0 } else { 26.0 };
            let mut flow = Flow::new(strip.shrink2(vec2(0.0, 4.0)), 16.0, reserve, 32.0);
            for entry in &entries {
                if entry.is_some() {
                    places.push(flow.next(vec2(30.0, 30.0), 32.0));
                } else {
                    if let Some(x) = flow.gap(8.0) {
                        ui.painter().vline(
                            x + 3.0,
                            (flow.y + 3.0)..=(flow.y + 27.0),
                            theme.hairline(),
                        );
                    }
                    places.push(None);
                }
            }
        } else {
            let bottom = strip.bottom() - if colors { 96.0 } else { 8.0 };
            let line = |y: f32| {
                ui.painter().hline(
                    strip.shrink2(vec2(8.0, 0.0)).x_range(),
                    y + 3.0,
                    theme.hairline(),
                );
            };
            let mut y = strip.top() + 6.0;
            if columns == 1 {
                let room = bottom - y;
                let buttons = entries.iter().flatten().count() as f32;
                let gaps = (entries.len() as f32 - buttons) * 8.0;
                let step = ((room - gaps) / buttons).clamp(20.0, 34.0);
                let width = (strip.width() - 8.0).min(38.0);
                let left = strip.center().x - width / 2.0;
                for entry in &entries {
                    if entry.is_none() {
                        line(y);
                        y += 8.0;
                        places.push(None);
                    } else if y + step > bottom {
                        places.push(None);
                    } else {
                        places.push(Some(Rect::from_min_size(
                            pos2(left, y),
                            vec2(width, step - 2.0),
                        )));
                        y += step;
                    }
                }
            } else {
                let cell = ((strip.width() - 8.0) / columns as f32).min(42.0);
                let left = strip.center().x - cell * columns as f32 / 2.0;
                let mut column = 0;
                for entry in &entries {
                    if entry.is_none() {
                        if column > 0 {
                            column = 0;
                            y += 32.0;
                        }
                        line(y);
                        y += 8.0;
                        places.push(None);
                        continue;
                    }
                    if y + 30.0 > bottom {
                        places.push(None);
                        continue;
                    }
                    places.push(Some(Rect::from_min_size(
                        pos2(left + column as f32 * cell + 1.0, y),
                        vec2(cell - 2.0, 30.0),
                    )));
                    column += 1;
                    if column == columns {
                        column = 0;
                        y += 32.0;
                    }
                }
            }
        }
        for (entry, place) in entries.iter().zip(places) {
            let (Some((icon, name, tools)), Some(rect)) = (entry, place) else {
                continue;
            };
            let is_liquify = *icon == Icon::Replace;
            let active = match tools.first() {
                Some(Tool::Blend) => liquify == is_liquify && current == Tool::Blend,
                Some(_) => tools.contains(&current),
                None => false,
            };
            let key = tools.first().and_then(|tool| tool.key());
            let tip = match key {
                Some(key) => format!("{name} ({key})"),
                None => name.clone(),
            };
            if !icon_button(ui, rect, *icon, true, active, &tip, theme) {
                continue;
            }
            match tools {
                // Liquify is a tool of its own in the toolbox: the entry
                // holding the Liquify sub tool.
                [Tool::Blend] if is_liquify => {
                    let liquify = self.toolbox.slot_holding(|sub| {
                        sub.tool == Tool::Blend && crate::blend::is_liquify(sub.mode)
                    });
                    match liquify {
                        Some(slot) => self.toolbox.pick_slot(slot),
                        None => self.toolbox.pick(Tool::Blend),
                    }
                }
                [Tool::Blend] => self.toolbox.pick(Tool::Blend),
                // A group: the next of its tools when one is already out.
                group => {
                    let next = group
                        .iter()
                        .position(|tool| *tool == current)
                        .map_or(group[0], |at| group[(at + 1) % group.len()]);
                    self.toolbox.pick(next);
                }
            }
        }
        if colors && horizontal {
            self.color_circles(
                ui,
                pos2(strip.right() - 50.0, strip.center().y - 12.0),
                theme,
            );
        } else if colors {
            self.color_circles(
                ui,
                pos2(strip.center().x - 6.0, strip.bottom() - 68.0),
                theme,
            );
        }
    }

    /// The main and sub colors as Clip Studio Paint shows them, in the tool
    /// strip and under the color wheel alike: overlapping circles with the
    /// main one at `main_at` and the sub behind it, the swap arrow, and the
    /// transparent color as a checkered pill below. A click picks which
    /// one brushes use.
    pub(super) fn color_circles(&mut self, ui: &mut Ui, main_at: Pos2, theme: &Theme) {
        let sub_at = main_at + vec2(12.0, 12.0);
        let radius = 12.0;
        let disc = |ui: &mut Ui, at: Pos2, rgb: [u8; 3], chosen: bool, tip: &str| {
            let rect = Rect::from_center_size(at, egui::Vec2::splat(radius * 2.0));
            let response = ui.interact(
                rect,
                Id::unique((
                    "sue-paint-color-circle",
                    tip,
                    main_at.x as i32,
                    main_at.y as i32,
                )),
                Sense::click(),
            );
            let [r, g, b] = rgb;
            ui.painter()
                .circle_filled(at, radius, Color32::from_rgb(r, g, b));
            ui.painter().circle_stroke(
                at,
                radius,
                Stroke::new(
                    if chosen { 2.0 } else { 1.0 },
                    if chosen {
                        theme.border_active
                    } else {
                        theme.border
                    },
                ),
            );
            response.on_hover_text(tip).clicked()
        };
        if disc(
            ui,
            sub_at,
            self.sub,
            self.sub_active && !self.transparent,
            &tr!("sub-color"),
        ) {
            self.sub_active = true;
            self.transparent = false;
            self.hex = color::hex(self.sub);
        }
        if disc(
            ui,
            main_at,
            self.main,
            !self.sub_active && !self.transparent,
            &tr!("main-color"),
        ) {
            self.sub_active = false;
            self.transparent = false;
            self.hex = color::hex(self.main);
        }
        let swap = Rect::from_min_size(pos2(main_at.x + 13.0, main_at.y - 22.0), vec2(14.0, 14.0));
        if icon_button(
            ui,
            swap,
            Icon::Swap,
            true,
            false,
            &tr!("swap-main-sub-color-x"),
            theme,
        ) {
            std::mem::swap(&mut self.main, &mut self.sub);
            self.hex = color::hex(self.drawing_color());
        }
        // The transparent color: a checker; brushes erase with it on.
        let clear = Rect::from_min_size(main_at + vec2(-10.0, 38.0), vec2(32.0, 14.0));
        let response = ui
            .interact(
                clear,
                Id::unique(("sue-paint-transparent", main_at.x as i32, main_at.y as i32)),
                Sense::click(),
            )
            .on_hover_text(tr!("transparent-color"));
        sue_wm::checker_fill(ui.painter(), clear, clear.height() / 2.0, 3.5);
        ui.painter().rect_stroke(
            clear,
            clear.height() / 2.0,
            Stroke::new(
                if self.transparent { 2.0 } else { 1.0 },
                if self.transparent {
                    theme.border_active
                } else {
                    theme.border
                },
            ),
            StrokeKind::Outside,
        );
        if response.clicked() {
            self.transparent = !self.transparent;
        }
    }
}
