//! The comments panel.

use egui::{Key, RichText, Ui};
use sue_wm::icons::Icon;
use sue_wm::ui::{self, Space, TextRole, Tip};
use sue_wm::{AppletContext, Theme};

use crate::studio::SueWriter;

use super::After;
use super::binder::{key_text, keys_of, tool_button};

/// How much of the commented words a thread quotes.
const QUOTE: usize = 80;

impl SueWriter {
    /// The comments of what is shown, as threads: the words each is about
    /// (a press selects them), what was said, a reply box, Resolve and
    /// Delete.
    pub(super) fn comments_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>, theme: &Theme) {
        let comments = self.editor.comments();
        if comments.is_empty() {
            ui::empty_state("writer-comments-empty", &tr!("no-comments"))
                .icon(Icon::Comment)
                .detail(&tr!("comments-empty-detail"))
                .show(ui, theme);
            return;
        }
        let mut act: Option<After> = None;
        let gap = theme.space(Space::S3);
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
                .take(QUOTE)
                .collect();
            ui::Card::new().padding(Space::S4).show(ui, theme, |ui| {
                ui.set_width(ui.available_width());
                // The words it is about: a press selects them in the text.
                let quote = format!("\u{201C}{}\u{201D}", words.trim());
                let response = ui::link(("writer-comment-quote", id), &quote)
                    .role(TextRole::Caption)
                    .show(ui, theme);
                let go = tr!("comments-go-to-words");
                if ui::tooltip(response, &Tip::new(&go), theme).clicked() {
                    let range = range.clone();
                    act = Some(Box::new(move |me: &mut Self, _| {
                        me.editor.select(doc_id, range);
                        me.editor.focus();
                    }));
                }
                for note in std::iter::once(&thread.first).chain(&thread.replies) {
                    ui.add_space(gap);
                    ui.label(
                        RichText::new(format!("{}, {}", note.author, note.date))
                            .font(theme.text_font(TextRole::Caption))
                            .color(theme.text_muted())
                            .strong(),
                    );
                    ui.label(RichText::new(&note.text).color(theme.text));
                }
                ui.add_space(gap);
                let reply = tr!("reply");
                let draft = self.drafts.entry((id, "reply")).or_default();
                let response = ui::text_field(("writer-comment-reply", id), draft)
                    .hint(&reply)
                    .label(&reply)
                    .show(ui, theme);
                let enter = key_text(Key::Enter);
                let response = ui::tooltip(response, &Tip::new(&reply).shortcut(&enter), theme);
                let send = response.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter));
                ui.add_space(gap);
                ui.horizontal(|ui| {
                    let pressed = ui::button(("writer-comment-send", id), &reply)
                        .enabled(!draft.trim().is_empty())
                        .show(ui, theme);
                    let pressed = ui::tooltip(pressed, &Tip::new(&reply).shortcut(&enter), theme);
                    if (pressed.clicked() || send) && !draft.trim().is_empty() {
                        let text = std::mem::take(draft).trim().to_owned();
                        act = Some(Box::new(move |me: &mut Self, cx| me.reply(cx, id, &text)));
                    }
                    let resolve = if thread.resolved {
                        tr!("reopen")
                    } else {
                        tr!("resolve")
                    };
                    let keys = keys_of("review.resolve", cx);
                    let pressed = ui::button(("writer-comment-resolve", id), &resolve)
                        .ghost()
                        .icon(Icon::Check)
                        .show(ui, theme);
                    if ui::tooltip(pressed, &Tip::new(&resolve).shortcut(&keys), theme).clicked() {
                        act = Some(Box::new(move |me: &mut Self, cx| me.resolve(cx, id)));
                    }
                    let delete = tr!("delete");
                    let keys = keys_of("review.delete_comment", cx);
                    let pressed = tool_button(
                        ui,
                        ("writer-comment-delete", id),
                        Icon::Delete,
                        &delete,
                        &keys,
                        theme,
                    );
                    if pressed.clicked() {
                        act = Some(Box::new(move |me: &mut Self, cx| {
                            me.editor.remove_comment(cx, id);
                        }));
                    }
                });
            });
            ui.add_space(theme.space(Space::S4));
        }
        let shown = tr!("show-resolved-comments");
        let response =
            ui::checkbox("writer-show-resolved", &shown, self.show_resolved).show(ui, theme);
        if ui::tooltip(response, &Tip::new(&shown), theme).clicked() {
            self.show_resolved = !self.show_resolved;
        }
        if let Some(act) = act {
            act(self, cx);
        }
    }
}
