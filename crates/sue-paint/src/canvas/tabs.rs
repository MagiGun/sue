//! The tab strip over the canvas, its right-click menu, and Window,
//! Canvas, Tile.
//!
//! The strip is Sue's own (`sue_wm::ui::tab_strip`): a tab is a canvas
//! of the column, named as its picture is, with a star while it has
//! changes that are not saved. A click shows it, its cross or the middle
//! button closes it, dragging it along the strip moves it, and dragging
//! it down out of the strip opens it in a window of its own. The button
//! after the last tab makes a new canvas.

use egui::{Id, Rect, Sense, Ui, Vec2, pos2};
use sue_model::picture;
use sue_wm::ui::{MenuEvent, MenuRequest, Space, Tab, tab_strip};
use sue_wm::{AppletContext, Theme};
use sue_wm::{ItemState, MenuItem};

use super::density;
use crate::commands::registry;
use crate::studio::SuePaint;

/// The commands of a tab's right-click menu, `None` for a line.
const TAB_MENU: [Option<&str>; 6] = [
    Some("file.save"),
    Some("file.save_as"),
    None,
    Some("window.float"),
    None,
    Some("file.close"),
];

impl SuePaint {
    /// The keys the menus show for `command` now (empty for none).
    pub(super) fn shortcut_of(&self, command: &str) -> String {
        fn find(items: &[MenuItem], command: &str) -> Option<String> {
            items.iter().find_map(|item| match item {
                MenuItem::Action {
                    command: found,
                    shortcut,
                    ..
                } if found == command => Some(shortcut.clone()),
                MenuItem::Sub { items, .. } => find(items, command),
                _ => None,
            })
        }
        self.menus
            .iter()
            .find_map(|menu| find(&menu.items, command))
            .unwrap_or_default()
    }

    /// `true` while the picture of canvas `index` has changes made since
    /// it was opened or saved. A picture never saved is clean as it first
    /// showed.
    fn modified(&mut self, index: usize) -> bool {
        let Some(picture) = self.docs.get(index).map(|doc| doc.picture) else {
            return false;
        };
        // A picture shown in two tabs (New Window) is as saved as its
        // first view says, the one Save marks clean.
        let Some(doc) = self.docs.iter().find(|doc| doc.picture == picture) else {
            return false;
        };
        let (_, _, version) = doc.compositor.display();
        if version > 0 {
            self.clean.entry(doc.picture).or_insert(version);
        }
        self.clean
            .get(&doc.picture)
            .is_some_and(|clean| *clean != version)
    }

    pub(super) fn tabs(
        &mut self,
        ui: &mut Ui,
        strip: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        ui.painter()
            .rect_filled(strip, egui::CornerRadius::ZERO, theme.pill);
        let density = density(ui);
        let gap = theme.space(Space::S2);
        let side = density.icon_button().min(strip.height());
        let main = self.main_docs();
        let shown = self.column_doc();
        let mut labels = Vec::with_capacity(main.len());
        let mut salts = Vec::with_capacity(main.len());
        for index in 0..self.docs.len() {
            let modified = self.modified(index);
            if !main.contains(&index) {
                continue;
            }
            let name = self.doc_name(index, cx);
            labels.push(if modified { format!("{name}*") } else { name });
            // A tab is named by its picture and, for a second view of the
            // same picture (New Window), by which view it is.
            let picture = self.docs[index].picture;
            let nth = self.docs[..index]
                .iter()
                .filter(|doc| doc.picture == picture)
                .count();
            salts.push((picture.to_u128(), nth));
        }
        let tabs: Vec<Tab<'_>> = labels
            .iter()
            .zip(&salts)
            .map(|(label, salt)| Tab::new(*salt, label).closable(true).shortcut(""))
            .collect();
        let chosen = shown
            .and_then(|shown| main.iter().position(|&index| index == shown))
            .unwrap_or(usize::MAX);
        let room = Rect::from_min_max(
            strip.min,
            pos2(
                (strip.right() - side - gap).max(strip.left()),
                strip.bottom(),
            ),
        );
        let mut inner = crate::panels::child(ui, room, "canvas-tabs");
        let label = tr!("open-canvases");
        let output = density.scope(&mut inner, |ui| {
            tab_strip("sue-paint-canvas-tabs", &tabs, chosen)
                .reorder(true)
                .tear(true)
                .label(&label)
                .show(ui, theme)
        });
        // The new canvas button after the last tab.
        let after = output
            .places
            .last()
            .map_or(strip.left(), |place| place.right())
            + gap;
        let plus = Rect::from_center_size(
            pos2(
                after.min(strip.right() - side) + side / 2.0,
                strip.center().y,
            ),
            Vec2::splat(side),
        );
        let new = registry().label("file.new").unwrap_or_default();
        let keys = self.shortcut_of("file.new");
        if keyed_icon_button(
            ui,
            plus,
            "sue-paint-canvas-new",
            sue_wm::icons::Icon::Plus,
            (&new, &keys),
            theme,
        )
        .clicked()
        {
            self.run("file.new", cx);
        }
        // A right click on a tab opens its menu.
        let secondary = ui.input(|input| {
            input
                .pointer
                .secondary_clicked()
                .then(|| input.pointer.interact_pos())
                .flatten()
        });
        if let Some(at) = secondary
            && let Some(place) = output.places.iter().position(|place| place.contains(at))
        {
            self.chrome.tab_menu = Some((main[place], at));
            self.chrome.menu.open();
        }
        self.tab_menu_ui(ui, cx, theme);
        if let Some(place) = output.chosen {
            self.active = main[place];
        }
        if let Some((from, to)) = output.moved {
            let (a, b) = (main[from], main[to]);
            self.docs.swap(a, b);
            for at in [&mut self.active, &mut self.chrome.main] {
                if *at == a {
                    *at = b;
                } else if *at == b {
                    *at = a;
                }
            }
        }
        if let Some(place) = output.closed {
            self.active = main[place];
            self.run("file.close", cx);
        } else if let Some((place, _)) = output.torn {
            self.float_canvas(main[place], cx);
        }
    }

    /// The right-click menu of a tab, while it is open.
    fn tab_menu_ui(&mut self, ui: &Ui, cx: &mut AppletContext<'_>, theme: &Theme) {
        let Some((index, at)) = self.chrome.tab_menu else {
            return;
        };
        let items: Vec<MenuItem> = TAB_MENU
            .iter()
            .map(|entry| match entry {
                Some(command) => {
                    MenuItem::action(registry().label(command).unwrap_or_default(), *command)
                        .key(self.shortcut_of(command))
                }
                None => MenuItem::Separator,
            })
            .collect();
        let state = |_: &str| ItemState::on();
        let event = self.chrome.menu.show(
            ui.ctx(),
            &MenuRequest {
                id: Id::unique("sue-paint-tab-menu"),
                anchor: at,
                owner: Rect::NOTHING,
                items: &items,
                state: &state,
                density: density(ui),
            },
            theme,
        );
        match event {
            MenuEvent::Picked(command) => {
                self.chrome.tab_menu = None;
                if index < self.docs.len() {
                    self.active = index;
                    if command == "window.float" {
                        self.float_canvas(index, cx);
                    } else {
                        self.run(&command, cx);
                    }
                }
            }
            MenuEvent::Closed => self.chrome.tab_menu = None,
            _ => {}
        }
    }

    /// Window, Canvas, Tile: every canvas of the column in a grid cell of
    /// `area`, the current one live in its cell (returned), the others as
    /// they last showed; a click on one makes it current.
    pub(super) fn tile_others(
        &mut self,
        ui: &mut Ui,
        area: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) -> Rect {
        let main = self.main_docs();
        let count = main.len();
        let columns = (count as f32).sqrt().ceil().max(1.0) as usize;
        let rows = count.div_ceil(columns);
        let cell_size = Vec2::new(area.width() / columns as f32, area.height() / rows as f32);
        let inset = theme.border_width;
        let cell = |place: usize| {
            let (column, row) = (place % columns, place / columns);
            Rect::from_min_size(
                area.min + Vec2::new(column as f32 * cell_size.x, row as f32 * cell_size.y),
                cell_size,
            )
            .shrink(inset)
        };
        let mut pick = None;
        let own = main.iter().position(|&index| index == self.active);
        for (place, &index) in main.iter().enumerate() {
            if Some(place) == own {
                continue;
            }
            let rect = cell(place);
            let doc = &self.docs[index];
            let painter = ui.painter().with_clip_rect(rect);
            painter.rect_filled(rect, egui::CornerRadius::ZERO, theme.void);
            let size = cx
                .world
                .arena
                .get_by_id(doc.picture)
                .ok()
                .and_then(picture::size);
            let (pixels, shown, version) = doc.compositor.display();
            if let Some(size) = size.filter(|size| *size == shown) {
                let mut view = doc.view;
                view.fit(rect, size);
                let corners = view.corners(rect, size);
                let compositor = &doc.compositor;
                if let Some(texture) = cx.textures.texture_changed(
                    crate::studio::texture_key(doc.picture, 0),
                    version,
                    size,
                    4,
                    pixels,
                    false,
                    &|since| compositor.changed_since(since),
                ) {
                    painter.add(super::view::picture_quad(texture, corners, 1.0));
                }
            }
            painter.rect_stroke(
                rect,
                egui::CornerRadius::ZERO,
                theme.hairline(),
                egui::StrokeKind::Inside,
            );
            // A cell is a button that shows its canvas: named for the
            // keyboard and a screen reader, ringed while it has the keys.
            let name = self.doc_name(index, cx);
            let response = ui.interact(rect, Id::unique(("sue-paint-tile", index)), Sense::click());
            let label = name.clone();
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label.clone())
            });
            sue_wm::ui::focus_ring(ui, &response, theme.radius(sue_wm::ui::Radius::Sm), theme);
            if sue_wm::ui::tooltip(response, &sue_wm::ui::Tip::new(&name), theme).clicked() {
                pick = Some(index);
            }
        }
        if let Some(index) = pick {
            self.active = index;
        }
        let own = cell(own.unwrap_or_default());
        ui.painter().rect_stroke(
            own,
            egui::CornerRadius::ZERO,
            theme.hairline_active(),
            egui::StrokeKind::Outside,
        );
        own
    }
}

/// An icon button that tells its name and `keys` in Sue's tooltip (and
/// to a screen reader its name), drawn at `rect`.
pub(super) fn keyed_icon_button(
    ui: &Ui,
    rect: Rect,
    salt: &str,
    icon: sue_wm::icons::Icon,
    (name, keys): (&str, &str),
    theme: &Theme,
) -> egui::Response {
    let response = sue_wm::ui::icon_button(salt, icon, "").show_at(ui, rect, theme);
    let label = name.to_owned();
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), label.clone())
    });
    sue_wm::ui::tooltip(response, &sue_wm::ui::Tip::new(name).shortcut(keys), theme)
}
