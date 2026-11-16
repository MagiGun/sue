//! The Animation Cels palette: the light tables of the cel being drawn.
//!
//! Top to bottom, as Clip Studio Paint's palette: three rows of light
//! table and cel buttons (the opacity and the color mode on the first),
//! then the cel being drawn, highlighted, and one entry for each layer on
//! its light table and on the general one. An entry reads like a row of
//! the Layer palette: its eye, its picture, its name and, under the
//! name, how strongly and in which colors it shows. A click picks an
//! entry, and the picked one opens its opacity (with the number) and its
//! color mode. A layer dragged from the Layer palette and let go over a
//! table is registered to it.

use egui::{Align2, CornerRadius, Rect, Sense, Stroke, Ui, pos2, vec2};
use sue_core::EntityId;
use sue_wm::ui::{self as wm_ui, Density, Space, TextRole};
use sue_wm::{AppletContext, Theme};

use crate::anim::{self, EntryMode, LightEntry};
use crate::panels::child;
use crate::panels::timeline::icon;
use crate::studio::SuePaint;

/// A button of the palette: its row and column, its icon's stable name,
/// its command, whether it can be used and whether it is lit.
type Button = (usize, usize, &'static str, &'static str, bool, bool);

/// One light table: a cel's own, or the picture's general one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Table {
    /// The cel, or the picture for the general table.
    pub from: EntityId,
    /// It is the general table.
    pub general: bool,
}

impl Table {
    /// The kind of link that puts a layer on it.
    pub(crate) fn kind(self) -> &'static str {
        if self.general {
            anim::GENERAL_LIGHT_TABLE
        } else {
            anim::LIGHT_TABLE
        }
    }
}

/// What the Animation Cels palette keeps between frames.
#[derive(Default)]
pub(crate) struct CelsUi {
    /// The entry picked: its table and its layer.
    pub picked: Option<(Table, EntityId)>,
    /// An entry's opacity while its slider is held: shown at once, kept
    /// as one undo step when it is let go.
    pub live: Option<(Table, EntityId, f32)>,
    /// Where each table was drawn last frame, for a layer dropped there.
    pub drop: Vec<(Rect, Table)>,
    /// Where each table is drawn this frame.
    pub drawn: Vec<(Rect, Table)>,
    /// How tall the picked entry's settings were drawn last time.
    pub foot: f32,
}

impl CelsUi {
    /// A new frame: what was drawn becomes where layers drop.
    pub(crate) fn next_frame(&mut self) {
        self.drop = std::mem::take(&mut self.drawn);
    }

    /// The opacity `layer` shows at on `table` now: the held slider's, or
    /// its own.
    pub(crate) fn opacity(&self, table: Table, layer: EntityId, kept: f32) -> f32 {
        match self.live {
            Some((held, held_layer, opacity)) if held == table && held_layer == layer => opacity,
            _ => kept,
        }
    }
}

/// The name of a color mode as the palette shows it.
fn mode_name(mode: EntryMode) -> String {
    match mode {
        EntryMode::Table => tr!("light-table-mode-table"),
        EntryMode::Own => tr!("light-table-mode-own"),
        EntryMode::Tinted => tr!("light-table-mode-tinted"),
    }
}

/// What a person did to an entry this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Did {
    Nothing,
    Pick,
    Off,
    Set(LightEntry),
}

impl SuePaint {
    /// The Animation Cels palette, as Clip Studio Paint's: three rows of
    /// light table and cel buttons with the opacity and color mode on the
    /// first, then the cel being drawn and an entry for each layer on its
    /// light table and on the general one.
    pub(super) fn cels_tab(
        &mut self,
        ui: &mut Ui,
        body: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let cel = self.editing_cel(&cx.world.arena);
        let has = cel.is_some();
        let registered = cel.zip(self.layer()).is_some_and(|(cel, layer)| {
            crate::anim::light_table(&cx.world.arena, cel).contains(&layer)
        });
        let gap = theme.space(Space::S1);
        let edge = theme.space(Space::S3);
        // Nine buttons on the second row: they shrink to fit a narrow
        // palette rather than run off its edge.
        let columns = 9.0;
        let side = Density::of(ui)
            .row()
            .min((body.width() - edge * 2.0 - gap * (columns - 1.0)) / columns)
            .max(gap);
        let at = |row: usize, column: usize| {
            Rect::from_min_size(
                pos2(
                    body.left() + edge + column as f32 * (side + gap),
                    body.top() + edge + row as f32 * (side + gap),
                ),
                egui::Vec2::splat(side),
            )
        };
        let buttons: [Button; 14] = [
            (
                0,
                0,
                "reset",
                "anim.light_table.reset",
                has || self.light_general,
                false,
            ),
            (0, 1, "reverse", "anim.light_table.reverse", has, false),
            (
                0,
                2,
                "layer-opacity-target",
                "anim.light_table.opacity_target",
                true,
                self.opacity_general,
            ),
            (
                1,
                0,
                "light-table",
                "anim.light_table",
                true,
                self.light_table,
            ),
            (
                1,
                1,
                "cel-lock",
                "anim.cel.lock",
                has || self.cel_lock.is_some(),
                self.cel_lock.is_some(),
            ),
            (1, 2, "chevron-left", "anim.cel.prev", true, false),
            (1, 3, "chevron-right", "anim.cel.next", true, false),
            (1, 4, "cel-new", "anim.cel.new", true, false),
            (
                1,
                6,
                "light-table-file",
                "anim.light_table.file",
                has,
                false,
            ),
            (
                1,
                7,
                "light-table-add",
                "anim.light_table.register",
                has,
                false,
            ),
            (
                1,
                8,
                "light-table-remove",
                "anim.light_table.deregister",
                registered,
                false,
            ),
            (
                2,
                0,
                "light-table-tool",
                "anim.light_table.tool",
                true,
                self.light_tool,
            ),
            (2, 1, "cel", "anim.light_table.cel", true, self.light_cel),
            (
                2,
                2,
                "layers",
                "anim.light_table.general",
                true,
                self.light_general,
            ),
        ];
        let mut run = None;
        for (row, column, name, command, enabled, lit) in buttons {
            let tip = self.button_name(cx, command);
            let mut button = wm_ui::icon_button(("cels-button", command), icon(name), &tip)
                .enabled(enabled)
                .quiet();
            if lit {
                button = button.selected(true);
            }
            if button.show_at(ui, at(row, column), theme).clicked() {
                run = Some(command);
            }
        }
        // Opacity and Color Mode, on the first row after its buttons. The
        // slider sets the cel's light table, or the general one (Switch
        // Opacity Target).
        let color_rect = Rect::from_min_size(
            pos2(body.right() - edge - side, at(0, 0).top()),
            egui::Vec2::splat(side),
        );
        // The light table's color (Change layer color), left of Color
        // Mode.
        let tint_rect = Rect::from_min_size(
            pos2(color_rect.left() - gap - side * 2.0, color_rect.top()),
            egui::vec2(side * 2.0, side),
        );
        let opacity_rect = Rect::from_min_max(
            pos2(at(0, 3).left() + gap, at(0, 3).top()),
            pos2(tint_rect.left() - gap, at(0, 3).bottom()),
        );
        let mut opacity = f64::from(if self.opacity_general {
            self.general_opacity
        } else {
            self.light_opacity
        }) * 100.0;
        if opacity_rect.width() > side
            && wm_ui::wedge("cels-opacity", &mut opacity, 0.0..=100.0)
                .label(&tr!("opacity"))
                .show_at(ui, opacity_rect, theme)
                .changed()
        {
            let opacity = (opacity / 100.0) as f32;
            if self.opacity_general {
                self.general_opacity = opacity;
            } else {
                self.light_opacity = opacity;
            }
        }
        let tip = self.button_name(cx, "anim.light_table.color");
        let mut color =
            wm_ui::icon_button("cels-color-mode", icon("layer-color-mode"), &tip).quiet();
        if self.light_color {
            color = color.selected(true);
        }
        if color.show_at(ui, color_rect, theme).clicked() {
            run = Some("anim.light_table.color");
        }
        let [red, green, blue] = self.light_tint;
        let mut tint = [red, green, blue, 255];
        let picked = ui
            .scope_builder(egui::UiBuilder::new().max_rect(tint_rect), |ui| {
                ui.spacing_mut().interact_size.y = side;
                sue_wm::color_button(ui, "cels-light-tint", &mut tint, false, theme)
            })
            .inner;
        if picked.changed {
            self.light_tint = [tint[0], tint[1], tint[2]];
            // Picking a color shows it.
            self.light_color = true;
        }

        // The entries: the cel being drawn, its light table, then the
        // general one.
        // The entry picked, while it is still on its table: its settings
        // sit at the foot of the palette, apart from the list that scrolls.
        // Only an entry of a table the palette shows: the cel's, or the
        // general one.
        let picked = self.anim_ui.cels.picked.filter(|(table, layer)| {
            (table.general || Some(table.from) == cel)
                && cx
                    .world
                    .arena
                    .relations()
                    .get(table.from, table.kind(), *layer)
                    .is_some()
        });
        // A held opacity belongs to the entry picked: once another is
        // picked (or none), it no longer shows anywhere.
        if self
            .anim_ui
            .cels
            .live
            .is_some_and(|(table, layer, _)| picked != Some((table, layer)))
        {
            self.anim_ui.cels.live = None;
        }
        // The foot holds the picked entry's settings: as tall as they
        // were drawn last time (their rows' real heights), at least the
        // two controls and their gaps, so nothing is cut at the bottom.
        let pad = theme.space(Space::S2);
        let foot_height = if picked.is_some() {
            (Density::of(ui).control_height() * 2.0 + pad * 3.0)
                .max(self.anim_ui.cels.foot + pad * 2.0)
        } else {
            0.0
        };
        let list = Rect::from_min_max(
            pos2(body.left(), at(2, 0).bottom() + edge),
            pos2(body.right(), body.bottom() - foot_height),
        );
        ui.painter()
            .hline(list.x_range(), list.top() - edge / 2.0, theme.hairline());
        let mut inner = child(ui, list, "cels-light-table");
        let mut did: Option<(Table, EntityId, Did)> = None;
        let dragging = self.layer_drag.is_some();
        wm_ui::ScrollView::vertical("cels-entries").show(&mut inner, theme, |ui| {
            match cel {
                None => self.caption(ui, &tr!("go-frame-cel-see-light-table"), theme),
                Some(cel) => {
                    let table = Table {
                        from: cel,
                        general: false,
                    };
                    let top = ui.cursor().top();
                    self.editing_row(ui, cel, cx, theme);
                    let layers = anim::light_table(&cx.world.arena, cel);
                    if layers.is_empty() {
                        self.caption(ui, &tr!("drag-layer-here-to-trace-it"), theme);
                    }
                    for layer in layers {
                        let done = self.light_table_entry(ui, table, layer, cx, theme);
                        if done != Did::Nothing {
                            did = Some((table, layer, done));
                        }
                    }
                    self.drop_zone(ui, table, top, dragging, theme);
                }
            }
            // The general light table: shown on every cel.
            if let Some(picture) = self.picture() {
                let table = Table {
                    from: picture,
                    general: true,
                };
                ui.add_space(theme.space(Space::S3));
                let top = ui.cursor().top();
                self.caption(ui, &tr!("general-light-table"), theme);
                for layer in anim::general_light_table(&cx.world.arena, picture) {
                    let done = self.light_table_entry(ui, table, layer, cx, theme);
                    if done != Did::Nothing {
                        did = Some((table, layer, done));
                    }
                }
                ui.add_space(theme.space(Space::S2));
                let register = tr!("register-layer-general-light-table");
                ui.horizontal(|ui| {
                    ui.add_space(edge);
                    if wm_ui::button("cels-register-general", &register)
                        .icon(icon("light-table-add"))
                        .show(ui, theme)
                        .clicked()
                    {
                        run = Some("anim.light_table.register_general");
                    }
                });
                self.drop_zone(ui, table, top, dragging, theme);
                ui.add_space(theme.space(Space::S3));
            }
        });
        if let Some((table, layer)) = picked {
            let foot = Rect::from_min_max(pos2(body.left(), list.bottom()), body.max);
            ui.painter()
                .hline(foot.x_range(), foot.top(), theme.hairline());
            let entry = anim::entry(&cx.world.arena, table.from, table.kind(), layer);
            let shown = self.anim_ui.cels.opacity(table, layer, entry.opacity);
            let mut strip = child(ui, foot.shrink(pad), "cels-entry-settings");
            if let Some(set) = self.entry_settings(&mut strip, table, layer, entry, shown, theme) {
                did = Some((table, layer, Did::Set(set)));
            }
            let used = strip.min_rect().height();
            if (used - self.anim_ui.cels.foot).abs() > 0.5 {
                self.anim_ui.cels.foot = used;
                ui.ctx().request_repaint();
            }
        }
        match did {
            Some((table, layer, Did::Pick)) => {
                self.anim_ui.cels.picked = if self.anim_ui.cels.picked == Some((table, layer)) {
                    None
                } else {
                    Some((table, layer))
                };
            }
            Some((table, layer, Did::Off)) => {
                let edit = if table.general {
                    anim::deregister_general(table.from, &[layer])
                } else {
                    anim::deregister(table.from, &[layer])
                };
                Self::apply(cx, edit);
                if self.anim_ui.cels.picked == Some((table, layer)) {
                    self.anim_ui.cels.picked = None;
                }
            }
            Some((table, layer, Did::Set(entry))) => {
                if let Some(edit) =
                    anim::set_entry(&cx.world.arena, table.from, table.kind(), layer, entry)
                {
                    Self::apply(cx, edit);
                }
            }
            Some((_, _, Did::Nothing)) | None => {}
        }
        if let Some(command) = run {
            self.run(command, cx);
        }
    }

    /// A line of muted help in the list.
    fn caption(&self, ui: &mut Ui, text: &str, theme: &Theme) {
        let edge = theme.space(Space::S3).round() as i8;
        egui::Frame::NONE
            .inner_margin(egui::Margin {
                left: edge,
                right: edge,
                ..egui::Margin::ZERO
            })
            .show(ui, |ui| {
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(text)
                            .font(theme.text_font(TextRole::Caption))
                            .color(theme.text_muted()),
                    )
                    .wrap(),
                );
            });
        ui.add_space(theme.space(Space::S1));
    }

    /// Records where `table` was drawn (from `top` to the cursor) for a
    /// dropped layer, and while a layer is dragged over it, rings it.
    fn drop_zone(&mut self, ui: &Ui, table: Table, top: f32, dragging: bool, theme: &Theme) {
        let rect = Rect::from_min_max(
            pos2(ui.max_rect().left(), top),
            pos2(ui.max_rect().right(), ui.cursor().top()),
        )
        .intersect(ui.clip_rect());
        if !rect.is_positive() {
            return;
        }
        self.anim_ui.cels.drawn.push((rect, table));
        let over = ui
            .ctx()
            .pointer_hover_pos()
            .is_some_and(|at| rect.contains(at));
        if dragging && over {
            ui.painter().rect_stroke(
                rect.shrink(theme.space(Space::S1)),
                theme.corner(wm_ui::Radius::Sm),
                theme.focus_stroke(),
                egui::StrokeKind::Inside,
            );
            let name = if table.general {
                tr!("drop-to-register-general")
            } else {
                tr!("drop-to-register-cel")
            };
            ui.painter().text(
                rect.right_bottom() - vec2(theme.space(Space::S3), theme.space(Space::S2)),
                Align2::RIGHT_BOTTOM,
                name,
                theme.text_font(TextRole::Caption),
                theme.focus(),
            );
        }
    }

    /// The name of `id`, or "Layer" when it is gone.
    fn entity_name(&self, cx: &AppletContext<'_>, id: EntityId) -> String {
        cx.world
            .arena
            .get_by_id(id)
            .map_or_else(|_| tr!("layer-2"), |entity| entity.name().to_owned())
    }

    /// The cel being drawn, at the top of the list and highlighted: a pen
    /// where an entry has its eye, its picture and its name.
    fn editing_row(
        &mut self,
        ui: &mut Ui,
        cel: EntityId,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let density = Density::of(ui);
        let side = density.row();
        let edge = theme.space(Space::S3);
        let height = side + theme.space(Space::S2);
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
        ui.painter()
            .rect_filled(rect, CornerRadius::ZERO, theme.fill_selected());
        ui.painter().rect_filled(
            Rect::from_min_size(rect.min, vec2(theme.space(Space::S1), rect.height())),
            CornerRadius::ZERO,
            theme.border_active,
        );
        let pen = Rect::from_center_size(
            pos2(rect.left() + edge + side / 2.0, rect.center().y),
            egui::Vec2::splat(density.icon()),
        );
        sue_wm::icons::paint(ui.painter(), pen, icon("tool-pen"), theme.text);
        let thumb = Rect::from_min_size(
            pos2(pen.right() + edge, rect.center().y - side / 2.0),
            egui::Vec2::splat(side),
        );
        self.cel_thumb(ui.painter(), thumb, cel, cx, theme);
        let name = self.entity_name(cx, cel);
        ui.painter().text(
            pos2(thumb.right() + edge, rect.center().y),
            Align2::LEFT_CENTER,
            tr!("light-table-of-cel", value = name),
            theme.text_font(TextRole::Body),
            theme.text,
        );
        response.on_hover_text(tr!("the-cel-being-drawn"));
    }

    /// One layer on a light table: its eye, its picture, its name and how
    /// it shows, and a button that takes it off; picked, its opacity and
    /// color mode under it.
    fn light_table_entry(
        &mut self,
        ui: &mut Ui,
        table: Table,
        layer: EntityId,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) -> Did {
        let density = Density::of(ui);
        let side = density.row();
        let edge = theme.space(Space::S3);
        let entry = anim::entry(&cx.world.arena, table.from, table.kind(), layer);
        let opacity = self.anim_ui.cels.opacity(table, layer, entry.opacity);
        let picked = self.anim_ui.cels.picked == Some((table, layer));
        let name = self.entity_name(cx, layer);
        let height = side + theme.space(Space::S2);
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
        // The cel being drawn above has the selected fill; the entry
        // picked here has the outline of what is chosen, so the two never
        // read as one, and its caption stays on the panel's own fill.
        if response.hovered() {
            ui.painter()
                .rect_filled(rect, CornerRadius::ZERO, theme.fill_hover());
        }
        if picked {
            ui.painter().rect_stroke(
                rect.shrink(theme.selected_stroke().width / 2.0),
                theme.corner(wm_ui::Radius::Xs),
                theme.selected_stroke(),
                egui::StrokeKind::Inside,
            );
        }
        wm_ui::focus_ring(ui, &response, 0.0, theme);
        let mut did = if response.clicked() {
            Did::Pick
        } else {
            Did::Nothing
        };
        let eye_rect = Rect::from_center_size(
            pos2(rect.left() + edge + side / 2.0, rect.center().y),
            egui::Vec2::splat(side),
        );
        let eye_name = tr!("show-or-hide-on-light-table");
        if wm_ui::icon_button(
            ("cels-eye", table.general, layer),
            icon(if entry.shown { "eye" } else { "eye-off" }),
            &eye_name,
        )
        .quiet()
        .show_at(ui, eye_rect, theme)
        .clicked()
        {
            did = Did::Set(LightEntry {
                shown: !entry.shown,
                ..entry
            });
        }
        let thumb = Rect::from_min_size(
            pos2(eye_rect.right() + edge, rect.center().y - side / 2.0),
            egui::Vec2::splat(side),
        );
        self.cel_thumb(ui.painter(), thumb, layer, cx, theme);
        let off_rect = Rect::from_center_size(
            pos2(rect.right() - edge - side / 2.0, rect.center().y),
            egui::Vec2::splat(side),
        );
        let ink = if entry.shown {
            theme.text
        } else {
            theme.text_disabled(theme.panel)
        };
        let text_left = thumb.right() + edge;
        let text_clip = Rect::from_min_max(
            pos2(text_left, rect.top()),
            pos2(off_rect.left() - theme.space(Space::S1), rect.bottom()),
        );
        let painter = ui
            .painter()
            .with_clip_rect(text_clip.intersect(ui.clip_rect()));
        // How it shows, over its name, as the Layer palette writes it.
        painter.text(
            pos2(text_left, rect.center().y),
            Align2::LEFT_BOTTOM,
            format!(
                "{} %  {}",
                sue_lang::number(f64::from(opacity) * 100.0, 0),
                mode_name(entry.mode)
            ),
            theme.text_font(TextRole::Caption),
            theme.text_muted(),
        );
        painter.text(
            pos2(text_left, rect.center().y),
            Align2::LEFT_TOP,
            &name,
            theme.text_font(TextRole::Body),
            ink,
        );
        if wm_ui::icon_button(
            ("cels-off", table.general, layer),
            icon("close"),
            &tr!("deregister-from-the-light-table"),
        )
        .quiet()
        .show_at(ui, off_rect, theme)
        .clicked()
        {
            did = Did::Off;
        }
        // The line under each entry, as between Layer palette rows.
        let bottom = ui.cursor().top();
        ui.painter().hline(
            rect.x_range(),
            bottom,
            Stroke::new(theme.hairline().width, theme.fill_hover()),
        );
        did
    }

    /// The picked entry's opacity (a slider with its number) and color
    /// mode, at the foot of the palette. The slider shows at once while it
    /// is held and is kept as one undo step when let go. The settings to
    /// keep, when there are new ones.
    fn entry_settings(
        &mut self,
        ui: &mut Ui,
        table: Table,
        layer: EntityId,
        entry: LightEntry,
        shown: f32,
        theme: &Theme,
    ) -> Option<LightEntry> {
        let opacity_name = tr!("opacity");
        let mode_label = tr!("color-mode-of-entry");
        let mut kept = None;
        let mut value = f64::from(shown) * 100.0;
        let response = wm_ui::slider(
            ("cels-entry-opacity", table.general, layer),
            &mut value,
            0.0..=100.0,
        )
        .label(&opacity_name)
        .suffix(" %")
        .show(ui, theme);
        let opacity = (value / 100.0).clamp(0.0, 1.0) as f32;
        let held = ui.input(|input| input.pointer.primary_down());
        let live = self
            .anim_ui
            .cels
            .live
            .is_some_and(|(at, live_layer, _)| at == table && live_layer == layer);
        if response.changed() {
            self.anim_ui.cels.live = Some((table, layer, opacity));
        }
        // Let go (or typed): what was shown while it was held is kept.
        let finished = response.lost_focus()
            || (!held && !response.has_focus() && (response.changed() || live));
        if finished && let Some((_, _, live_opacity)) = self.anim_ui.cels.live.take() {
            kept = Some(LightEntry {
                opacity: live_opacity,
                ..entry
            });
        }
        let names: Vec<String> = EntryMode::ALL.into_iter().map(mode_name).collect();
        let mut chosen = EntryMode::ALL
            .iter()
            .position(|mode| *mode == entry.mode)
            .unwrap_or(0);
        ui.add_space(theme.space(Space::S2));
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(&mode_label)
                    .font(theme.text_font(TextRole::Caption))
                    .color(theme.text_muted()),
            );
            if wm_ui::dropdown(
                ("cels-entry-mode", table.general, layer),
                &mut chosen,
                &names,
            )
            .label(&mode_label)
            .show(ui, theme)
            .changed()
                && let Some(mode) = EntryMode::ALL.get(chosen)
            {
                kept = Some(LightEntry {
                    mode: *mode,
                    ..kept.unwrap_or(entry)
                });
            }
        });
        kept
    }

    /// A layer dragged from the Layer palette let go at `at`: over a light
    /// table of this palette it is registered there (with every layer
    /// picked with it), as one undo step. `true` when it was.
    pub(crate) fn register_dropped(
        &mut self,
        dragged: EntityId,
        at: egui::Pos2,
        cx: &mut AppletContext<'_>,
    ) -> bool {
        let Some(table) = self
            .anim_ui
            .cels
            .drop
            .iter()
            .find(|(rect, _)| rect.contains(at))
            .map(|(_, table)| *table)
        else {
            return false;
        };
        let picked = self.picked_layers(&cx.world.arena);
        let layers: Vec<EntityId> = if picked.len() > 1 && picked.contains(&dragged) {
            picked
        } else {
            vec![dragged]
        };
        let relations = cx.world.arena.relations();
        let on = relations.targets(table.from, table.kind());
        // Last on the table, in their order, after the layers on it.
        let mut order = relations.next_order(table.from, table.kind());
        let mut edit = crate::doc::Edit::new(tr!("register-to-light-table"));
        let mut last = None;
        for layer in layers {
            // A cel never traces itself, and a layer is on a table once.
            if layer == table.from || on.contains(&layer) {
                continue;
            }
            edit = edit.then(sue_core::txn::Relate::new(
                sue_core::Relation::new(table.from, table.kind(), layer).ordered(order.clone()),
            ));
            order = sue_core::OrderKey::between(Some(&order), None);
            last = Some(layer);
        }
        match last {
            Some(layer) => {
                if Self::apply(cx, edit) {
                    // The entry made is picked, its settings at hand.
                    self.anim_ui.cels.picked = Some((table, layer));
                }
            }
            None => cx.notify(tr!("already-on-this-light-table")),
        }
        true
    }
}
