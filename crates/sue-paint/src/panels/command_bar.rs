//! The command bar, and what it shares with Quick Access: how a button of
//! either is drawn, the right-click menu, moving buttons by a drag and
//! dropping new ones on, and the popovers a button opens (a palette that
//! pops up, a group's buttons, the icons to give it).
//!
//! Every button is drawn from the design system's tokens with an icon of
//! the shared set (by its stable name, so icon packs replace it), a focus
//! ring, and a tooltip with its name and the key that runs it.

use egui::{CursorIcon, Id, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2, pos2, vec2};
use sue_wm::icons::{self, Icon, IconStyle};
use sue_wm::ui::{Density, Radius, Space, TextRole, Tip};
use sue_wm::{AppletContext, MenuItem, Theme};

use crate::bar_items::{BarPayload, Holder, Item, Popover, Reorder, Slot, edit};
use crate::studio::SuePaint;

/// The right-click menu's commands: not commands of the studio, only of
/// the menu.
const MENU_ICON: &str = "bar.icon";
const MENU_DELETE: &str = "bar.delete";
const MENU_ADD_COLOR: &str = "bar.add_color";
const MENU_GROUP_COLOR: &str = "bar.group_color";
const MENU_SEPARATOR: &str = "bar.separator";
const MENU_DRAG: &str = "bar.order.drag";
const MENU_CTRL_DRAG: &str = "bar.order.ctrl_drag";
const MENU_SETTINGS: &str = "bar.settings";

/// How a button looks this frame.
#[derive(Clone, Debug)]
pub(crate) struct Look {
    /// What it is called.
    pub name: String,
    /// The key that runs it, as menus show it; empty for none.
    pub keys: String,
    /// It can be pressed.
    pub enabled: bool,
    /// It is on: the tool picked, the color in use, a check.
    pub on: bool,
}

/// The size of one button of `slot` on a bar `side` high: a square with
/// an icon or a swatch, a narrow line, or text for a command without an
/// icon.
fn bar_size(ui: &Ui, slot: &Slot, name: &str, side: f32, theme: &Theme) -> Vec2 {
    match face(slot).item {
        Item::Separator => vec2(theme.space(Space::S4), side),
        Item::Color(_) => Vec2::splat(side),
        _ if slot.shown_icon().is_some() => Vec2::splat(side),
        _ => {
            let galley = ui.painter().layout_no_wrap(
                name.to_owned(),
                theme.text_font(TextRole::Caption),
                theme.text,
            );
            vec2(galley.size().x + theme.space(Space::S5), side)
        }
    }
}

/// What a button shows on its face: a group shows its first member (the
/// one it runs), so a group of colors is a swatch with a list.
fn face(slot: &Slot) -> &Slot {
    match &slot.item {
        Item::Group(members) => members.first().unwrap_or(slot),
        _ => slot,
    }
}

/// Draws the button `slot` at `rect`, its name beside its icon when
/// `named` (Quick Access) and alone otherwise (the command bar). It senses
/// clicks, right clicks and drags.
pub(crate) fn draw_slot(
    ui: &Ui,
    rect: Rect,
    id: Id,
    slot: &Slot,
    look: &Look,
    named: bool,
    theme: &Theme,
) -> Response {
    let response = ui.interact(rect, id, Sense::click_and_drag());
    let painter = ui.painter();
    let radius = theme.radius(Radius::Sm);
    let surface = theme.panel;
    if slot.item != Item::Separator {
        let fill = if response.is_pointer_button_down_on() && look.enabled {
            Some(theme.fill_pressed())
        } else if look.on {
            Some(theme.fill_selected())
        } else if response.hovered() && look.enabled {
            Some(theme.fill_hover())
        } else {
            None
        };
        if let Some(fill) = fill {
            painter.rect_filled(rect, radius, fill);
        }
    }
    let ink = if look.enabled {
        theme.text
    } else {
        theme.text_disabled(surface)
    };
    let pad = theme.space(Space::S2);
    let density = Density::of(ui);
    let icon_side = density.icon().min(rect.height() - pad * 2.0);
    // Where the icon or swatch goes: the middle, or the left of a row.
    let mark = if named {
        Rect::from_center_size(
            pos2(rect.left() + pad + icon_side / 2.0, rect.center().y),
            Vec2::splat(icon_side),
        )
    } else {
        Rect::from_center_size(rect.center(), Vec2::splat(icon_side))
    };
    match &face(slot).item {
        Item::Separator => {
            if named {
                painter.hline(rect.x_range(), rect.center().y, theme.hairline());
            } else {
                let inset = theme.space(Space::S3);
                painter.vline(
                    rect.center().x,
                    (rect.top() + inset)..=(rect.bottom() - inset),
                    theme.hairline(),
                );
            }
        }
        Item::Color(rgb) => {
            let swatch = mark.shrink(theme.space(Space::S1));
            painter.rect_filled(
                swatch,
                theme.radius(Radius::Xs),
                egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]),
            );
            painter.rect_stroke(
                swatch,
                theme.radius(Radius::Xs),
                Stroke::new(theme.border_width, theme.border_strong()),
                egui::StrokeKind::Outside,
            );
        }
        _ => {
            let style = if look.on {
                IconStyle::Solid
            } else {
                IconStyle::Line
            };
            match slot.shown_icon() {
                Some(icon) => icons::paint_styled(painter, mark, icon, style, ink),
                None if !named => {
                    painter.text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        &look.name,
                        theme.text_font(TextRole::Caption),
                        ink,
                    );
                }
                None => {}
            }
        }
    }
    // A group and a popup say they open more with a small mark.
    if matches!(slot.item, Item::Group(_) | Item::Popup(_)) {
        let side = theme.space(Space::S3);
        let at = Rect::from_min_size(
            rect.right_bottom() - Vec2::splat(side + theme.space(Space::S1)),
            Vec2::splat(side),
        );
        icons::paint(painter, at, Icon::CaretDown, theme.text_muted());
    }
    if named && slot.item != Item::Separator {
        painter.text(
            pos2(mark.right() + theme.space(Space::S3), rect.center().y),
            egui::Align2::LEFT_CENTER,
            &look.name,
            theme.text_font(TextRole::Body),
            ink,
        );
    }
    sue_wm::ui::focus_ring(ui, &response, radius, theme);
    let (enabled, on, name) = (look.enabled, look.on, look.name.clone());
    response.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, enabled, on, &name));
    let tip = Tip::new(&look.name).shortcut(&look.keys);
    sue_wm::ui::tooltip(response, &tip, theme)
}

impl SuePaint {
    /// How the button `slot` looks now.
    pub(crate) fn look_of(&self, slot: &Slot, cx: &AppletContext<'_>) -> Look {
        let (enabled, on) = self.item_state(&slot.item, cx);
        Look {
            name: slot.item.label(),
            keys: self.item_keys(&slot.item, cx),
            enabled,
            on,
        }
    }

    /// The command bar: its buttons in lines that wrap when the bar is
    /// narrower than they are (in a column, or a taller top row), its
    /// right-click menu, its popovers, and buttons moved or dropped onto
    /// it by a drag.
    pub(crate) fn command_bar(
        &mut self,
        ui: &mut Ui,
        strip: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        ui.painter()
            .rect_filled(strip, egui::CornerRadius::ZERO, theme.panel);
        ui.painter()
            .hline(strip.x_range(), strip.bottom(), theme.hairline());
        let holder = Holder::CommandBar;
        let density = Density::of(ui);
        let side = density
            .icon_button()
            .min(strip.height() - theme.space(Space::S2));
        let gap = theme.space(Space::S1);
        // The palette's menu button sits at the right end, drawn by the
        // dock: the buttons stop before it.
        let area = Rect::from_min_max(
            strip.min + Vec2::splat(theme.space(Space::S1)),
            pos2(strip.right() - side - gap, strip.bottom()),
        );
        let background = ui.interact(strip, Id::unique("sue-paint-bar-ground"), Sense::click());
        let looks: Vec<Look> = self
            .bar
            .items
            .iter()
            .map(|slot| self.look_of(slot, cx))
            .collect();
        let mut at = area.min;
        let mut rects = Vec::with_capacity(looks.len());
        for (slot, look) in self.bar.items.iter().zip(&looks) {
            let size = bar_size(ui, slot, &look.name, side, theme);
            if at.x + size.x > area.right() && at.x > area.left() {
                at = pos2(area.left(), at.y + side + gap);
            }
            rects.push(Rect::from_min_size(at, size));
            at.x += size.x + gap;
        }
        let (clicked, menu) =
            self.bar_buttons(ui, holder, &rects, &looks, false, &background, theme);
        if let Some((index, caret)) = clicked {
            self.press_button(holder, index, rects[index], caret, cx);
        }
        self.bar_menu(ui, holder, &menu, cx, theme);
        self.bar_drops(ui, holder, area.expand(gap), &rects, cx, theme);
        self.bar_popovers(ui, holder, cx, theme);
    }

    /// Draws the buttons of `holder` at `rects` and hands back the one
    /// clicked and the response the right-click menu hangs on (the bar's
    /// own `background` and every button's together).
    #[allow(clippy::too_many_arguments, reason = "one button loop for both bars")]
    pub(crate) fn bar_buttons(
        &mut self,
        ui: &mut Ui,
        holder: Holder,
        rects: &[Rect],
        looks: &[Look],
        named: bool,
        background: &Response,
        theme: &Theme,
    ) -> (Option<(usize, bool)>, Response) {
        let mut clicked = None;
        let mut menu = background.clone();
        if background.secondary_clicked() {
            self.bar_ui.menu_target = Some((holder, None));
        }
        let ctrl = ui.input(|input| input.modifiers.command);
        let reorder = self.holder_reorder(holder);
        let items = self.holder_items(holder).to_vec();
        for (index, ((slot, rect), look)) in items.iter().zip(rects).zip(looks).enumerate() {
            let id = Id::unique(("sue-paint-bar-button", holder, index));
            let response = draw_slot(ui, *rect, id, slot, look, named, theme);
            if response.secondary_clicked() {
                self.bar_ui.menu_target = Some((holder, Some(index)));
            }
            if response.clicked() && look.enabled {
                // A group's list opens from the mark at its corner; the
                // rest of it runs the button it shows.
                let caret = response
                    .interact_pointer_pos()
                    .is_some_and(|at| at.x > rect.right() - caret_zone(*rect, named, theme))
                    && matches!(slot.item, Item::Group(_));
                clicked = Some((index, caret));
            }
            // A drag moves the button (with Ctrl, as Change Order says),
            // here or onto the other bar.
            if response.drag_started() && (reorder == Reorder::Drag || ctrl) {
                self.bar_ui.dragging = Some((holder, index));
                egui::DragAndDrop::set_payload(ui.ctx(), BarPayload(slot.clone()));
            }
            menu = menu.union(response);
        }
        (clicked, menu)
    }

    /// What a click on button `index` of `holder` does: runs it, or opens
    /// what it holds (a popup palette; a group's buttons, on its `caret`).
    pub(crate) fn press_button(
        &mut self,
        holder: Holder,
        index: usize,
        rect: Rect,
        caret: bool,
        cx: &mut AppletContext<'_>,
    ) {
        let Some(slot) = self.holder_items(holder).get(index).cloned() else {
            return;
        };
        // A palette already on screen is not drawn twice (its controls
        // would be two of one): it comes to the front of its tabs, where
        // the eye already is, and only a palette out of sight pops up.
        if let Item::Popup(palette) = slot.item
            && self.palette_drawn(palette)
        {
            self.bar_ui.open = None;
            self.show_palette(palette);
            return;
        }
        let opens = match slot.item {
            Item::Popup(_) => Some(Popover::Palette),
            Item::Group(_) if caret => Some(Popover::Group),
            _ => None,
        };
        match opens {
            Some(kind) => {
                let same = matches!(
                    self.bar_ui.open,
                    Some((open, at, place, _)) if open == kind && at == holder && place == index
                );
                self.bar_ui.open = (!same).then_some((kind, holder, index, rect));
            }
            None => self.run_item(&slot.item, cx),
        }
    }

    /// The right-click menu of a bar's button or of the bar itself: Icon
    /// Settings, Delete, Add Drawing Color, Insert Drawing Color into
    /// Group, Add Separator, Change Order and the bar's settings.
    pub(crate) fn bar_menu(
        &mut self,
        ui: &Ui,
        holder: Holder,
        response: &Response,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let target = match self.bar_ui.menu_target {
            Some((at, target)) if at == holder => target,
            _ => None,
        };
        let items = self.holder_items(holder);
        let on_button = target.filter(|index| *index < items.len());
        let groupable = on_button
            .and_then(|index| items.get(index))
            .is_some_and(|slot| matches!(slot.item, Item::Color(_) | Item::Group(_)));
        let line_beside = on_button.is_some_and(|index| {
            items
                .get(index)
                .is_some_and(|slot| slot.item == Item::Separator)
                || items
                    .get(index + 1)
                    .is_some_and(|slot| slot.item == Item::Separator)
        });
        let reorder = self.holder_reorder(holder);
        let settings = match holder {
            Holder::CommandBar => tr!("command-bar-settings-ellipsis"),
            Holder::QuickAccess(_) => tr!("quick-access-settings-ellipsis"),
        };
        let menu = vec![
            MenuItem::action(tr!("icon-settings-ellipsis"), MENU_ICON),
            MenuItem::action(tr!("delete"), MENU_DELETE),
            MenuItem::Separator,
            MenuItem::action(tr!("add-drawing-color"), MENU_ADD_COLOR),
            MenuItem::action(tr!("insert-drawing-color-into-group"), MENU_GROUP_COLOR),
            MenuItem::action(tr!("add-separator"), MENU_SEPARATOR),
            MenuItem::Separator,
            MenuItem::sub(
                tr!("change-order"),
                vec![
                    MenuItem::action(tr!("drag"), MENU_DRAG),
                    MenuItem::action(tr!("ctrl-drag"), MENU_CTRL_DRAG),
                ],
            ),
            MenuItem::Separator,
            MenuItem::action(settings, MENU_SETTINGS),
        ];
        let has_icon = on_button
            .and_then(|index| items.get(index))
            .is_some_and(|slot| !matches!(slot.item, Item::Separator | Item::Color(_)));
        let state = |command: &str| {
            let on = match command {
                MENU_ICON => has_icon,
                MENU_DELETE => on_button.is_some(),
                MENU_GROUP_COLOR => groupable,
                MENU_SEPARATOR => !line_beside,
                _ => true,
            };
            let checked = match command {
                MENU_DRAG => Some(reorder == Reorder::Drag),
                MENU_CTRL_DRAG => Some(reorder == Reorder::CtrlDrag),
                _ => None,
            };
            sue_wm::ItemState {
                enabled: on,
                checked,
            }
        };
        let Some(picked) = sue_wm::ui::context_menu(ui, response, &menu, &state, theme) else {
            return;
        };
        let rect = on_button
            .and_then(|index| self.bar_rect(ui, holder, index))
            .unwrap_or(response.rect);
        self.bar_ui.menu_target = None;
        let color = Slot::of(Item::Color(self.drawing_color()));
        let after = on_button.map(|index| index + 1);
        match picked.as_str() {
            MENU_ICON => {
                if let Some(index) = on_button {
                    self.bar_ui.icon_query.clear();
                    self.bar_ui.open = Some((Popover::Icon, holder, index, rect));
                }
                return;
            }
            MENU_DELETE => {
                if let (Some(index), Some(items)) = (on_button, self.holder_items_mut(holder)) {
                    edit::delete(items, index);
                }
                self.bar_ui.open = None;
            }
            MENU_ADD_COLOR => {
                if let Some(items) = self.holder_items_mut(holder) {
                    edit::insert(items, after, color);
                }
            }
            MENU_GROUP_COLOR => {
                if let (Some(index), Some(items)) = (on_button, self.holder_items_mut(holder)) {
                    edit::into_group(items, index, color);
                }
            }
            MENU_SEPARATOR => {
                if let Some(items) = self.holder_items_mut(holder) {
                    edit::insert(items, after, Slot::of(Item::Separator));
                }
            }
            MENU_DRAG => self.set_holder_reorder(holder, Reorder::Drag),
            MENU_CTRL_DRAG => self.set_holder_reorder(holder, Reorder::CtrlDrag),
            MENU_SETTINGS => {
                let target = match holder {
                    Holder::CommandBar => crate::dialogs::BarTarget::CommandBar,
                    Holder::QuickAccess(_) => crate::dialogs::BarTarget::QuickAccess,
                };
                self.dialog = Some(crate::dialogs::Dialog::CommandBar(
                    crate::dialogs::BarDialog::new(target),
                ));
                return;
            }
            _ => return,
        }
        self.save_bars(cx);
    }

    /// Where button `index` of `holder` was drawn last, as its response
    /// remembers it.
    fn bar_rect(&self, ui: &Ui, holder: Holder, index: usize) -> Option<Rect> {
        let id = Id::unique(("sue-paint-bar-button", holder, index));
        ui.ctx().read_response(id).map(|response| response.rect)
    }

    /// A button dragged over the bar `holder` (its buttons at `rects`, in
    /// `area`): a line shows where it goes, and letting go puts it there.
    /// A button of this bar moves; one from elsewhere is copied in.
    pub(crate) fn bar_drops(
        &mut self,
        ui: &Ui,
        holder: Holder,
        area: Rect,
        rects: &[Rect],
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let ctx = ui.ctx();
        if !egui::DragAndDrop::has_payload_of_type::<BarPayload>(ctx) {
            if self.bar_ui.dragging.is_some_and(|(at, _)| at == holder) {
                self.bar_ui.dragging = None;
            }
            return;
        }
        let Some(pointer) = ctx.pointer_latest_pos() else {
            return;
        };
        if !area.contains(pointer) {
            // Over neither bar, the pointer says nothing can go there.
            if !ctx.is_pointer_over_egui() || self.bar_ui.dragging.is_some() {
                ctx.set_cursor_icon(CursorIcon::NoDrop);
            }
            return;
        }
        ctx.set_cursor_icon(CursorIcon::Grabbing);
        let at = drop_place(rects, pointer);
        // Over the middle of another button it goes into that button's
        // group (a button alone becomes a group of two), as Clip Studio
        // Paint groups a bar's icons; near a button's edge it goes before
        // or after it.
        let moving = self
            .bar_ui
            .dragging
            .filter(|(source, _)| *source == holder)
            .map(|(_, index)| index);
        // A group or a separator never goes into a group: it moves.
        let groupable = egui::DragAndDrop::payload::<BarPayload>(ctx)
            .is_some_and(|payload| !matches!(payload.0.item, Item::Group(_) | Item::Separator));
        let onto = drop_onto(rects, pointer).filter(|&target| {
            groupable
                && Some(target) != moving
                && !matches!(
                    self.holder_items(holder).get(target).map(|slot| &slot.item),
                    Some(Item::Separator) | None
                )
        });
        let painter = ui.painter().clone().with_layer_id(egui::LayerId::new(
            egui::Order::Tooltip,
            Id::unique("sue-paint-bar-drop"),
        ));
        match onto {
            Some(target) => {
                painter.rect_stroke(
                    rects[target].expand(1.0),
                    theme.radius(Radius::Sm),
                    theme.focus_stroke(),
                    egui::StrokeKind::Outside,
                );
            }
            None => {
                let mark = drop_mark(rects, at, area, theme);
                painter.line_segment(mark, theme.focus_stroke());
            }
        }
        if !ui.input(|input| input.pointer.any_released()) {
            return;
        }
        let Some(payload) = egui::DragAndDrop::take_payload::<BarPayload>(ctx) else {
            return;
        };
        let from = self.bar_ui.dragging.take();
        if let Some(items) = self.holder_items_mut(holder) {
            match (from, onto) {
                (Some((source, index)), Some(target)) if source == holder => {
                    if let Some(slot) = (index < items.len()).then(|| items.remove(index)) {
                        let target = if target > index { target - 1 } else { target };
                        edit::into_group(items, target, slot);
                    }
                }
                (_, Some(target)) => {
                    edit::into_group(items, target, payload.0.clone());
                }
                (Some((source, index)), None) if source == holder => {
                    edit::move_to(items, index, at);
                }
                _ => {
                    edit::insert(items, Some(at), payload.0.clone());
                }
            }
        }
        self.save_bars(cx);
    }

    /// The popover a button of `holder` opened, if any: the palette it
    /// pops up, its group's buttons, or Icon Settings.
    pub(crate) fn bar_popovers(
        &mut self,
        ui: &Ui,
        holder: Holder,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let Some((kind, at, index, rect)) = self.bar_ui.open else {
            return;
        };
        if at != holder {
            return;
        }
        // Opened by a route (the snapshots), it learns its place here.
        let rect = if rect.is_positive() {
            rect
        } else {
            self.bar_rect(ui, holder, index).unwrap_or(rect)
        };
        let Some(slot) = self.holder_items(holder).get(index).cloned() else {
            self.bar_ui.open = None;
            return;
        };
        let mut open = true;
        let id = Id::unique(("sue-paint-bar-popover", holder, index));
        let ctx = ui.ctx().clone();
        match (kind, &slot.item) {
            (Popover::Palette, Item::Popup(palette)) if self.palette_drawn(*palette) => {
                // The dock shows it now: one copy on screen is enough.
                open = false;
            }
            (Popover::Palette, Item::Popup(palette)) => {
                let palette = *palette;
                sue_wm::ui::popover(&ctx, id, rect, &mut open, theme, |ui| {
                    let size = sue_wm::dock::DockPanel::float_size(palette);
                    let (body, _) = ui.allocate_exact_size(size, Sense::hover());
                    // A popup has no dock header: the palette names itself.
                    self.draw_palette(ui, palette, body, false, cx, theme);
                });
            }
            (Popover::Group, Item::Group(members)) => {
                let looks: Vec<Look> = members.iter().map(|slot| self.look_of(slot, cx)).collect();
                let mut picked = None;
                sue_wm::ui::popover(&ctx, id, rect, &mut open, theme, |ui| {
                    let density = Density::of(ui);
                    let width = theme.space(Space::S9) * 5.0;
                    for (member, (slot, look)) in members.iter().zip(&looks).enumerate() {
                        let (row, _) =
                            ui.allocate_exact_size(vec2(width, density.row()), Sense::hover());
                        let id = id.with(member);
                        if draw_slot(ui, row, id, slot, look, true, theme).clicked() && look.enabled
                        {
                            picked = Some(member);
                        }
                    }
                });
                if let Some(member) = picked {
                    let item = members[member].item.clone();
                    self.run_item(&item, cx);
                    if let Some(items) = self.holder_items_mut(holder) {
                        edit::choose_in_group(items, index, member);
                    }
                    self.save_bars(cx);
                    open = false;
                }
            }
            (Popover::Icon, _) => {
                let mut query = std::mem::take(&mut self.bar_ui.icon_query);
                let picked = sue_wm::ui::popover(&ctx, id, rect, &mut open, theme, |ui| {
                    icon_picker(
                        ui,
                        &mut query,
                        slot.icon.as_deref(),
                        theme.surface(3),
                        theme,
                    )
                })
                .flatten();
                self.bar_ui.icon_query = query;
                if let Some(icon) = picked {
                    if let Some(target) = self
                        .holder_items_mut(holder)
                        .and_then(|items| items.get_mut(index))
                    {
                        target.icon = icon;
                    }
                    self.save_bars(cx);
                    open = false;
                }
            }
            _ => open = false,
        }
        if !open {
            self.bar_ui.open = None;
        }
    }
}

/// How wide the part at the right of a group's button is that opens its
/// list: the mark and a little room around it, a third of a square
/// button.
fn caret_zone(rect: Rect, named: bool, theme: &Theme) -> f32 {
    if named {
        theme.space(Space::S7)
    } else {
        rect.width() / 3.0 + theme.space(Space::S1)
    }
}

/// Where a button let go at `pointer` goes among buttons at `rects`: the
/// place before the first button of its line that starts right of it
/// (below it, for a column of rows), else after the last of that line.
#[must_use]
pub(crate) fn drop_place(rects: &[Rect], pointer: Pos2) -> usize {
    let mut place = rects.len();
    for (index, rect) in rects.iter().enumerate() {
        let same_line = rect.y_range().contains(pointer.y);
        if same_line && pointer.x < rect.center().x || pointer.y < rect.top() {
            place = index;
            break;
        }
    }
    place
}

/// The button among `rects` whose middle `pointer` is over: the middle
/// half of its width, so its left and right quarters still put a button
/// before or after it as [`drop_place`] does (on a square button of the
/// bar and a named row of Quick Access alike). A button let go there
/// joins its group.
#[must_use]
pub(crate) fn drop_onto(rects: &[Rect], pointer: Pos2) -> Option<usize> {
    rects.iter().position(|rect| {
        rect.contains(pointer) && (pointer.x - rect.center().x).abs() < rect.width() / 4.0
    })
}

/// The line drawn where a dropped button would go: upright before the
/// button at `at` (or after the last).
fn drop_mark(rects: &[Rect], at: usize, area: Rect, theme: &Theme) -> [Pos2; 2] {
    let half = theme.space(Space::S1);
    let rect = match (rects.get(at), rects.last()) {
        (Some(rect), _) => Rect::from_min_max(rect.left_top() - Vec2::X * half, rect.left_bottom()),
        (None, Some(last)) => {
            Rect::from_min_max(last.right_top(), last.right_bottom() + Vec2::X * half)
        }
        (None, None) => Rect::from_min_size(area.min, vec2(half, area.height())),
    };
    [rect.center_top(), rect.center_bottom()]
}

/// Icon Settings: a search and a grid of every icon of Sue's set (an
/// icon pack's own drawings showing where it has them). Hands back the
/// icon picked: `Some(Some(name))`, or `Some(None)` for the button's own.
/// `surface` is the fill it lies on, which its grid's edges fade into.
pub(crate) fn icon_picker(
    ui: &mut Ui,
    query: &mut String,
    current: Option<&str>,
    surface: egui::Color32,
    theme: &Theme,
) -> Option<Option<String>> {
    let mut picked = None;
    let density = Density::of(ui);
    let side = density.icon_button();
    let columns = 8_usize;
    let width = side * columns as f32;
    sue_wm::ui::search_field("sue-paint-icon-search", query)
        .hint(&tr!("search-icons"))
        .width(width)
        .show(ui, theme);
    ui.add_space(theme.space(Space::S2));
    let words = query.trim().to_lowercase();
    let icons: Vec<Icon> = Icon::ALL
        .iter()
        .copied()
        .filter(|icon| words.is_empty() || icon.name().contains(&words))
        .collect();
    sue_wm::ui::ScrollView::vertical("sue-paint-icon-grid")
        .on(surface)
        .auto_shrink(true, true)
        .max_height(side * columns as f32 * 0.75)
        .show(ui, theme, |ui| {
            let rows = icons.len().div_ceil(columns);
            let (grid, _) = ui.allocate_exact_size(vec2(width, side * rows as f32), Sense::hover());
            for (index, icon) in icons.iter().enumerate() {
                let rect = Rect::from_min_size(
                    grid.min
                        + vec2(
                            side * (index % columns) as f32,
                            side * (index / columns) as f32,
                        ),
                    Vec2::splat(side),
                );
                if sue_wm::ui::icon_button(("sue-paint-icon-pick", index), *icon, icon.name())
                    .selected(current == Some(icon.name()))
                    .show_at(ui, rect, theme)
                    .clicked()
                {
                    picked = Some(Some(icon.name().to_owned()));
                }
            }
        });
    ui.add_space(theme.space(Space::S2));
    if sue_wm::ui::button("sue-paint-icon-own", &tr!("use-its-own-icon"))
        .enabled(current.is_some())
        .show(ui, theme)
        .clicked()
    {
        picked = Some(None);
    }
    picked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_button_let_go_over_the_middle_of_another_joins_its_group() {
        let rects = [
            Rect::from_min_size(pos2(0.0, 0.0), vec2(32.0, 32.0)),
            Rect::from_min_size(pos2(36.0, 0.0), vec2(32.0, 32.0)),
        ];
        assert_eq!(drop_onto(&rects, pos2(52.0, 16.0)), Some(1));
        assert_eq!(drop_onto(&rects, pos2(38.0, 16.0)), None, "its edge");
        assert_eq!(drop_place(&rects, pos2(38.0, 16.0)), 1, "goes before it");
        assert_eq!(drop_onto(&rects, pos2(34.0, 16.0)), None, "between");
        // Quick Access's named rows: the middle of their width, so a
        // button let go over a row's left or right end still moves
        // before or after it, whatever its height.
        let rows = [
            Rect::from_min_size(pos2(0.0, 0.0), vec2(200.0, 24.0)),
            Rect::from_min_size(pos2(0.0, 24.0), vec2(200.0, 24.0)),
        ];
        assert_eq!(drop_onto(&rows, pos2(100.0, 12.0)), Some(0));
        assert_eq!(drop_onto(&rows, pos2(100.0, 2.0)), Some(0));
        assert_eq!(drop_onto(&rows, pos2(20.0, 12.0)), None);
        assert_eq!(drop_place(&rows, pos2(20.0, 12.0)), 0, "before it");
        assert_eq!(drop_onto(&rows, pos2(180.0, 12.0)), None);
        assert_eq!(drop_place(&rows, pos2(180.0, 12.0)), 1, "after it");
    }
}
