//! The Layer palette and the thumbnail of a layer.

use egui::{Align2, Color32, Id, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use sue_core::Value;
use sue_model::BlendMode;
use sue_model::picture::{self, LayerKind};
use sue_wm::{AppletContext, Theme};

use crate::doc;
use crate::icons::{self, Icon};
use crate::panels::{Wedge, child, icon_button};
use crate::studio::{SuePaint, texture_key};

impl SuePaint {
    #[allow(clippy::too_many_lines, reason = "the layer palette's three tabs")]
    pub(crate) fn layer_panel(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        use crate::dock::Palette;
        let tabs = [
            (tr!("layer-2"), Icon::Layers, true, None),
            (
                tr!("layer-property"),
                Icon::LayerProperty,
                true,
                Some(Palette::LayerProperty),
            ),
            (tr!("history"), Icon::History, true, Some(Palette::History)),
            (tr!("animation-cels"), Icon::Cel, true, Some(Palette::Cels)),
        ];
        let (body, tab) = self.torn_header(ui, rect, &tabs, self.layer_tab, theme);
        self.layer_tab = tab;
        let Some(picture_id) = self.picture() else {
            ui.painter().text(
                body.center(),
                Align2::CENTER_CENTER,
                "No canvas",
                theme.font(theme.small_size + 0.5),
                theme.muted,
            );
            return;
        };
        let stack = doc::layers(&cx.world.arena, picture_id);
        let current = self.layer_info(&cx.world.arena);
        if self.layer_tab == 3 {
            self.cels_tab(ui, body, cx, theme);
            return;
        }
        match self.layer_tab {
            1 => {
                let mut inner = child(ui, body.shrink(8.0), "layer-property");
                self.layer_property(&mut inner, current.as_ref(), cx, theme);
                return;
            }
            2 => {
                self.history_tab(ui, body, cx, theme);
                return;
            }
            _ => {}
        }
        // Blend mode and opacity of the current layer.
        let mut inner = child(ui, body.shrink2(vec2(6.0, 6.0)), "layer-top");
        let raster = current
            .as_ref()
            .is_some_and(|info| info.kind == LayerKind::Raster);
        inner.horizontal(|ui| {
            let blend = current
                .as_ref()
                .map_or(BlendMode::Normal, |info| info.style.blend);
            ui.add_enabled_ui(raster, |ui| {
                egui::ComboBox::from_id_salt("sue-paint-blend")
                    .selected_text(sue_lang::translate("model", blend.label()))
                    .width(108.0)
                    .height(420.0)
                    .show_ui(ui, |ui| {
                        let mut group = 0;
                        for mode in BlendMode::ALL {
                            if mode.group() != group {
                                ui.separator();
                                group = mode.group();
                            }
                            if ui
                                .selectable_label(
                                    mode == blend,
                                    sue_lang::translate("model", mode.label()),
                                )
                                .clicked()
                                && let Some(info) = &current
                            {
                                Self::apply(cx, doc::set(info.id, picture::BLEND, mode.name()));
                            }
                        }
                    });
            });
            let mut opacity = current
                .as_ref()
                .map_or(100.0, |info| info.style.opacity * 100.0);
            let width = (ui.available_width() - 52.0).max(40.0);
            ui.add_enabled_ui(current.is_some(), |ui| {
                let wedge = ui.add(Wedge::new(&mut opacity, 0.0..=100.0, width, theme));
                let typed = ui.add(
                    egui::DragValue::new(&mut opacity)
                        .range(0.0..=100.0)
                        .max_decimals(0),
                );
                let response = if typed.changed() || typed.lost_focus() {
                    typed
                } else {
                    wedge
                };
                if let Some(info) = &current {
                    self.live_property(
                        cx,
                        &response,
                        info.id,
                        picture::OPACITY,
                        Value::Float(f64::from(opacity) / 100.0),
                    );
                }
            });
        });
        // Layer switches and actions.
        let row_top = inner.min_rect().bottom() + 4.0;
        let flags: [(Icon, &str, &str); 6] = [
            (Icon::Clip, "layer.toggle.clip", &tr!("clip-to-layer-below")),
            (
                Icon::Reference,
                "layer.toggle.reference",
                &tr!("set-as-reference-layer"),
            ),
            (
                Icon::Draft,
                "layer.toggle.draft",
                &tr!("set-as-draft-layer"),
            ),
            (Icon::Lock, "layer.toggle.lock", &tr!("lock-layer")),
            (
                Icon::LockAlpha,
                "layer.toggle.lock_alpha",
                &tr!("lock-transparent-pixels"),
            ),
            (Icon::Mask, "layer.mask.toggle", &tr!("enable-mask")),
        ];
        let mut run = None;
        // What a grey button needs before it works.
        let why = |command: &str, tip: &str| match command {
            "layer.mask.toggle" | "layer.mask.apply" => {
                tr!("needs-a-layer-mask", tip = tip)
            }
            "layer.merge_down" => tr!("needs-a-raster-layer-below", tip = tip),
            _ => tip.to_owned(),
        };
        for (index, (icon, command, tip)) in flags.iter().enumerate() {
            let rect = Rect::from_min_size(
                pos2(body.left() + 6.0 + index as f32 * 28.0, row_top),
                vec2(26.0, 24.0),
            );
            let state = self.state(command, &sue_wm::Facts::of(cx));
            let tip = &if state.enabled {
                (*tip).to_owned()
            } else {
                why(command, tip)
            };
            let checked = match *command {
                "layer.toggle.lock_alpha" => current.as_ref().is_some_and(|info| info.alpha_locked),
                _ => state.checked == Some(true),
            };
            if icon_button(ui, rect, *icon, state.enabled, checked, tip, theme) {
                run = Some(*command);
            }
        }
        let actions: [(Icon, &str, &str); 8] = [
            (
                Icon::NewLayer,
                "layer.new",
                &tr!("new-raster-layer-ctrl-shift-n"),
            ),
            (
                Icon::NewVector,
                "layer.new.vector",
                &tr!("new-vector-layer"),
            ),
            (
                Icon::NewFolder,
                "layer.new.folder",
                &tr!("new-layer-folder"),
            ),
            (
                Icon::TransferDown,
                "layer.transfer_down",
                &tr!("transfer-to-lower-layer"),
            ),
            (
                Icon::MergeDown,
                "layer.merge_down",
                &tr!("merge-layer-below-ctrl-e"),
            ),
            (
                Icon::NewMask,
                "layer.mask.outside",
                &tr!("create-layer-mask"),
            ),
            (
                Icon::ApplyMask,
                "layer.mask.apply",
                &tr!("apply-mask-to-layer"),
            ),
            (Icon::Delete, "layer.delete", &tr!("delete-layer")),
        ];
        let action_top = row_top + 28.0;
        for (index, (icon, command, tip)) in actions.iter().enumerate() {
            let rect = Rect::from_min_size(
                pos2(body.left() + 6.0 + index as f32 * 28.0, action_top),
                vec2(26.0, 24.0),
            );
            let state = self.state(command, &sue_wm::Facts::of(cx));
            let tip = &if state.enabled {
                (*tip).to_owned()
            } else {
                why(command, tip)
            };
            if icon_button(ui, rect, *icon, state.enabled, false, tip, theme) {
                run = Some(*command);
            }
        }
        if let Some(command) = run {
            self.run(command, cx);
            return;
        }
        // The list, top layer first.
        let list = Rect::from_min_max(pos2(body.left(), action_top + 30.0), body.max);
        ui.painter()
            .hline(list.x_range(), list.top(), theme.hairline());
        let mut list_ui = child(ui, list, "layer-list");
        let mut pick = None;
        let mut also_pick = None;
        // Right-click on a layer: Clip Studio Paint's layer menu.
        let layer_menu: Vec<sue_wm::MenuItem> = crate::menus::menus()
            .into_iter()
            .find(|menu| menu.title == tr!("layer-2"))
            .map(|menu| menu.items)
            .unwrap_or_default();
        let mut from_menu = None;
        let mut toggle = None;
        let mut rename_done = None;
        let mut fold = None;
        let mut mask_click = None;
        let mut reopen = None;
        let mut dragging = None;
        let mut dropped = None;
        let mut drop_onto = None;
        egui::ScrollArea::vertical()
            .id_salt("sue-paint-layers")
            .show(&mut list_ui, |ui| {
                for info in stack.iter().rev() {
                    // Layers in a closed folder stay out of sight; layers in
                    // folders sit indented under them.
                    let around = doc::folders_around(&cx.world.arena, info.id);
                    if around
                        .iter()
                        .any(|folder| !doc::is_open(&cx.world.arena, *folder))
                    {
                        continue;
                    }
                    let is_folder = doc::is_folder(&cx.world.arena, info.id);
                    // On touch screens a drag scrolls the list instead.
                    let (full, response) = ui.allocate_exact_size(
                        vec2(ui.available_width(), 46.0),
                        if cx.view.form_factor == sue_platform::FormFactor::Desktop {
                            Sense::click_and_drag()
                        } else {
                            Sense::click()
                        },
                    );
                    // Drag a layer onto a folder to put it in, or onto a
                    // layer to put it right above, in that layer's folder.
                    if response.drag_started() {
                        dragging = Some(info.id);
                    }
                    if response.drag_stopped() {
                        dropped = Some(info.id);
                    }
                    let drop_here = self.layer_drag.is_some_and(|dragged| dragged != info.id)
                        && ui
                            .ctx()
                            .pointer_hover_pos()
                            .is_some_and(|pos| full.contains(pos));
                    if drop_here {
                        drop_onto = Some(info.id);
                    }
                    let row = Rect::from_min_max(
                        full.min + vec2(around.len() as f32 * 14.0, 0.0),
                        full.max,
                    );
                    let chosen = Some(info.id) == self.layer();
                    let also = self.doc().is_some_and(|doc| doc.picked.contains(&info.id));
                    if also {
                        ui.painter()
                            .rect_filled(full, 0.0, theme.active.gamma_multiply(0.6));
                    }
                    if chosen {
                        ui.painter().rect_filled(full, 0.0, theme.active);
                    } else if response.hovered() {
                        ui.painter().rect_filled(full, 0.0, theme.hover);
                    }
                    ui.painter()
                        .hline(full.x_range(), full.bottom(), theme.hairline());
                    // Where a dragged layer would go: into a folder (the
                    // whole row), or right above a layer (a line on top).
                    if drop_here && is_folder {
                        ui.painter().rect_stroke(
                            full.shrink(1.0),
                            2.0,
                            Stroke::new(2.0, theme.border_active),
                            egui::StrokeKind::Inside,
                        );
                    } else if drop_here {
                        ui.painter().hline(
                            full.x_range(),
                            full.top() + 1.0,
                            Stroke::new(2.0, theme.border_active),
                        );
                    }
                    // Visibility.
                    let eye = Rect::from_min_size(row.min + vec2(4.0, 13.0), vec2(20.0, 20.0));
                    let eye_response = ui.interact(
                        eye,
                        Id::unique(("sue-paint-eye", info.id.to_u128())),
                        Sense::click(),
                    );
                    icons::paint(
                        ui.painter(),
                        eye,
                        Icon::Eye,
                        if info.style.visible {
                            theme.text
                        } else {
                            theme.muted.gamma_multiply(0.35)
                        },
                    );
                    if eye_response.on_hover_text(tr!("show-or-hide")).clicked() {
                        toggle = Some((info.id, !info.style.visible));
                    }
                    // State marks: the pen on the current layer, locks.
                    let mark = Rect::from_min_size(row.min + vec2(26.0, 15.0), vec2(16.0, 16.0));
                    if info.locked {
                        icons::paint(ui.painter(), mark, Icon::Lock, theme.muted);
                    } else if chosen {
                        icons::paint(ui.painter(), mark, Icon::Pen, theme.text);
                    }
                    if info.style.clip {
                        ui.painter().vline(
                            row.left() + 46.0,
                            row.shrink2(vec2(0.0, 6.0)).y_range(),
                            Stroke::new(2.0, theme.border_active),
                        );
                    }
                    // Thumbnail, or a folder and its open and close arrow.
                    let thumb_rect =
                        Rect::from_min_size(row.min + vec2(50.0, 5.0), vec2(48.0, 36.0));
                    if is_folder {
                        let open = doc::is_open(&cx.world.arena, info.id);
                        let arrow =
                            Rect::from_min_size(thumb_rect.min + vec2(0.0, 8.0), vec2(18.0, 20.0));
                        let arrow_response = ui.interact(
                            arrow,
                            Id::unique(("sue-paint-fold", info.id.to_u128())),
                            Sense::click(),
                        );
                        let c = arrow.center();
                        let points = if open {
                            vec![
                                c + vec2(-5.0, -3.0),
                                c + vec2(5.0, -3.0),
                                c + vec2(0.0, 4.0),
                            ]
                        } else {
                            vec![
                                c + vec2(-3.0, -5.0),
                                c + vec2(4.0, 0.0),
                                c + vec2(-3.0, 5.0),
                            ]
                        };
                        ui.painter().add(egui::Shape::convex_polygon(
                            points,
                            theme.text,
                            Stroke::NONE,
                        ));
                        if arrow_response
                            .on_hover_text(if open {
                                tr!("close-the-folder")
                            } else {
                                tr!("open-the-folder")
                            })
                            .clicked()
                        {
                            fold = Some((info.id, !open));
                        }
                        // An animation folder shows as a strip of film.
                        let animation = crate::anim::is_animation_folder(&cx.world.arena, info.id);
                        icons::paint(
                            ui.painter(),
                            Rect::from_min_size(thumb_rect.min + vec2(20.0, 6.0), vec2(24.0, 24.0)),
                            if animation {
                                Icon::Timeline
                            } else {
                                Icon::NewFolder
                            },
                            theme.text,
                        );
                    } else {
                        self.layer_thumbnail(ui, thumb_rect, info, cx, theme);
                    }
                    // The mask beside the layer: click it to draw on it.
                    let mut text_left = thumb_rect.right() + 8.0;
                    if doc::mask_of(&cx.world.arena, info.id).is_some() {
                        let mask_rect = Rect::from_min_size(
                            pos2(thumb_rect.right() + 4.0, thumb_rect.top()),
                            vec2(28.0, 36.0),
                        );
                        let on = doc::mask_on(&cx.world.arena, info.id);
                        let editing = chosen && self.doc().is_some_and(|doc| doc.mask_edit);
                        ui.painter().rect_filled(
                            mask_rect,
                            2.0,
                            egui::Color32::from_gray(if on { 225 } else { 120 }),
                        );
                        icons::paint(
                            ui.painter(),
                            mask_rect.shrink(6.0),
                            Icon::Mask,
                            egui::Color32::from_gray(40),
                        );
                        if editing {
                            ui.painter().rect_stroke(
                                mask_rect,
                                2.0,
                                Stroke::new(2.0, theme.border_active),
                                egui::StrokeKind::Outside,
                            );
                        }
                        let mask_response = ui.interact(
                            mask_rect,
                            Id::unique(("sue-paint-mask", info.id.to_u128())),
                            Sense::click(),
                        );
                        if mask_response
                            .on_hover_text(tr!("draw-mask-pen-shows-eraser-hides"))
                            .clicked()
                        {
                            mask_click = Some(info.id);
                        }
                        text_left = mask_rect.right() + 8.0;
                    }
                    // Text.
                    let top_line = match info.kind {
                        LayerKind::Paper => tr!("paper-2"),
                        _ => format!(
                            "{} % {}",
                            sue_lang::number(f64::from(info.style.opacity) * 100.0, 0),
                            sue_lang::translate("model", info.style.blend.label())
                        ),
                    };
                    ui.painter().text(
                        pos2(text_left, row.top() + 14.0),
                        Align2::LEFT_CENTER,
                        top_line,
                        theme.font(theme.small_size),
                        theme.muted,
                    );
                    let renaming = self
                        .renaming
                        .as_ref()
                        .filter(|(id, _)| *id == info.id)
                        .map(|(_, name)| name.clone());
                    if let Some(mut name) = renaming {
                        let field = Rect::from_min_max(
                            pos2(text_left, row.top() + 22.0),
                            pos2(row.right() - 6.0, row.bottom() - 4.0),
                        );
                        let mut field_ui = child(ui, field, "rename");
                        let edit = field_ui.add(
                            egui::TextEdit::singleline(&mut name).desired_width(field.width()),
                        );
                        edit.request_focus();
                        if edit.lost_focus() {
                            rename_done = Some((
                                info.id,
                                name.clone(),
                                ui.input(|input| input.key_pressed(egui::Key::Escape)),
                            ));
                        }
                        self.renaming = Some((info.id, name));
                    } else {
                        let name = match (
                            info.style.draft,
                            doc::is_reference(&cx.world.arena, info.id),
                        ) {
                            (true, _) => tr!("draft", name = info.name),
                            (false, true) => tr!("reference", name = info.name),
                            _ if crate::three::is_3d(&cx.world.arena, info.id) => {
                                format!("{} (3D)", info.name)
                            }
                            _ => info.name.clone(),
                        };
                        ui.painter().text(
                            pos2(text_left, row.top() + 31.0),
                            Align2::LEFT_CENTER,
                            name,
                            theme.font(theme.ui_size),
                            if info.style.visible {
                                theme.text
                            } else {
                                theme.muted
                            },
                        );
                    }
                    let correction = cx
                        .world
                        .arena
                        .get_by_id(info.id)
                        .ok()
                        .filter(|entity| entity.class() == doc::CORRECTION)
                        .and_then(|entity| entity.property(doc::CORRECTION_FILTER))
                        .and_then(Value::as_str)
                        .and_then(|text| {
                            crate::filters::Filter::from_text(text)
                                .map(|filter| (filter, text.to_owned()))
                        });
                    if response.double_clicked()
                        && let Some((filter, text)) = correction
                    {
                        // A correction layer's settings, as Clip Studio Paint
                        // opens them on a double-click.
                        reopen = Some((info.id, filter, text));
                    } else if response.double_clicked() {
                        self.renaming = Some((info.id, info.name.clone()));
                    } else if response.clicked() && ui.input(|input| input.modifiers.command) {
                        // Ctrl+click picks more layers (Merge Selected Layers).
                        also_pick = Some(info.id);
                    } else if response.clicked() || response.secondary_clicked() {
                        pick = Some(info.id);
                    }
                    let state = |command: &str| self.state(command, &sue_wm::Facts::of(cx));
                    response.context_menu(|ui| {
                        if let Some(command) = sue_wm::context_items(ui, &layer_menu, &state) {
                            from_menu = Some(command);
                        }
                    });
                }
            });
        if let Some(id) = pick
            && let Some(doc) = self.doc_mut()
        {
            if doc.layer != Some(id) {
                doc.mask_edit = false;
                doc.picked.clear();
            }
            doc.layer = Some(id);
        }
        if let Some(id) = also_pick
            && let Some(doc) = self.doc_mut()
            && doc.layer != Some(id)
        {
            if let Some(at) = doc.picked.iter().position(|known| *known == id) {
                doc.picked.remove(at);
            } else {
                doc.picked.push(id);
            }
        }
        if let Some((id, visible)) = toggle {
            Self::apply(cx, doc::set(id, picture::VISIBLE, visible));
        }
        if let Some(command) = from_menu {
            self.run(&command, cx);
        }
        if let Some((id, open)) = fold {
            Self::apply(cx, doc::set(id, doc::OPEN, open));
        }
        if let Some(id) = dragging {
            self.layer_drag = Some(id);
        }
        if let Some(dragged) = dropped {
            self.layer_drag = None;
            if let (Some(picture), Some(onto)) = (self.picture(), drop_onto)
                && let Some(edit) = crate::anim::drop_layer(&cx.world.arena, picture, dragged, onto)
            {
                Self::apply(cx, edit);
            }
        }
        if let Some((id, filter, text)) = reopen {
            self.dialog = Some(crate::dialogs::Dialog::Correction(id, filter, text, false));
        }
        if let Some(id) = mask_click
            && let Some(doc) = self.doc_mut()
        {
            // The mask of the current layer toggles; another layer's mask
            // picks that layer with its mask.
            doc.mask_edit = doc.layer != Some(id) || !doc.mask_edit;
            doc.layer = Some(id);
        }
        if let Some((id, name, cancelled)) = rename_done {
            self.renaming = None;
            let name = name.trim().to_owned();
            let old = cx
                .world
                .arena
                .get_by_id(id)
                .map(|entity| entity.name().to_owned())
                .unwrap_or_default();
            if !cancelled && !name.is_empty() && name != old {
                Self::apply(cx, sue_core::txn::Rename::new(id, name));
            }
        }
    }

    fn layer_thumbnail(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        info: &doc::LayerInfo,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let key = texture_key(info.id, 0x7448_554d_4253);
        if self
            .thumbs
            .get(&info.id)
            .is_none_or(|(revision, _, _)| *revision != info.revision)
            && let Some((pixels, size)) =
                crate::composite::thumbnail(&cx.world.arena, info.id, [96, 72])
        {
            self.thumbs.insert(info.id, (info.revision, pixels, size));
        }
        let texture = self
            .thumbs
            .get(&info.id)
            .and_then(|(revision, pixels, size)| {
                cx.textures.texture(key, *revision, *size, 4, pixels, false)
            });
        ui.painter().rect_filled(rect, 0.0, theme.void);
        if let Some(texture) = texture {
            let aspect = self
                .picture()
                .and_then(|picture| cx.world.arena.get_by_id(picture).ok())
                .and_then(picture::size)
                .map_or(4.0 / 3.0, |[w, h]| w as f32 / h.max(1) as f32);
            let size = if aspect > rect.width() / rect.height() {
                vec2(rect.width(), rect.width() / aspect)
            } else {
                vec2(rect.height() * aspect, rect.height())
            };
            let image = Rect::from_center_size(rect.center(), size);
            ui.painter().image(
                texture,
                image,
                Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)),
                Color32::WHITE,
            );
            ui.painter()
                .rect_stroke(image, 0.0, theme.hairline(), StrokeKind::Outside);
        }
    }
}
