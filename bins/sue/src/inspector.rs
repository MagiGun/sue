//! Inspector: what the selected canvas entity is, live. Its place and size
//! on the canvas, the projections it carries (Document, Timeline, Audio and
//! so on, all views of the same arena entity), and its metadata, such as a
//! JPEG XL file's size, color and codestream facts.

use egui::{Grid, RichText, Ui};
use sue_core::Projection;
use sue_wm::{AppletContext, AppletManifest, AppletRole, SueApplet, widgets};

use crate::util::human_bytes;

/// The applet id.
pub const ID: &str = "inspector";

/// The inspector.
#[derive(Debug, Default)]
pub struct Inspector;

impl SueApplet for Inspector {
    fn manifest(&self) -> AppletManifest {
        let mut manifest =
            AppletManifest::new(ID, "Inspector", AppletRole::RightDrawer, "\u{2139}");
        manifest.summary = "Facts about the selected entity.".to_owned();
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
        ui.label(
            RichText::new(entity.name())
                .size(theme.heading_size)
                .color(theme.text),
        );
        ui.label(
            RichText::new(format!("Entity {}  |  node {}", node.entity, node_id.0))
                .size(theme.small_size)
                .color(theme.muted),
        );
        ui.add_space(theme.padding);
        widgets::heading(ui, "On the canvas", &theme);
        Grid::new("sue-inspector-rect")
            .num_columns(4)
            .spacing([theme.padding, 4.0])
            .show(ui, |ui| {
                for row in [
                    [("X", node.rect.left()), ("Y", node.rect.top())],
                    [("W", node.rect.width()), ("H", node.rect.height())],
                ] {
                    for (label, value) in row {
                        ui.label(RichText::new(label).color(theme.muted));
                        ui.label(
                            RichText::new(format!("{value:.0}")).font(theme.mono(theme.ui_size)),
                        );
                    }
                    ui.end_row();
                }
            });
        if !node.props.is_empty() {
            Grid::new("sue-inspector-props")
                .num_columns(2)
                .spacing([theme.padding, 4.0])
                .show(ui, |ui| {
                    for (key, value) in &node.props {
                        ui.label(RichText::new(key).color(theme.muted));
                        ui.label(
                            RichText::new(format!("{value:.2}")).font(theme.mono(theme.ui_size)),
                        );
                        ui.end_row();
                    }
                });
        }
        ui.add_space(theme.padding);
        widgets::heading(ui, "Projections", &theme);
        let projections = entity.projections();
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
            RichText::new(format!(
                "Holds {} in memory",
                human_bytes(entity.heap_bytes() as u64)
            ))
            .size(theme.small_size)
            .color(theme.muted),
        );
        ui.add_space(theme.padding);
        widgets::heading(ui, "Metadata", &theme);
        let metadata: Vec<(String, String)> = entity
            .metadata()
            .map(|(key, value)| (key.to_owned(), value.to_owned()))
            .collect();
        if metadata.is_empty() {
            ui.label(RichText::new("None").color(theme.muted));
        }
        for (key, value) in metadata {
            ui.label(RichText::new(key).size(theme.small_size).color(theme.muted));
            ui.add(egui::Label::new(RichText::new(value).color(theme.text)).wrap());
            ui.add_space(4.0);
        }
    }
}
