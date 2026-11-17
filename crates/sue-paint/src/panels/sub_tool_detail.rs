//! Sub Tool Detail: every setting of the picked sub tool, sorted into
//! categories, each with an eye that says whether Tool Property shows it
//! (Clip Studio Paint's Sub Tool Detail palette). Above them a sample
//! stroke, beside them the list of categories, under them a help box and
//! the buttons that reset the sub tool and lock it.
//!
//! Also what the tool palettes keep between frames ([`ToolPalettes`]) and
//! the small dialog that names sub tools, groups and tools ([`Naming`]).

use egui::{Rect, RichText, Ui, pos2};
use sue_wm::icons::Icon;
use sue_wm::ui::{Answer, Density, Space, TextRole};
use sue_wm::{AppletContext, Theme};

use crate::panels::child;
use crate::panels::tool_property::{Category, Rows};
use crate::studio::SuePaint;

/// What the tool palettes keep between frames.
#[derive(Debug, Default)]
pub(crate) struct ToolPalettes {
    /// Tool Property shows the name of each category over its first row
    /// (Show Category).
    pub headings: bool,
    /// The category Sub Tool Detail shows, when it was picked.
    pub detail: Option<Category>,
    /// The naming dialog, while it is open.
    pub naming: Option<Naming>,
    /// The tool palette's arrangement as last written to the data folder
    /// (or read from it).
    pub kept: Option<crate::tools::layout::ToolLayout>,
    /// The kept arrangement was read this run.
    pub loaded: bool,
    /// Tools deleted this run, with where each stood, newest last, so
    /// Add Tool from Default brings them back as they were.
    pub removed: Vec<(usize, crate::tools::ToolSlot)>,
}

/// What the naming dialog names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NamingKind {
    /// A copy of sub tool `index` of tool palette entry `slot`
    /// (Duplicate Sub Tool).
    Duplicate { slot: usize, index: usize },
    /// Sub tool `index` of entry `slot`.
    RenameSub { slot: usize, index: usize },
    /// A new group starting at sub tool `index` of entry `slot`.
    NewGroup { slot: usize, index: usize },
    /// Group `group` of entry `slot`.
    RenameGroup { slot: usize, group: usize },
    /// Entry `slot` itself (Tool Settings).
    Tool { slot: usize },
}

/// The naming dialog: what it names and the name typed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Naming {
    pub kind: NamingKind,
    pub name: String,
    /// The name field has not held the keys yet: it asks for them until
    /// it has, so typing goes there at once and Enter then accepts.
    pub fresh: bool,
}

impl SuePaint {
    /// Opens the naming dialog for `kind`, with the name it has now.
    pub(crate) fn open_naming(&mut self, kind: NamingKind) {
        let slots = self.toolbox.slots();
        let sub_name = |slot: usize, index: usize| {
            slots
                .get(slot)
                .and_then(|entry| entry.subs().get(index))
                .map(|sub| crate::localize(&sub.name))
                .unwrap_or_default()
        };
        let name = match kind {
            NamingKind::Duplicate { slot, index } => {
                tr!("copy-of-sub-tool", name = sub_name(slot, index))
            }
            NamingKind::RenameSub { slot, index } => sub_name(slot, index),
            NamingKind::NewGroup { .. } => tr!("new-sub-tool-group"),
            NamingKind::RenameGroup { slot, group } => slots
                .get(slot)
                .and_then(|entry| entry.groups().get(group))
                .map(|found| crate::localize(&found.name))
                .unwrap_or_default(),
            NamingKind::Tool { slot } => slots
                .get(slot)
                .map(|entry| crate::localize(&entry.name))
                .unwrap_or_default(),
        };
        self.tool_palettes.naming = Some(Naming {
            kind,
            name,
            fresh: true,
        });
    }

    /// The naming dialog, while it is open: a name, OK
    /// and Cancel. What it names changes only on OK.
    pub(crate) fn naming_dialog(&mut self, ctx: &egui::Context, theme: &Theme) {
        let Some(mut naming) = self.tool_palettes.naming.take() else {
            return;
        };
        let title = match naming.kind {
            NamingKind::Duplicate { .. } => tr!("duplicate-sub-tool-title"),
            NamingKind::RenameSub { .. } => tr!("rename-sub-tool-title"),
            NamingKind::NewGroup { .. } => tr!("new-sub-tool-group-title"),
            NamingKind::RenameGroup { .. } => tr!("rename-sub-tool-group-title"),
            NamingKind::Tool { .. } => tr!("tool-settings-title"),
        };
        let fine = !naming.name.trim().is_empty();
        let shown = sue_wm::ui::Dialog::new("sue-paint-naming", &title, theme)
            .ok_cancel()
            .accept_enabled(fine)
            .show(ctx, |ui| {
                let label = tr!("name");
                let field =
                    sue_wm::ui::input::text_field("sue-paint-naming-name", &mut naming.name)
                        .label(&label)
                        .show(ui, theme)
                        .on_hover_text(&label);
                // Until the field has had the keys once; after that Enter
                // gives them up and accepts.
                if field.has_focus() {
                    naming.fresh = false;
                } else if naming.fresh {
                    field.request_focus();
                }
            });
        match shown.answer {
            Some(Answer::Accept) if fine => self.name_it(&naming),
            Some(_) => {}
            None => self.tool_palettes.naming = Some(naming),
        }
    }

    /// Does what the naming dialog asked for.
    fn name_it(&mut self, naming: &Naming) {
        let name = naming.name.trim();
        match naming.kind {
            NamingKind::Duplicate { slot, index } => {
                crate::commands::subtool::pick(&mut self.toolbox, slot, index);
                let kind = self.toolbox.current_entry().kind;
                self.toolbox.duplicate_sub(kind, index);
                name.clone_into(&mut self.toolbox.current_mut().name);
            }
            NamingKind::RenameSub { slot, index } => {
                crate::commands::subtool::pick(&mut self.toolbox, slot, index);
                name.clone_into(&mut self.toolbox.current_mut().name);
            }
            NamingKind::NewGroup { slot, index } => {
                // The new group shows, with the sub tool it starts at.
                if self.toolbox.split_group(slot, index, name) {
                    crate::commands::subtool::pick(&mut self.toolbox, slot, index);
                }
            }
            NamingKind::RenameGroup { slot, group } => {
                self.toolbox.rename_group(slot, group, name);
            }
            NamingKind::Tool { slot } => {
                self.toolbox.rename_slot(slot, name);
            }
        }
    }

    /// Sub Tool Detail's body in `body`.
    pub(crate) fn sub_tool_detail(
        &mut self,
        ui: &mut Ui,
        body: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let tool = self.toolbox.tool();
        let pad = theme.space(Space::S4);
        let mut inner = child(ui, body.shrink(pad), "sub-tool-detail");
        // The sub tool's name, and a sample stroke for the painting tools.
        let name = crate::localize(&self.toolbox.current().name);
        inner.label(
            RichText::new(&name)
                .size(theme.text_size(TextRole::Title))
                .color(theme.text),
        );
        if tool.paints() {
            self.stroke_preview(&mut inner, cx, theme);
        }
        // The categories the sub tool's rows fall in.
        let mut found = Vec::new();
        {
            let mut census = Rows::census(&mut found, theme);
            let mut scratch = inner.new_child(egui::UiBuilder::new().invisible());
            let _ = self.sub_tool_rows(&mut scratch, &mut census, tool, theme);
        }
        found.sort();
        if found.is_empty() {
            inner.label(
                RichText::new(tr!("sub-tool-has-no-settings"))
                    .size(theme.text_size(TextRole::Caption))
                    .color(theme.text_muted()),
            );
            return;
        }
        let chosen = self
            .tool_palettes
            .detail
            .filter(|category| found.contains(category))
            .unwrap_or(found[0]);
        let room = inner.available_rect_before_wrap();
        let footer = Density::of(&inner).control_height() + pad;
        let list_width =
            (room.width() * 0.34).clamp(theme.space(Space::S9) * 2.0, theme.space(Space::S9) * 4.0);
        let list = Rect::from_min_max(
            room.min,
            pos2(room.left() + list_width, room.bottom() - footer),
        );
        let settings = Rect::from_min_max(
            pos2(list.right() + pad, room.top()),
            pos2(room.right(), room.bottom() - footer),
        );
        // The list of categories.
        let mut list_ui = child(&mut inner, list, "sub-tool-detail-list");
        // The list in the compact rows of a side list, so a brush's
        // fifteen categories show whole in the palette's own height.
        egui::ScrollArea::vertical()
            .id_salt("sue-paint-detail-categories")
            .show(&mut list_ui, |ui| {
                Density::Compact.scope(ui, |ui| {
                    for category in &found {
                        let label = category.label();
                        let row = sue_wm::ui::list_row(
                            ("sue-paint-detail-category", *category as u8),
                            &label,
                        )
                        .selected(*category == chosen)
                        .show(ui, theme);
                        if row.response.on_hover_text(&label).clicked() {
                            self.tool_palettes.detail = Some(*category);
                        }
                    }
                });
            });
        // The rows of the chosen category, each with its eye.
        let mut rows_ui = child(&mut inner, settings, "sub-tool-detail-rows");
        let mut hidden = std::mem::take(&mut self.toolbox.current_mut().hidden);
        let enabled = !self.toolbox.current().locked;
        let command = egui::ScrollArea::vertical()
            .id_salt("sue-paint-detail-rows")
            .show(&mut rows_ui, |ui| {
                ui.spacing_mut().item_spacing.y = theme.space(Space::S3);
                let mut rows = Rows::detail(chosen, &mut hidden, enabled, theme);
                let command = self.sub_tool_rows(ui, &mut rows, tool, theme);
                ui.add_space(pad);
                // The help box.
                egui::Frame::new()
                    .fill(theme.surface(2))
                    .corner_radius(theme.corner(sue_wm::ui::Radius::Md))
                    .inner_margin(egui::Margin::same(pad as i8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(
                            RichText::new(chosen.help())
                                .size(theme.text_size(TextRole::Caption))
                                .color(theme.text_muted()),
                        );
                    });
                command
            })
            .inner;
        self.toolbox.current_mut().hidden = hidden;
        if let Some(command) = command {
            self.run(command, cx);
        }
        // The footer: Show Category, Reset All, and the lock.
        let footer_rect =
            Rect::from_min_max(pos2(room.left(), room.bottom() - footer + pad), room.max);
        let mut bar = child(&mut inner, footer_rect, "sub-tool-detail-footer");
        let mut saved = false;
        bar.horizontal(|ui| {
            let label = tr!("show-category-in-tool-property");
            if sue_wm::ui::checkbox(
                "sue-paint-detail-headings",
                &label,
                self.tool_palettes.headings,
            )
            .show(ui, theme)
            .on_hover_text(&label)
            .clicked()
            {
                self.tool_palettes.headings = !self.tool_palettes.headings;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                self.sub_tool_header_buttons(ui, theme);
                // Save all settings as default (FGV-12): what the sub tool
                // starts as, and what Reset brings back.
                if self.toolbox.tool().paints() {
                    let save = tr!("save-all-settings-as-default");
                    if sue_wm::ui::button("sue-paint-detail-save-default", &save)
                        .show(ui, theme)
                        .on_hover_text(tr!("save-all-settings-as-default-hint"))
                        .clicked()
                    {
                        saved = true;
                    }
                }
            });
        });
        if saved {
            self.save_sub_tool_default(cx);
        }
    }

    /// The buttons that every view of a sub tool's settings ends its
    /// header with, from the trailing edge: Lock, Reset to Initial
    /// Settings and, outside Sub Tool Detail, the wrench that opens it.
    pub(crate) fn sub_tool_header_buttons(&mut self, ui: &mut Ui, theme: &Theme) {
        let locked = self.toolbox.current().locked;
        let lock = if locked {
            tr!("unlock-sub-tool-settings")
        } else {
            tr!("lock-sub-tool-settings")
        };
        if sue_wm::ui::icon_button(
            "sue-paint-sub-tool-lock",
            if locked { Icon::Lock } else { Icon::Unlock },
            &lock,
        )
        .selected(locked)
        .quiet()
        .show(ui, theme)
        .clicked()
        {
            self.toolbox.lock_settings(!locked);
        }
        let reset = tr!("reset-to-initial-settings");
        let can_reset = !locked && self.can_reset_current();
        if sue_wm::ui::icon_button("sue-paint-sub-tool-reset", Icon::Reset, &reset)
            .enabled(can_reset)
            .quiet()
            .show(ui, theme)
            .clicked()
        {
            self.reset_current_sub();
        }
    }

    /// `true` when the picked sub tool is one Sue Paint brings, so Reset
    /// has something to go back to.
    pub(crate) fn can_reset_current(&self) -> bool {
        let sub = self.toolbox.current();
        crate::tools::SubTool::builtin(sub.tool, &sub.name).is_some() || self.has_saved_default()
    }
}

impl SuePaint {
    /// Shows category `index` of Sub Tool Detail's list
    /// ([`Category::ALL`]), as scripts and snapshots name it:
    /// `detail/<index>`.
    pub(crate) fn detail_navigate(&mut self, target: &str) -> bool {
        let Some(category) = target
            .strip_prefix("detail/")
            .and_then(|index| index.parse::<usize>().ok())
            .and_then(|index| Category::ALL.get(index).copied())
        else {
            return false;
        };
        self.tool_palettes.detail = Some(category);
        true
    }

    /// What the tool palettes do every frame, wherever they are shown:
    /// the naming dialog, and keeping the tool palette's arrangement.
    pub(crate) fn tool_palettes_frame(
        &mut self,
        ctx: &egui::Context,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        self.naming_dialog(ctx, theme);
        self.keep_tools(cx);
        self.brush_rows_frame(cx);
    }
}
