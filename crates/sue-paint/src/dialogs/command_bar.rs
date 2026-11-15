//! Command Bar Settings and Quick Access Settings: one dialog for both
//! bars. At the left, what can become a button, by category (Clip Studio
//! Paint's Main Menu, Pop-up Palettes, Tools, Auto Actions, Drawing
//! Colors and Options, with sub tools as a category of their own) and
//! with a search; at the right, the bar's buttons as they stand; between
//! them, what moves one to the other. Every change shows on the bar at
//! once and is kept when the dialog closes.
//!
//! Clip Studio Paint's dialog picks the button to delete by a click on
//! the bar itself, behind the dialog. Here the bar's buttons are a list in
//! the dialog, so every button can be picked, moved and deleted without
//! leaving it, by pointer or by keyboard.

use egui::{Align, Layout, Rect, Ui, Vec2, vec2};
use sue_wm::icons::Icon;
use sue_wm::ui::{Density, ListRow, Radius, Space};
use sue_wm::{AppletContext, FilePick, MenuItem, ShellRequest, Theme};

use super::Outcome;
use crate::bar_items::{
    Bar, Holder, Item, POPUP_PALETTES, QuickAccess, Reorder, SHARE_EXTENSION, Slot, edit,
    known_command,
};
use crate::studio::SuePaint;
use crate::tools::Tool;

/// Which bar the dialog sets up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BarTarget {
    CommandBar,
    /// Quick Access's set showing.
    QuickAccess,
}

/// What can become a button, in groups.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Category {
    MainMenu,
    PopupPalettes,
    Tools,
    SubTools,
    AutoActions,
    DrawingColors,
    Options,
}

impl Category {
    const ALL: [Self; 7] = [
        Self::MainMenu,
        Self::PopupPalettes,
        Self::Tools,
        Self::SubTools,
        Self::AutoActions,
        Self::DrawingColors,
        Self::Options,
    ];

    fn label(self) -> String {
        match self {
            Self::MainMenu => tr!("main-menu"),
            Self::PopupPalettes => tr!("pop-up-palettes"),
            Self::Tools => tr!("bar-tools"),
            Self::SubTools => tr!("sub-tools"),
            Self::AutoActions => tr!("auto-actions"),
            Self::DrawingColors => tr!("drawing-colors"),
            Self::Options => tr!("other-commands"),
        }
    }
}

/// Command Bar Settings or Quick Access Settings, open.
#[derive(Clone, Debug)]
pub(crate) struct BarDialog {
    target: BarTarget,
    /// The category shown at the left, by its place in [`Category::ALL`].
    category: usize,
    /// The search over the category's entries.
    query: String,
    /// The entry picked at the left.
    picked: Option<Slot>,
    /// The bar's button picked at the right.
    chosen: Option<usize>,
    /// The icons show at the left in place of the entries, for the button
    /// picked at the right.
    icons: bool,
    /// The search over the icons.
    icon_query: String,
}

impl BarDialog {
    /// The dialog for `target`, its first category showing.
    #[must_use]
    pub(crate) fn new(target: BarTarget) -> Self {
        Self {
            target,
            category: 0,
            query: String::new(),
            picked: None,
            chosen: None,
            icons: false,
            icon_query: String::new(),
        }
    }

    /// Its title.
    #[must_use]
    pub(crate) fn title(&self) -> String {
        match self.target {
            BarTarget::CommandBar => tr!("command-bar-settings"),
            BarTarget::QuickAccess => tr!("quick-access-settings"),
        }
    }
}

/// An entry at the left: the heading it is under and the button it makes.
type Entry = (String, Slot);

impl SuePaint {
    /// The bar the dialog sets up.
    fn dialog_holder(&self, target: BarTarget) -> Holder {
        match target {
            BarTarget::CommandBar => Holder::CommandBar,
            BarTarget::QuickAccess => Holder::QuickAccess(self.quick_access.shown),
        }
    }

    /// What `category` offers, under its headings.
    fn entries(&self, category: Category) -> Vec<Entry> {
        let registry = crate::commands::registry();
        let mut out = Vec::new();
        match category {
            Category::MainMenu => {
                fn walk(items: &[MenuItem], heading: &str, out: &mut Vec<Entry>) {
                    for item in items {
                        match item {
                            MenuItem::Action { command, .. }
                                if known_command(command)
                                    && !crate::commands::registry()
                                        .find(command)
                                        .is_some_and(|(found, _)| found.is_family()) =>
                            {
                                out.push((heading.to_owned(), Slot::command(command)));
                            }
                            MenuItem::Sub { label, items } => {
                                walk(items, &format!("{heading}, {label}"), out);
                            }
                            _ => {}
                        }
                    }
                }
                for menu in &self.menus {
                    walk(&menu.items, &menu.title, &mut out);
                }
                // A submenu in the middle of a menu would split it under
                // two headings of the same name: each heading gathers all
                // its entries, in the order the menus first show it.
                let mut order: Vec<String> = Vec::new();
                for (heading, _) in &out {
                    if !order.contains(heading) {
                        order.push(heading.clone());
                    }
                }
                out.sort_by_key(|(heading, _)| order.iter().position(|known| known == heading));
            }
            Category::PopupPalettes => {
                out.extend(
                    POPUP_PALETTES
                        .iter()
                        .map(|palette| (String::new(), Slot::of(Item::Popup(*palette)))),
                );
            }
            Category::Tools => {
                out.extend(
                    Tool::ALL
                        .iter()
                        .map(|tool| (String::new(), Slot::of(Item::Tool(*tool)))),
                );
            }
            Category::SubTools => {
                for tool in Tool::ALL {
                    out.extend(self.toolbox.subs(tool).iter().map(|sub| {
                        (
                            tool.label(),
                            Slot::of(Item::SubTool(tool, sub.name.clone())),
                        )
                    }));
                }
            }
            Category::AutoActions => {
                out.extend(
                    self.auto
                        .sets
                        .iter()
                        .map(|set| (String::new(), Slot::of(Item::AutoAction(set.name.clone())))),
                );
            }
            Category::DrawingColors => {
                out.push((
                    tr!("bar-drawing-color"),
                    Slot::of(Item::Color(self.drawing_color())),
                ));
                for set in &self.color_sets.sets {
                    out.extend(
                        set.cells
                            .iter()
                            .flatten()
                            .map(|swatch| (set.name.clone(), Slot::of(Item::Color(swatch.rgb)))),
                    );
                }
            }
            Category::Options => {
                let in_menus: std::collections::HashSet<String> =
                    crate::menus::commands(&self.menus)
                        .into_iter()
                        .map(|(_, command)| command)
                        .collect();
                for command in registry.iter().filter(|command| {
                    !command.is_family()
                        && !command.flags.has(sue_wm::Flags::HIDDEN)
                        && !in_menus.contains(command.name)
                }) {
                    out.push((command.group.to_owned(), Slot::command(command.name)));
                }
                out.push((
                    String::new(),
                    Slot::command(crate::bar_items::PALETTE_COMMAND),
                ));
            }
        }
        out
    }

    /// The settings dialog of the command bar or of Quick Access.
    pub(super) fn command_bar_ui(
        &mut self,
        ui: &mut Ui,
        dialog: &mut BarDialog,
        cx: &mut AppletContext<'_>,
        size: Vec2,
        theme: &Theme,
    ) -> Outcome {
        let mut outcome = Outcome::Open;
        let gap = theme.space(Space::S5);
        // The body sits in a scroll area that scrolls both ways, so what it
        // has left is unbounded: the dialog's visible part is what counts.
        let visible = ui.clip_rect();
        // Room at the right for the scroll bar, which may show.
        let width = (visible.right() - ui.cursor().min.x - theme.space(Space::S5))
            .min(size.x)
            .max(0.0);
        ui.set_max_width(width);
        if dialog.target == BarTarget::QuickAccess {
            self.quick_set_row(ui, dialog, cx, theme);
            ui.add_space(theme.space(Space::S3));
        }
        let holder = self.dialog_holder(dialog.target);
        // Wide enough for the longest of its buttons, Restore Default
        // Layout.
        let middle = theme.space(Space::S9) * 5.0;
        let side = ((width - middle - gap * 2.0) / 2.0).max(middle);
        let height =
            (visible.bottom() - ui.cursor().min.y - Density::of(ui).control_height() * 3.0)
                .min(size.y)
                .max(middle);
        let column = Layout::top_down(Align::Min);
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(vec2(side, height), column, |ui| {
                ui.set_width(side);
                if dialog.icons {
                    self.icons_column(ui, dialog, holder, cx, theme);
                } else {
                    self.entries_column(ui, dialog, holder, cx, side, theme);
                }
            });
            ui.add_space(gap);
            ui.allocate_ui_with_layout(vec2(middle, height), column, |ui| {
                ui.set_width(middle);
                self.middle_buttons(ui, dialog, holder, cx, theme);
            });
            ui.add_space(gap);
            ui.allocate_ui_with_layout(vec2(side, height), column, |ui| {
                ui.set_width(side);
                self.bar_column(ui, dialog, holder, side, theme);
            });
        });
        ui.add_space(theme.space(Space::S4));
        ui.horizontal(|ui| {
            ui.label(tr!("change-order"));
            let options = [tr!("drag"), tr!("ctrl-drag")];
            let reorder = self.holder_reorder(holder);
            let chosen = usize::from(reorder == Reorder::CtrlDrag);
            if let Some(picked) =
                sue_wm::ui::segmented("sue-paint-bar-order", &options, Some(chosen))
                    .show(ui, theme)
                    .picked
            {
                let reorder = if picked == 0 {
                    Reorder::Drag
                } else {
                    Reorder::CtrlDrag
                };
                self.set_holder_reorder(holder, reorder);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if sue_wm::ui::button("sue-paint-bar-close", &tr!("close"))
                    .primary()
                    .hint(&tr!("keeps-the-bar-as-it-is-now"))
                    .show(ui, theme)
                    .clicked()
                {
                    outcome = Outcome::Ok;
                }
                let (purpose, title) = match dialog.target {
                    BarTarget::CommandBar => ("bar", tr!("command-bar")),
                    BarTarget::QuickAccess => ("quick", tr!("quick-access")),
                };
                if sue_wm::ui::button("sue-paint-bar-export", &tr!("export-ellipsis"))
                    .hint(&tr!("keeps-the-bar-in-a-file-to-share"))
                    .show(ui, theme)
                    .clicked()
                {
                    cx.request(ShellRequest::PickFile(
                        FilePick::save(
                            Some(crate::studio::me_id()),
                            &format!("{purpose}-export"),
                            &title,
                            &format!("{title}.{SHARE_EXTENSION}"),
                        )
                        .extensions(&[SHARE_EXTENSION]),
                    ));
                }
                if sue_wm::ui::button("sue-paint-bar-import", &tr!("import-ellipsis"))
                    .hint(&tr!("takes-a-bar-someone-shared"))
                    .show(ui, theme)
                    .clicked()
                {
                    cx.request(ShellRequest::PickFile(
                        FilePick::open(
                            Some(crate::studio::me_id()),
                            &format!("{purpose}-import"),
                            &title,
                        )
                        .extensions(&[SHARE_EXTENSION]),
                    ));
                }
            });
        });
        ui.label(
            egui::RichText::new(tr!("command-bar-settings-hint"))
                .font(theme.text_font(sue_wm::ui::TextRole::Caption))
                .color(theme.text_muted()),
        );
        outcome
    }

    /// Quick Access Settings: the set being set up, its name, and a new
    /// set or the set gone.
    fn quick_set_row(
        &mut self,
        ui: &mut Ui,
        dialog: &mut BarDialog,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        ui.horizontal(|ui| {
            let names: Vec<String> = self
                .quick_access
                .sets
                .iter()
                .map(|set| set.name.clone())
                .collect();
            let mut shown = self.quick_access.shown;
            sue_wm::ui::dropdown("sue-paint-quick-set", &mut shown, &names)
                .label(&tr!("bar-set"))
                .show(ui, theme);
            if shown != self.quick_access.shown {
                self.quick_access.shown = shown;
                dialog.chosen = None;
            }
            if let Some(set) = self.quick_access.set_mut() {
                sue_wm::ui::text_field("sue-paint-quick-set-name", &mut set.name)
                    .label(&tr!("bar-set-name"))
                    .width(theme.space(Space::S9) * 5.0)
                    .show(ui, theme);
            }
            if sue_wm::ui::button("sue-paint-quick-new", &tr!("new-set"))
                .icon(Icon::Plus)
                .show(ui, theme)
                .clicked()
            {
                self.quick_access.add_set();
                dialog.chosen = None;
                self.save_bars(cx);
            }
            if sue_wm::ui::button("sue-paint-quick-delete", &tr!("delete-set"))
                .icon(Icon::Delete)
                .enabled(self.quick_access.sets.len() > 1)
                .show(ui, theme)
                .clicked()
                && self.quick_access.delete_set()
            {
                dialog.chosen = None;
                self.save_bars(cx);
            }
        });
    }

    /// The left: a category, a search, and its entries under headings.
    fn entries_column(
        &mut self,
        ui: &mut Ui,
        dialog: &mut BarDialog,
        holder: Holder,
        cx: &mut AppletContext<'_>,
        width: f32,
        theme: &Theme,
    ) {
        let labels: Vec<String> = Category::ALL
            .iter()
            .map(|category| category.label())
            .collect();
        let was = dialog.category;
        sue_wm::ui::dropdown("sue-paint-bar-category", &mut dialog.category, &labels)
            .label(&tr!("category"))
            .width(width)
            .show(ui, theme);
        if was != dialog.category {
            dialog.picked = None;
        }
        sue_wm::ui::search_field("sue-paint-bar-search", &mut dialog.query)
            .hint(&tr!("bar-search"))
            .width(width)
            .show(ui, theme);
        let category = Category::ALL[dialog.category.min(Category::ALL.len() - 1)];
        let words = dialog.query.trim().to_lowercase();
        let entries: Vec<Entry> = self
            .entries(category)
            .into_iter()
            .filter(|(heading, slot)| {
                words.is_empty()
                    || slot.item.label().to_lowercase().contains(&words)
                    || heading.to_lowercase().contains(&words)
            })
            .collect();
        let mut add = None;
        sue_wm::ui::ScrollView::vertical("sue-paint-bar-entries")
            .on(theme.panel)
            .show(ui, theme, |ui| {
                let mut heading = None;
                for (index, (under, slot)) in entries.iter().enumerate() {
                    if heading != Some(under) && !under.is_empty() {
                        sue_wm::ui::heading(ui, under, theme);
                    }
                    heading = Some(under);
                    let label = slot.item.label();
                    let mut row = ListRow::new(("bar-entry", index), &label)
                        .selected(dialog.picked.as_ref() == Some(slot))
                        .width(width);
                    if let Some(icon) = slot.item.icon() {
                        row = row.icon(icon);
                    }
                    let output = row.show(ui, theme);
                    swatch(ui, output.response.rect, &slot.item, theme);
                    if output.response.clicked() {
                        dialog.picked = Some(slot.clone());
                    }
                    if output.response.double_clicked() {
                        add = Some(slot.clone());
                    }
                }
                if entries.is_empty() {
                    ui.label(egui::RichText::new(tr!("nothing-matches")).color(theme.text_muted()));
                }
            });
        if let Some(slot) = add {
            self.add_to_bar(dialog, holder, slot, cx);
        }
    }

    /// Puts `slot` on the bar after its button picked (or at the end) and
    /// picks it.
    fn add_to_bar(
        &mut self,
        dialog: &mut BarDialog,
        holder: Holder,
        slot: Slot,
        cx: &mut AppletContext<'_>,
    ) {
        let after = dialog.chosen.map(|index| index + 1);
        if let Some(items) = self.holder_items_mut(holder) {
            dialog.chosen = Some(edit::insert(items, after, slot));
        }
        self.save_bars(cx);
    }

    /// The left, for Icon Settings: every icon, to give the button picked
    /// at the right.
    fn icons_column(
        &mut self,
        ui: &mut Ui,
        dialog: &mut BarDialog,
        holder: Holder,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let Some(index) = dialog.chosen else {
            dialog.icons = false;
            return;
        };
        let current = self
            .holder_items(holder)
            .get(index)
            .and_then(|slot| slot.icon.clone());
        sue_wm::ui::heading(ui, &tr!("icon-settings"), theme);
        if let Some(icon) = crate::panels::icon_picker(
            ui,
            &mut dialog.icon_query,
            current.as_deref(),
            theme.panel,
            theme,
        ) {
            if let Some(slot) = self
                .holder_items_mut(holder)
                .and_then(|items| items.get_mut(index))
            {
                slot.icon = icon;
            }
            dialog.icons = false;
            self.save_bars(cx);
        }
        if sue_wm::ui::button("sue-paint-bar-icons-back", &tr!("back"))
            .ghost()
            .icon(Icon::ArrowLeft)
            .show(ui, theme)
            .clicked()
        {
            dialog.icons = false;
        }
    }

    /// The buttons between the two lists.
    fn middle_buttons(
        &mut self,
        ui: &mut Ui,
        dialog: &mut BarDialog,
        holder: Holder,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let items = self.holder_items(holder);
        let chosen = dialog.chosen.filter(|index| *index < items.len());
        let count = items.len();
        let chosen_slot = chosen.and_then(|index| items.get(index)).cloned();
        let groupable = chosen_slot
            .as_ref()
            .is_some_and(|slot| slot.item != Item::Separator);
        let has_icon = chosen_slot
            .as_ref()
            .is_some_and(|slot| !matches!(slot.item, Item::Separator | Item::Color(_)));
        let button =
            |ui: &mut Ui, salt: &str, label: String, hint: String, on: bool, icon: Icon| {
                sue_wm::ui::button(salt, &label)
                    .icon(icon)
                    .hint(&hint)
                    .enabled(on)
                    .full_width()
                    .show(ui, theme)
                    .clicked()
            };
        let picked = dialog.picked.clone();
        if button(
            ui,
            "sue-paint-bar-add",
            tr!("bar-add"),
            tr!("add-hint"),
            picked.is_some(),
            Icon::ArrowRight,
        ) && let Some(slot) = picked.clone()
        {
            self.add_to_bar(dialog, holder, slot, cx);
        }
        if button(
            ui,
            "sue-paint-bar-group",
            tr!("insert-into-group"),
            tr!("insert-into-group-hint"),
            picked.is_some() && groupable,
            Icon::Plus,
        ) && let (Some(slot), Some(index)) = (picked, chosen)
            && let Some(items) = self.holder_items_mut(holder)
        {
            edit::into_group(items, index, slot);
            self.save_bars(cx);
        }
        if button(
            ui,
            "sue-paint-bar-separator",
            tr!("add-separator"),
            tr!("add-separator-hint"),
            true,
            Icon::Minus,
        ) {
            self.add_to_bar(dialog, holder, Slot::of(Item::Separator), cx);
        }
        ui.add_space(theme.space(Space::S3));
        if button(
            ui,
            "sue-paint-bar-icon",
            tr!("icon-settings-ellipsis"),
            tr!("icon-settings-hint"),
            has_icon,
            Icon::Picture,
        ) {
            dialog.icons = true;
        }
        if button(
            ui,
            "sue-paint-bar-up",
            tr!("move-up"),
            tr!("move-up-hint"),
            chosen.is_some_and(|index| index > 0),
            Icon::ArrowUp,
        ) && let (Some(index), Some(items)) = (chosen, self.holder_items_mut(holder))
        {
            dialog.chosen = Some(edit::move_to(items, index, index - 1));
            self.save_bars(cx);
        }
        if button(
            ui,
            "sue-paint-bar-down",
            tr!("move-down"),
            tr!("move-down-hint"),
            chosen.is_some_and(|index| index + 1 < count),
            Icon::ArrowDown,
        ) && let (Some(index), Some(items)) = (chosen, self.holder_items_mut(holder))
        {
            dialog.chosen = Some(edit::move_to(items, index, index + 2));
            self.save_bars(cx);
        }
        if button(
            ui,
            "sue-paint-bar-delete",
            tr!("delete"),
            tr!("delete-button-hint"),
            chosen.is_some(),
            Icon::Delete,
        ) && let (Some(index), Some(items)) = (chosen, self.holder_items_mut(holder))
        {
            edit::delete(items, index);
            dialog.chosen = (index < items.len())
                .then_some(index)
                .or(index.checked_sub(1));
            self.save_bars(cx);
        }
        ui.add_space(theme.space(Space::S3));
        if button(
            ui,
            "sue-paint-bar-restore",
            tr!("restore-default-layout"),
            tr!("restore-default-layout-hint"),
            true,
            Icon::Reset,
        ) {
            match holder {
                Holder::CommandBar => self.bar.items = Bar::default().items,
                Holder::QuickAccess(set) => {
                    let defaults = QuickAccess::default();
                    if let (Some(target), Some(default)) =
                        (self.quick_access.sets.get_mut(set), defaults.sets.get(set))
                    {
                        target.items = default.items.clone();
                    }
                }
            }
            dialog.chosen = None;
            self.save_bars(cx);
        }
    }

    /// The right: the bar's buttons as they stand, a group's buttons under
    /// it.
    fn bar_column(
        &mut self,
        ui: &mut Ui,
        dialog: &mut BarDialog,
        holder: Holder,
        width: f32,
        theme: &Theme,
    ) {
        let title = match holder {
            Holder::CommandBar => tr!("on-the-command-bar"),
            Holder::QuickAccess(_) => tr!("in-this-set"),
        };
        sue_wm::ui::heading(ui, &title, theme);
        let items = self.holder_items(holder).to_vec();
        sue_wm::ui::ScrollView::vertical("sue-paint-bar-items")
            .on(theme.panel)
            .show(ui, theme, |ui| {
                for (index, slot) in items.iter().enumerate() {
                    let label = slot.item.label();
                    let mut row = ListRow::new(("bar-item", index), &label)
                        .selected(dialog.chosen == Some(index))
                        .width(width);
                    if let Some(icon) = slot.shown_icon() {
                        row = row.icon(icon);
                    }
                    if let Item::Group(members) = &slot.item {
                        let names: Vec<String> =
                            members.iter().map(|member| member.item.label()).collect();
                        let detail = names.join(", ");
                        let output = row.detail(&detail).show(ui, theme);
                        if output.response.clicked() {
                            dialog.chosen = Some(index);
                        }
                        continue;
                    }
                    let output = row.show(ui, theme);
                    swatch(ui, output.response.rect, &slot.item, theme);
                    if output.response.clicked() {
                        dialog.chosen = Some(index);
                    }
                }
                if items.is_empty() {
                    ui.label(egui::RichText::new(tr!("no-buttons-yet")).color(theme.text_muted()));
                }
            });
    }
}

/// A color's swatch at the right end of its row.
fn swatch(ui: &Ui, row: Rect, item: &Item, theme: &Theme) {
    let Item::Color(rgb) = item else {
        return;
    };
    let side = Density::of(ui).icon();
    let rect = Rect::from_center_size(
        row.right_center() - egui::Vec2::X * (side / 2.0 + theme.space(Space::S4)),
        Vec2::splat(side),
    );
    ui.painter().rect_filled(
        rect,
        theme.radius(Radius::Xs),
        egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2]),
    );
    ui.painter().rect_stroke(
        rect,
        theme.radius(Radius::Xs),
        egui::Stroke::new(theme.border_width, theme.border_strong()),
        egui::StrokeKind::Outside,
    );
}
