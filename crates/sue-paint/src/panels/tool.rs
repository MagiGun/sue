//! The Tool palette: a button for every entry of the toolbox, in its
//! order, then the main and sub colors as overlapping circles with their
//! swap, and the transparent color.
//!
//! A button is dragged to another place to reorder the palette; a sub
//! tool dragged from the Sub Tool palette and let go between two buttons
//! becomes a tool of its own there, or let go on a button joins that
//! tool. The arrangement is kept in the data folder as a tool layout
//! (`tools.suetools`, `docs/tool-layout-format.md`), so it comes back
//! next time and can be exported, shared and imported.

use egui::{Color32, Id, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use sue_wm::icons::Icon as WmIcon;
use sue_wm::ui::{Density, Radius, Space};
use sue_wm::{AppletContext, Theme};

use crate::color;
use crate::panels::Flow;
use crate::panels::tool_property::is_liquify_mode;
use crate::studio::SuePaint;
use crate::tools::layout::ToolLayout;
use crate::tools::{Tool, ToolSlot};

/// The file in Sue Paint's data folder that keeps the tool palette's
/// arrangement between runs.
pub(crate) const TOOLS_FILE: &str = "tools.suetools";

/// The Tool palette's own menu (Clip Studio Paint's, without what Sue
/// Paint does by dragging: Change Order is a drag of a button).
pub(crate) const TOOL_MENU: &[&str] = &[
    "tool_palette.settings",
    "tool_palette.delete",
    "tool_palette.restore",
    "tool_palette.reset",
    "tool_palette.import",
    "tool_palette.export",
    "tool_palette.color_icon",
];

/// The Sub Tool palette's own menu.
pub(crate) const SUB_TOOL_MENU: &[&str] = &[
    "sub_tool.duplicate",
    "sub_tool.rename",
    "sub_tool.delete",
    "sub_tool.reset",
    "sub_tool.new_group",
    "sub_tool.rename_group",
    "sub_tool.import",
    "sub_tool.detail",
];

/// Tool Property's and Sub Tool Detail's own menu.
pub(crate) const PROPERTY_MENU: &[&str] = &[
    "sub_tool.detail",
    "sub_tool.lock",
    "sub_tool.reset",
    "sub_tool.save_default",
    "sub_tool.categories",
];

/// What is being dragged between the tool palettes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolDrag {
    /// Tool palette entry `.0`.
    Slot(usize),
    /// Sub tool `index` of tool palette entry `slot`.
    Sub { slot: usize, index: usize },
}

/// `true` if tool palette entry `slot` is Liquify: Blend sub tools that
/// all liquify.
pub(crate) fn slot_is_liquify(slot: &ToolSlot) -> bool {
    slot.kind == Tool::Blend && slot.subs().iter().all(|sub| is_liquify_mode(sub.mode))
}

/// The icon of tool palette entry `slot` in Sue's icon set, which icon
/// packs replace by name.
pub(crate) fn slot_wm_icon(slot: &ToolSlot) -> WmIcon {
    if slot_is_liquify(slot) {
        return WmIcon::ToolLiquify;
    }
    match slot.kind {
        Tool::Zoom => WmIcon::ToolZoom,
        Tool::Hand => WmIcon::ToolHand,
        Tool::MoveLayer => WmIcon::ToolMoveLayer,
        Tool::Select => WmIcon::ToolSelect,
        Tool::AutoSelect => WmIcon::ToolAutoSelect,
        Tool::Eyedropper => WmIcon::ToolEyedropper,
        Tool::Pen => WmIcon::ToolPen,
        Tool::Pencil => WmIcon::ToolPencil,
        Tool::Brush => WmIcon::ToolBrush,
        Tool::Airbrush => WmIcon::ToolAirbrush,
        Tool::Decoration => WmIcon::ToolDecoration,
        Tool::Eraser => WmIcon::ToolEraser,
        Tool::Blend => WmIcon::ToolBlend,
        Tool::Fill => WmIcon::ToolFill,
        Tool::Gradient => WmIcon::ToolGradient,
        Tool::Figure => WmIcon::ToolFigure,
        Tool::Text => WmIcon::ToolText,
        Tool::Balloon => WmIcon::ToolBalloon,
        Tool::Frame => WmIcon::ToolFrame,
        Tool::Ruler => WmIcon::ToolRuler,
        Tool::CorrectLine => WmIcon::ToolCorrectLine,
        Tool::Operation => WmIcon::ToolOperation,
    }
}

/// What a tool palette button says when the pointer rests on it: its
/// name, and its key when it has one (`Pen (P)`).
pub(crate) fn slot_tip(slot: &ToolSlot) -> String {
    let name = crate::localize(&slot.name);
    match slot.key {
        Some(key) => format!("{name} ({key})"),
        None => name,
    }
}

/// Where a drag over the tool palette would land: onto button `.0`
/// (`true`), or before it (`false`). Past the last button it lands
/// before `buttons.len()`.
fn landing(buttons: &[(usize, Rect)], pointer: Pos2, horizontal: bool) -> Option<(usize, bool)> {
    let along = |at: Pos2| if horizontal { at.x } else { at.y };
    let across = |rect: Rect| {
        if horizontal {
            rect.x_range()
        } else {
            rect.y_range()
        }
    };
    for (index, rect) in buttons {
        if rect.contains(pointer) {
            let range = across(*rect);
            let t = (along(pointer) - range.min) / range.span().max(1.0);
            return Some(match t {
                t if t < 0.25 => (*index, false),
                t if t > 0.75 => (*index + 1, false),
                _ => (*index, true),
            });
        }
    }
    let last = buttons.last()?;
    let nearest = buttons
        .iter()
        .min_by(|a, b| {
            a.1.center()
                .distance(pointer)
                .total_cmp(&b.1.center().distance(pointer))
        })
        .copied()
        .unwrap_or(*last);
    let after = along(pointer) > along(nearest.1.center());
    Some((nearest.0 + usize::from(after), false))
}

impl SuePaint {
    /// Clip Studio Paint's tool palette: a button for each entry of the
    /// toolbox in its order (lit while it is out; a click takes it out),
    /// then the main and sub colors as overlapping circles with their
    /// swap, and the transparent color.
    pub(crate) fn tool_strip(
        &mut self,
        ui: &mut Ui,
        strip: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        ui.painter()
            .rect_filled(strip, egui::CornerRadius::ZERO, theme.panel);
        ui.painter()
            .vline(strip.right(), strip.y_range(), theme.hairline());
        let count = self.toolbox.slots().len();
        let current = self.toolbox.current_slot();
        // Where each button goes: in a line along a row (the top or the
        // bottom), in one column, or in several once people widen the
        // palette's column, as Clip Studio Paint's tool palette does.
        let horizontal = strip.width() > strip.height() * 1.3;
        let show_colors = cx
            .rice()
            .get_bool(crate::panels::TOOL_COLOR_ICON)
            .unwrap_or(true);
        let colors = show_colors
            && if horizontal {
                strip.width() >= theme.space(Space::S9) * 5.5
                    && strip.height() >= theme.space(Space::S8) * 2.0
            } else {
                strip.height() >= theme.space(Space::S9) * 5.0
            };
        let cell = Density::Compact.icon_button() + theme.space(Space::S1) * 3.0;
        let gap = theme.space(Space::S1);
        let mut places: Vec<(usize, Rect)> = Vec::with_capacity(count);
        if horizontal {
            let reserve = if colors {
                theme.space(Space::S9) * 1.5
            } else {
                theme.space(Space::S7)
            };
            let mut flow = Flow::new(
                strip.shrink2(egui::Vec2::Y * theme.space(Space::S2)),
                theme.space(Space::S6),
                reserve,
                cell + gap,
            );
            for index in 0..count {
                if let Some(rect) = flow.next(vec2(cell, cell), cell + gap) {
                    places.push((index, rect));
                }
            }
        } else {
            let colors_room = if colors {
                theme.space(Space::S9) * 2.4
            } else {
                theme.space(Space::S4)
            };
            let top = strip.top() + theme.space(Space::S3);
            let bottom = strip.bottom() - colors_room;
            let columns = ((strip.width() - theme.space(Space::S4)) / (cell + gap))
                .floor()
                .max(1.0) as usize;
            // One column shrinks its buttons a little before any is cut
            // off; several keep them square.
            let step = if columns == 1 {
                ((bottom - top) / count.max(1) as f32).clamp(cell * 0.7, cell + gap)
            } else {
                cell + gap
            };
            let width = cell.min(strip.width() - theme.space(Space::S2));
            let left = strip.center().x - (columns as f32 * (width + gap) - gap) / 2.0;
            for index in 0..count {
                let (row, column) = (index / columns, index % columns);
                let y = top + row as f32 * step;
                if y + step - gap > bottom {
                    break;
                }
                places.push((
                    index,
                    Rect::from_min_size(
                        pos2(left + column as f32 * (width + gap), y),
                        vec2(width, step - gap),
                    ),
                ));
            }
        }
        let dragging = egui::DragAndDrop::payload::<ToolDrag>(ui.ctx()).map(|drag| *drag);
        for (index, rect) in &places {
            let slot = &self.toolbox.slots()[*index];
            let tip = slot_tip(slot);
            // Under the button, a sense of drags only: the button above
            // takes the clicks (egui finds the topmost of each kind).
            let grip = ui.interact(
                *rect,
                Id::unique(("sue-paint-tool-drag", *index)),
                Sense::drag(),
            );
            let response =
                sue_wm::ui::icon_button(("sue-paint-tool", *index), slot_wm_icon(slot), &tip)
                    .selected(*index == current)
                    .quiet()
                    .show_at(ui, *rect, theme);
            if response.clicked() {
                self.toolbox.pick_slot(*index);
            }
            if grip.drag_started() {
                egui::DragAndDrop::set_payload(ui.ctx(), ToolDrag::Slot(*index));
            }
        }
        // A drag over the palette: where it would land, and the landing.
        let pointer = ui.ctx().pointer_latest_pos();
        if let (Some(drag), Some(pointer)) = (dragging, pointer)
            && strip.contains(pointer)
            && let Some((at, onto)) = landing(&places, pointer, horizontal)
        {
            let released = ui.ctx().input(|input| input.pointer.any_released());
            let mark = theme.focus_stroke();
            let onto = onto && matches!(drag, ToolDrag::Sub { .. });
            if onto {
                if let Some((_, rect)) = places.iter().find(|(index, _)| *index == at) {
                    ui.painter().rect_stroke(
                        *rect,
                        theme.radius(Radius::Md),
                        mark,
                        StrokeKind::Outside,
                    );
                }
            } else {
                let edge = places
                    .iter()
                    .find(|(index, _)| *index == at)
                    .map(|(_, rect)| if horizontal { rect.left() } else { rect.top() })
                    .or_else(|| {
                        places.last().map(|(_, rect)| {
                            if horizontal {
                                rect.right()
                            } else {
                                rect.bottom()
                            }
                        })
                    });
                if let Some(edge) = edge {
                    let painter = ui.painter();
                    if horizontal {
                        painter.vline(edge - gap / 2.0, strip.shrink(gap * 2.0).y_range(), mark);
                    } else {
                        painter.hline(strip.shrink(gap * 2.0).x_range(), edge - gap / 2.0, mark);
                    }
                }
            }
            if released {
                egui::DragAndDrop::clear_payload(ui.ctx());
                match drag {
                    ToolDrag::Slot(from) => {
                        let to = if at > from { at - 1 } else { at };
                        self.toolbox.move_slot(from, to);
                    }
                    ToolDrag::Sub { slot, index } if onto => {
                        let group = self.toolbox.slots()[at].groups().len().saturating_sub(1);
                        self.toolbox.move_sub(slot, index, at, group, usize::MAX);
                    }
                    ToolDrag::Sub { slot, index } => {
                        if let Some(made) = self.toolbox.promote_sub(slot, index, at) {
                            self.toolbox.pick_slot(made);
                        }
                    }
                }
            }
        }
        // What is being dragged follows the pointer.
        if let (Some(drag), Some(pointer)) = (dragging, pointer) {
            self.drag_ghost(ui, drag, pointer, theme);
        }
        if colors && horizontal {
            self.color_circles(
                ui,
                pos2(
                    strip.right() - theme.space(Space::S9) * 1.25,
                    strip.center().y - theme.space(Space::S5),
                ),
                theme,
            );
        } else if colors {
            self.color_circles(
                ui,
                pos2(
                    strip.center().x - theme.space(Space::S3),
                    strip.bottom() - theme.space(Space::S9) * 1.7,
                ),
                theme,
            );
        }
    }

    /// What a drag between the tool palettes carries, drawn at the pointer
    /// over everything: the tool's icon, or the sub tool's name.
    fn drag_ghost(&self, ui: &Ui, drag: ToolDrag, pointer: Pos2, theme: &Theme) {
        let painter = ui.ctx().layer_painter(egui::LayerId::new(
            egui::Order::Tooltip,
            Id::unique("sue-paint-tool-drag-ghost"),
        ));
        let pad = theme.space(Space::S2);
        let (icon, text) = match drag {
            ToolDrag::Slot(index) => match self.toolbox.slots().get(index) {
                Some(slot) => (slot_wm_icon(slot), crate::localize(&slot.name)),
                None => return,
            },
            ToolDrag::Sub { slot, index } => {
                match self.toolbox.slots().get(slot).and_then(|entry| {
                    entry
                        .subs()
                        .get(index)
                        .map(|sub| (slot_wm_icon(entry), sub))
                }) {
                    Some((icon, sub)) => (icon, crate::localize(&sub.name)),
                    None => return,
                }
            }
        };
        let font = theme.text_font(sue_wm::ui::TextRole::Caption);
        let galley = painter.layout_no_wrap(text, font, theme.text);
        let side = Density::Compact.icon();
        // Around the icon and the name: `pad` inside, a wider step from the
        // pointer, so the hand does not hide it.
        let (wide, double) = (theme.space(Space::S5), theme.space(Space::S4));
        let size = vec2(
            side + wide + galley.size().x,
            side.max(galley.size().y) + double,
        );
        let rect = Rect::from_min_size(pointer + egui::Vec2::splat(wide), size);
        painter.rect_filled(rect, theme.radius(Radius::Md), theme.surface(3));
        painter.rect_stroke(
            rect,
            theme.radius(Radius::Md),
            theme.hairline(),
            StrokeKind::Inside,
        );
        sue_wm::icons::paint(
            &painter,
            Rect::from_min_size(rect.min + vec2(pad, pad), egui::Vec2::splat(side)),
            icon,
            theme.text,
        );
        painter.galley(
            pos2(
                rect.left() + side + double,
                rect.center().y - galley.size().y / 2.0,
            ),
            galley,
            theme.text,
        );
    }

    /// Reads the tool palette's kept arrangement once, then writes it
    /// again whenever it changed (Settings, Remember Palette Layout).
    pub(crate) fn keep_tools(&mut self, cx: &mut AppletContext<'_>) {
        let remember = cx
            .rice()
            .get_bool(crate::extras::REMEMBER_LAYOUT)
            .unwrap_or(true);
        let Some(folder) = crate::colorsets::folder(cx).filter(|_| remember) else {
            return;
        };
        let file = folder.join(TOOLS_FILE);
        if !self.tool_palettes.loaded {
            self.tool_palettes.loaded = true;
            if let Ok(bytes) = sue_platform::fs::read(&file) {
                let (layout, _) = ToolLayout::parse(&String::from_utf8_lossy(&bytes));
                if !layout.tools.is_empty() {
                    // What it cannot follow (a sub tool made last time and
                    // not kept) is left where Sue Paint puts it.
                    let _ = self.toolbox.arrange(&layout);
                }
            }
            self.tool_palettes.kept = Some(self.toolbox.layout());
            return;
        }
        let now = self.toolbox.layout();
        if self.tool_palettes.kept.as_ref() == Some(&now) {
            return;
        }
        let written = sue_platform::fs::create_dir_all(&folder)
            .and_then(|()| sue_platform::fs::write(&file, now.to_text()));
        if let Err(error) = written {
            cx.fail(tr!("cannot-keep-the-palettes-settings", error = error));
        }
        self.tool_palettes.kept = Some(now);
    }

    /// The main and sub colors as Clip Studio Paint shows them, in the tool
    /// strip and under the color wheel alike: overlapping circles with the
    /// main one at `main_at` and the sub behind it, the swap arrow, and the
    /// transparent color as a checkered pill below. A click picks which
    /// one brushes use.
    pub(super) fn color_circles(&mut self, ui: &mut Ui, main_at: Pos2, theme: &Theme) {
        let radius = theme.space(Space::S5);
        let diameter = theme.space(Space::S7);
        let sub_at = main_at + egui::Vec2::splat(radius);
        let disc = |ui: &mut Ui, at: Pos2, rgb: [u8; 3], chosen: bool, tip: &str| {
            let rect = Rect::from_center_size(at, egui::Vec2::splat(diameter));
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
                if chosen {
                    theme.selected_stroke()
                } else {
                    Stroke::new(theme.hairline().width, theme.border)
                },
            );
            response.widget_info(|| {
                egui::WidgetInfo::selected(egui::Role::RadioButton, true, chosen, tip)
            });
            sue_wm::ui::focus_ring(ui, &response, radius, theme);
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
        let swap_side = theme.space(Space::S5) + theme.space(Space::S1);
        let swap = Rect::from_min_size(
            pos2(
                main_at.x + radius + theme.space(Space::S1) / 2.0,
                main_at.y - radius - swap_side * 0.85,
            ),
            egui::Vec2::splat(swap_side),
        );
        if sue_wm::ui::icon_button(
            ("sue-paint-color-swap", main_at.x as i32, main_at.y as i32),
            WmIcon::ColorSwap,
            &tr!("swap-main-and-sub-color-values"),
        )
        .quiet()
        .show_at(ui, swap, theme)
        .clicked()
        {
            std::mem::swap(&mut self.main, &mut self.sub);
            self.hex = color::hex(self.drawing_color());
        }
        // The transparent color: a checker; brushes erase with it on.
        // Under the circles, as wide as both together: its left edge a
        // little in from the main circle's, its top below the sub circle.
        const PILL_INSET: f32 = 0.85;
        const PILL_DROP: f32 = 3.2;
        const PILL_WIDTH: f32 = 2.7;
        let clear = Rect::from_min_size(
            main_at + vec2(-radius * PILL_INSET, radius * PILL_DROP),
            vec2(radius * PILL_WIDTH, swap_side),
        );
        let response = ui
            .interact(
                clear,
                Id::unique(("sue-paint-transparent", main_at.x as i32, main_at.y as i32)),
                Sense::click(),
            )
            .on_hover_text(tr!("transparent-color"));
        let transparent = self.transparent;
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::Role::CheckBox,
                true,
                transparent,
                tr!("transparent-color"),
            )
        });
        let round = clear.height() / 2.0;
        sue_wm::checker_fill(ui.painter(), clear, round, theme.space(Space::S1) * 1.75);
        ui.painter().rect_stroke(
            clear,
            round,
            if self.transparent {
                theme.selected_stroke()
            } else {
                Stroke::new(theme.hairline().width, theme.border)
            },
            StrokeKind::Outside,
        );
        sue_wm::ui::focus_ring(ui, &response, round, theme);
        if response.clicked() {
            self.transparent = !self.transparent;
        }
        self.color_circles_double_click(ui, main_at);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn column(count: usize) -> Vec<(usize, Rect)> {
        (0..count)
            .map(|index| {
                (
                    index,
                    Rect::from_min_size(pos2(0.0, index as f32 * 30.0), vec2(30.0, 28.0)),
                )
            })
            .collect()
    }

    #[test]
    fn a_drag_lands_before_on_or_after_a_button() {
        let buttons = column(4);
        // The top quarter of a button: before it; its middle: onto it;
        // its bottom quarter: after it.
        assert_eq!(landing(&buttons, pos2(15.0, 31.0), false), Some((1, false)));
        assert_eq!(landing(&buttons, pos2(15.0, 44.0), false), Some((1, true)));
        assert_eq!(landing(&buttons, pos2(15.0, 57.0), false), Some((2, false)));
        // Past the last button: at the end.
        assert_eq!(
            landing(&buttons, pos2(15.0, 400.0), false),
            Some((4, false))
        );
        assert_eq!(landing(&[], pos2(0.0, 0.0), false), None);
    }

    #[test]
    fn every_slot_has_an_icon_of_the_set_and_a_tip() {
        let toolbox = crate::tools::Toolbox::default();
        for slot in toolbox.slots() {
            let icon = slot_wm_icon(slot);
            assert!(sue_wm::icons::Icon::from_name(icon.name()).is_some());
            assert!(!slot_tip(slot).is_empty());
        }
        let liquify = toolbox
            .slots()
            .iter()
            .find(|slot| slot.name == "Liquify")
            .expect("Liquify is a tool");
        assert_eq!(slot_wm_icon(liquify), WmIcon::ToolLiquify);
    }
}
