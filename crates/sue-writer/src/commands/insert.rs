//! The Insert menu: breaks, notes, links, pictures, dates and formatting
//! marks.

use sue_core::Value;
use sue_model::mark_kind as kind;
use sue_wm::{AppletContext, Call, Command, Facts, FilePick, Flags, ItemState, ShellRequest};

use crate::project;
use crate::studio::{Dialog, Furniture, SueWriter, me};

const fn c(name: &'static str, label: &'static str) -> Command<SueWriter> {
    Command::new(name, label, "insert", run).state(state)
}

pub(super) const COMMANDS: &[Command<SueWriter>] = &[
    c("insert.page_break", "page-break").keys("Ctrl+Enter"),
    c("insert.scene_break", "scene-break"),
    c("insert.footnote", "footnote-ellipsis").keys("Ctrl+Alt+F"),
    c("insert.endnote", "endnote-ellipsis").keys("Ctrl+Alt+D"),
    c("insert.header_footer", "header-and-footer-ellipsis"),
    c("insert.contents", "table-of-contents-ellipsis"),
    c("insert.link", "link-ellipsis").keys("Ctrl+K"),
    c("insert.picture", "picture-ellipsis").with(Flags::LEAVES_CANVAS),
    c("insert.date", "todays-date"),
    c("insert.mark.nbsp", "non-breaking-space").keys("Ctrl+Shift+Space"),
    c("insert.mark.nbhyphen", "non-breaking-hyphen"),
    c("insert.mark.soft_hyphen", "soft-hyphen"),
    c("insert.mark.narrow", "narrow-non-breaking-space"),
    c("insert.mark.zwsp", "zero-width-space"),
];

/// Does what a Insert menu command does.
fn run(writer: &mut SueWriter, cx: &mut AppletContext<'_>, call: Call<'_>) {
    let command = call.name;
    let sheet = writer.sheet(cx);
    match command {
        "insert.contents" => {
            writer.dialog = Some(Dialog::Contents(sue_model::contents::Contents::default()));
        }
        "insert.header_footer" => {
            let page = writer.page(cx);
            writer.dialog = Some(Dialog::Furniture(Furniture {
                header: page.header,
                footer: page.footer,
                different_first: page.different_first,
            }));
        }
        mark if mark.starts_with("insert.mark.") => {
            let ch = match &mark["insert.mark.".len()..] {
                "nbsp" => "\u{A0}",
                "nbhyphen" => "\u{2011}",
                "soft_hyphen" => "\u{AD}",
                "narrow" => "\u{202F}",
                "zwsp" => "\u{200B}",
                _ => return,
            };
            writer.editor.insert(cx, ch);
        }
        "insert.page_break" => {
            writer.editor.insert(cx, "\n");
            writer.editor.set_paragraph(
                cx,
                kind::PAGE_BREAK,
                Some(Value::Bool(true)),
                &tr!("page-break"),
            );
        }
        "insert.scene_break" => {
            writer.editor.insert(cx, "\n#\n");
            // The caret sits after the break; style the line above.
            let caret = writer.editor.caret();
            if let Some(doc) = writer.editor.current() {
                let at = caret.offset.saturating_sub(2);
                writer.editor.select(doc, at..at);
                writer.editor.set_style(
                    cx,
                    if sheet.has("break") {
                        "break"
                    } else {
                        sue_type::BODY
                    },
                    &tr!("scene-break"),
                );
                writer.editor.select(doc, caret.offset..caret.offset);
            }
        }
        "insert.date" => {
            let date = project::today();
            writer.editor.insert(cx, &date);
        }
        "insert.link" => {
            let (_, look) = writer.editor.look_here(&sheet);
            writer.dialog = Some(Dialog::Link(
                look.link.unwrap_or_else(|| "https://".to_owned()),
            ));
        }
        "insert.footnote" | "insert.endnote" => {
            if writer.editor.selection().is_empty() {
                cx.notify(tr!("select-word-footnote-follows"));
            } else if command == "insert.footnote" {
                writer.dialog = Some(Dialog::Footnote(String::new()));
            } else {
                writer.dialog = Some(Dialog::Endnote(String::new()));
            }
        }
        "insert.picture" => {
            let pick = FilePick::open(Some(me()), "picture", &tr!("insert-a-picture"))
                .extensions(&[
                    "png", "jpg", "jpeg", "gif", "bmp", "webp", "tga", "tif", "tiff",
                ])
                .starting_in(SueWriter::export_dir(cx));
            cx.request(ShellRequest::PickFile(pick));
        }
        _ => {}
    }
}

/// Whether an Insert menu command works now: with a text open.
fn state(writer: &SueWriter, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    if writer.editor.current().is_some() {
        ItemState::on()
    } else {
        ItemState::off()
    }
}
