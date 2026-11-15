//! The Item Bank palette: sub tools kept to add to their tool again.

use egui::{Rect, Ui};
use sue_wm::{AppletContext, Theme};

use crate::brush::Tip;
use crate::studio::SuePaint;
use crate::tools::{SubTool, Tool};

/// A sub tool kept in the Item Bank.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Item {
    pub tool: Tool,
    pub name: String,
    pub tip: Tip,
    pub stabilization: u32,
}

impl Item {
    /// The item for `sub`.
    #[must_use]
    pub fn of(sub: &SubTool) -> Self {
        Self {
            tool: sub.tool,
            name: sub.name.clone(),
            tip: sub.tip,
            stabilization: sub.stabilization,
        }
    }

    /// One line: tool, name, stabilization and brush, tab separated.
    #[must_use]
    pub fn to_line(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}",
            self.tool.label(),
            self.name.replace('\t', " "),
            self.stabilization,
            self.tip.to_text()
        )
    }

    /// An item from [`Self::to_line`].
    #[must_use]
    pub fn from_line(line: &str) -> Option<Self> {
        let mut parts = line.splitn(4, '\t');
        let label = parts.next()?;
        let tool = Tool::ALL.into_iter().find(|tool| tool.label() == label)?;
        Some(Self {
            tool,
            name: parts.next()?.to_owned(),
            stabilization: parts.next()?.parse().ok()?,
            tip: Tip::from_text(parts.next()?),
        })
    }
}

impl SuePaint {
    /// The Item Bank tab: sub tools kept to add to their tool again.
    pub(crate) fn item_bank_tab(
        &mut self,
        ui: &mut Ui,
        body: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(body.shrink(6.0)));
        let mut changed = false;
        if inner.button(tr!("keep-current-sub-tool")).clicked() {
            self.item_bank.push(Item::of(self.toolbox.current()));
            changed = true;
        }
        if self.item_bank.is_empty() {
            inner.colored_label(theme.muted, tr!("kept-sub-tools-show-here-add"));
        }
        let mut add = None;
        let mut remove = None;
        egui::ScrollArea::vertical()
            .id_salt("sue-paint-bank")
            .show(&mut inner, |ui| {
                for (index, item) in self.item_bank.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(format!("{} ({})", item.name, item.tool.label()));
                        if ui.small_button(tr!("add-2")).clicked() {
                            add = Some(index);
                        }
                        if ui.small_button("x").clicked() {
                            remove = Some(index);
                        }
                    });
                }
            });
        if let Some(index) = add
            && let Some(item) = self.item_bank.get(index).cloned()
        {
            let mut sub =
                SubTool::new(item.tool, &item.name, 0, item.tip).stabilization(item.stabilization);
            sub.name = item.name;
            self.toolbox.add_sub(sub);
        }
        if let Some(index) = remove {
            self.item_bank.remove(index);
            changed = true;
        }
        if changed {
            self.save_extras(cx);
        }
    }
}
