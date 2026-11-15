//! Quick Access: sets of buttons, each with its icon and its name, a tab
//! per set. Its buttons are the command bar's kinds (commands, tools, sub
//! tools, drawing colors, palettes that pop up, Auto Actions, groups) and
//! share its right-click menu, its drags and its settings dialog.

use egui::{Id, Rect, Sense, Ui, pos2, vec2};
use sue_wm::ui::{Density, Space, Tab, TextRole};
use sue_wm::{AppletContext, Theme};

use super::command_bar::Look;
use crate::bar_items::{Holder, Item};
use crate::studio::SuePaint;

/// The commands of Quick Access's palette menu: a new set, the set
/// showing gone, and its settings (also its own button in Set 1).
pub(crate) const MENU_COMMANDS: [&str; 3] = [
    "window.quick_access.new_set",
    "window.quick_access.delete_set",
    "window.quick_access.settings",
];

impl SuePaint {
    /// The Quick Access palette: its sets as tabs along the top (dragged
    /// to reorder), then the set's buttons in columns that fill the
    /// palette's width, a line across between groups.
    pub(crate) fn quick_access_panel(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::ZERO, theme.panel);
        let density = Density::of(ui);
        // The palette's menu button sits at the top left, drawn by the
        // dock: the tabs start after it.
        let inset = ui
            .ctx()
            .data(|data| data.get_temp::<f32>(Id::unique(super::MENU_INSET)))
            .unwrap_or_default();
        let strip = Rect::from_min_size(
            rect.min + egui::Vec2::X * inset,
            vec2(rect.width() - inset, density.tab_strip()),
        );
        let names: Vec<String> = self
            .quick_access
            .sets
            .iter()
            .map(|set| set.name.clone())
            .collect();
        let tabs: Vec<Tab<'_>> = names
            .iter()
            .enumerate()
            .map(|(index, name)| Tab::new(("quick-set", index), name))
            .collect();
        let mut strip_ui = super::child(ui, strip, "quick-access-sets");
        let output = sue_wm::ui::tab_strip("sue-paint-quick-sets", &tabs, self.quick_access.shown)
            .reorder(true)
            .label(&tr!("quick-access"))
            .show(&mut strip_ui, theme);
        if let Some(chosen) = output.chosen {
            self.quick_access.shown = chosen;
            self.bar_ui.open = None;
            self.save_bars(cx);
        }
        if let Some((from, to)) = output.moved {
            let set = self.quick_access.sets.remove(from);
            self.quick_access.sets.insert(to, set);
            self.quick_access.shown = to;
            self.save_bars(cx);
        }
        let body = Rect::from_min_max(pos2(rect.left(), strip.bottom()), rect.max)
            .shrink(theme.space(Space::S2));
        let holder = Holder::QuickAccess(self.quick_access.shown);
        let looks: Vec<Look> = self
            .holder_items(holder)
            .iter()
            .map(|slot| self.look_of(slot, cx))
            .collect();
        let items = self.holder_items(holder).to_vec();
        // Columns as wide as a name needs, as many as fit; laid out from
        // the top left of the set, then placed where the set scrolls to.
        let row = density.row();
        let least = theme.space(Space::S9) * 4.0;
        let columns = ((body.width() / least).floor() as usize).max(1);
        let column = body.width() / columns as f32;
        let mut places = Vec::with_capacity(items.len());
        let mut at = 0_usize;
        let mut y = 0.0;
        for slot in &items {
            if slot.item == Item::Separator {
                if at > 0 {
                    y += row;
                    at = 0;
                }
                let line = theme.space(Space::S4);
                places.push(Rect::from_min_size(pos2(0.0, y), vec2(body.width(), line)));
                y += line;
                continue;
            }
            if at == columns {
                y += row;
                at = 0;
            }
            places.push(Rect::from_min_size(
                pos2(column * at as f32, y),
                vec2(column, row),
            ));
            at += 1;
        }
        let height = if at > 0 { y + row } else { y };
        let background = ui.interact(body, Id::unique("sue-paint-quick-ground"), Sense::click());
        let mut inner = super::child(ui, body, "quick-access-buttons");
        // A set with more buttons than the palette is high scrolls.
        let shown = sue_wm::ui::ScrollView::vertical(("sue-paint-quick-set", holder))
            .on(theme.panel)
            .show(&mut inner, theme, |ui| {
                let (content, _) =
                    ui.allocate_exact_size(vec2(body.width(), height), Sense::hover());
                let rects: Vec<Rect> = places
                    .iter()
                    .map(|place| place.translate(content.min.to_vec2()))
                    .collect();
                let (clicked, menu) =
                    self.bar_buttons(ui, holder, &rects, &looks, true, &background, theme);
                (rects, clicked, menu)
            });
        let (rects, clicked, menu) = shown.inner;
        if let Some((index, caret)) = clicked {
            self.press_button(holder, index, rects[index], caret, cx);
        }
        if items.is_empty() {
            inner.painter().text(
                body.center_top() + egui::Vec2::Y * row,
                egui::Align2::CENTER_CENTER,
                tr!("quick-access-empty"),
                theme.text_font(TextRole::Caption),
                theme.text_muted(),
            );
        }
        self.bar_menu(&inner, holder, &menu, cx, theme);
        self.bar_drops(&inner, holder, body, &rects, cx, theme);
        self.bar_popovers(&inner, holder, cx, theme);
    }
}
