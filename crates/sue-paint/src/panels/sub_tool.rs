//! The Sub Tool palette (its group tabs, its list of sub tools with their
//! sample strokes, the "Add Sub Tool" row and the buttons under the list)
//! and the Tool Property palette's header.
//!
//! A sub tool is dragged to another place in its list, onto a group tab
//! to move it into that group, or onto the Tool palette to make it a tool
//! of its own; a tool dragged here from the Tool palette goes back into
//! the tool shown. Right-click a sub tool for everything else.

use egui::{Align2, Color32, Pos2, Rect, RichText, Sense, Ui, pos2, vec2};
use sue_wm::icons::Icon;
use sue_wm::ui::{Density, Radius, Space, TextRole};
use sue_wm::{AppletContext, ItemState, MenuItem, ShellRequest, Theme};

use crate::brush::Stroke as BrushStroke;
use crate::color;
use crate::commands::subtool;
use crate::panels::child;
use crate::panels::sub_tool_detail::NamingKind;
use crate::panels::tool::ToolDrag;
use crate::studio::SuePaint;

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
        use crate::icons::Icon as PaintIcon;
        let tabs = [
            (
                tr!("tool-group"),
                PaintIcon::SubTool,
                self.panels.sub_tool,
                None,
            ),
            (
                tr!("tool-settings"),
                PaintIcon::Sliders,
                self.panels.tool_property,
                Some(Palette::ToolProperty),
            ),
            (
                tr!("brush-size"),
                PaintIcon::BrushSize,
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
        egui::ScrollArea::vertical()
            .id_salt(("sue-paint-subs", slot, shown_group))
            .auto_shrink([false, false])
            .show(&mut inner, |ui| {
                // Solid bars: the list says when it holds more than shows.
                ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
                for (offset, name) in names.iter().enumerate() {
                    let index = range.start + offset;
                    let sub = &entry.subs()[index];
                    let chosen = index == picked;
                    let tall = if paints { line * 1.4 } else { line };
                    let (row, response) = ui.allocate_exact_size(
                        vec2(ui.available_width(), tall),
                        Sense::click_and_drag(),
                    );
                    rows.push((index, row));
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
                    if paints {
                        let size = [
                            (row.width().round() as u32).clamp(40, 600),
                            (tall - pad).max(8.0) as u32,
                        ];
                        if let Some(texture) =
                            Self::sub_preview(&mut self.sub_previews, cx, sub.tip, size)
                        {
                            ui.painter().image(
                                texture,
                                Rect::from_min_size(
                                    row.min + egui::Vec2::Y * (pad / 2.0),
                                    vec2(size[0] as f32, size[1] as f32),
                                ),
                                Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                                if chosen {
                                    theme.text
                                } else {
                                    theme.text_muted()
                                },
                            );
                        }
                        ui.painter().text(
                            row.right_bottom() - vec2(pad, half_pad),
                            Align2::RIGHT_BOTTOM,
                            name,
                            theme.text_font(TextRole::Caption),
                            theme.text,
                        );
                    } else {
                        ui.painter().text(
                            row.left_center() + egui::Vec2::X * pad,
                            Align2::LEFT_CENTER,
                            name,
                            theme.text_font(TextRole::Body),
                            theme.text,
                        );
                    }
                    if let Some(key) = &key {
                        ui.painter().text(
                            row.right_top() + vec2(-pad, half_pad),
                            Align2::RIGHT_TOP,
                            key,
                            theme.text_font(TextRole::Micro),
                            theme.text_muted(),
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
        // The buttons under the list, at its trailing edge.
        let bar = Rect::from_min_max(pos2(body.left(), body.bottom() - footer), body.max)
            .shrink2(egui::Vec2::X * pad);
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
        let sub_name = crate::localize(&self.toolbox.current().name);
        inner.horizontal(|ui| {
            ui.label(
                RichText::new(&sub_name)
                    .size(theme.text_size(TextRole::Body))
                    .strong()
                    .color(theme.text),
            );
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
        if self.toolbox.current().locked {
            inner.label(
                RichText::new(tr!("sub-tool-settings-locked"))
                    .size(theme.text_size(TextRole::Caption))
                    .color(theme.text_muted()),
            );
        }
        if tool.paints() {
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

    /// The texture of `tip`'s sample stroke for the Sub Tool list, `size`
    /// pixels: white ink on clear, tinted when drawn.
    fn sub_preview(
        cache: &mut std::collections::HashMap<u64, Vec<u8>>,
        cx: &mut AppletContext<'_>,
        mut tip: crate::brush::Tip,
        size: [u32; 2],
    ) -> Option<egui::TextureId> {
        use std::hash::{Hash, Hasher};
        // Thin pens stay thin and big brushes fit the row.
        tip.size = (tip.size * 0.7).clamp(1.0, size[1] as f32 * 0.4);
        let key = {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            format!("{tip:?}{size:?}").hash(&mut hasher);
            hasher.finish()
        };
        if !cache.contains_key(&key) {
            if cache.len() > 256 {
                cache.clear();
            }
            cache.insert(key, Self::render_stroke(tip, size)?);
        }
        let pixels = cache.get(&key)?;
        cx.textures
            .texture(0x5355_4253_5452_4b45 ^ key, 1, size, 4, pixels, false)
    }

    /// A sample stroke with `tip` in white over nothing, `size` pixels.
    fn render_stroke(tip: crate::brush::Tip, [width, height]: [u32; 2]) -> Option<Vec<u8>> {
        let mut raster =
            sue_core::RasterTile2D::new(width, height, sue_core::PixelFormat::Rgba8).ok()?;
        let mut tip = tip;
        // An eraser's sample is drawn as ink, so it shows.
        tip.eraser = false;
        let mut stroke = BrushStroke::new(sue_core::EntityId::NIL, tip, [1.0, 1.0, 1.0], false);
        for step in 0..=48 {
            let t = step as f32 / 48.0;
            let x = 10.0 + t * (width as f32 * 0.62);
            let y = height as f32 * (0.42 - 0.2 * (t * std::f32::consts::TAU * 0.8).sin());
            let pressure = (t * std::f32::consts::PI).sin().max(0.05);
            stroke.to(&mut raster, [x, y], pressure, None);
        }
        stroke.end(&mut raster, crate::brush::Finish::default(), None);
        raster.bytes().map(<[u8]>::to_vec)
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
        let sub = self.toolbox.current();
        let (width, pixels_high) = (200_u32, 44_u32);
        let mut tip = sub.tip;
        tip.size = tip.size.clamp(1.0, 30.0);
        let key = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            format!("{tip:?}{:?}", self.drawing_color()).hash(&mut hasher);
            hasher.finish()
        };
        if self
            .preview
            .as_ref()
            .is_none_or(|(cached, _)| *cached != key)
        {
            self.preview =
                Self::render_preview(tip, color::unit(self.drawing_color()), [width, pixels_high])
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

    /// A sample stroke with `tip` over the light preview ground.
    fn render_preview(
        tip: crate::brush::Tip,
        rgb: [f32; 3],
        [width, height]: [u32; 2],
    ) -> Option<Vec<u8>> {
        let mut raster =
            sue_core::RasterTile2D::new(width, height, sue_core::PixelFormat::Rgba8).ok()?;
        // An eraser shows what it takes out of a dark ground.
        if tip.eraser
            && let Some(bytes) = raster.bytes_mut()
        {
            for pixel in bytes.as_chunks_mut::<4>().0 {
                *pixel = [40, 40, 40, 255];
            }
        }
        let mut stroke = BrushStroke::new(sue_core::EntityId::NIL, tip, rgb, false);
        for step in 0..=40 {
            let t = step as f32 / 40.0;
            let x = 12.0 + t * (width as f32 - 24.0);
            let y = height as f32 * (0.5 - 0.28 * (t * std::f32::consts::TAU).sin());
            let pressure = (t * std::f32::consts::PI).sin().max(0.05);
            stroke.to(&mut raster, [x, y], pressure, None);
        }
        stroke.end(&mut raster, crate::brush::Finish::default(), None);
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
