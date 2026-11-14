//! The Review menu: spelling, comments and tracked changes.

use sue_core::Value;
use sue_wm::{AppletContext, ArenaEdit, Call, Command, Facts, ItemState};

use crate::project;
use crate::studio::{Dialog, SPELL_CHECK, SueWriter};

const fn c(name: &'static str, label: &'static str) -> Command<SueWriter> {
    Command::new(name, label, "review", run).state(state)
}

pub(super) const COMMANDS: &[Command<SueWriter>] = &[
    c("review.spelling", "check-spelling"),
    c("review.next_misspelling", "next-misspelling").keys("F7"),
    c("review.comment", "new-comment-ellipsis").keys("Ctrl+Alt+C"),
    c("review.previous_comment", "previous-comment"),
    c("review.next_comment", "next-comment"),
    c("review.resolve", "resolve-comment"),
    c("review.delete_comment", "delete-comment"),
    c("review.delete_all_comments", "delete-all-comments"),
    c("review.comments", "comments"),
    c("review.track", "record-changes").keys("Ctrl+Shift+C"),
    c("review.previous_change", "previous-change"),
    c("review.next_change", "next-change"),
    c("review.accept", "accept-change"),
    c("review.reject", "reject-change"),
    c("review.accept_all", "accept-all"),
    c("review.reject_all", "reject-all"),
];

/// Does what a Review menu command does.
fn run(writer: &mut SueWriter, cx: &mut AppletContext<'_>, call: Call<'_>) {
    let command = call.name;
    match command {
        "review.comment" => {
            if writer.editor.current().is_some() {
                writer.dialog = Some(Dialog::Comment(String::new()));
            }
        }
        "review.spelling" => {
            let on = !writer.spell_check(cx);
            let _ = cx.edit_rice(sue_rice::SetRiceValue::new(
                SPELL_CHECK,
                sue_rice::RiceValue::Bool(on),
            ));
        }
        "review.next_misspelling" => writer.next_misspelling(cx),
        "review.track" => {
            let target = writer.project.or(writer.editor.current());
            if let Some(target) = target {
                let on = !writer.tracking(cx);
                let step = sue_core::txn::Batch::new(if on {
                    tr!("record-changes")
                } else {
                    tr!("stop-recording-changes")
                })
                .then(project::set(
                    target,
                    sue_model::changes::TRACK,
                    on.then_some(Value::Bool(true)),
                ));
                if cx.apply(ArenaEdit(step)).is_ok() {
                    cx.notify(if on {
                        tr!("recording-changes")
                    } else {
                        tr!("no-longer-recording-changes")
                    });
                }
            }
        }
        "review.next_change" | "review.previous_change" => {
            writer.next_change(cx, command == "review.previous_change");
        }
        "review.accept" | "review.reject" | "review.accept_all" | "review.reject_all" => {
            let keep = command.starts_with("review.accept");
            let all = command.ends_with("_all");
            writer.settle_changes(cx, keep, all);
        }
        "review.comments" => {
            let shown = writer.inspector && writer.comments;
            writer.inspector = !shown;
            writer.comments = !shown;
        }
        "review.next_comment" | "review.previous_comment" => {
            writer.next_comment(cx, command == "review.previous_comment");
        }
        "review.resolve" => match writer.editor.comment_here() {
            Some(id) => writer.resolve(cx, id),
            None => cx.notify(tr!("put-caret-comments-words-first")),
        },
        "review.delete_comment" => match writer.editor.comment_here() {
            Some(id) => {
                writer.editor.remove_comment(cx, id);
            }
            None => cx.notify(tr!("put-caret-comments-words-first")),
        },
        "review.delete_all_comments" => {
            for (_, _, id) in writer.editor.comments() {
                writer.editor.remove_comment(cx, id);
            }
        }
        _ => {}
    }
}

/// Whether a Review menu command works now, and its check mark.
fn state(writer: &SueWriter, facts: &Facts<'_>, call: Call<'_>) -> ItemState {
    match call.name {
        "review.comments" => ItemState::check(writer.inspector && writer.comments),
        "review.track" => ItemState::check(writer.tracking_in(facts.arena)),
        "review.spelling" => ItemState::check(SueWriter::spell_check_in(facts.rice)),
        _ => ItemState::on(),
    }
}
