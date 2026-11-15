//! Modifier Key Settings: what each key held on the canvas, the mouse
//! wheel with each key, and the right and middle buttons do, for every
//! tool (General) or for one tool.
//!
//! The dialog shows one kind of input at a time (keys on the canvas, the
//! wheel, the buttons) as a list of rows, each a key combination with what
//! it does; a row that borrows a tool names the tool beside it. A tool's
//! rows start "as in General" and say what that is, so changing General
//! changes every tool that did not ask for its own.

use egui::{RichText, Ui};
use sue_wm::Theme;
use sue_wm::ui::tokens::{Space, TextRole};

use super::{Outcome, ok_cancel};
use crate::modifier_keys::{Action, Input, Keys, ModifierKeys, borrowed_name};
use crate::tools::Tool;

/// The dialog while it is open: the rows being changed and what it shows.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ModifiersDialog {
    /// The modifier keys as they will be kept on OK.
    pub(crate) map: ModifierKeys,
    /// General (`false`) or one tool's rows.
    pub(crate) for_tool: bool,
    /// The tool whose rows show, as a place in [`Tool::ALL`].
    pub(crate) tool: usize,
    /// The page: keys on the canvas, the wheel, the buttons.
    pub(crate) page: usize,
}

impl ModifiersDialog {
    /// The dialog over `map`, with `tool` ready in the tool picker.
    pub(crate) fn new(map: ModifierKeys, tool: Tool) -> Self {
        Self {
            map,
            for_tool: false,
            tool: Tool::ALL.iter().position(|at| *at == tool).unwrap_or(0),
            page: 0,
        }
    }

    /// The rows shown: General's or the picked tool's.
    pub(crate) fn scope(&self) -> Option<Tool> {
        self.for_tool
            .then(|| Tool::ALL.get(self.tool).copied())
            .flatten()
    }
}

/// The inputs each page shows.
fn page_inputs(page: usize) -> &'static [Input] {
    match page {
        0 => &[Input::Press],
        1 => &[Input::Wheel],
        _ => &[Input::Right, Input::Middle],
    }
}

/// Every tool a row can borrow, with its sub tools where it has several
/// that do different things.
fn borrow_targets() -> Vec<(Tool, Option<u8>)> {
    let mut targets = Vec::new();
    for tool in Tool::ALL {
        targets.push((tool, None));
        let subs = tool.sub_tools();
        let mut modes: Vec<u8> = subs.iter().map(|sub| sub.mode).collect();
        modes.dedup();
        if modes.len() == subs.len() && subs.len() > 1 && !tool.paints() {
            targets.extend(modes.into_iter().map(|mode| (tool, Some(mode))));
        }
    }
    targets
}

/// What an action does, in a few words: a borrowed tool by its name.
fn describe(action: Action) -> String {
    match action {
        Action::Borrow { tool, mode } => borrowed_name(tool, mode),
        action => action.kind_label(),
    }
}

/// Modifier Key Settings' body.
pub(super) fn modifiers_ui(ui: &mut Ui, dialog: &mut ModifiersDialog, theme: &Theme) -> Outcome {
    let mut outcome = Outcome::Open;
    let gap = theme.space(Space::S4);
    ui.spacing_mut().item_spacing.y = theme.space(Space::S3);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        let scopes = [tr!("modifiers-general"), tr!("modifiers-for-a-tool")];
        let scope = sue_wm::ui::segmented(
            "sue-paint-modifiers-scope",
            &scopes,
            Some(usize::from(dialog.for_tool)),
        )
        .show(ui, theme);
        scope.response.on_hover_text(tr!("modifiers-scope-tip"));
        if let Some(picked) = scope.picked {
            dialog.for_tool = picked == 1;
        }
        if dialog.for_tool {
            let names: Vec<String> = Tool::ALL.iter().map(|tool| tool.label()).collect();
            let tool_label = tr!("modifiers-tool");
            sue_wm::ui::dropdown("sue-paint-modifiers-tool", &mut dialog.tool, &names)
                .label(&tool_label)
                .show(ui, theme)
                .on_hover_text(tr!("modifiers-tool-tip"));
        }
    });
    let pages = [
        Input::Press.label(),
        Input::Wheel.label(),
        tr!("modifiers-buttons"),
    ];
    let page = sue_wm::ui::segmented("sue-paint-modifiers-page", &pages, Some(dialog.page))
        .full_width()
        .show(ui, theme);
    page.response.on_hover_text(tr!("modifiers-page-tip"));
    if let Some(picked) = page.picked {
        dialog.page = picked;
    }
    let scope = dialog.scope();
    let map = &mut dialog.map;
    // The rows take the room the dialog has, less its foot (Reset, the
    // note about the file, OK and Cancel), so no gap is left under them.
    let foot = theme.space(Space::S9) * 2.5;
    let rows_height = (ui.available_height() - foot).max(theme.space(Space::S9) * 3.0);
    egui::ScrollArea::vertical()
        .id_salt("sue-paint-modifiers-rows")
        .max_height(rows_height)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            sue_wm::ui::form_grid(
                ui,
                ("sue-paint-modifiers-grid", dialog.page),
                theme,
                |form| {
                    for input in page_inputs(dialog.page) {
                        for keys in input.rows() {
                            let name = match input {
                                Input::Right | Input::Middle => input.label(),
                                _ => keys.label(),
                            };
                            form.row(&name, |ui| {
                                row_ui(ui, map, scope, *input, keys, &name, theme);
                            });
                        }
                    }
                },
            );
        });
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        let reset = sue_wm::ui::button("sue-paint-modifiers-reset", &tr!("modifiers-reset"))
            .enabled(map.changed(scope) || (scope.is_none() && map.legacy() != Default::default()))
            .show(ui, theme)
            .on_hover_text(match scope {
                Some(tool) => tr!("modifiers-reset-tool-tip", tool = tool.label()),
                None => tr!("modifiers-reset-general-tip"),
            });
        if reset.clicked() {
            map.reset(scope);
        }
    });
    ui.label(
        RichText::new(tr!("modifiers-file-note"))
            .font(theme.text_font(TextRole::Caption))
            .color(theme.text_muted()),
    );
    ok_cancel(ui, &mut outcome);
    outcome
}

/// One row: what `keys` do on `input` for `scope`, and the tool it
/// borrows.
fn row_ui(
    ui: &mut Ui,
    map: &mut ModifierKeys,
    scope: Option<Tool>,
    input: Input,
    keys: Keys,
    name: &str,
    theme: &Theme,
) {
    ui.spacing_mut().item_spacing.x = theme.space(Space::S3);
    let now = map.row(scope, input, keys);
    let kinds: Vec<Action> = Action::KINDS
        .into_iter()
        .filter(|kind| input.allows(kind) && !(scope.is_none() && *kind == Action::Common))
        .collect();
    let names: Vec<String> = kinds
        .iter()
        .map(|kind| match kind {
            Action::Common => format!(
                "{}: {}",
                kind.kind_label(),
                describe(map.row(None, input, keys))
            ),
            kind => kind.kind_label(),
        })
        .collect();
    let mut chosen = kinds
        .iter()
        .position(|kind| kind.same_kind(now))
        .unwrap_or(0);
    let salt = (scope.map(Tool::id), input.id(), keys.id());
    let picked = sue_wm::ui::dropdown(
        ("sue-paint-modifier-kind", salt.clone()),
        &mut chosen,
        &names,
    )
    .label(name)
    .show(ui, theme)
    .on_hover_text(tr!("modifiers-row-tip", keys = name));
    if picked.changed()
        && let Some(kind) = kinds.get(chosen).copied()
    {
        let next = match kind {
            // A row that starts to borrow takes what General borrows
            // there, else the Eyedropper.
            Action::Borrow { .. } => match map.row(None, input, keys) {
                borrow @ Action::Borrow { .. } => borrow,
                _ => kind,
            },
            kind => kind,
        };
        map.set(scope, input, keys, next);
    }
    if let Action::Borrow { tool, mode } = map.row(scope, input, keys) {
        let targets = borrow_targets();
        let labels: Vec<String> = targets
            .iter()
            .map(|(tool, mode)| borrowed_name(*tool, *mode))
            .collect();
        let mut at = targets
            .iter()
            .position(|target| *target == (tool, mode))
            .unwrap_or(0);
        let tool_label = tr!("modifiers-borrowed-tool");
        let changed = sue_wm::ui::dropdown(("sue-paint-modifier-tool", salt), &mut at, &labels)
            .label(&tool_label)
            .show(ui, theme)
            .on_hover_text(tr!("modifiers-borrowed-tip", keys = name))
            .changed();
        if changed && let Some((tool, mode)) = targets.get(at).copied() {
            map.set(scope, input, keys, Action::Borrow { tool, mode });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_can_be_borrowed_and_view_tools_by_sub_tool() {
        let targets = borrow_targets();
        for tool in Tool::ALL {
            assert!(targets.contains(&(tool, None)), "{tool:?}");
        }
        assert!(targets.contains(&(Tool::Hand, Some(1))));
        assert!(targets.contains(&(Tool::Zoom, Some(1))));
        assert!(targets.contains(&(Tool::Operation, Some(0))));
        assert!(targets.contains(&(Tool::Eyedropper, Some(0))));
        // The brushes' sub tools are pictures of brushes, not modes.
        assert!(
            !targets
                .iter()
                .any(|(tool, mode)| *tool == Tool::Pen && mode.is_some())
        );
        // Every default borrow is offered, so the list can show it.
        let map = ModifierKeys::default();
        for input in Input::ALL {
            for keys in input.rows() {
                if let Action::Borrow { tool, mode } = map.row(None, input, keys) {
                    assert!(targets.contains(&(tool, mode)), "{tool:?} {mode:?}");
                }
            }
        }
    }

    #[test]
    fn a_tool_scope_starts_at_the_current_tool() {
        let mut dialog = ModifiersDialog::new(ModifierKeys::default(), Tool::Fill);
        assert_eq!(dialog.scope(), None);
        dialog.for_tool = true;
        assert_eq!(dialog.scope(), Some(Tool::Fill));
    }
}
