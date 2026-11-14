//! The comments panel.

use egui::{RichText, Ui};
use sue_wm::{AppletContext, Theme};

use crate::studio::SueWriter;

use super::After;

impl SueWriter {
    /// A text property edited in place, committed when the field lets go.
    /// The comments of what is shown, as threads: the words each is about
    /// (a press selects them), what was said, a reply box, Resolve and
    /// Delete.
    pub(super) fn comments_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>, theme: &Theme) {
        let comments = self.editor.comments();
        if comments.is_empty() {
            ui.label(
                RichText::new(tr!("no-comments-select-some-words-choose"))
                    .size(theme.small_size)
                    .color(theme.muted),
            );
            return;
        }
        let mut act: Option<After> = None;
        for (doc, range, id) in comments {
            let Some(thread) = sue_model::comments::thread(cx.arena(), id) else {
                continue;
            };
            if thread.resolved && !self.show_resolved {
                continue;
            }
            let Some(doc_id) = self.editor.docs().get(doc).copied() else {
                continue;
            };
            let words: String = sue_type::edit::text_of(cx.arena(), doc_id)
                .and_then(|text| text.get(range.clone()))
                .unwrap_or("")
                .chars()
                .take(80)
                .collect();
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                let quote = RichText::new(format!("\u{201C}{}\u{201D}", words.trim()))
                    .italics()
                    .color(theme.muted);
                if ui
                    .add(egui::Label::new(quote).sense(egui::Sense::click()))
                    .clicked()
                {
                    let range = range.clone();
                    act = Some(Box::new(move |me: &mut Self, _| {
                        me.editor.select(doc_id, range);
                        me.editor.focus();
                    }));
                }
                for note in std::iter::once(&thread.first).chain(&thread.replies) {
                    ui.label(
                        RichText::new(format!("{}, {}", note.author, note.date))
                            .size(theme.small_size)
                            .strong(),
                    );
                    ui.label(&note.text);
                }
                let draft = self.drafts.entry((id, "reply")).or_default();
                let response = ui.add(
                    egui::TextEdit::singleline(draft)
                        .hint_text(tr!("reply"))
                        .desired_width(f32::INFINITY),
                );
                let send =
                    response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                ui.horizontal(|ui| {
                    if (ui.small_button(tr!("reply")).clicked() || send) && !draft.trim().is_empty()
                    {
                        let text = std::mem::take(draft).trim().to_owned();
                        act = Some(Box::new(move |me: &mut Self, cx| me.reply(cx, id, &text)));
                    }
                    let resolve = if thread.resolved {
                        tr!("reopen")
                    } else {
                        tr!("resolve")
                    };
                    if ui.small_button(resolve).clicked() {
                        act = Some(Box::new(move |me: &mut Self, cx| me.resolve(cx, id)));
                    }
                    if ui.small_button(tr!("delete")).clicked() {
                        act = Some(Box::new(move |me: &mut Self, cx| {
                            me.editor.remove_comment(cx, id);
                        }));
                    }
                });
            });
            ui.add_space(6.0);
        }
        ui.checkbox(&mut self.show_resolved, tr!("show-resolved-comments"));
        if let Some(act) = act {
            act(self, cx);
        }
    }
}
