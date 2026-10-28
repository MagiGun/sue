//! Code / Hex: source code and raw bytes on the canvas. Code entities are
//! `StringBuffer`s edited in place in a monospace `egui::TextEdit` (undo
//! through the global tree). Byte entities (the byte map of an unknown file)
//! show a hex dump in a read-only text field, so any part can be selected
//! and copied.

use std::collections::HashMap;

use egui::{FontId, Ui, vec2};
use sue_core::{AssetHandle, ComponentKind, Entity, StringBuffer};
use sue_io::meta;
use sue_wm::{
    AppletContext, AppletManifest, AppletRole, ArenaText, MediaClass, ModeSpec, NewEntity, NodeId,
    SueApplet, widgets,
};

use crate::document::{KIND, KIND_CODE};
use crate::util::human_bytes;

/// The applet id.
pub const ID: &str = "hex";

/// Most bytes a hex dump shows.
const DUMP_LIMIT: usize = 64 * 1024;

/// `offset  hex bytes  ascii` lines, 16 bytes each.
#[must_use]
pub fn hex_dump(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().min(DUMP_LIMIT) * 4 + 64);
    for (row, chunk) in bytes.chunks(16).take(DUMP_LIMIT / 16).enumerate() {
        out.push_str(&format!("{:08x}  ", row * 16));
        for column in 0..16 {
            match chunk.get(column) {
                Some(byte) => out.push_str(&format!("{byte:02x} ")),
                None => out.push_str("   "),
            }
            if column == 7 {
                out.push(' ');
            }
        }
        out.push(' ');
        out.extend(chunk.iter().map(|&byte| {
            if byte.is_ascii_graphic() || byte == b' ' {
                char::from(byte)
            } else {
                '.'
            }
        }));
        out.push('\n');
    }
    if bytes.len() > DUMP_LIMIT {
        out.push_str(&format!("... {} more bytes\n", bytes.len() - DUMP_LIMIT));
    }
    out
}

/// The bytes of a byte-map entity, without its padding.
fn bytes_of(entity: &Entity) -> Option<&[u8]> {
    let bytes = entity
        .component(ComponentKind::RasterTile2D)?
        .as_raster()?
        .bytes()?;
    let length = entity
        .meta(meta::BYTES)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(bytes.len())
        .min(bytes.len());
    Some(&bytes[..length])
}

/// Code and bytes on the canvas.
#[derive(Debug, Default)]
pub struct Hex {
    dumps: HashMap<AssetHandle, String>,
}

/// The kinds of new code files: key, label, file name and starting text.
const CODE: [(&str, &str, &str, &str); 7] = [
    (
        "rust",
        "Rust",
        "main.rs",
        "// A scratch file. Every edit can be undone.\nfn main() {\n    println!(\"Sue\");\n}\n",
    ),
    (
        "python",
        "Python",
        "main.py",
        "def main():\n    print(\"Sue\")\n\n\nif __name__ == \"__main__\":\n    main()\n",
    ),
    (
        "javascript",
        "JavaScript",
        "main.js",
        "function main() {\n  console.log(\"Sue\");\n}\n\nmain();\n",
    ),
    (
        "html",
        "HTML",
        "index.html",
        "<!doctype html>\n<html lang=\"en\">\n<head>\n  <meta charset=\"utf-8\">\n  <title>Sue</title>\n</head>\n<body>\n  <h1>Sue</h1>\n</body>\n</html>\n",
    ),
    (
        "css",
        "CSS",
        "style.css",
        "body {\n  margin: 0;\n  font-family: sans-serif;\n}\n",
    ),
    ("json", "JSON", "data.json", "{\n  \"name\": \"Sue\"\n}\n"),
    (
        "shell",
        "Shell Script",
        "script.sh",
        "#!/bin/sh\necho \"Sue\"\n",
    ),
];

impl SueApplet for Hex {
    fn manifest(&self) -> AppletManifest {
        let mut manifest = AppletManifest::new(ID, "Code / Hex", AppletRole::Node, "\u{2328}");
        manifest.summary = "Edit code, or read any file byte by byte.".to_owned();
        manifest.order = 50;
        manifest.opens = vec![MediaClass::Binary];
        manifest.mode = Some(ModeSpec {
            label: "Code / Hex".to_owned(),
            in_pill: false,
        });
        manifest
    }

    fn accepts(&self, entity: &Entity) -> bool {
        entity.meta(meta::ENTROPY).is_some()
            || (entity.base_kind() == Some(ComponentKind::StringBuffer)
                && entity.meta(KIND) == Some(KIND_CODE))
    }

    fn node_size(&self, _entity: &Entity) -> egui::Vec2 {
        vec2(620.0, 460.0)
    }

    fn new_entity(&mut self, cx: &mut AppletContext<'_>) -> Option<NewEntity> {
        self.create("rust", cx)
    }

    fn templates(&self) -> Vec<sue_wm::FileTemplate> {
        CODE.iter()
            .map(|(key, label, _, _)| sue_wm::FileTemplate::new("Code", *label, *key))
            .collect()
    }

    fn create(&mut self, key: &str, _cx: &mut AppletContext<'_>) -> Option<NewEntity> {
        let (_, _, name, text) = CODE.iter().find(|(id, ..)| *id == key)?;
        Some(NewEntity {
            name: (*name).to_owned(),
            base: StringBuffer::from((*text).to_owned()).into(),
            meta: vec![(KIND.to_owned(), KIND_CODE.to_owned())],
            size: vec2(620.0, 360.0),
            props: Vec::new(),
        })
    }

    fn node_ui(&mut self, ui: &mut Ui, node: NodeId, cx: &mut AppletContext<'_>) {
        let Some(handle) = cx.node(node).map(|n| n.entity) else {
            return;
        };
        let area = ui.max_rect().shrink(12.0);
        let font = FontId::monospace(cx.theme.mono_size);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area));
        let rows = ((area.height() / (cx.theme.mono_size * 1.35)).floor() as usize).max(1);
        let mut ops = Vec::new();
        let Ok(entity) = cx.world.arena.get_mut(handle) else {
            return;
        };
        if entity.base_kind() == Some(ComponentKind::StringBuffer) {
            if let Some(buffer) = entity.string_buffer_mut() {
                let mut text = ArenaText::new(buffer, &mut ops);
                egui::TextEdit::multiline(&mut text)
                    .id(egui::Id::unique(("sue-code", node.0)))
                    .code_editor()
                    .font(font)
                    .frame(egui::Frame::NONE)
                    .desired_width(area.width())
                    .desired_rows(rows)
                    .lock_focus(true)
                    .show(&mut child);
            }
        } else if let Some(bytes) = bytes_of(entity) {
            let dump = self.dumps.entry(handle).or_insert_with(|| hex_dump(bytes));
            let mut view: &str = dump.as_str();
            egui::TextEdit::multiline(&mut view)
                .id(egui::Id::unique(("sue-hex", node.0)))
                .font(font)
                .frame(egui::Frame::NONE)
                .desired_width(area.width())
                .desired_rows(rows)
                .show(&mut child);
        }
        if !ops.is_empty() {
            cx.record_typing(handle, ops);
        }
    }

    fn pill_ui(&mut self, ui: &mut Ui, node: NodeId, cx: &mut AppletContext<'_>) {
        let theme = cx.theme.clone();
        let Some(entity) = cx.entity(node) else {
            return;
        };
        let (summary, copy) = if let Some(text) = entity.string_buffer() {
            (
                format!("{} lines", text.as_str().lines().count()),
                text.as_str().to_owned(),
            )
        } else {
            let length = bytes_of(entity).map_or(0, <[u8]>::len);
            let entropy = entity.meta(meta::ENTROPY).unwrap_or("?").to_owned();
            (
                format!(
                    "{}, entropy {entropy} bits per byte",
                    human_bytes(length as u64)
                ),
                self.dumps
                    .get(&entity_handle(cx, node))
                    .cloned()
                    .unwrap_or_default(),
            )
        };
        ui.label(
            egui::RichText::new(summary)
                .size(theme.small_size)
                .color(theme.muted),
        );
        widgets::pill_divider(ui, &theme);
        if widgets::pill_button(ui, "Copy All", false, &theme).clicked() {
            match sue_platform::copy_text(&copy) {
                Ok(()) => cx.notify("Copied to the clipboard"),
                Err(error) => cx.fail(format!("The clipboard is unavailable: {error}")),
            }
        }
    }
}

fn entity_handle(cx: &AppletContext<'_>, node: NodeId) -> AssetHandle {
    cx.node(node).map_or(AssetHandle::NULL, |node| node.entity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dumps_show_offsets_hex_and_ascii() {
        let dump = hex_dump(b"Sue\x00\x01 jack of all trades");
        let first = dump.lines().next().unwrap();
        assert!(first.starts_with("00000000  53 75 65 00 01 20"));
        assert!(first.ends_with("Sue.. jack of al"));
        assert_eq!(dump.lines().count(), 2);
    }
}
