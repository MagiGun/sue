//! Inspector: what the selected canvas entity is, live, and a place to
//! change it. Its name; its place and size on the canvas; its values such
//! as a document's text size or a clip's gain; its text, for documents and
//! code; the projections it carries (Document, Timeline, Audio and so on,
//! all views of the same arena entity); and its metadata, such as a JPEG XL
//! file's size and colors or a song's tags, which can be changed, added and
//! removed. Every change is one undo step.

use std::collections::BTreeMap;

use egui::{Grid, Rect, RichText, ScrollArea, Ui, pos2, vec2};
use sue_core::txn::{Rename, SetMeta};
use sue_core::{EntityId, Projection};
use sue_wm::{
    AppletContext, AppletManifest, AppletRole, ArenaEdit, ArenaText, NodeId, SetNodeProp,
    SetNodeRect, SueApplet, widgets,
};

use crate::util::human_bytes;

/// The applet id.
pub const ID: &str = "inspector";

/// Smallest width or height a card can be given here.
const MIN_SIDE: f32 = 16.0;

/// The inspector and the edits in progress in it.
#[derive(Debug, Default)]
pub struct Inspector {
    /// The name being typed, for its entity.
    name: Option<(EntityId, String)>,
    /// A card's rect before a drag of its numbers began.
    rect_from: Option<(NodeId, Rect)>,
    /// A node value before a drag began: node, key, old value.
    prop_from: Option<(NodeId, String, Option<f64>)>,
    /// Metadata values being typed, by key, for their entity.
    meta: Option<(EntityId, BTreeMap<String, String>)>,
    /// A new metadata entry: key and value.
    new_meta: (String, String),
}

impl Inspector {
    fn name_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>, id: EntityId, name: &str) {
        let theme = cx.theme.clone();
        let mut text = match &self.name {
            Some((editing, text)) if *editing == id => text.clone(),
            _ => name.to_owned(),
        };
        let response = ui.add(
            egui::TextEdit::singleline(&mut text)
                .font(theme.font(theme.heading_size))
                .desired_width(ui.available_width())
                .hint_text("Name"),
        );
        if response.has_focus() || response.changed() {
            self.name = Some((id, text.clone()));
        }
        if response.lost_focus() {
            self.name = None;
            let text = text.trim();
            if !text.is_empty()
                && text != name
                && let Err(error) = cx.apply(ArenaEdit(Rename::new(id, text)))
            {
                cx.fail(format!("Cannot rename it: {error}"));
            }
        }
    }

    /// X, Y, W and H: live while dragged, one undo step when let go.
    fn rect_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>, node_id: NodeId, rect: Rect) {
        let theme = cx.theme.clone();
        let (mut x, mut y, mut w, mut h) = (rect.left(), rect.top(), rect.width(), rect.height());
        let mut changed = false;
        let mut dragging = false;
        let mut finished = false;
        Grid::new("sue-inspector-rect")
            .num_columns(4)
            .spacing([theme.padding, 4.0])
            .show(ui, |ui| {
                for row in [
                    [("X", &mut x), ("Y", &mut y)],
                    [("W", &mut w), ("H", &mut h)],
                ] {
                    for (label, value) in row {
                        ui.label(RichText::new(label).color(theme.muted));
                        let response =
                            ui.add(egui::DragValue::new(value).speed(1.0).max_decimals(0));
                        changed |= response.changed();
                        dragging |= response.dragged();
                        finished |= response.drag_stopped() || response.lost_focus();
                    }
                    ui.end_row();
                }
            });
        let to = Rect::from_min_size(pos2(x, y), vec2(w.max(MIN_SIDE), h.max(MIN_SIDE)));
        if changed && to != rect {
            if self.rect_from.is_none_or(|(id, _)| id != node_id) {
                self.rect_from = Some((node_id, rect));
            }
            if let Some(node) = cx.world.scene.get_mut(node_id) {
                node.rect = to;
            }
        }
        if (finished || (changed && !dragging))
            && let Some((id, from)) = self.rect_from.take()
            && id == node_id
        {
            let now = cx.node(id).map_or(to, |node| node.rect);
            if let Some(node) = cx.world.scene.get_mut(id) {
                node.rect = from;
            }
            if now != from
                && let Err(error) = cx.apply(SetNodeRect::new(id, from, now))
            {
                cx.fail(format!("Cannot move it: {error}"));
            }
        }
    }

    /// The node's numbers (text size, gain, clip ends): live while dragged,
    /// one undo step when let go.
    fn props_ui(
        &mut self,
        ui: &mut Ui,
        cx: &mut AppletContext<'_>,
        node_id: NodeId,
        props: &BTreeMap<String, f64>,
    ) {
        let theme = cx.theme.clone();
        let mut done: Option<String> = None;
        Grid::new("sue-inspector-props")
            .num_columns(2)
            .spacing([theme.padding, 4.0])
            .show(ui, |ui| {
                for (key, value) in props {
                    ui.label(RichText::new(key.replace('_', " ")).color(theme.muted));
                    let mut edited = *value;
                    let speed = if value.abs() < 10.0 { 0.01 } else { 1.0 };
                    let response = ui.add(
                        egui::DragValue::new(&mut edited)
                            .speed(speed)
                            .max_decimals(2),
                    );
                    if response.changed() && (edited - value).abs() > f64::EPSILON {
                        if self
                            .prop_from
                            .as_ref()
                            .is_none_or(|(id, from_key, _)| *id != node_id || from_key != key)
                        {
                            self.prop_from = Some((node_id, key.clone(), Some(*value)));
                        }
                        if let Some(node) = cx.world.scene.get_mut(node_id) {
                            node.props.insert(key.clone(), edited);
                        }
                    }
                    if response.drag_stopped()
                        || response.lost_focus()
                        || (response.changed() && !response.dragged())
                    {
                        done = Some(key.clone());
                    }
                    ui.end_row();
                }
            });
        if let Some(key) = done
            && let Some((id, from_key, from)) = self.prop_from.take()
        {
            if id == node_id && from_key == key {
                let now = cx.node(id).and_then(|node| node.props.get(&key).copied());
                if let Some(node) = cx.world.scene.get_mut(id) {
                    match from {
                        Some(value) => {
                            node.props.insert(key.clone(), value);
                        }
                        None => {
                            node.props.remove(&key);
                        }
                    }
                }
                if now != from
                    && let Err(error) =
                        cx.apply(SetNodeProp::new(id, key.clone(), now, format!("Set {key}")))
                {
                    cx.fail(error.to_string());
                }
            } else {
                self.prop_from = Some((id, from_key, from));
            }
        }
    }

    /// The entity's text, edited in place like the document itself.
    fn text_ui(ui: &mut Ui, cx: &mut AppletContext<'_>, handle: sue_core::AssetHandle) {
        let theme = cx.theme.clone();
        let mut ops = Vec::new();
        if let Ok(entity) = cx.world.arena.get_mut(handle)
            && let Some(buffer) = entity.string_buffer_mut()
        {
            widgets::heading(ui, "Text", &theme);
            ScrollArea::vertical()
                .id_salt("sue-inspector-text")
                .max_height(240.0)
                .show(ui, |ui| {
                    let mut text = ArenaText::new(buffer, &mut ops);
                    egui::TextEdit::multiline(&mut text)
                        .id(egui::Id::unique("sue-inspector-text-edit"))
                        .desired_width(f32::INFINITY)
                        .desired_rows(6)
                        .show(ui);
                });
            ui.add_space(theme.padding);
        }
        if !ops.is_empty() {
            cx.record_typing(handle, ops);
        }
    }

    /// Every metadata entry, editable, with a row to add one.
    fn meta_ui(
        &mut self,
        ui: &mut Ui,
        cx: &mut AppletContext<'_>,
        id: EntityId,
        metadata: &[(String, String)],
    ) {
        let theme = cx.theme.clone();
        if self.meta.as_ref().is_none_or(|(editing, _)| *editing != id) {
            self.meta = Some((id, BTreeMap::new()));
        }
        let mut commit: Option<(String, String)> = None;
        let mut remove: Option<String> = None;
        for (key, value) in metadata {
            ui.horizontal(|ui| {
                ui.label(RichText::new(key).size(theme.small_size).color(theme.muted));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(
                            egui::Button::new(RichText::new("\u{00D7}").color(theme.muted))
                                .frame(false)
                                .small(),
                        )
                        .on_hover_text("Remove this entry")
                        .clicked()
                    {
                        remove = Some(key.clone());
                    }
                });
            });
            let typing = self.meta.as_mut().map(|(_, typing)| typing);
            let Some(typing) = typing else {
                continue;
            };
            let mut text = typing.get(key).cloned().unwrap_or_else(|| value.clone());
            let response = ui.add(
                egui::TextEdit::multiline(&mut text)
                    .desired_rows(1)
                    .desired_width(ui.available_width()),
            );
            if response.changed() {
                typing.insert(key.clone(), text.clone());
            }
            if response.lost_focus() {
                typing.remove(key);
                if text != *value {
                    commit = Some((key.clone(), text));
                }
            }
            ui.add_space(4.0);
        }
        ui.horizontal(|ui| {
            let (key, value) = &mut self.new_meta;
            ui.add(
                egui::TextEdit::singleline(key)
                    .hint_text("New entry")
                    .desired_width(90.0),
            );
            ui.add(
                egui::TextEdit::singleline(value)
                    .hint_text("Value")
                    .desired_width((ui.available_width() - 44.0).max(60.0)),
            );
            if widgets::pill_button(ui, "Add", false, &theme).clicked() && !key.trim().is_empty() {
                commit = Some((key.trim().to_owned(), value.clone()));
                key.clear();
                value.clear();
            }
        });
        if let Some((key, value)) = commit
            && let Err(error) = cx.apply(ArenaEdit(SetMeta::new(id, key, value)))
        {
            cx.fail(error.to_string());
        }
        if let Some(key) = remove
            && let Err(error) = cx.apply(ArenaEdit(SetMeta::remove(id, key)))
        {
            cx.fail(error.to_string());
        }
    }
}

impl SueApplet for Inspector {
    fn manifest(&self) -> AppletManifest {
        let mut manifest =
            AppletManifest::new(ID, "Inspector", AppletRole::RightDrawer, "\u{2139}");
        manifest.summary =
            "Facts about the selected entity, and a place to change them.".to_owned();
        manifest.order = 10;
        manifest
    }

    fn drawer_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        let theme = cx.theme.clone();
        ui.set_max_width(ui.available_width());
        let Some(node_id) = cx.view.selected else {
            ui.label(RichText::new("Select an entity on the canvas.").color(theme.muted));
            return;
        };
        let Some(node) = cx.node(node_id).cloned() else {
            return;
        };
        let Ok(entity) = cx.arena().get(node.entity) else {
            return;
        };
        let (id, name) = (entity.id(), entity.name().to_owned());
        let heap = entity.heap_bytes();
        let projections = entity.projections();
        let has_text = entity.string_buffer().is_some();
        let metadata: Vec<(String, String)> = entity
            .metadata()
            .map(|(key, value)| (key.to_owned(), value.to_owned()))
            .collect();

        self.name_ui(ui, cx, id, &name);
        ui.label(
            RichText::new(format!("Entity {}  |  node {}", node.entity, node_id.0))
                .size(theme.small_size)
                .color(theme.muted),
        );
        ui.add_space(theme.padding);
        widgets::heading(ui, "On the canvas", &theme);
        self.rect_ui(ui, cx, node_id, node.rect);
        if !node.props.is_empty() {
            let props: BTreeMap<String, f64> = node
                .props
                .iter()
                .map(|(key, value)| (key.clone(), *value))
                .collect();
            self.props_ui(ui, cx, node_id, &props);
        }
        ui.add_space(theme.padding);
        if has_text {
            Self::text_ui(ui, cx, node.entity);
        }
        widgets::heading(ui, "Projections", &theme);
        ui.horizontal_wrapped(|ui| {
            for projection in Projection::ALL {
                let on = projections.contains(projection);
                ui.label(
                    RichText::new(projection.name())
                        .color(if on {
                            theme.text
                        } else {
                            theme.muted.gamma_multiply(0.6)
                        })
                        .size(theme.small_size),
                );
            }
        });
        ui.label(
            RichText::new(format!("Holds {} in memory", human_bytes(heap as u64)))
                .size(theme.small_size)
                .color(theme.muted),
        );
        ui.add_space(theme.padding);
        widgets::heading(ui, "Metadata", &theme);
        if metadata.is_empty() {
            ui.label(RichText::new("None yet").color(theme.muted));
        }
        self.meta_ui(ui, cx, id, &metadata);
    }
}
