//! The modifier keys on the canvas, as the Modifier Key Settings say
//! (`crate::modifier_keys`): what a held Ctrl, Shift, Alt or Space does to
//! a press before any tool sees it, which tool and sub tool the press then
//! goes to (a key may borrow one while it is held), how a selection
//! combines, and the brush size drag. The right and middle buttons borrow
//! a tool the same way, and a tool key held down picks its tool only until
//! it comes up.

use egui::{Modifiers, Pos2, Rect};
use sue_wm::AppletContext;

use super::Press;
use crate::modifier_keys::{Action, HOLD, HeldTool, Input, Keys, Legacy, ModifierKeys};
use crate::select::Op;
use crate::studio::{Gesture, SuePaint};
use crate::tools::{SubTool, Tool};

impl SuePaint {
    /// The modifier keys as set now: the file's rows, read on first use,
    /// over Sue's own, with the three old settings as Settings has them.
    pub(crate) fn modifier_map(&mut self, cx: &mut AppletContext<'_>) -> &ModifierKeys {
        let legacy = Legacy::of(cx.rice());
        if self.modifier_keys.is_none() {
            let text = crate::colorsets::folder(cx).and_then(|folder| {
                sue_platform::fs::read_to_string(folder.join(crate::modifier_keys::FILE)).ok()
            });
            let (map, unread) = text.map_or_else(
                || (ModifierKeys::new(legacy), 0),
                |text| ModifierKeys::parse(&text, legacy),
            );
            if unread > 0 {
                cx.notify(tr!("modifiers-lines-unread", count = unread));
            }
            self.modifier_keys = Some(map);
        }
        let map = self.modifier_keys.get_or_insert_with(ModifierKeys::default);
        map.set_legacy(legacy);
        map
    }

    /// Keeps `map` as the modifier keys: its rows in the file, the three
    /// rows the old settings can say in those settings.
    pub(crate) fn keep_modifier_map(&mut self, mut map: ModifierKeys, cx: &mut AppletContext<'_>) {
        map.fold_legacy();
        for (key, value) in crate::modifier_keys::LEGACY_KEYS
            .into_iter()
            .zip(map.legacy().values())
        {
            if cx.rice().get_str(key) != Some(value)
                && let Err(error) = cx.edit_rice(sue_rice::SetRiceValue::new(
                    key,
                    sue_rice::RiceValue::Text(value.to_owned()),
                ))
            {
                cx.fail(tr!("cannot-keep-the-modifier-keys", error = error));
            }
        }
        let written = crate::colorsets::folder(cx)
            .ok_or_else(|| tr!("sue-paint-has-no-data-folder"))
            .and_then(|folder| {
                sue_platform::fs::create_dir_all(&folder)
                    .and_then(|()| {
                        sue_platform::fs::write(
                            folder.join(crate::modifier_keys::FILE),
                            map.to_text(),
                        )
                    })
                    .map_err(|error| error.to_string())
            });
        if let Err(error) = written {
            cx.fail(tr!("cannot-keep-the-modifier-keys", error = error));
        }
        self.modifier_keys = Some(map);
    }

    /// The tool a press goes to with `keys` held, as the cursor shows it.
    pub(in crate::canvas) fn tool_under_keys(
        &mut self,
        keys: Keys,
        cx: &mut AppletContext<'_>,
    ) -> Tool {
        let tool = self.toolbox.tool();
        if keys == Keys::NONE {
            return tool;
        }
        match self.modifier_map(cx).action(tool, Input::Press, keys) {
            Action::Borrow { tool: borrowed, .. } => borrowed,
            _ => tool,
        }
    }

    /// The sub tool `tool` lends while a key borrows it: the one of
    /// `mode` on its palette entry (else Sue's own of that mode), or the
    /// one picked on it.
    fn borrowed_sub(&self, tool: Tool, mode: Option<u8>) -> SubTool {
        let picked = || {
            if tool == self.toolbox.tool() {
                self.toolbox.current().clone()
            } else {
                self.toolbox.of(tool).clone()
            }
        };
        let Some(mode) = mode else {
            return picked();
        };
        if picked().tool == tool && picked().mode == mode {
            return picked();
        }
        self.toolbox
            .subs(tool)
            .iter()
            .find(|sub| sub.tool == tool && sub.mode == mode)
            .cloned()
            .or_else(|| tool.sub_tools().into_iter().find(|sub| sub.mode == mode))
            .unwrap_or_else(picked)
    }

    /// A press at `pos` (on screen) as the held keys see it: `None` when
    /// a key's own gesture took it, else the tool and sub tool it goes to
    /// and where it landed.
    pub(super) fn press_keys(
        &mut self,
        pos: Pos2,
        modifiers: Modifiers,
        space: bool,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) -> Option<(Tool, SubTool, Press)> {
        let view = self.docs.get(self.active)?.view;
        // A tool borrowed by holding its key was used.
        if let Some(held) = self.held_tool.as_mut() {
            held.used = true;
        }
        let p = view.to_picture(viewport, size, pos);
        let current = self.toolbox.tool();
        let keys = Keys::of(modifiers, space);
        let action = if keys == Keys::NONE {
            Action::Own
        } else {
            self.modifier_map(cx).action(current, Input::Press, keys)
        };
        let borrowed = match action {
            Action::Borrow { tool, mode } => Some((tool, self.borrowed_sub(tool, mode))),
            _ => None,
        };
        // The view tools work whatever else is going on.
        let view_tool = borrowed
            .as_ref()
            .is_some_and(|(tool, _)| matches!(tool, Tool::Hand | Tool::Zoom));
        if !view_tool {
            if self.light_tool && !space && self.light_table_press(p, modifiers, cx) {
                return None;
            }
            // Picking the layer under the pointer and sizing the brush
            // come before a transform or a warp.
            if let Some((Tool::Operation, sub)) = &borrowed
                && sub.mode == 1
            {
                self.pick_layer_at(&cx.world.arena, p, sub.exclude);
                return None;
            }
            if action == Action::BrushSize && sized(self.toolbox.current()) {
                self.gesture = Some(Gesture::BrushSize {
                    start: pos.x,
                    size: self.toolbox.current().tip.size,
                });
                return None;
            }
            if self.transform.is_some() {
                self.transform_press(pos, view, viewport, size);
                return None;
            }
            if self.warp.is_some() {
                self.warp_press(pos, view, modifiers, viewport, size, cx);
                return None;
            }
            // Blend's Copy stamp: Alt and click sets where it copies from.
            if borrowed.is_none()
                && modifiers.alt
                && current == Tool::Blend
                && self.toolbox.current().mode == crate::blend::COPY_STAMP
            {
                self.copy_stamp_source(p, cx);
                return None;
            }
        }
        let (tool, sub, modifiers, op) = match borrowed {
            // A borrowed tool sees none of the keys that borrowed it.
            Some((tool, sub)) => (tool, sub, Modifiers::NONE, self.select_op),
            None => {
                let op = match (modifiers.shift, modifiers.alt) {
                    (true, true) => Op::Intersect,
                    (true, false) => Op::Add,
                    (false, true) => Op::Subtract,
                    (false, false) => self.select_op,
                };
                (current, self.toolbox.current().clone(), modifiers, op)
            }
        };
        Some((
            tool,
            sub,
            Press {
                pos,
                p,
                modifiers,
                op,
                space: view_tool && keys.space,
                viewport,
                size,
            },
        ))
    }

    /// A right-click (`Input::Right`) or a middle-click
    /// (`Input::Middle`) at `pos`: the tool it borrows takes the press, as
    /// the Modifier Key Settings say (the Eyedropper and the Hand unless
    /// changed). `false` when it does nothing.
    pub(in crate::canvas) fn press_button(
        &mut self,
        input: Input,
        pos: Pos2,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) -> bool {
        let tool = self.toolbox.tool();
        let Action::Borrow { tool, mode } = self.modifier_map(cx).action(tool, input, Keys::NONE)
        else {
            return false;
        };
        let Some(view) = self.docs.get(self.active).map(|doc| doc.view) else {
            return false;
        };
        let sub = self.borrowed_sub(tool, mode);
        let press = Press {
            pos,
            p: view.to_picture(viewport, size, pos),
            modifiers: Modifiers::NONE,
            op: self.select_op,
            space: false,
            viewport,
            size,
        };
        self.press_as(tool, sub, &press, cx);
        true
    }

    /// What the mouse wheel does with the keys held, as the Modifier Key
    /// Settings say: zoom, turn or scroll the view at `pointer`, or size
    /// the brush. `amount` is the wheel's turn (up is positive), `across`
    /// its sideways part.
    #[allow(clippy::too_many_arguments, reason = "one wheel turn and where")]
    pub(in crate::canvas) fn wheel(
        &mut self,
        keys: Keys,
        amount: f32,
        across: f32,
        pointer: Pos2,
        viewport: Rect,
        size: [u32; 2],
        cx: &mut AppletContext<'_>,
    ) {
        let tool = self.toolbox.tool();
        let action = self.modifier_map(cx).action(tool, Input::Wheel, keys);
        if action == Action::BrushSize {
            if sized(self.toolbox.current()) {
                let tip = &mut self.toolbox.current_mut().tip;
                tip.size = (tip.size * (amount * 0.0025).exp()).clamp(0.5, 2000.0);
            }
            return;
        }
        let Some(doc) = self.docs.get_mut(self.active) else {
            return;
        };
        match action {
            Action::Zoom => {
                if amount != 0.0 {
                    doc.view
                        .zoom_at(viewport, size, pointer, (amount * 0.0025).exp());
                }
                doc.view.pan.x += across;
            }
            Action::Rotate => doc.view.rotate_by(viewport, size, (amount + across) * 0.15),
            Action::Scroll => doc.view.pan += egui::vec2(across, amount),
            _ => {}
        }
    }

    /// Ctrl and Alt dragged sideways to `pos`: the brush size, larger to
    /// the right.
    pub(super) fn brush_size_drag(&mut self, pos: Pos2) {
        let (Some(Gesture::BrushSize { start, size }), Some(_)) =
            (self.gesture.as_mut(), self.docs.get_mut(self.active))
        else {
            return;
        };
        let factor = ((pos.x - *start) / 120.0).exp();
        let next = (*size * factor).clamp(0.5, 2000.0);
        self.toolbox.current_mut().tip.size = next;
    }

    /// The tool palette entry and its sub tool picked now, to come back
    /// to.
    pub(crate) fn tool_state(&self) -> (usize, usize) {
        let slot = self.toolbox.current_slot();
        (slot, self.toolbox.picked(self.toolbox.tool()))
    }

    /// The command `name` picked a tool from `before`: while the keys that
    /// ran it stay down it is only borrowed. Letting go after using it, or
    /// after holding the key a moment, goes back to the tool before; a tap
    /// keeps it. Which keys are down is read on the next frame
    /// ([`Self::follow_held_tool`]): a command run from a menu or a script
    /// has none, and picks the tool for good.
    pub(crate) fn hold_tool_key(&mut self, name: &str, before: (usize, usize)) {
        let picks = (name.starts_with("tool.")
            && !matches!(name, "tool.next_sub" | "tool.previous_sub"))
            || name.starts_with("subtool.");
        if !picks || self.tool_state() == before {
            return;
        }
        // A tool picked again while one is borrowed goes back to the
        // first tool.
        let before = self.held_tool.take().map_or(before, |held| held.before);
        self.held_tool = Some(HeldTool {
            command: name.to_owned(),
            keys: Vec::new(),
            before,
            since: f64::NAN,
            used: false,
        });
    }

    /// Follows a tool key held down: goes back to the tool before once the
    /// key is up (and what it began on the canvas has ended), when the
    /// tool was used or the key was held a moment.
    pub(crate) fn follow_held_tool(&mut self, ctx: &egui::Context, cx: &AppletContext<'_>) {
        let Some(held) = self.held_tool.as_mut() else {
            return;
        };
        // Just picked: the keys of its command that are down now are the
        // ones that picked it.
        if held.keys.is_empty() {
            let table = crate::studio::shortcut_table();
            let chords = sue_wm::shortcuts::resolve(cx.rice(), crate::studio::KEYS_SECTION, table);
            let (keys, now) = ctx.input(|input| {
                let keys = table
                    .iter()
                    .zip(&chords)
                    .filter(|(shortcut, _)| shortcut.command == held.command)
                    .flat_map(|(_, chords)| chords.chords().iter())
                    .filter_map(|chord| sue_wm::keys::egui_key(chord.key))
                    .filter(|key| input.key_down(*key))
                    .collect::<Vec<_>>();
                (keys, input.time)
            });
            if keys.is_empty() {
                self.held_tool = None;
            } else {
                held.keys = keys;
                held.since = now;
            }
            return;
        }
        let held = &*held;
        // What the tool began on the canvas ends with it.
        if self.gesture.is_some() {
            return;
        }
        let (down, now) =
            ctx.input(|input| (held.keys.iter().any(|key| input.key_down(*key)), input.time));
        if down {
            return;
        }
        let Some(held) = self.held_tool.take() else {
            return;
        };
        if held.used || now - held.since >= HOLD {
            let (slot, sub) = held.before;
            self.toolbox.pick_slot(slot);
            let tool = self.toolbox.tool();
            self.toolbox.pick_sub(tool, sub);
            ctx.request_repaint();
        }
    }
}

/// Whether `sub` has a brush size for Ctrl and Alt to drag: the tools that
/// paint, Blend (Liquify too) and the selection pens.
fn sized(sub: &SubTool) -> bool {
    sub.tool.paints() || sub.tool == Tool::Blend || (sub.tool == Tool::Select && sub.mode >= 4)
}
