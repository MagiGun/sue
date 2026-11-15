//! The Material palette and the thumbnail of a material.
//!
//! The palette (gap FG-28 of the 0.0.8 plan) is a tree of folders at the
//! left (All with each kind under it, each with an icon of its own
//! (WS-25), then Favorites and Recent), a search over names and tags, and
//! the materials as small or large thumbnails or as a list. A click picks
//! one, Ctrl or Shift and a click picks several; a double click pastes,
//! and a material dragged onto the canvas lands where it is let go (a
//! picture there, a tone or pattern over the whole canvas). The picked
//! material's tags are typed under the list. Favorites, tags and use are
//! the material library's own (the Hub's Materials place shows the same).

use std::hash::{Hash, Hasher};

use egui::{Id, Modifiers, Pos2, Rect, Sense, StrokeKind, Ui, pos2, vec2};
use sue_wm::icons::{Icon, IconStyle};
use sue_wm::ui::{Density, ListRow, Space, TextRole, Tip, TreeRow};
use sue_wm::{AppletContext, Theme};

use crate::hub::Node;
use crate::materials::{Entry, Material, Placement};
use crate::panels::item_bank::CanvasDrop;
use crate::studio::SuePaint;

/// The commands of the Material palette's menu.
pub(crate) const MENU_COMMANDS: [&str; 6] = [
    "material.paste",
    "material.favorite",
    "material.delete",
    "material.size_small",
    "material.size_large",
    "material.size_list",
];

/// How big the materials show.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ThumbSize {
    /// Small thumbnails, many to a row.
    Small,
    /// Large thumbnails.
    #[default]
    Large,
    /// A list with a small thumbnail each.
    List,
}

impl ThumbSize {
    /// The size a `material.size_<word>` command picks.
    pub(crate) fn of_command(command: &str) -> Option<Self> {
        match command.strip_prefix("material.size_")? {
            "small" => Some(Self::Small),
            "large" => Some(Self::Large),
            "list" => Some(Self::List),
            _ => None,
        }
    }
}

/// What the Material palette shows and has picked.
#[derive(Debug, Default)]
pub(crate) struct MaterialView {
    /// The folder of the tree that is open.
    pub folder: Node,
    /// The search, over names and tags.
    pub search: String,
    /// How big the materials show.
    pub size: ThumbSize,
    /// The materials picked, by library key, in the order picked.
    pub picked: Vec<String>,
    /// The tags of the first one picked, as typed.
    pub tags: Option<(String, String)>,
    /// The tree's All folder is folded.
    pub tree_closed: bool,
}

/// The icon of the kind of material `kind`.
pub(crate) fn kind_icon(kind: &str) -> Icon {
    match kind {
        crate::materials::TONE => Icon::MaterialTone,
        crate::materials::PATTERN => Icon::MaterialPattern,
        crate::materials::IMAGE => Icon::MaterialImage,
        crate::materials::TIP => Icon::MaterialBrushTip,
        crate::materials::POSE => Icon::MaterialPose,
        _ => Icon::Materials,
    }
}

/// Whether `entry` is in `folder` and matches `search` (its name or one
/// of its tags holds it, whatever the case).
pub(crate) fn matches(entry: &Entry, folder: &Node, search: &str) -> bool {
    let in_folder = match folder {
        Node::All => true,
        Node::Kind(kind) => entry.material.kind == *kind,
        Node::Favorite => entry.favorite,
        Node::Recent => entry.used > 0,
    };
    let needle = search.trim().to_lowercase();
    in_folder
        && (needle.is_empty()
            || crate::localize(&entry.material.name)
                .to_lowercase()
                .contains(&needle)
            || entry
                .tags
                .iter()
                .any(|tag| tag.to_lowercase().contains(&needle)))
}

/// A material's thumbnail, `side` square, over white.
pub(crate) fn material_thumb(material: &crate::materials::Material, side: u32) -> Vec<u8> {
    let tile = match material.placement {
        crate::materials::Placement::Fit => crate::materials::layer(material, [side, side], [0; 3]),
        crate::materials::Placement::Cover | crate::materials::Placement::Pose => {
            crate::materials::picture(material, [side, side], [0; 3])
        }
    };
    let mut out = vec![255_u8; (side * side * 4) as usize];
    if let Some(bytes) = tile.as_ref().and_then(|tile| tile.bytes()) {
        for (into, from) in out.chunks_mut(4).zip(bytes.chunks(4)) {
            let alpha = u32::from(from[3]);
            for channel in 0..3 {
                into[channel] =
                    ((u32::from(from[channel]) * alpha + 255 * (255 - alpha)) / 255) as u8;
            }
        }
    }
    out
}

/// The cache key of `material`'s thumbnail.
fn thumb_key(material: &Material) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    material.key().hash(&mut hasher);
    hasher.finish()
}

impl SuePaint {
    /// Every material of the library, as the Hub's Materials place finds
    /// them.
    pub(crate) fn material_entries(&mut self, cx: &AppletContext<'_>) -> Vec<Entry> {
        let folder = crate::materials::location(cx);
        self.hub.library(&self.material_kinds, folder).entries()
    }

    /// The picked materials' library entries, in the order picked.
    fn picked_entries(&mut self, cx: &AppletContext<'_>) -> Vec<Entry> {
        let all = self.material_entries(cx);
        self.extras
            .material
            .picked
            .iter()
            .filter_map(|key| {
                all.iter()
                    .find(|entry| entry.material.key() == *key)
                    .cloned()
            })
            .collect()
    }

    /// Whether the first picked material is a favorite.
    pub(crate) fn material_picked_favorite(&self) -> bool {
        self.extras
            .material
            .picked
            .first()
            .is_some_and(|key| self.extras.material_favorites.contains(key))
    }

    /// Whether every picked material is one people added (and may
    /// delete).
    pub(crate) fn material_picked_removable(&self) -> bool {
        !self.extras.material.picked.is_empty()
            && self
                .extras
                .material
                .picked
                .iter()
                .all(|key| key.starts_with("file:"))
    }

    /// Pastes every picked material onto the canvas, one undo step each.
    pub(crate) fn paste_picked_materials(&mut self, cx: &mut AppletContext<'_>) {
        for entry in self.picked_entries(cx) {
            self.paste_entry(&entry, None, cx);
        }
    }

    /// Puts `entry` on the canvas: a picture with its middle at the picture
    /// point `at` (in the middle without one), a tone or pattern over the
    /// whole canvas, a pose onto the figure; and notes it as used.
    fn paste_entry(&mut self, entry: &Entry, at: Option<[f32; 2]>, cx: &mut AppletContext<'_>) {
        let material = &entry.material;
        if material.placement == Placement::Fit {
            let Some(tile) = crate::materials::picture(material, [512, 512], self.drawing_color())
            else {
                cx.fail(tr!("cannot-read-the-material", name = material.name));
                return;
            };
            let name = crate::localize(&material.name);
            if !self.paste_picture_at(cx, &name, &tile, at) {
                return;
            }
        } else {
            self.paste_material(cx, material);
        }
        let now = sue_platform::time::SystemTime::now()
            .duration_since(sue_platform::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_secs());
        let folder = crate::materials::location(cx);
        let library = self.hub.library(&self.material_kinds, folder);
        if let Err(error) = library.used(entry.id, now) {
            cx.fail(error);
        }
    }

    /// Puts the material whose library key is `key` on the canvas at the
    /// picture point `at`.
    pub(crate) fn drop_material(
        &mut self,
        key: &str,
        at: Option<[f32; 2]>,
        cx: &mut AppletContext<'_>,
    ) {
        if let Some(entry) = self
            .material_entries(cx)
            .into_iter()
            .find(|entry| entry.material.key() == key)
        {
            self.paste_entry(&entry, at, cx);
        }
    }

    /// Makes the picked materials favorites, or not when the first is.
    pub(crate) fn toggle_material_favorite(&mut self, cx: &mut AppletContext<'_>) {
        let picked = self.picked_entries(cx);
        let on = !picked.first().is_some_and(|entry| entry.favorite);
        let folder = crate::materials::location(cx);
        let library = self.hub.library(&self.material_kinds, folder);
        for entry in &picked {
            if let Err(error) = library.set_favorite(entry.id, on) {
                cx.fail(error);
                return;
            }
        }
    }

    /// Deletes the picked materials people added (their files).
    pub(crate) fn delete_picked_materials(&mut self, cx: &mut AppletContext<'_>) {
        for entry in self.picked_entries(cx) {
            if let crate::materials::Source::File(path) = &entry.material.source {
                match sue_platform::fs::remove_file(path) {
                    Ok(()) => cx.notify(tr!("is-no-longer-a-material", name = entry.material.name)),
                    Err(error) => cx.fail(tr!("cannot-delete-the-material", error = error)),
                }
            }
        }
        self.extras.material.picked.clear();
        self.extras.material.tags = None;
        self.hub.materials_changed();
    }

    /// The Material palette.
    pub(crate) fn material_panel(
        &mut self,
        ui: &mut Ui,
        body: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let all = self.material_entries(cx);
        // Edit, Register Material picks the material it made.
        if let Some(name) = self.material_pick.take()
            && let Some(entry) = all.iter().find(|entry| entry.material.name == name)
        {
            self.extras.material.picked = vec![entry.material.key()];
        }
        if self.material_kind != 0 {
            let kinds: Vec<String> = self
                .material_kinds
                .iter()
                .map(|kind| kind.id.clone())
                .collect();
            if let Some(kind) = kinds.get(usize::from(self.material_kind) - 1) {
                self.extras.material.folder = Node::Kind(kind.clone());
            }
            self.material_kind = 0;
        }
        self.extras.material_favorites = all
            .iter()
            .filter(|entry| entry.favorite)
            .map(|entry| entry.material.key())
            .collect();
        let density = Density::of(ui);
        let pad = theme.space(Space::S3);
        let gap = theme.space(Space::S2);
        let control = density.control_height();
        let button = density.icon_button();
        let body = body.shrink(pad);
        // The search and the size buttons.
        let top = Rect::from_min_size(body.min, vec2(body.width(), control));
        let sizes = 3.0 * (button + gap);
        let mut search = std::mem::take(&mut self.extras.material.search);
        {
            let mut inner = crate::panels::child(ui, top, "material-search");
            let hint = tr!("material-search");
            sue_wm::ui::search_field("sue-paint-material-search", &mut search)
                .hint(&hint)
                .label(&hint)
                .show_at(
                    &mut inner,
                    Rect::from_min_max(top.min, pos2(top.right() - sizes, top.bottom())),
                    theme,
                );
        }
        self.extras.material.search = search;
        let buttons = crate::panels::child(ui, top, "material-sizes");
        let mut x = top.right() - sizes + gap;
        for (command, icon) in [
            ("material.size_small", Icon::ViewGridSmall),
            ("material.size_large", Icon::ViewGrid),
            ("material.size_list", Icon::ViewList),
        ] {
            let rect =
                Rect::from_min_size(pos2(x, top.center().y - button / 2.0), vec2(button, button));
            self.command_button(&buttons, rect, command, icon, cx, theme);
            x += button + gap;
        }
        // The bottom: the picked material's tags and its buttons.
        let bottom = Rect::from_min_max(
            pos2(body.left(), body.bottom() - control - gap - button),
            body.max,
        );
        let middle = Rect::from_min_max(
            pos2(body.left(), top.bottom() + gap),
            pos2(body.right(), bottom.top() - gap),
        );
        // The tree of folders, at the left when there is room.
        let tree_width = if middle.width() >= control * 9.0 {
            (middle.width() * 0.38).round()
        } else {
            0.0
        };
        // Without room for the tree, the same folders are a picker above
        // the materials, so Favorites and Recent stay one click away.
        let picker = if tree_width > 0.0 {
            let tree = Rect::from_min_size(middle.min, vec2(tree_width, middle.height()));
            self.material_tree(ui, tree, theme);
            0.0
        } else {
            let rect = Rect::from_min_size(middle.min, vec2(middle.width(), control));
            self.material_folder_picker(ui, rect, theme);
            control + gap
        };
        let grid = Rect::from_min_max(
            pos2(
                middle.left() + tree_width + if tree_width > 0.0 { gap } else { 0.0 },
                middle.top() + picker,
            ),
            middle.max,
        );
        let shown: Vec<Entry> = all
            .iter()
            .filter(|entry| {
                matches(
                    entry,
                    &self.extras.material.folder,
                    &self.extras.material.search,
                )
            })
            .cloned()
            .collect();
        let mut paste = None;
        let mut clicked: Option<(String, Modifiers)> = None;
        {
            let mut inner = crate::panels::child(ui, grid, "materials");
            if shown.is_empty() {
                sue_wm::ui::empty_state("sue-paint-material-empty", &tr!("material-none-here"))
                    .show(&mut inner, theme);
            }
            let size = self.extras.material.size;
            let side = match size {
                ThumbSize::Small => (button * 1.6).round(),
                ThumbSize::Large => (button * 2.6).round(),
                ThumbSize::List => button,
            };
            let picked = self.extras.material.picked.clone();
            let thumbs = &mut self.material_thumbs;
            sue_wm::ui::ScrollView::vertical("sue-paint-materials")
                .on(theme.panel)
                .show(&mut inner, theme, |ui| {
                    let columns = if size == ThumbSize::List {
                        1
                    } else {
                        ((ui.available_width() + gap) / (side + gap))
                            .floor()
                            .max(1.0) as usize
                    };
                    for row in shown.chunks(columns) {
                        ui.horizontal(|ui| {
                            for entry in row {
                                let material = &entry.material;
                                let key = material.key();
                                let name = crate::localize(&material.name);
                                let thumb = thumbs
                                    .entry(thumb_key(material) ^ side as u64)
                                    .or_insert_with(|| material_thumb(material, side as u32));
                                let texture = cx.textures.texture(
                                    thumb_key(material) ^ 0x4d41_5445 ^ side as u64,
                                    1,
                                    [side as u32; 2],
                                    4,
                                    thumb,
                                    false,
                                );
                                let is_picked = picked.contains(&key);
                                let id = Id::unique(("sue-paint-material", &key));
                                let response = ui
                                    .dnd_drag_source(id, CanvasDrop::Material(key.clone()), |ui| {
                                        if size == ThumbSize::List {
                                            let leading = texture.map_or(
                                                sue_wm::ui::Leading::Icon(kind_icon(
                                                    &material.kind,
                                                )),
                                                sue_wm::ui::Leading::Picture,
                                            );
                                            let favorite = tr!("material-favorite");
                                            let note =
                                                if entry.favorite { Some(favorite) } else { None };
                                            let mut row =
                                                ListRow::new(("material-row", &key), &name)
                                                    .leading(leading)
                                                    .selected(is_picked);
                                            if let Some(note) = note.as_deref() {
                                                row = row.note(note);
                                            }
                                            row.show(ui, theme).response.rect
                                        } else {
                                            let (rect, _) = ui.allocate_exact_size(
                                                vec2(side, side),
                                                Sense::hover(),
                                            );
                                            if let Some(texture) = texture {
                                                ui.painter().image(
                                                    texture,
                                                    rect,
                                                    Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                                                    crate::panels::run_button::UNTINTED,
                                                );
                                            }
                                            let corner = theme.corner(sue_wm::ui::Radius::Sm);
                                            let stroke = if is_picked {
                                                theme.selected_stroke()
                                            } else {
                                                theme.hairline()
                                            };
                                            ui.painter().rect_stroke(
                                                rect,
                                                corner,
                                                stroke,
                                                StrokeKind::Outside,
                                            );
                                            if entry.favorite {
                                                let mark = Rect::from_min_size(
                                                    rect.min + vec2(gap, gap),
                                                    vec2(density.icon(), density.icon()) * 0.6,
                                                );
                                                sue_wm::icons::paint_styled(
                                                    ui.painter(),
                                                    mark,
                                                    Icon::Star,
                                                    IconStyle::Solid,
                                                    theme.text,
                                                );
                                            }
                                            rect
                                        }
                                    })
                                    .response;
                                let response = response.interact(Sense::click());
                                let selected = is_picked;
                                response.widget_info(|| {
                                    egui::WidgetInfo::selected(
                                        egui::Role::Button,
                                        true,
                                        selected,
                                        &name,
                                    )
                                });
                                sue_wm::ui::focus_ring(
                                    ui,
                                    &response,
                                    theme.radius(sue_wm::ui::Radius::Sm),
                                    theme,
                                );
                                let help = if entry.tags.is_empty() {
                                    tr!("material-help")
                                } else {
                                    tr!("material-tagged", tags = entry.tags.join(", "))
                                };
                                let response = sue_wm::ui::tooltip(
                                    response,
                                    &Tip::new(&name).help(&help),
                                    theme,
                                );
                                if response.clicked() {
                                    clicked =
                                        Some((key.clone(), ui.input(|input| input.modifiers)));
                                }
                                if response.double_clicked() {
                                    paste = Some(entry.clone());
                                }
                            }
                        });
                    }
                });
        }
        if let Some((key, modifiers)) = clicked {
            let picked = &mut self.extras.material.picked;
            if modifiers.command || modifiers.shift {
                if let Some(at) = picked.iter().position(|known| *known == key) {
                    picked.remove(at);
                } else {
                    picked.push(key);
                }
            } else {
                *picked = vec![key];
            }
            self.extras.material.tags = None;
        }
        // The picked material's tags, as typed.
        let first = self
            .extras
            .material
            .picked
            .first()
            .and_then(|key| all.iter().find(|entry| entry.material.key() == *key))
            .cloned();
        let tags_row = Rect::from_min_size(bottom.min, vec2(bottom.width(), control));
        if let Some(entry) = &first {
            let key = entry.material.key();
            let typed = match self.extras.material.tags.take() {
                Some((of, typed)) if of == key => typed,
                _ => entry.tags.join(", "),
            };
            let mut typed = typed;
            let mut inner = crate::panels::child(ui, tags_row, "material-tags");
            let label = tr!("material-tags");
            let response = sue_wm::ui::text_field("sue-paint-material-tags", &mut typed)
                .hint(&label)
                .label(&label)
                .show_at(&mut inner, tags_row, theme);
            let done = response.lost_focus();
            if done {
                let tags = crate::materials::tags_of(&typed);
                if tags != entry.tags {
                    let folder = crate::materials::location(cx);
                    let library = self.hub.library(&self.material_kinds, folder);
                    if let Err(error) = library.set_tags(entry.id, &tags) {
                        cx.fail(error);
                    }
                }
            } else {
                self.extras.material.tags = Some((key, typed));
            }
        } else {
            ui.painter().text(
                tags_row.left_center(),
                egui::Align2::LEFT_CENTER,
                tr!("material-pick-one"),
                theme.text_font(TextRole::Caption),
                theme.text_muted(),
            );
        }
        let row = Rect::from_min_size(
            pos2(bottom.left(), bottom.bottom() - button),
            vec2(bottom.width(), button),
        );
        let buttons = crate::panels::child(ui, row, "material-buttons");
        let mut x = row.left();
        for (command, icon) in [
            ("material.paste", Icon::Paste),
            ("material.favorite", Icon::Star),
            ("material.delete", Icon::Delete),
        ] {
            let rect = Rect::from_min_size(pos2(x, row.top()), vec2(button, button));
            self.command_button(&buttons, rect, command, icon, cx, theme);
            x += button + gap;
        }
        if let Some(entry) = paste {
            self.paste_entry(&entry, None, cx);
        }
        self.canvas_drops(ui.ctx(), cx);
    }

    /// The folders of the tree as one picker, for a palette too narrow
    /// for the tree: All, each kind, Favorites and Recent.
    fn material_folder_picker(&mut self, ui: &mut Ui, rect: Rect, theme: &Theme) {
        let mut nodes = vec![(Node::All, tr!("all"))];
        nodes.extend(
            self.material_kinds
                .iter()
                .map(|kind| (Node::Kind(kind.id.clone()), crate::localize(&kind.name))),
        );
        nodes.push((Node::Favorite, tr!("material-favorites")));
        nodes.push((Node::Recent, tr!("material-recent")));
        let names: Vec<String> = nodes.iter().map(|(_, name)| name.clone()).collect();
        let view = &mut self.extras.material;
        let mut chosen = nodes
            .iter()
            .position(|(node, _)| *node == view.folder)
            .unwrap_or(0);
        let label = tr!("material-folder");
        let mut inner = crate::panels::child(ui, rect, "material-folder");
        let response = sue_wm::ui::dropdown("sue-paint-material-folder", &mut chosen, &names)
            .label(&label)
            .show_at(&mut inner, rect, theme);
        sue_wm::ui::tooltip(response, &Tip::new(&label), theme);
        if let Some((node, _)) = nodes.get(chosen)
            && *node != view.folder
        {
            view.folder = node.clone();
        }
    }

    /// The tree of folders: All with each kind under it, Favorites and
    /// Recent.
    fn material_tree(&mut self, ui: &mut Ui, rect: Rect, theme: &Theme) {
        let kinds: Vec<(String, String)> = self
            .material_kinds
            .iter()
            .map(|kind| (kind.id.clone(), crate::localize(&kind.name)))
            .collect();
        let mut inner = crate::panels::child(ui, rect, "material-tree");
        let view = &mut self.extras.material;
        let mut open = !view.tree_closed;
        sue_wm::ui::ScrollView::vertical("sue-paint-material-tree")
            .on(theme.panel)
            .show(&mut inner, theme, |ui| {
                let all = tr!("all");
                let output = TreeRow::new("material-all", &all)
                    .icon(Icon::Materials)
                    .expanded(open)
                    .selected(view.folder == Node::All)
                    .show(ui, theme);
                if output.toggled {
                    open = !open;
                }
                if output.response.clicked() {
                    view.folder = Node::All;
                }
                if open {
                    for (id, name) in &kinds {
                        let node = Node::Kind(id.clone());
                        if TreeRow::new(("material-kind", id), name)
                            .depth(1)
                            .icon(kind_icon(id))
                            .selected(view.folder == node)
                            .show(ui, theme)
                            .response
                            .clicked()
                        {
                            view.folder = node;
                        }
                    }
                }
                for (node, label, icon) in [
                    (Node::Favorite, tr!("material-favorites"), Icon::Star),
                    (Node::Recent, tr!("material-recent"), Icon::Recent),
                ] {
                    if TreeRow::new(("material-node", &label), &label)
                        .icon(icon)
                        .selected(view.folder == node)
                        .show(ui, theme)
                        .response
                        .clicked()
                    {
                        view.folder = node;
                    }
                }
            });
        self.extras.material.tree_closed = !open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_search_finds_names_and_tags_in_the_folder_open() {
        let kinds = crate::materials::MaterialKinds::builtin();
        let entries = crate::materials::Library::scan(&kinds, None).entries();
        let tone = entries
            .iter()
            .find(|entry| entry.material.kind == crate::materials::TONE)
            .expect("Sue Paint has tones")
            .clone();
        let mut tagged = tone.clone();
        tagged.tags = vec!["Sky".to_owned(), "clouds".to_owned()];
        assert!(matches(&tagged, &Node::All, "sky"));
        assert!(matches(&tagged, &Node::All, "CLOUD"));
        assert!(!matches(&tone, &Node::All, "sky"));
        assert!(matches(
            &tone,
            &Node::Kind(crate::materials::TONE.to_owned()),
            ""
        ));
        assert!(!matches(
            &tone,
            &Node::Kind(crate::materials::IMAGE.to_owned()),
            ""
        ));
        assert!(!matches(&tone, &Node::Favorite, ""));
        assert_eq!(
            crate::materials::tags_of(" sky, Sky ,, clouds "),
            ["sky", "clouds"]
        );
        assert_eq!(
            ThumbSize::of_command("material.size_list"),
            Some(ThumbSize::List)
        );
        // Every kind Sue Paint has shows an icon of its own.
        let icons: Vec<Icon> = kinds.iter().map(|kind| kind_icon(&kind.id)).collect();
        for (index, icon) in icons.iter().enumerate() {
            assert!(!icons[..index].contains(icon), "{icon:?} twice");
        }
    }
}
