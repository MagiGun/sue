//! Document: a text entity on the canvas. The text lives in the arena as a
//! `StringBuffer`; the node edits it in place with an `egui::TextEdit`
//! (selection, double-click words, `Ctrl+A`, `Ctrl+C`, `Ctrl+X`, `Ctrl+V`),
//! and every burst of typing becomes one step of the global undo tree.
//! Styling is Markdown-like and stays in the text: `# ` and `## ` start
//! headings, `**bold**` and `` `monospace` `` mark spans.

use std::sync::Arc;

use egui::text::{CCursorRange, LayoutJob, TextFormat};
use egui::{Color32, FontFamily, FontId, Galley, TextBuffer, Ui};
use sue_core::txn::TextOp;
use sue_core::{ComponentKind, Entity, StringBuffer};
use sue_wm::{
    AppletContext, AppletManifest, AppletRole, ArenaText, MediaClass, ModeSpec, NewEntity, NodeId,
    SceneNode, SetNodeProp, SueApplet, Theme, World, project_to_timeline, widgets,
};

/// The applet id.
pub const ID: &str = "document";

/// Metadata key naming what a text entity is.
pub const KIND: &str = "kind";

/// The [`KIND`] of source code, which Code / Hex shows instead.
pub const KIND_CODE: &str = "code";

/// The egui id of a node's text field.
#[must_use]
pub fn text_id(node: NodeId) -> egui::Id {
    egui::Id::unique(("sue-document-text", node.0))
}

/// Lays out Markdown-like text without hiding any character, so the text
/// field's cursor positions stay exact.
#[must_use]
pub fn markdown_job(text: &str, size: f32, wrap: f32, theme: &Theme) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap;
    let body = theme.text.gamma_multiply(0.86);
    let marker = theme.muted;
    for line in text.split_inclusive('\n') {
        let (heading, scale) = if line.starts_with("## ") {
            (3, 1.35)
        } else if line.starts_with("# ") {
            (2, 1.7)
        } else {
            (0, 1.0)
        };
        let font = FontId::new(size * scale, FontFamily::Proportional);
        let format = |color: Color32, font: FontId| TextFormat {
            font_id: font,
            color,
            ..TextFormat::default()
        };
        if heading > 0 {
            job.append(&line[..heading], 0.0, format(marker, font.clone()));
            job.append(&line[heading..], 0.0, format(theme.text, font));
            continue;
        }
        // Inline spans: **bold** and `code`.
        let mut rest = line;
        while !rest.is_empty() {
            let bold = rest.find("**");
            let code = rest.find('`');
            match (bold, code) {
                (Some(b), c) if c.is_none_or(|c| b < c) => {
                    job.append(&rest[..b], 0.0, format(body, font.clone()));
                    let after = &rest[b + 2..];
                    match after.find("**") {
                        Some(end) => {
                            job.append("**", 0.0, format(marker, font.clone()));
                            let mut strong = format(theme.text, font.clone());
                            strong.extra_letter_spacing = size * 0.04;
                            job.append(&after[..end], 0.0, strong);
                            job.append("**", 0.0, format(marker, font.clone()));
                            rest = &after[end + 2..];
                        }
                        None => {
                            job.append(&rest[b..], 0.0, format(body, font.clone()));
                            rest = "";
                        }
                    }
                }
                (_, Some(c)) => {
                    job.append(&rest[..c], 0.0, format(body, font.clone()));
                    let after = &rest[c + 1..];
                    match after.find('`') {
                        Some(end) => {
                            let mono = FontId::new(size * 0.92, FontFamily::Monospace);
                            job.append("`", 0.0, format(marker, mono.clone()));
                            let mut code = format(theme.text, mono.clone());
                            code.background = theme.hover;
                            job.append(&after[..end], 0.0, code);
                            job.append("`", 0.0, format(marker, mono));
                            rest = &after[end + 1..];
                        }
                        None => {
                            job.append(&rest[c..], 0.0, format(body, font.clone()));
                            rest = "";
                        }
                    }
                }
                _ => {
                    job.append(rest, 0.0, format(body, font.clone()));
                    rest = "";
                }
            }
        }
    }
    if text.is_empty() {
        job.append(
            "",
            0.0,
            TextFormat::simple(FontId::proportional(size), body),
        );
    }
    job
}

/// Selects characters `from` to `to` of `node`s text field and focuses it
/// (scripted runs and snapshots).
pub fn select_words(ctx: &egui::Context, node: NodeId, from: usize, to: usize) {
    let id = text_id(node);
    let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
    state.cursor.set_char_range(Some(CCursorRange::two(
        egui::text::CCursor::new(from),
        egui::text::CCursor::new(to),
    )));
    state.store(ctx, id);
    ctx.memory_mut(|memory| memory.request_focus(id));
}

/// Runs `edit` on `node`'s text as one undo step.
fn edit_text(cx: &mut AppletContext<'_>, node: NodeId, edit: impl FnOnce(&mut ArenaText<'_>)) {
    let Some(handle) = cx.node(node).map(|node| node.entity) else {
        return;
    };
    let mut ops: Vec<TextOp> = Vec::new();
    if let Ok(entity) = cx.world.arena.get_mut(handle)
        && let Some(buffer) = entity.string_buffer_mut()
    {
        edit(&mut ArenaText::new(buffer, &mut ops));
    }
    if !ops.is_empty() {
        cx.typing.flush(cx.history);
        let mut group = sue_core::txn::TextEdits::new(handle);
        for op in ops {
            group.push(op);
        }
        cx.history.record(sue_wm::ArenaEdit(group));
    }
}

/// The selected character range of `node`'s text field, or the caret.
fn selection(ui: &Ui, node: NodeId) -> Option<CCursorRange> {
    egui::TextEdit::load_state(ui.ctx(), text_id(node)).and_then(|state| state.cursor.char_range())
}

/// Text documents on the canvas.
#[derive(Debug, Default)]
pub struct Document;

impl Document {
    fn font_size(node: &SceneNode, theme: &Theme) -> f32 {
        node.prop("font_size", f64::from(theme.document_size)) as f32
    }

    /// Toggles a heading prefix on the caret's line.
    fn heading(ui: &Ui, cx: &mut AppletContext<'_>, node: NodeId, prefix: &str) {
        let caret = selection(ui, node).map_or(0, |range| range.primary.index.0);
        edit_text(cx, node, |text| {
            let (start_char, existing) = {
                let content = text.as_str();
                let caret_byte = content
                    .char_indices()
                    .nth(caret)
                    .map_or(content.len(), |(byte, _)| byte);
                let line_start = content[..caret_byte].rfind('\n').map_or(0, |at| at + 1);
                let line = &content[line_start..];
                let existing = if line.starts_with("## ") {
                    3
                } else if line.starts_with("# ") {
                    2
                } else {
                    0
                };
                (content[..line_start].chars().count(), existing)
            };
            if existing > 0 {
                text.delete_char_range(
                    egui::text::CharIndex(start_char)..egui::text::CharIndex(start_char + existing),
                );
            }
            if existing != prefix.len() {
                text.insert_text(prefix, egui::text::CharIndex(start_char));
            }
        });
    }

    /// Wraps the selection in `marker` (or inserts a pair at the caret).
    fn wrap(ui: &Ui, cx: &mut AppletContext<'_>, node: NodeId, marker: &str) {
        let Some(range) = selection(ui, node) else {
            return;
        };
        let [start, end] = range.sorted_cursors();
        edit_text(cx, node, |text| {
            text.insert_text(marker, end.index);
            text.insert_text(marker, start.index);
        });
    }
}

/// The kinds of new documents: key, label, name and starting text.
const TEMPLATES: [(&str, &str, &str, &str); 5] = [
    ("blank", "Blank", "Untitled", "# Untitled\n\n"),
    ("notes", "Notes", "Notes", "# Notes\n\n- \n"),
    ("todo", "To-Do List", "To Do", "# To do\n\n- [ ] \n- [ ] \n"),
    (
        "letter",
        "Letter",
        "Letter",
        "Your Name\nYour Address\n\nDate\n\nDear ,\n\n\n\nKind regards,\n\nYour Name\n",
    ),
    (
        "report",
        "Report",
        "Report",
        "# Title\n\n## Summary\n\n\n## Background\n\n\n## Findings\n\n\n## Next steps\n\n",
    ),
];

impl SueApplet for Document {
    fn manifest(&self) -> AppletManifest {
        let mut manifest = AppletManifest::new(ID, "Document", AppletRole::Node, "\u{1F4C4}");
        manifest.summary =
            "Write with headings, bold and code spans; every edit can be undone.".to_owned();
        manifest.order = 10;
        manifest.opens = vec![MediaClass::Text];
        manifest.mode = Some(ModeSpec {
            label: "Document".to_owned(),
            in_pill: true,
        });
        manifest.lane = Some("Document Track".to_owned());
        manifest
    }

    fn accepts(&self, entity: &Entity) -> bool {
        entity.base_kind() == Some(ComponentKind::StringBuffer)
            && entity.meta(KIND) != Some(KIND_CODE)
    }

    fn node_size(&self, _entity: &Entity) -> egui::Vec2 {
        egui::vec2(560.0, 420.0)
    }

    fn new_entity(&mut self, cx: &mut AppletContext<'_>) -> Option<NewEntity> {
        self.create("blank", cx)
    }

    fn templates(&self) -> Vec<sue_wm::FileTemplate> {
        TEMPLATES
            .iter()
            .map(|(key, label, _, _)| sue_wm::FileTemplate::new("Document", *label, *key))
            .collect()
    }

    fn create(&mut self, key: &str, cx: &mut AppletContext<'_>) -> Option<NewEntity> {
        let (_, _, name, text) = TEMPLATES.iter().find(|(id, ..)| *id == key)?;
        Some(NewEntity {
            name: (*name).to_owned(),
            base: StringBuffer::from((*text).to_owned()).into(),
            meta: Vec::new(),
            size: egui::vec2(560.0, 420.0),
            props: vec![("font_size".to_owned(), f64::from(cx.theme.document_size))],
        })
    }

    fn node_ui(&mut self, ui: &mut Ui, node: NodeId, cx: &mut AppletContext<'_>) {
        let Some(scene_node) = cx.node(node).cloned() else {
            return;
        };
        let size = Self::font_size(&scene_node, cx.theme);
        let theme = cx.theme.clone();
        let mut ops = Vec::new();
        let area = ui.max_rect().shrink(14.0);
        if let Ok(entity) = cx.world.arena.get_mut(scene_node.entity)
            && let Some(buffer) = entity.string_buffer_mut()
        {
            let mut text = ArenaText::new(buffer, &mut ops);
            let mut layouter = |ui: &Ui, buffer: &dyn TextBuffer, wrap: f32| -> Arc<Galley> {
                let job = markdown_job(buffer.as_str(), size, wrap, &theme);
                ui.fonts_mut(|fonts| fonts.layout_job(job))
            };
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(area));
            egui::TextEdit::multiline(&mut text)
                .id(text_id(node))
                .frame(egui::Frame::NONE)
                .desired_width(area.width())
                .desired_rows(((area.height() / (size * 1.4)).floor() as usize).max(1))
                .lock_focus(true)
                .layouter(&mut layouter)
                .show(&mut child);
        }
        if !ops.is_empty() {
            cx.record_typing(scene_node.entity, ops);
        }
    }

    fn pill_ui(&mut self, ui: &mut Ui, node: NodeId, cx: &mut AppletContext<'_>) {
        let theme = cx.theme.clone();
        if widgets::pill_button(ui, "H1", false, &theme)
            .on_hover_text("Heading 1 on the caret's line")
            .clicked()
        {
            Self::heading(ui, cx, node, "# ");
        }
        if widgets::pill_button(ui, "H2", false, &theme)
            .on_hover_text("Heading 2 on the caret's line")
            .clicked()
        {
            Self::heading(ui, cx, node, "## ");
        }
        if widgets::pill_button(ui, "Bold", false, &theme)
            .on_hover_text("Wrap the selection in **")
            .clicked()
        {
            Self::wrap(ui, cx, node, "**");
        }
        if widgets::pill_button(ui, "Monospace", false, &theme)
            .on_hover_text("Wrap the selection in backticks")
            .clicked()
        {
            Self::wrap(ui, cx, node, "`");
        }
        widgets::pill_divider(ui, &theme);
        let size = cx.node(node).map_or(f64::from(theme.document_size), |n| {
            n.prop("font_size", f64::from(theme.document_size))
        });
        for (label, step, hint) in [("A-", -1.0, "Smaller text"), ("A+", 1.0, "Larger text")] {
            if widgets::pill_button(ui, label, false, &theme)
                .on_hover_text(hint)
                .clicked()
            {
                let next = (size + step).clamp(8.0, 72.0);
                let result = cx.apply(SetNodeProp::new(node, "font_size", Some(next), "Text size"));
                if let Err(error) = result {
                    cx.fail(error.to_string());
                }
            }
        }
        ui.label(
            egui::RichText::new(format!("{size:.0} px"))
                .size(theme.small_size)
                .color(theme.muted),
        );
        widgets::pill_divider(ui, &theme);
        if widgets::pill_button(ui, "Project to Timeline", false, &theme).clicked() {
            project_to_timeline(cx, node, 4.0);
        }
    }

    fn duration(&self, node: &SceneNode, _world: &World) -> f64 {
        node.prop("dur", 4.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_layout_keeps_every_character() {
        let theme = Theme::from_rice(&sue_rice::RiceSchema::new());
        let text = "# Title\nSome **bold** and `code`.\n## Sub\n";
        let job = markdown_job(text, 15.0, 400.0, &theme);
        assert_eq!(job.text, text);
        let unclosed = "a ** b ` c";
        assert_eq!(markdown_job(unclosed, 15.0, 400.0, &theme).text, unclosed);
    }
}
