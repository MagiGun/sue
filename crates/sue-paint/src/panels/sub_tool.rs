//! The Sub Tool palette: its list of sub tools and the stroke previews
//! drawn beside them.

use egui::{Align2, Color32, Pos2, Rect, RichText, Sense, Ui, pos2, vec2};
use sue_wm::{AppletContext, ShellRequest, Theme};

use crate::brush::Stroke as BrushStroke;
use crate::color;
use crate::icons::{self, Icon};
use crate::panels::child;
use crate::studio::SuePaint;

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
            (tr!("tool-group"), Icon::SubTool, self.panels.sub_tool, None),
            (
                tr!("tool-settings"),
                Icon::Sliders,
                self.panels.tool_property,
                Some(Palette::ToolProperty),
            ),
            (
                tr!("brush-size"),
                Icon::BrushSize,
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
        let tool = self.toolbox.tool();
        if self.tool_tab == 2 {
            self.brush_size_grid(ui, body.shrink2(vec2(6.0, 6.0)), theme);
            return;
        }
        let mut inner = child(ui, body.shrink2(vec2(8.0, 6.0)), "tool-panel");
        if self.tool_tab == 0 {
            let (group, _) = inner.allocate_exact_size(
                vec2(inner.available_width().min(140.0), 22.0),
                Sense::hover(),
            );
            inner.painter().rect_filled(
                group,
                theme.control_radius,
                theme.selection.gamma_multiply(0.6),
            );
            // The tool palette entry that is current: its icon and name
            // (Liquify for the Liquify tool, Fill while on its Lasso fill).
            let entry = self.toolbox.current_entry();
            icons::paint(
                inner.painter(),
                Rect::from_center_size(group.left_center() + vec2(12.0, 0.0), vec2(14.0, 14.0)),
                crate::panels::slot_icon(entry),
                theme.text,
            );
            inner.painter().text(
                group.left_center() + vec2(24.0, 0.0),
                Align2::LEFT_CENTER,
                crate::localize(&entry.name),
                theme.font(theme.small_size + 0.5),
                theme.text,
            );
            let picked = self.toolbox.picked(tool);
            let names: Vec<String> = self
                .toolbox
                .subs(tool)
                .iter()
                .map(|sub| crate::localize(&sub.name))
                .collect();
            // Sample strokes when the entry is a painting tool's, whichever
            // sub tool is picked (Fill's Lasso fill is a Figure sub tool).
            let paints = self.toolbox.current_entry().kind.paints();
            let tips: Vec<Option<crate::brush::Tip>> = self
                .toolbox
                .subs(tool)
                .iter()
                .map(|sub| paints.then_some(sub.tip))
                .collect();
            egui::ScrollArea::vertical()
                .id_salt("sue-paint-subs")
                .show(&mut inner, |ui| {
                    let mut action = None;
                    for (index, name) in names.iter().enumerate() {
                        let chosen = index == picked;
                        // A painting sub tool is a row with its own sample
                        // stroke and its name, as in Clip Studio Paint.
                        let tall = if tips[index].is_some() { 44.0 } else { 26.0 };
                        let (row, response) = ui
                            .allocate_exact_size(vec2(ui.available_width(), tall), Sense::click());
                        let painter = ui.painter();
                        let fill = if chosen {
                            theme.active
                        } else if response.hovered() {
                            theme.hover
                        } else {
                            theme.pill
                        };
                        painter.rect_filled(
                            row.shrink2(vec2(0.0, 1.0)),
                            theme.control_radius,
                            fill,
                        );
                        if let Some(tip) = tips[index] {
                            let size = [
                                (row.width().round() as u32).clamp(40, 600),
                                (tall as u32) - 8,
                            ];
                            if let Some(texture) =
                                Self::sub_preview(&mut self.sub_previews, cx, tip, size)
                            {
                                ui.painter().image(
                                    texture,
                                    Rect::from_min_size(
                                        row.min + vec2(0.0, 2.0),
                                        vec2(size[0] as f32, size[1] as f32),
                                    ),
                                    Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                                    if chosen {
                                        theme.text
                                    } else {
                                        theme.text.gamma_multiply(0.85)
                                    },
                                );
                            }
                            ui.painter().text(
                                row.right_bottom() + vec2(-6.0, -4.0),
                                Align2::RIGHT_BOTTOM,
                                name,
                                theme.font(theme.small_size + 0.5),
                                theme.text,
                            );
                        } else {
                            ui.painter().text(
                                row.left_center() + vec2(8.0, 0.0),
                                Align2::LEFT_CENTER,
                                name,
                                theme.font(theme.small_size + 0.5),
                                theme.text,
                            );
                        }
                        if response.clicked() {
                            self.toolbox.pick_sub(tool, index);
                            self.tool_tab = 1;
                        }
                        // Clip Studio Paint's sub tool menu.
                        response.context_menu(|ui| {
                            for (what, label) in [
                                (0, tr!("duplicate-sub-tool")),
                                (1, tr!("delete-sub-tool")),
                                (2, tr!("reset-to-initial-settings")),
                            ] {
                                if ui
                                    .add_enabled(
                                        what != 1 || names.len() > 1,
                                        egui::Button::new(label),
                                    )
                                    .clicked()
                                {
                                    action = Some((what, index));
                                    ui.close();
                                }
                            }
                        });
                    }
                    match action {
                        Some((0, index)) => self.toolbox.duplicate_sub(tool, index),
                        Some((1, index)) => {
                            self.toolbox.delete_sub(tool, index);
                        }
                        Some((_, index)) => {
                            self.toolbox.reset_sub(tool, index);
                        }
                        None => {}
                    }
                    // Clip Studio Paint's Add from downloads: brushes people
                    // downloaded (Clip Studio's .sut, Photoshop's .abr,
                    // GIMP's .gbr and .gih, or a picture) become sub tools.
                    ui.add_space(6.0);
                    ui.vertical_centered(|ui| {
                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new(format!("+  {}", tr!("add-from-downloads")))
                                        .size(theme.small_size + 0.5)
                                        .color(theme.text),
                                )
                                .frame(false),
                            )
                            .on_hover_text(tr!("brushes-from-clip-studio-photoshop-gimp"))
                            .clicked()
                        {
                            cx.request(ShellRequest::PickFile(
                                sue_wm::FilePick::open(
                                    Some(sue_wm::AppletId::new(crate::ID).expect("a valid id")),
                                    "brush-import",
                                    &tr!("add-brushes-from-downloads"),
                                )
                                .extensions(&["sut", "abr", "gbr", "gih", "png"]),
                            ));
                        }
                    });
                });
            return;
        }
        // Tool Property.
        let sub_name = crate::localize(&self.toolbox.current().name);
        inner.horizontal(|ui| {
            ui.label(
                RichText::new(&sub_name)
                    .color(theme.text)
                    .size(theme.ui_size),
            );
            let count = self.toolbox.subs(tool).len();
            if count > 1 {
                egui::ComboBox::from_id_salt("sue-paint-sub-pick")
                    .selected_text("")
                    .width(24.0)
                    .show_ui(ui, |ui| {
                        let picked = self.toolbox.picked(tool);
                        let names: Vec<String> = self
                            .toolbox
                            .subs(tool)
                            .iter()
                            .map(|sub| crate::localize(&sub.name))
                            .collect();
                        for (index, name) in names.iter().enumerate() {
                            if ui.selectable_label(index == picked, name).clicked() {
                                self.toolbox.pick_sub(tool, index);
                            }
                        }
                    });
            }
        });
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

    /// A sample stroke drawn with the current brush.
    fn stroke_preview(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>, theme: &Theme) {
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::hover());
        ui.painter()
            .rect_filled(rect, theme.control_radius, Color32::from_gray(236));
        let sub = self.toolbox.current();
        let (width, height) = (200_u32, 44_u32);
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
                Self::render_preview(tip, color::unit(self.drawing_color()), [width, height])
                    .map(|pixels| (key, pixels));
        }
        let Some((_, shown)) = &self.preview else {
            return;
        };
        if let Some(texture) =
            cx.textures
                .texture(0x5355_4550_5245_5649, key, [width, height], 4, shown, false)
        {
            let image = Rect::from_center_size(
                rect.center(),
                vec2(rect.width(), rect.width() * height as f32 / width as f32).min(rect.size()),
            );
            ui.painter().image(
                texture,
                image,
                Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                Color32::WHITE,
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
