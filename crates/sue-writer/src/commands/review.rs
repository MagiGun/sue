//! The Review menu: spelling, comments and tracked changes.

use sue_wm::Command;

use crate::studio::SueWriter;

const fn c(name: &'static str, label: &'static str) -> Command<SueWriter> {
    super::old(name, label, "review")
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
