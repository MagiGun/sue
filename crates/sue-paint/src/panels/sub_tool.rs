//! The Sub Tool palette (its group tabs, its list of sub tools with their
//! sample strokes, the "Add Sub Tool" row and the buttons under the list)
//! and the Tool Property palette's header.
//!
//! A sub tool is dragged to another place in its list, onto a group tab
//! to move it into that group, or onto the Tool palette to make it a tool
//! of its own; a tool dragged here from the Tool palette goes back into
//! the tool shown. Right-click a sub tool for everything else.

use egui::{Align2, Color32, Id, Pos2, Rect, RichText, Sense, Ui, pos2, vec2};
use sue_wm::icons::Icon;
use sue_wm::ui::{Density, Radius, Space, TextRole};
use sue_wm::{AppletContext, ItemState, MenuItem, ShellRequest, Theme};

use crate::color;
use crate::commands::subtool;
use crate::panels::child;
use crate::panels::sub_tool_detail::NamingKind;
use crate::panels::tool::ToolDrag;
use crate::studio::SuePaint;
use crate::tools::SubTool;

/// How tall the Sub Tool palette's list shows in `room`: all of it when
/// its `count` rows of `tall` points (and the Add Sub Tool row of `add`
/// points) fit, else as many whole rows as fit, so no row is cut through
/// and the scroll bar says there are more.
fn list_height(count: usize, tall: f32, add: f32, spacing: f32, room: f32) -> f32 {
    let pitch = tall + spacing;
    let whole = count as f32 * pitch + add;
    if whole <= room {
        return room;
    }
    ((room + spacing) / pitch).floor().max(1.0) * pitch - spacing
}

/// Where a sub tool dragged in the list would land: before sub tool
/// `index` of the entry shown (or after the last when it is the list's
/// length).
fn landing(rows: &[(usize, Rect)], pointer: Pos2) -> Option<usize> {
    let (first, last) = (rows.first()?, rows.last()?);
    if pointer.y > last.1.bottom() {
        return Some(last.0 + 1);
    }
    rows.iter()
        .find(|(_, rect)| pointer.y < rect.center().y)
        .map(|(index, _)| *index)
        .or(Some(first.0))
}

impl SuePaint {
    pub(crate) fn tool_panel(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        use crate::dock::Palette;
        let tabs = [
            (
                tr!("tool-group"),
                Palette::SubTool.symbol(),
                self.panels.sub_tool,
                None,
            ),
            (
                tr!("tool-settings"),
                Palette::ToolProperty.symbol(),
                self.panels.tool_property,
                Some(Palette::ToolProperty),
            ),
            (
                tr!("brush-size"),
                Palette::BrushGrid.symbol(),
                true,
                Some(Palette::BrushGrid),
            ),
        ];
        let (body, tab) = self.torn_header(ui, rect, &tabs, self.tool_tab, theme);
        self.tool_tab = tab;
        if !self.panels.sub_tool && self.tool_tab == 0 && !self.dock.is_open(Palette::ToolProperty)
        {
            self.tool_tab = 1;
        } else if !self.panels.tool_property && self.tool_tab == 1 {
            self.tool_tab = 0;
        }
        self.tool_panel_body(ui, body, cx, theme);
    }

    /// The Sub Tool palette's tab showing (`tool_tab`), in `body`.
    pub(crate) fn tool_panel_body(
        &mut self,
        ui: &mut Ui,
        body: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        if self.tool_tab == 2 {
            let pad = theme.space(Space::S3);
            self.brush_size_grid(ui, body.shrink(pad), theme);
            return;
        }
        if self.tool_tab == 0 {
            self.sub_tool_list(ui, body, cx, theme);
            return;
        }
        self.tool_property_body(ui, body, cx, theme);
    }

    /// The Sub Tool palette: the group tabs of the tool out, the sub tools
    /// of the group shown, the "Add Sub Tool" row, and the buttons under
    /// the list.
    fn sub_tool_list(
        &mut self,
        ui: &mut Ui,
        body: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let pad = theme.space(Space::S3);
        let half_pad = pad / 2.0;
        let density = Density::of(ui);
        let footer = density.icon_button() + pad;
        let area = Rect::from_min_max(body.min, pos2(body.right(), body.bottom() - footer));
        let mut inner = child(ui, area.shrink(pad), "tool-panel");
        let slot = self.toolbox.current_slot();
        let entry = self.toolbox.current_entry().clone();
        let picked = entry.picked();
        let shown_group = entry.group_of(picked);
        let pointer = inner.ctx().pointer_latest_pos();
        let released = inner.ctx().input(|input| input.pointer.any_released());
        let dragging = egui::DragAndDrop::payload::<ToolDrag>(inner.ctx()).map(|drag| *drag);
        // The group tabs, as chips that wrap.
        let mut chips: Vec<(usize, Rect)> = Vec::new();
        let mut menu_on_group = None;
        inner.horizontal_wrapped(|ui| {
            for (group, found) in entry.groups().iter().enumerate() {
                let name = crate::localize(&found.name);
                let chip = sue_wm::ui::chip(
                    ("sue-paint-sub-group", slot, group),
                    &name,
                    group == shown_group,
                )
                .show(ui, theme);
                let response = chip.response.on_hover_text(&name);
                chips.push((group, response.rect));
                if response.clicked() && group != shown_group {
                    let first = entry.group_range(group).start;
                    subtool::pick(&mut self.toolbox, slot, first);
                }
                let items = [MenuItem::Action {
                    label: tr!("rename-sub-tool-group"),
                    command: "rename".to_owned(),
                    shortcut: String::new(),
                    note: String::new(),
                }];
                if sue_wm::ui::context_menu(ui, &response, &items, &|_| ItemState::on(), theme)
                    .is_some()
                {
                    menu_on_group = Some(group);
                }
            }
        });
        if let Some(group) = menu_on_group {
            self.open_naming(NamingKind::RenameGroup { slot, group });
        }
        inner.add_space(theme.space(Space::S2));
        // The sub tools of the group shown.
        let range = entry.group_range(shown_group);
        let names: Vec<String> = entry.subs()[range.clone()]
            .iter()
            .map(|sub| crate::localize(&sub.name))
            .collect();
        // Sample strokes when the entry is a painting tool's, whichever
        // sub tool is picked (Fill's Lasso fill is a Figure sub tool).
        let paints = entry.kind.paints();
        let line = density.row();
        let mut rows: Vec<(usize, Rect)> = Vec::new();
        let mut action: Option<(String, usize)> = None;
        let mut add_row = None;
        let keys = subtool::keys(cx.rice());
        // Solid bars: the list says when it holds more than shows. (They
        // are the area's own style, so it is set before the area.)
        inner.spacing_mut().scroll = egui::style::ScrollStyle::solid();
        // The list scrolls to the chosen sub tool when the choice changes
        // (a key, a click on the canvas's launcher, another tool), once,
        // so it can still be scrolled away by hand.
        let followed = Id::unique(("sue-paint-subs-followed", slot, shown_group));
        let follow = inner.ctx().data(|data| data.get_temp::<usize>(followed)) != Some(picked);
        // A painting tool's row holds its sample stroke and its name on
        // one line, as tall as a plain row, so a palette of ordinary
        // height lists five or six of them.
        let tall = line;
        let height = list_height(
            names.len(),
            tall,
            line,
            inner.spacing().item_spacing.y,
            inner.available_height(),
        );
        egui::ScrollArea::vertical()
            .id_salt(("sue-paint-subs", slot, shown_group))
            .auto_shrink([false, false])
            .max_height(height)
            .show(&mut inner, |ui| {
                for (offset, name) in names.iter().enumerate() {
                    let index = range.start + offset;
                    let sub = &entry.subs()[index];
                    let chosen = index == picked;
                    let (row, response) = ui.allocate_exact_size(
                        vec2(ui.available_width(), tall),
                        Sense::click_and_drag(),
                    );
                    rows.push((index, row));
                    if chosen && follow {
                        ui.scroll_to_rect(row, None);
                        ui.ctx().data_mut(|data| data.insert_temp(followed, picked));
                    }
                    let command = subtool::command_name(sub.tool, &sub.name);
                    let key = subtool::key_text(&keys, &command);
                    let tip = match &key {
                        Some(key) => format!("{name} ({key})"),
                        None => name.clone(),
                    };
                    let response = response.on_hover_text(&tip);
                    response.widget_info(|| {
                        egui::WidgetInfo::selected(egui::Role::ListItem, true, chosen, &tip)
                    });
                    let radius = theme.radius(Radius::Sm);
                    let fill = if chosen {
                        theme.fill_selected()
                    } else if response.hovered() {
                        theme.fill_hover()
                    } else {
                        Color32::TRANSPARENT
                    };
                    ui.painter().rect_filled(
                        row.shrink2(egui::Vec2::Y * (theme.space(Space::S1) / 2.0)),
                        radius,
                        fill,
                    );
                    // The key that picks it, at the row's end, then the
                    // name before it.
                    let mut end = row.right() - pad;
                    if let Some(key) = &key {
                        let shown = ui.painter().text(
                            pos2(end, row.center().y),
                            Align2::RIGHT_CENTER,
                            key,
                            theme.text_font(TextRole::Micro),
                            // Readable on the chosen row's fill too.
                            if fill == Color32::TRANSPARENT {
                                theme.text_muted()
                            } else {
                                theme.text_muted_on(fill)
                            },
                        );
                        end = shown.left() - theme.space(Space::S2);
                    }
                    if paints {
                        let named = ui.painter().text(
                            pos2(end, row.center().y),
                            Align2::RIGHT_CENTER,
                            name,
                            theme.text_font(TextRole::Body),
                            theme.text,
                        );
                        // Every row's stroke is as long, so they line up,
                        // and stops before a long name.
                        let room = (row.width() * 0.58)
                            .min(named.left() - row.left() - pad)
                            .max(row.width() * 0.3);
                        // Drawn at the display's own pixels, so the
                        // stroke is as sharp at 2.0 scale as at 1.0.
                        let scale = ui.ctx().pixels_per_point();
                        let high = tall - half_pad * 2.0;
                        let points = vec2(room, high);
                        let size = [
                            ((points.x * scale).round() as u32).clamp(40, 1800),
                            ((points.y * scale).round() as u32).max(8),
                        ];
                        if let Some(texture) =
                            Self::sub_preview(&mut self.sub_previews, cx, sub, size, scale)
                        {
                            ui.painter().image(
                                texture,
                                Rect::from_min_size(row.min + vec2(half_pad, half_pad), points),
                                Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                                if chosen {
                                    theme.text
                                } else {
                                    theme.text_muted()
                                },
                            );
                        }
                    } else {
                        ui.painter().text(
                            row.left_center() + egui::Vec2::X * pad,
                            Align2::LEFT_CENTER,
                            name,
                            theme.text_font(TextRole::Body),
                            theme.text,
                        );
                    }
                    sue_wm::ui::focus_ring(ui, &response, radius, theme);
                    if response.clicked() {
                        subtool::pick(&mut self.toolbox, slot, index);
                        // Tool Property is a palette of its own beside
                        // this one; a phone's sheet goes to its tab.
                        if self.sheet.is_some() {
                            self.tool_tab = 1;
                        }
                    }
                    if response.drag_started() {
                        egui::DragAndDrop::set_payload(ui.ctx(), ToolDrag::Sub { slot, index });
                    }
                    if let Some(picked) = sue_wm::ui::context_menu(
                        ui,
                        &response,
                        &sub_tool_menu(),
                        &|command| ItemState {
                            enabled: command != "delete" || entry.subs().len() > 1,
                            checked: None,
                        },
                        theme,
                    ) {
                        action = Some((picked, index));
                    }
                }
                // Every list ends with a row that adds sub tools.
                let label = tr!("add-sub-tool");
                let row = sue_wm::ui::list_row(("sue-paint-add-sub-tool", slot), &label)
                    .icon(Icon::Plus)
                    .show(ui, theme);
                if row
                    .response
                    .on_hover_text(tr!("brushes-from-clip-studio-photoshop-gimp"))
                    .clicked()
                {
                    add_row = Some(());
                }
            });
        if add_row.is_some() {
            cx.request(ShellRequest::PickFile(
                sue_wm::FilePick::open(
                    Some(sue_wm::AppletId::new(crate::ID).expect("a valid id")),
                    "brush-import",
                    &tr!("add-sub-tool-from-file"),
                )
                .extensions(&["sut", "abr", "gbr", "gih", "png"]),
            ));
        }
        // A drag over the list: where it would land.
        if let (Some(drag), Some(pointer)) = (dragging, pointer)
            && area.contains(pointer)
        {
            let over_chip = chips.iter().find(|(_, rect)| rect.contains(pointer));
            match (drag, over_chip) {
                (ToolDrag::Sub { slot: from, index }, Some((group, rect))) => {
                    ui.painter().rect_stroke(
                        *rect,
                        theme.radius(Radius::Md),
                        theme.focus_stroke(),
                        egui::StrokeKind::Outside,
                    );
                    if released {
                        egui::DragAndDrop::clear_payload(ui.ctx());
                        self.toolbox.move_sub(from, index, slot, *group, usize::MAX);
                    }
                }
                (ToolDrag::Sub { slot: from, index }, None) if from == slot => {
                    if let Some(at) = landing(&rows, pointer) {
                        let y = rows.iter().find(|(found, _)| *found == at).map_or_else(
                            || rows.last().map_or(area.top(), |(_, rect)| rect.bottom()),
                            |(_, rect)| rect.top(),
                        );
                        ui.painter()
                            .hline(area.shrink(pad).x_range(), y, theme.focus_stroke());
                        if released {
                            egui::DragAndDrop::clear_payload(ui.ctx());
                            let place = if at > index { at - 1 } else { at };
                            let place = place.saturating_sub(range.start);
                            self.toolbox.move_sub(slot, index, slot, shown_group, place);
                        }
                    }
                }
                (ToolDrag::Slot(from), _) if from != slot => {
                    ui.painter().rect_stroke(
                        area,
                        theme.radius(Radius::Md),
                        theme.focus_stroke(),
                        egui::StrokeKind::Inside,
                    );
                    if released {
                        egui::DragAndDrop::clear_payload(ui.ctx());
                        self.toolbox.demote_slot(from, slot);
                    }
                }
                _ => {}
            }
        }
        if let Some((what, index)) = action {
            self.sub_tool_action(&what, slot, index);
        }
        // The buttons under the list, at its trailing edge, where Clip
        // Studio Paint has them.
        let bar = Rect::from_min_max(pos2(body.left(), body.bottom() - footer), body.max)
            .shrink2(egui::Vec2::X * pad);
        self.sub_tool_buttons(ui, bar, slot, theme);
    }

    /// The Sub Tool palette's reset, duplicate and delete buttons, right
    /// aligned in `bar`.
    fn sub_tool_buttons(&mut self, ui: &mut Ui, bar: Rect, slot: usize, theme: &Theme) {
        let mut bar_ui = child(ui, bar, "sub-tool-buttons");
        bar_ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let picked = self.toolbox.current_entry().picked();
            for (what, icon, label, enabled) in [
                (
                    "delete",
                    Icon::Delete,
                    tr!("delete-sub-tool"),
                    self.toolbox.current_entry().subs().len() > 1,
                ),
                (
                    "duplicate",
                    Icon::Duplicate,
                    tr!("duplicate-sub-tool"),
                    true,
                ),
                (
                    "reset",
                    Icon::Reset,
                    tr!("reset-to-initial-settings"),
                    self.can_reset_current() && !self.toolbox.current().locked,
                ),
            ] {
                if sue_wm::ui::icon_button(("sue-paint-sub-tool-button", what), icon, &label)
                    .enabled(enabled)
                    .quiet()
                    .show(ui, theme)
                    .clicked()
                {
                    self.sub_tool_action(what, slot, picked);
                }
            }
        });
    }

    /// Does what the Sub Tool palette's menu or its buttons asked of sub
    /// tool `index` of tool palette entry `slot`.
    fn sub_tool_action(&mut self, what: &str, slot: usize, index: usize) {
        let Some(entry) = self.toolbox.slots().get(slot) else {
            return;
        };
        let kind = entry.kind;
        match what {
            "duplicate" => self.open_naming(NamingKind::Duplicate { slot, index }),
            "rename" => self.open_naming(NamingKind::RenameSub { slot, index }),
            "group" => self.open_naming(NamingKind::NewGroup { slot, index }),
            "delete" => {
                subtool::pick(&mut self.toolbox, slot, index);
                self.toolbox.delete_sub(kind, index);
            }
            "reset" => {
                subtool::pick(&mut self.toolbox, slot, index);
                self.toolbox.reset_sub(kind, index);
            }
            "tool" => {
                if let Some(at) = self.toolbox.promote_sub(slot, index, slot + 1) {
                    self.toolbox.pick_slot(at);
                }
            }
            "detail" => {
                subtool::pick(&mut self.toolbox, slot, index);
                self.show_palette(crate::dock::Palette::SubToolDetail);
            }
            _ => {}
        }
    }

    /// Tool Property: the sub tool's name with its lock, reset and Sub
    /// Tool Detail buttons, a sample stroke for the painting tools, then
    /// its settings.
    fn tool_property_body(
        &mut self,
        ui: &mut Ui,
        body: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let tool = self.toolbox.tool();
        let pad = theme.space(Space::S4);
        let mut inner = child(
            ui,
            body.shrink2(vec2(pad, theme.space(Space::S3))),
            "tool-panel",
        );
        // A gradient layer edited from Tool Property, or an open
        // transform, is named in the title, since the rows below are the
        // layer's or the transform's, not the sub tool's.
        let transforming = self.transform_title();
        let sub_name = match (&transforming, self.edited_gradient_layer(&cx.world.arena)) {
            (Some(name), _) => name.clone(),
            (None, Some(layer)) => self
                .layer_info(&cx.world.arena)
                .filter(|info| info.id == layer)
                .map_or_else(|| tr!("gradient"), |info| info.name),
            (None, None) => crate::localize(&self.toolbox.current().name),
        };
        inner.horizontal(|ui| {
            ui.label(
                RichText::new(&sub_name)
                    .size(theme.text_size(TextRole::Body))
                    .strong()
                    .color(theme.text),
            );
            if transforming.is_some() {
                return;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let detail = tr!("open-sub-tool-detail");
                if sue_wm::ui::icon_button("sue-paint-open-detail", Icon::Wrench, &detail)
                    .quiet()
                    .show(ui, theme)
                    .clicked()
                {
                    self.show_palette(crate::dock::Palette::SubToolDetail);
                }
                self.sub_tool_header_buttons(ui, theme);
            });
        });
        if self.toolbox.current().locked && transforming.is_none() {
            inner.label(
                RichText::new(tr!("sub-tool-settings-locked"))
                    .size(theme.text_size(TextRole::Caption))
                    .color(theme.text_muted()),
            );
        }
        if tool.paints() && transforming.is_none() {
            self.stroke_preview(&mut inner, cx, theme);
        }
        let command = egui::ScrollArea::vertical()
            .id_salt("sue-paint-props")
            .show(&mut inner, |ui| {
                self.tool_properties(ui, tool, &cx.world.arena, theme)
            })
            .inner;
        if let Some(command) = command {
            self.run(command, cx);
        }
        if let Some(command) = self.pending_command.take() {
            self.run(command, cx);
        }
        // Effect lines from Tool Property, drawn again as one step.
        if let Some((layer, lines)) = self.pending_effect.take() {
            self.redraw_effect(cx, layer, &lines);
        }
        // Picked vector lines from Tool Property: shown now, kept when let
        // go.
        let held = inner.ctx().input(|input| input.pointer.any_down());
        self.line_changes(held, cx);
        // The Gradient tool's sets, and a gradient layer from Tool
        // Property: shown now, kept when let go.
        self.gradient_frame(held, cx);
        // A 3D layer's view from Tool Property: shown now, kept when let go.
        if let Some((layer, view)) = self.pending_view3d.take() {
            self.preview_view3d(cx, layer, &view);
        }
        // A figure's pose from Tool Property: shown now, kept when let go.
        if let Some((layer, pose)) = self.pending_pose.take() {
            self.preview_pose(cx, layer, &pose);
        }
        if std::mem::take(&mut self.pose_release)
            && let Some((layer, base, before)) = self.pose_before.take()
        {
            self.finish_pose(cx, layer, &base, &before);
        }
        if let Some((layer, body)) = self.pending_body.take()
            && let Some(edit) =
                crate::figure::set_body(&cx.world.arena, layer, &body, &tr!("body-shape"))
        {
            Self::apply(cx, edit);
        }
        if std::mem::take(&mut self.view3d_release)
            && let Some((layer, base, before)) = self.view3d_before.take()
        {
            self.finish_view3d(cx, layer, base, &before);
        }
    }

    /// The texture of `sub`'s sample stroke for the Sub Tool list, `size`
    /// pixels at `scale` pixels to the point: white ink on clear, tinted
    /// when drawn.
    fn sub_preview(
        cache: &mut std::collections::HashMap<u64, Vec<u8>>,
        cx: &mut AppletContext<'_>,
        sub: &SubTool,
        size: [u32; 2],
        scale: f32,
    ) -> Option<egui::TextureId> {
        use std::hash::{Hash, Hasher};
        let sub = Self::list_sample(sub, size, scale);
        let key = {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            format!("{:?}{}{:?}{size:?}", sub.tool, sub.name, sub.tip).hash(&mut hasher);
            hasher.finish()
        };
        if !cache.contains_key(&key) {
            if cache.len() > 256 {
                cache.clear();
            }
            cache.insert(key, Self::render_stroke(&sub, size)?);
        }
        let pixels = cache.get(&key)?;
        cx.textures
            .texture(0x5355_4253_5452_4b45 ^ key, 1, size, 4, pixels, false)
    }

    /// `sub` as its sample in the Sub Tool list is drawn, `size` pixels
    /// at `scale` pixels to the point.
    fn list_sample(sub: &SubTool, size: [u32; 2], scale: f32) -> SubTool {
        let mut sub = sub.clone();
        // Thin pens stay thin and big brushes fit the row.
        sub.tip.size = (sub.tip.size * 0.7 * scale).clamp(scale, size[1] as f32 * 0.4);
        // An eraser's sample is drawn as ink, so it shows.
        sub.tip.eraser = false;
        sub
    }

    /// A sample stroke of `sub` in white over nothing, `size` pixels.
    fn render_stroke(sub: &SubTool, [width, height]: [u32; 2]) -> Option<Vec<u8>> {
        let mut raster =
            sue_core::RasterTile2D::new(width, height, sue_core::PixelFormat::Rgba8).ok()?;
        let points: Vec<[f32; 2]> = (0..=48)
            .map(|step| {
                let t = step as f32 / 48.0;
                [
                    10.0 + t * (width as f32 * 0.62),
                    height as f32 * (0.42 - 0.2 * (t * std::f32::consts::TAU * 0.8).sin()),
                ]
            })
            .collect();
        sample_stroke(sub, [1.0; 3], &points, &mut raster)?;
        // White ink by its cover alone, premultiplied as textures are
        // drawn. The brush engine may leave color in pixels it cleared
        // again (a watercolor edge does, a tile at a time); sent as they
        // were, those lit up as a comb of bars beside the stroke.
        let bytes = raster.bytes()?;
        Some(
            bytes
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|pixel| [pixel[3]; 4])
                .collect(),
        )
    }

    /// A sample stroke drawn with the current brush, over the light
    /// ground strokes are judged on (the picture's paper, not the theme).
    pub(crate) fn stroke_preview(
        &mut self,
        ui: &mut Ui,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let height = theme.space(Space::S9) + theme.space(Space::S2);
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
        let mut sub = self.toolbox.current().clone();
        let (width, pixels_high) = (200_u32, 44_u32);
        sub.tip.size = sub.tip.size.clamp(1.0, 30.0);
        let key = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            format!(
                "{:?}{}{:?}{:?}",
                sub.tool,
                sub.name,
                sub.tip,
                self.drawing_color()
            )
            .hash(&mut hasher);
            hasher.finish()
        };
        if self
            .preview
            .as_ref()
            .is_none_or(|(cached, _)| *cached != key)
        {
            self.preview = Self::render_preview(
                &sub,
                color::unit(self.drawing_color()),
                [width, pixels_high],
            )
            .map(|pixels| (key, pixels));
        }
        let Some((_, shown)) = &self.preview else {
            return;
        };
        if let Some(texture) = cx.textures.texture(
            0x5355_4550_5245_5649,
            key,
            [width, pixels_high],
            4,
            shown,
            false,
        ) {
            let image = Rect::from_center_size(
                rect.center(),
                vec2(
                    rect.width(),
                    rect.width() * pixels_high as f32 / width as f32,
                )
                .min(rect.size()),
            );
            // Untinted: the sample is drawn on its own ground.
            egui::Image::from_texture((texture, image.size())).paint_at(ui, image);
            ui.painter().rect_stroke(
                image,
                theme.radius(Radius::Sm),
                theme.hairline(),
                egui::StrokeKind::Inside,
            );
        }
    }

    /// A sample stroke of `sub` in `rgb` over the light preview ground.
    fn render_preview(sub: &SubTool, rgb: [f32; 3], [width, height]: [u32; 2]) -> Option<Vec<u8>> {
        let mut raster =
            sue_core::RasterTile2D::new(width, height, sue_core::PixelFormat::Rgba8).ok()?;
        // An eraser shows what it takes out of a dark ground.
        if sub.tip.eraser
            && let Some(bytes) = raster.bytes_mut()
        {
            for pixel in bytes.as_chunks_mut::<4>().0 {
                *pixel = [40, 40, 40, 255];
            }
        }
        let points: Vec<[f32; 2]> = (0..=40)
            .map(|step| {
                let t = step as f32 / 40.0;
                [
                    12.0 + t * (width as f32 - 24.0),
                    height as f32 * (0.5 - 0.28 * (t * std::f32::consts::TAU).sin()),
                ]
            })
            .collect();
        sample_stroke(sub, rgb, &points, &mut raster)?;
        let bytes = raster.bytes()?;
        let mut shown = Vec::with_capacity(bytes.len());
        for pixel in bytes.as_chunks::<4>().0 {
            let alpha = f32::from(pixel[3]) / 255.0;
            for value in &pixel[..3] {
                shown.push((f32::from(*value) * alpha + 236.0 * (1.0 - alpha)).round() as u8);
            }
            shown.push(255);
        }
        Some(shown)
    }
}

/// Paints a sample stroke of `sub` in `rgb` along `points` onto `raster`
/// with the brush engine, exactly as the canvas paints the sub tool (its
/// whole built-in brush: spray, pictures, paper, mixing), pressed lightly
/// at its ends and fully in its middle. `None` for a sub tool that does
/// not paint.
fn sample_stroke(
    sub: &SubTool,
    rgb: [f32; 3],
    points: &[[f32; 2]],
    raster: &mut sue_core::RasterTile2D,
) -> Option<()> {
    let mut brush = crate::canvas::engine::brush_of(sub, false, false)?;
    // A sample is drawn as laid, not steadied or flicked.
    brush.preset.input = sue_model::brush::InputSettings::default();
    brush.preset.erase = sub.tip.eraser;
    let mut line = crate::vector::figure_engine_line(points, sub.tip, [rgb, rgb], false);
    let engine = line.engine.as_mut()?;
    engine.brush_hash = brush.hash();
    engine.brush = Some(std::sync::Arc::new(brush));
    engine.device = sue_brush::input::Device::Pen;
    let count = engine.pen.len().max(2) - 1;
    for (index, part) in engine.pen.iter_mut().enumerate() {
        let t = index as f32 / count as f32;
        part.pressure = Some((t * std::f32::consts::PI).sin().max(0.05));
    }
    line.paint_into(raster, false, None);
    Some(())
}

/// The Sub Tool palette's right-click menu: Clip Studio Paint's items,
/// each a word the palette acts on.
fn sub_tool_menu() -> Vec<MenuItem> {
    let item = |label: String, command: &str| MenuItem::Action {
        label,
        command: command.to_owned(),
        shortcut: String::new(),
        note: String::new(),
    };
    vec![
        item(tr!("duplicate-sub-tool-menu"), "duplicate"),
        item(tr!("rename-sub-tool"), "rename"),
        item(tr!("delete-sub-tool"), "delete"),
        item(tr!("reset-to-initial-settings"), "reset"),
        MenuItem::Separator,
        item(tr!("new-group-from-here"), "group"),
        item(tr!("make-into-a-tool"), "tool"),
        MenuItem::Separator,
        item(tr!("sub-tool-detail-menu"), "detail"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{Tool, Toolbox};

    /// The columns of a `width` wide sample between the first and the
    /// last inked one that hold no ink at all: a stroke drawn in one go
    /// has none.
    fn empty_columns(pixels: &[u8], width: usize) -> Vec<usize> {
        let inked: Vec<bool> = (0..width)
            .map(|x| pixels.chunks(width * 4).any(|row| row[x * 4 + 3] > 0))
            .collect();
        let first = inked.iter().position(|ink| *ink).unwrap_or(0);
        let last = inked.iter().rposition(|ink| *ink).unwrap_or(0);
        (first..=last).filter(|x| !inked[*x]).collect()
    }

    /// The first and the last column of a `width` wide sample with ink.
    fn span(pixels: &[u8], width: usize) -> (usize, usize) {
        let inked = |x: usize| pixels.chunks(width * 4).any(|row| row[x * 4 + 3] > 0);
        (
            (0..width).find(|x| inked(*x)).unwrap_or(0),
            (0..width).rfind(|x| inked(*x)).unwrap_or(0),
        )
    }

    /// The Sub Tool list's sample of every built-in sub tool that lays
    /// a line (pens, pencils, paint brushes, erasers and the airbrushes
    /// that do not spray drops) is one stroke from end to end at every
    /// size a row has: no column inside it is empty, and nothing shows
    /// beside it. The watercolor brushes came out as a comb of bars:
    /// the color the engine left in pixels it had cleared again.
    #[test]
    fn every_line_sample_in_the_list_is_one_stroke() {
        let toolbox = Toolbox::default();
        let mut failures = Vec::new();
        for tool in [
            Tool::Pen,
            Tool::Pencil,
            Tool::Brush,
            Tool::Airbrush,
            Tool::Decoration,
            Tool::Eraser,
        ] {
            for sub in toolbox.subs(tool) {
                // Sprays and dots are dots by nature.
                let dots = tool == Tool::Decoration
                    || [
                        "Spray",
                        "Droplet",
                        "Tone scraping",
                        "Running color",
                        "Pointillism",
                    ]
                    .iter()
                    .any(|name| sub.name.contains(name));
                for (size, scale) in [
                    ([120_u32, 20_u32], 1.0_f32),
                    ([137, 22], 1.0),
                    ([164, 24], 1.0),
                    ([205, 28], 1.25),
                    ([274, 44], 2.0),
                    ([40, 8], 1.0),
                ] {
                    let sample = SuePaint::list_sample(sub, size, scale);
                    let pixels = SuePaint::render_stroke(&sample, size).expect("a sample");
                    // Nothing shows where the stroke is not: no color
                    // without cover.
                    assert!(
                        pixels.chunks(4).all(|pixel| pixel[..3] == [pixel[3]; 3]),
                        "{} at {size:?}: white by its cover alone",
                        sub.name
                    );
                    if dots {
                        continue;
                    }
                    // A grainy pencil's faint ends may skip a pixel of
                    // paper; its body may not.
                    let all = empty_columns(&pixels, size[0] as usize);
                    let (first, last) = span(&pixels, size[0] as usize);
                    let ends = (last - first) / 8;
                    let empty: Vec<usize> = all
                        .into_iter()
                        .filter(|x| *x > first + ends && *x < last - ends)
                        .collect();
                    if !empty.is_empty() {
                        failures.push(format!(
                            "{} at {size:?}: {} empty columns",
                            sub.name,
                            empty.len()
                        ));
                        if let Ok(dir) = std::env::var("SUE_SUB_FAILURES") {
                            let name = format!("{}-{}x{}.png", sub.name, size[0], size[1]);
                            let png = sue_io::encode_png_rgba(size[0], size[1], &pixels).unwrap();
                            std::fs::write(std::path::Path::new(&dir).join(name), png).unwrap();
                        }
                    }
                }
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }

    /// Every built-in brush sub tool's sample stroke paints, with the
    /// brush engine and the sub tool's whole built-in brush: a sub tool
    /// whose brush has paint behaviours (spray, pictures, paper) shows
    /// them, so two such sub tools of one tip size show apart.
    #[test]
    fn every_brush_sample_paints_with_its_own_brush() {
        let toolbox = Toolbox::default();
        let mut seen = std::collections::HashSet::new();
        for tool in [
            Tool::Pen,
            Tool::Pencil,
            Tool::Brush,
            Tool::Airbrush,
            Tool::Decoration,
            Tool::Eraser,
        ] {
            for sub in toolbox.subs(tool) {
                let mut sub = sub.clone();
                sub.tip.eraser = false;
                let pixels = SuePaint::render_stroke(&sub, [160, 40]).expect("a sample");
                let inked = pixels.chunks(4).filter(|pixel| pixel[3] > 0).count();
                assert!(inked > 40, "{}: the sample shows ({inked})", sub.name);
                seen.insert(pixels);
            }
        }
        assert!(seen.len() > 40, "the samples differ: {}", seen.len());
        // The current brush's sample: an eraser takes from a dark ground.
        let eraser = toolbox.subs(Tool::Eraser)[0].clone();
        assert!(eraser.tip.eraser);
        let shown = SuePaint::render_preview(&eraser, [0.0; 3], [200, 44]).expect("a sample");
        assert!(
            shown.chunks(4).any(|pixel| pixel[0] > 200),
            "the eraser shows the light ground through the dark"
        );
    }

    /// Writes every brush sub tool's sample (as the Sub Tool list tints
    /// it on a dark and a light theme, and the current brush's preview)
    /// to the PNG `SUE_SUB_SAMPLES` names, for a person to look at.
    #[test]
    #[ignore = "writes the Sub Tool samples for a person to look at"]
    fn sub_tool_samples_sheet() {
        let Ok(path) = std::env::var("SUE_SUB_SAMPLES") else {
            return;
        };
        let toolbox = Toolbox::default();
        let (cell, preview) = ([240_u32, 34_u32], [200_u32, 44_u32]);
        let width = cell[0] * 2 + preview[0];
        let mut rows: Vec<Vec<u8>> = Vec::new();
        for tool in [
            Tool::Pen,
            Tool::Pencil,
            Tool::Brush,
            Tool::Airbrush,
            Tool::Decoration,
            Tool::Eraser,
        ] {
            for sub in toolbox.subs(tool) {
                let mut sample = sub.clone();
                sample.tip.size = (sample.tip.size * 0.7).clamp(1.0, cell[1] as f32 * 0.4);
                sample.tip.eraser = false;
                let white = SuePaint::render_stroke(&sample, cell).unwrap();
                let mut current = sub.clone();
                current.tip.size = current.tip.size.clamp(1.0, 30.0);
                let shown = SuePaint::render_preview(&current, [0.1, 0.1, 0.12], preview).unwrap();
                for y in 0..preview[1] as usize {
                    let mut row = Vec::with_capacity(width as usize * 4);
                    for (ground, ink) in [
                        ([24_u8, 24, 26], [230_u8, 230, 230]),
                        ([246, 244, 238], [40, 40, 44]),
                    ] {
                        for x in 0..cell[0] as usize {
                            let a = if y < cell[1] as usize {
                                f32::from(white[(y * cell[0] as usize + x) * 4 + 3]) / 255.0
                            } else {
                                0.0
                            };
                            for c in 0..3 {
                                let v = f32::from(ink[c]) * a + f32::from(ground[c]) * (1.0 - a);
                                row.push(v.round() as u8);
                            }
                            row.push(255);
                        }
                    }
                    let i = y * preview[0] as usize * 4;
                    row.extend_from_slice(&shown[i..i + preview[0] as usize * 4]);
                    rows.push(row);
                }
            }
        }
        let bytes: Vec<u8> = rows.concat();
        let png = sue_io::encode_png_rgba(width, rows.len() as u32, &bytes).unwrap();
        std::fs::write(path, png).unwrap();
    }
}
