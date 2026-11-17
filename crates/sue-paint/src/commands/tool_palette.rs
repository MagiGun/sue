//! The commands of the tool palettes' menus: the Tool palette's (Tool
//! Settings, Delete Tool, Add Tool from Default, Reset Tool Palette,
//! Import and Export Tool Layout, Show Color Icon), the Sub Tool
//! palette's and Tool Property's.
//!
//! None of them changes a document: the tool palettes are the person's
//! own arrangement, kept in the data folder, so none has an undo pair. A
//! tool deleted by mistake comes back whole with Add Tool from Default,
//! its own sub tools included, for as long as Sue Paint is open.

use sue_wm::{AppletContext, Call, Command, Facts, ItemState, ShellRequest};

use super::flag;
use crate::panels::sub_tool_detail::NamingKind;
use crate::studio::SuePaint;
use crate::tools::Toolbox;
use crate::tools::layout::{EXTENSION, ToolLayout};

/// A command of the tool palettes' menus.
const fn c(
    name: &'static str,
    label: &'static str,
    icon: &'static str,
    run: fn(&mut SuePaint, &mut AppletContext<'_>, Call<'_>),
) -> Command<SuePaint> {
    Command::new(name, label, "tool_palette", run).icon(icon)
}

/// Grayed while Sue Paint brings no such sub tool to go back to, or the
/// sub tool is locked.
fn resettable(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    super::when(paint.can_reset_current() && !paint.toolbox.current().locked)
}

/// Grayed when the tool out holds one sub tool only.
fn deletable(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    super::when(paint.toolbox.current_entry().subs().len() > 1)
}

/// Grayed when the tool palette holds one tool only.
fn tool_deletable(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    super::when(paint.toolbox.slots().len() > 1)
}

/// Grayed when every tool Sue Paint brings is on the palette and none was
/// deleted.
fn restorable(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    super::when(!paint.tool_palettes.removed.is_empty() || !missing(&paint.toolbox).is_empty())
}

/// The tools Sue Paint brings that the tool palette lacks (by name and
/// tool), with the place each has in Sue Paint's own arrangement.
fn missing(toolbox: &Toolbox) -> Vec<(usize, crate::tools::ToolSlot)> {
    Toolbox::default()
        .slots()
        .iter()
        .enumerate()
        .filter(|(_, own)| {
            !toolbox
                .slots()
                .iter()
                .any(|slot| slot.kind == own.kind && slot.name == own.name)
        })
        .map(|(at, own)| (at, own.clone()))
        .collect()
}

impl SuePaint {
    /// Deletes the tool out, kept so Add Tool from Default can bring it
    /// back as it was.
    pub(crate) fn delete_tool(&mut self, cx: &mut AppletContext<'_>) {
        let slot = self.toolbox.current_slot();
        if let Some(removed) = self.toolbox.remove_slot(slot) {
            cx.notify(tr!("tool-deleted", name = crate::localize(&removed.name)));
            self.tool_palettes.removed.push((slot, removed));
        }
    }

    /// Brings back the tools deleted this run, then every tool Sue Paint
    /// brings that the palette lacks, each where it was.
    pub(crate) fn restore_tools(&mut self) {
        while let Some((at, slot)) = self.tool_palettes.removed.pop() {
            self.toolbox.insert_slot(at, slot);
        }
        for (at, slot) in missing(&self.toolbox) {
            self.toolbox.insert_slot(at, slot);
        }
    }

    /// Reads a tool layout file and arranges the tool palette by it,
    /// saying what it could not follow.
    pub(crate) fn import_tool_layout(
        &mut self,
        cx: &mut AppletContext<'_>,
        path: &std::path::Path,
    ) {
        let text = match sue_platform::fs::read(path) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(error) => {
                cx.fail(error.to_string());
                return;
            }
        };
        let (layout, mut problems) = ToolLayout::parse(&text);
        if layout.tools.is_empty() {
            cx.fail(tr!("no-tool-layout-in-file"));
            return;
        }
        problems.extend(self.toolbox.arrange(&layout));
        if problems.is_empty() {
            cx.notify(tr!("tool-layout-loaded"));
        } else {
            cx.notify(tr!(
                "tool-layout-loaded-with-problems",
                count = problems.len(),
                first = problems[0].as_str()
            ));
        }
    }
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    // The Tool palette's menu.
    c(
        "tool_palette.settings",
        "tool-settings-menu",
        "settings",
        |paint, _, _| {
            let slot = paint.toolbox.current_slot();
            paint.open_naming(NamingKind::Tool { slot });
        },
    ),
    c(
        "tool_palette.delete",
        "delete-tool",
        "delete",
        |paint, cx, _| {
            paint.delete_tool(cx);
        },
    )
    .state(tool_deletable),
    c(
        "tool_palette.restore",
        "add-tool-from-default",
        "plus",
        |paint, _, _| {
            paint.restore_tools();
        },
    )
    .state(restorable),
    c(
        "tool_palette.reset",
        "reset-tool-palette",
        "reset",
        |paint, _, _| {
            paint.restore_tools();
            let _ = paint.toolbox.arrange(&ToolLayout::builtin());
        },
    ),
    c(
        "tool_palette.import",
        "import-tool-layout",
        "import",
        |_, cx, _| {
            cx.request(ShellRequest::PickFile(
                sue_wm::FilePick::open(
                    Some(sue_wm::AppletId::new(crate::ID).expect("a valid id")),
                    "tool-layout",
                    &tr!("import-tool-layout-title"),
                )
                .extensions(&[EXTENSION]),
            ));
        },
    ),
    c(
        "tool_palette.export",
        "export-tool-layout",
        "export",
        |paint, cx, _| {
            paint.pending_export = Some(paint.toolbox.layout().to_text().into_bytes());
            cx.request(ShellRequest::PickFile(
                sue_wm::FilePick::save(
                    Some(sue_wm::AppletId::new(crate::ID).expect("a valid id")),
                    "export",
                    &tr!("export-tool-layout-title"),
                    &format!("{}.{EXTENSION}", tr!("tool-layout-file-name")),
                )
                .extensions(&[EXTENSION]),
            ));
        },
    ),
    c(
        "tool_palette.color_icon",
        "show-color-icon",
        "palette-color-wheel",
        |_, cx, _| {
            let on = cx
                .rice()
                .get_bool(crate::panels::TOOL_COLOR_ICON)
                .unwrap_or(true);
            let edit = sue_rice::SetRiceValue::new(
                crate::panels::TOOL_COLOR_ICON,
                sue_rice::RiceValue::Bool(!on),
            );
            if let Err(error) = cx.edit_rice(edit) {
                cx.fail(error.to_string());
            }
        },
    )
    .state(|_, facts, _| {
        flag(
            facts
                .rice
                .get_bool(crate::panels::TOOL_COLOR_ICON)
                .unwrap_or(true),
        )
    }),
    // The Sub Tool palette's menu and Tool Property's.
    c(
        "sub_tool.duplicate",
        "duplicate-sub-tool-menu",
        "duplicate",
        |paint, _, _| {
            let entry = paint.toolbox.current_entry();
            let (slot, index) = (paint.toolbox.current_slot(), entry.picked());
            paint.open_naming(NamingKind::Duplicate { slot, index });
        },
    ),
    c(
        "sub_tool.rename",
        "rename-sub-tool",
        "rename",
        |paint, _, _| {
            let entry = paint.toolbox.current_entry();
            let (slot, index) = (paint.toolbox.current_slot(), entry.picked());
            paint.open_naming(NamingKind::RenameSub { slot, index });
        },
    ),
    c(
        "sub_tool.delete",
        "delete-sub-tool",
        "delete",
        |paint, _, _| {
            let entry = paint.toolbox.current_entry();
            let (kind, index) = (entry.kind, entry.picked());
            paint.toolbox.delete_sub(kind, index);
        },
    )
    .state(deletable),
    c(
        "sub_tool.reset",
        "reset-to-initial-settings",
        "reset",
        |paint, _, _| {
            paint.reset_current_sub();
        },
    )
    .state(resettable),
    c(
        "sub_tool.new_group",
        "new-sub-tool-group-menu",
        "folder-new",
        |paint, _, _| {
            let entry = paint.toolbox.current_entry();
            let (slot, index) = (paint.toolbox.current_slot(), entry.picked());
            paint.open_naming(NamingKind::NewGroup { slot, index });
        },
    ),
    c(
        "sub_tool.rename_group",
        "rename-sub-tool-group",
        "rename",
        |paint, _, _| {
            let entry = paint.toolbox.current_entry();
            let (slot, group) = (paint.toolbox.current_slot(), entry.group_of(entry.picked()));
            paint.open_naming(NamingKind::RenameGroup { slot, group });
        },
    ),
    c(
        "sub_tool.import",
        "add-sub-tool-from-file-menu",
        "import",
        |_, cx, _| {
            cx.request(ShellRequest::PickFile(
                sue_wm::FilePick::open(
                    Some(sue_wm::AppletId::new(crate::ID).expect("a valid id")),
                    "brush-import",
                    &tr!("add-sub-tool-from-file"),
                )
                .extensions(&["sut", "abr", "gbr", "gih", "png"]),
            ));
        },
    ),
    c(
        "sub_tool.detail",
        "sub-tool-detail-menu",
        "wrench",
        |paint, _, _| {
            paint.show_palette(crate::dock::Palette::SubToolDetail);
        },
    ),
    c(
        "sub_tool.lock",
        "lock-sub-tool-settings-menu",
        "lock",
        |paint, _, _| {
            let locked = paint.toolbox.current().locked;
            paint.toolbox.lock_settings(!locked);
        },
    )
    .state(|paint, _, _| flag(paint.toolbox.current().locked)),
    c(
        "sub_tool.categories",
        "show-category-in-tool-property",
        "view-list",
        |paint, _, _| {
            paint.tool_palettes.headings = !paint.tool_palettes.headings;
        },
    )
    .state(|paint, _, _| flag(paint.tool_palettes.headings)),
    // Sub Tool Detail's Save all settings as default (full guide FGV-12).
    c(
        "sub_tool.save_default",
        "save-all-settings-as-default",
        "save",
        |paint, cx, _| {
            paint.save_sub_tool_default(cx);
        },
    )
    .state(paints),
    // The Brush shape category's Register (full guide 00:29:47).
    c(
        "sub_tool.register_shape",
        "register-brush-shape",
        "plus",
        |paint, cx, _| {
            paint.register_brush_shape(cx);
        },
    )
    .state(paints),
];

fn paints(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    super::when(paint.toolbox.tool().paints())
}
