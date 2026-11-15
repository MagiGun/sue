//! Sue Paint's dialogs: New (a canvas from presets, paper sizes and
//! units), the filter dialogs with a live preview, and the shortcut list.
//! One file per family; this one holds the open dialog, what it ends
//! with, the host that draws it, and the pieces the families share.

mod anim;
mod canvas;
mod comps;
mod fill;
mod filter;
mod frame;
mod new;
mod pressure;
mod settings;
mod story;
mod text;

use egui::{Align, Id, Layout, Order, Rect, RichText, Sense, Ui, vec2};
use sue_core::EntityId;
use sue_wm::{AppletContext, Theme, widgets};

use crate::filters::Filter;
use crate::studio::SuePaint;

use anim::{onion_ui, timelapse_ui, timeline_ui};
use canvas::{
    canvas_properties_ui, canvas_ui, convert_ui, material_ui, perspective_ui, print_ui, proof_ui,
    resolution_ui, shape_3d_ui,
};
use fill::{gamut_ui, select_by_ui};
use filter::{correction_ui, filter_ui};
use frame::frame_ui;
use pressure::pressure_ui;
use settings::{modifiers_ui, shortcuts_ui, workspace_ui};
use story::{page_settings_ui, story_editor_ui, story_info_ui, webtoon_ui};
use text::text_ui;

pub(crate) use canvas::{CanvasDialog, CanvasProperties, ResolutionDialog};
pub(crate) use comps::CompsDialog;
pub(crate) use fill::AdvancedFill;
pub(crate) use filter::FilterDialog;
#[cfg(test)]
pub(crate) use new::SIZES;
pub(crate) use new::{CanvasPreset, NEW_PRESET_TARGET, NewDialog, Project, new_presets};
pub(crate) use pressure::{DEFAULT_PEN_PRESSURE, PEN_PRESSURE, PressureDialog};
#[cfg(test)]
pub(crate) use settings::GESTURES;
pub(crate) use settings::MODIFIERS;
pub(crate) use story::{PageSettings, PagesDialog, StoryInfo};

/// An open dialog.
#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant, reason = "one dialog is open at a time")]
pub(crate) enum Dialog {
    /// Edit, Advanced Fill (its settings live on the studio).
    AdvancedFill,
    New(NewDialog),
    Filter(FilterDialog),
    Shortcuts,
    Pressure(PressureDialog),
    Pages(PagesDialog),
    /// Layer Comps (Clip Studio Paint EX).
    Comps(CompsDialog),
    /// The exposure sheet (Clip Studio Paint EX).
    Exposure,
    Frame(crate::comic::FrameSpec),
    Canvas(CanvasDialog),
    Resolution(ResolutionDialog),
    /// The Text tool's words: the text layer they go to (new if `None`).
    Text(Option<EntityId>, crate::lettering::Lettering),
    /// A correction layer's settings, shown live: the layer, the
    /// correction, its setting when the dialog opened, and whether the
    /// layer was just made (Cancel takes it away again).
    Correction(EntityId, Filter, String, bool),
    /// Create Perspective Ruler: how many vanishing points, 1 to 3.
    Perspective(u32),
    /// Onion Skin Settings.
    Onion(crate::anim::Onion),
    /// Export Timelapse: the video's size and length.
    Timelapse(crate::timelapse::Export),
    /// Register Material, Image: its name.
    Material(String),
    /// Expand (0), Shrink (1) or Blur the border of (2) the selection, by
    /// pixels.
    SelectBy(u8, f32),
    /// Select Color Gamut: the tolerance and whether it looks at every
    /// layer.
    Gamut(f32, bool),
    /// Export Webtoon: the height each piece is cut at and the width the
    /// pieces are scaled to, in pixels.
    Webtoon(u32, u32),
    /// Edit Timeline: frames a second and length in frames.
    Timeline(u32, u32),
    /// New 3D Layer: which of Sue Paint's shapes.
    Shape3D(usize),
    /// Register Workspace: its name.
    Workspace(String),
    /// Manage Workspaces.
    Workspaces,
    /// A page to read: its title and paragraphs.
    Message(String, Vec<String>),
    /// Print Settings, or Print when set.
    Print(crate::print::PrintSettings, bool),
    /// Modifier Key Settings: what Alt, Space and Ctrl+Shift do on the
    /// canvas (indexes into their choices).
    Modifiers([usize; 3]),
    /// Command Bar Settings.
    CommandBar(String),
    /// Canvas Properties.
    CanvasProperties(CanvasProperties),
    /// Color Profile Preview Settings: which profile.
    Proof(usize),
    /// Change Page Settings.
    PageSettings(PageSettings),
    /// Change Story Information.
    StoryInfo(StoryInfo),
    /// The Story Editor: each text layer's page number, page, layer and
    /// words.
    StoryEditor(Vec<(usize, sue_core::EntityId, sue_core::EntityId, String)>),
    /// Convert Layer: the layer, to raster (0) or vector (1), and its name.
    Convert(sue_core::EntityId, u8, String),
    /// File, Manage Fonts: Sue's shared font list.
    Fonts(sue_wm::fonts::FontManager),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Open,
    Cancel,
    Ok,
}

impl Dialog {
    /// The dialog's title and its size before the screen clamps it.
    fn title_and_size(&self) -> (String, egui::Vec2) {
        match self {
            Dialog::New(_) => (tr!("new-2"), vec2(500.0, 690.0)),
            // The gallery keeps one title (and so one window) whatever look
            // is picked.
            Dialog::Filter(FilterDialog {
                filter: Filter::Quick { effect, .. },
                ..
            }) => (
                if *effect {
                    tr!("quick-effects")
                } else {
                    tr!("quick-filters")
                },
                vec2(500.0, 400.0),
            ),
            Dialog::Filter(filter) => (
                filter.filter.label(),
                vec2(
                    400.0,
                    match filter.filter {
                        Filter::ToneCurve { .. } => 360.0,
                        Filter::Levels { .. } => 330.0,
                        _ => 250.0,
                    },
                ),
            ),
            Dialog::Shortcuts => (tr!("keyboard-shortcuts"), vec2(470.0, 560.0)),
            Dialog::Pages(_) => (tr!("page-manager"), vec2(820.0, 620.0)),
            Dialog::Comps(_) => (tr!("layer-comps"), vec2(420.0, 300.0)),
            Dialog::Exposure => (tr!("exposure-sheet"), vec2(520.0, 560.0)),
            Dialog::Frame(_) => (tr!("new-frame-border-folder"), vec2(420.0, 330.0)),
            Dialog::Canvas(_) => (tr!("change-canvas-size"), vec2(380.0, 300.0)),
            Dialog::Resolution(_) => (tr!("change-image-resolution"), vec2(380.0, 250.0)),
            Dialog::Text(..) => (tr!("text"), vec2(440.0, 380.0)),
            Dialog::Perspective(_) => (tr!("create-perspective-ruler"), vec2(320.0, 190.0)),
            Dialog::Onion(_) => (tr!("onion-skin-settings"), vec2(360.0, 330.0)),
            Dialog::Timelapse(_) => (tr!("export-timelapse"), vec2(360.0, 230.0)),
            Dialog::Material(_) => (tr!("register-material"), vec2(360.0, 160.0)),
            Dialog::SelectBy(kind, _) => (
                match kind {
                    0 => tr!("expand-selected-area-2"),
                    1 => tr!("shrink-selected-area-2"),
                    3 => tr!("outline-selection"),
                    _ => tr!("blur-border"),
                },
                vec2(320.0, 140.0),
            ),
            Dialog::Gamut(..) => (tr!("select-color-gamut"), vec2(340.0, 170.0)),
            Dialog::Webtoon(..) => (tr!("export-webtoon"), vec2(340.0, 170.0)),
            Dialog::Timeline(..) => (tr!("edit-timeline"), vec2(320.0, 150.0)),
            Dialog::Shape3D(_) => (tr!("new-3d-layer"), vec2(300.0, 230.0)),
            Dialog::Workspace(_) => (tr!("register-workspace"), vec2(340.0, 140.0)),
            Dialog::Workspaces => (tr!("manage-workspaces"), vec2(380.0, 320.0)),
            Dialog::Message(title, _) => (title.clone(), vec2(520.0, 460.0)),
            Dialog::Print(_, true) => (tr!("print"), vec2(380.0, 330.0)),
            Dialog::Print(_, false) => (tr!("print-settings"), vec2(380.0, 330.0)),
            Dialog::Modifiers(_) => (tr!("modifier-key-settings"), vec2(400.0, 230.0)),
            Dialog::AdvancedFill => (tr!("advanced-fill"), vec2(520.0, 470.0)),
            Dialog::CommandBar(_) => (tr!("command-bar-settings"), vec2(420.0, 460.0)),
            Dialog::CanvasProperties(_) => (tr!("canvas-properties"), vec2(380.0, 230.0)),
            Dialog::Proof(_) => (tr!("color-profile-preview-settings"), vec2(400.0, 230.0)),
            Dialog::PageSettings(_) => (tr!("change-page-settings"), vec2(380.0, 270.0)),
            Dialog::StoryInfo(_) => (tr!("change-story-information"), vec2(420.0, 360.0)),
            Dialog::StoryEditor(_) => (tr!("story-editor"), vec2(520.0, 520.0)),
            Dialog::Convert(..) => (tr!("convert-layer"), vec2(360.0, 190.0)),
            Dialog::Fonts(_) => (tr!("manage-fonts"), sue_wm::fonts::MANAGE_SIZE),
            Dialog::Correction(_, filter, ..) => (
                filter.label(),
                vec2(
                    400.0,
                    match filter {
                        Filter::ToneCurve { .. } => 340.0,
                        Filter::Levels { .. } => 310.0,
                        _ => 230.0,
                    },
                ),
            ),
            Dialog::Pressure(pressure) => (
                tr!("adjust-pen-pressure"),
                vec2(560.0, if pressure.show_graph { 520.0 } else { 330.0 }),
            ),
        }
    }
}

impl SuePaint {
    /// Draws the open dialog, if any, over the studio.
    pub(crate) fn dialog_ui(
        &mut self,
        ctx: &egui::Context,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        self.save_ask_ui(ctx, cx, theme);
        let Some(mut dialog) = self.dialog.take() else {
            return;
        };
        let screen = ctx.content_rect();
        egui::Area::new(Id::unique("sue-paint-scrim"))
            .fade_in(false)
            .order(Order::Middle)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                ui.allocate_rect(screen, Sense::click());
                ui.painter()
                    .rect_filled(screen, 0.0, theme.shadow.gamma_multiply(0.45));
            });
        let (title, size) = dialog.title_and_size();
        let size = size.min(screen.size() - vec2(16.0, 16.0));
        let at = screen.center() - size / 2.0;
        let mut outcome = Outcome::Open;
        let mut change_keys = false;
        // The dialog's body, in its own window or (in the browser and on
        // phones) over the studio.
        let own_window = !ctx.embed_viewports();
        let mut body = |ui: &mut Ui, inner: egui::Vec2| {
            ui.set_min_size(inner);
            ui.set_max_size(inner);
            if !own_window {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(title.as_str())
                            .size(theme.ui_size + 1.0)
                            .color(theme.text),
                    );
                });
                ui.separator();
            }
            // A dialog larger than the screen (a phone) scrolls inside
            // its frame, so its buttons stay in reach.
            egui::ScrollArea::both()
                .id_salt("sue-paint-dialog-body")
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    outcome = self.dialog_body(ui, &mut dialog, cx, theme, size, &mut change_keys);
                });
        };
        let mut closed = false;
        let mut keys_here = (false, false);
        if own_window {
            // A window of its own, as Clip Studio Paint's dialogs: it can
            // be moved anywhere, outside Sue's window too, and resized.
            let main = ctx
                .input(|input| input.viewport().outer_rect)
                .unwrap_or(screen);
            let own_frame = widgets::own_frames(ctx);
            let bar = if own_frame { widgets::TITLE_BAR } else { 0.0 };
            let builder = egui::ViewportBuilder::default()
                .with_title(title.clone())
                .with_inner_size(size + vec2(0.0, bar))
                .with_min_inner_size(vec2(240.0, 120.0 + bar))
                .with_position(main.center() - size / 2.0)
                .with_resizable(true)
                .with_decorations(!own_frame);
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of(("sue-paint-dialog", title.as_str())),
                builder,
                |ui, _| {
                    let rect = ui.max_rect();
                    ui.painter().rect_filled(rect, 0.0, theme.panel);
                    let mut shut = false;
                    if own_frame {
                        // Sue's own title bar, as Clip Studio Paint's.
                        let top = Rect::from_min_size(rect.min, vec2(rect.width(), bar));
                        shut = widgets::title_bar(ui, top, &title, false, theme);
                        ui.add_space(bar);
                    }
                    egui::Frame::new()
                        .inner_margin(egui::Margin::same(16))
                        .show(ui, |ui| {
                            let inner = ui.available_size();
                            body(ui, inner);
                        });
                    if own_frame {
                        widgets::resize_edges(ui, rect);
                    }
                    closed = shut || ui.input(|input| input.viewport().close_requested());
                    keys_here = ui.input(|input| {
                        (
                            input.key_pressed(egui::Key::Escape),
                            input.key_pressed(egui::Key::Enter),
                        )
                    });
                },
            );
        } else {
            egui::Area::new(Id::unique("sue-paint-dialog"))
                .fade_in(false)
                .order(Order::Foreground)
                .fixed_pos(at)
                .show(ctx, |ui| {
                    widgets::window_frame(theme)
                        .inner_margin(egui::Margin::same(16))
                        .show(ui, |ui| body(ui, size - vec2(32.0, 32.0)));
                });
        }
        if closed {
            outcome = Outcome::Cancel;
        }
        let (escape, enter) = ctx.input(|input| {
            (
                input.key_pressed(egui::Key::Escape) || keys_here.0,
                input.key_pressed(egui::Key::Enter) || keys_here.1,
            )
        });
        if change_keys {
            cx.request(sue_wm::ShellRequest::OpenSettings(Some(
                "keymaps".to_owned(),
            )));
        }
        if escape {
            outcome = Outcome::Cancel;
        } else if enter
            && !ctx.text_edit_focused()
            && !matches!(
                dialog,
                Dialog::Shortcuts
                    | Dialog::Pressure(_)
                    | Dialog::Pages(_)
                    | Dialog::Comps(_)
                    | Dialog::Exposure
                    | Dialog::Text(..)
            )
        {
            outcome = Outcome::Ok;
        }
        match outcome {
            Outcome::Open => self.keep_open(ctx, cx, dialog),
            Outcome::Ok => self.dialog_ok(cx, dialog),
            Outcome::Cancel => self.dialog_cancel(cx, dialog),
        }
    }

    /// The open dialog's body: one function per kind of dialog, each
    /// answering whether the dialog stays open, is kept or is cancelled.
    fn dialog_body(
        &mut self,
        ui: &mut Ui,
        dialog: &mut Dialog,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
        size: egui::Vec2,
        change_keys: &mut bool,
    ) -> Outcome {
        match dialog {
            Dialog::New(new) => self.new_dialog_ui(ui, new, cx, theme),
            Dialog::AdvancedFill => self.advanced_fill_dialog_ui(ui, theme),
            Dialog::Filter(filter) => filter_ui(ui, filter, theme),
            Dialog::Pressure(pressure) => pressure_ui(ui, pressure, theme),
            Dialog::Pages(pages) => self.pages_ui(ui, pages, cx, theme),
            Dialog::Comps(comps) => self.comps_ui(ui, comps, cx, theme),
            Dialog::Exposure => self.exposure_ui(ui, cx, theme),
            Dialog::Frame(frame) => frame_ui(ui, frame, theme),
            Dialog::Text(_, lettering) => text_ui(ui, lettering, theme),
            Dialog::Onion(onion) => onion_ui(ui, onion, theme),
            Dialog::SelectBy(kind, amount) => select_by_ui(ui, kind, amount, theme),
            Dialog::Workspace(name) => workspace_ui(ui, name, theme),
            Dialog::Workspaces => self.workspaces_ui(ui, theme),
            Dialog::Print(settings, printing) => print_ui(ui, settings, *printing, theme),
            Dialog::Modifiers(chosen) => modifiers_ui(ui, chosen, theme),
            Dialog::CommandBar(adding) => self.command_bar_ui(ui, adding, size, theme),
            Dialog::PageSettings(settings) => page_settings_ui(ui, settings, theme),
            Dialog::StoryInfo(info) => story_info_ui(ui, info, theme),
            Dialog::StoryEditor(texts) => story_editor_ui(ui, texts, size, theme),
            Dialog::Proof(profile) => proof_ui(ui, profile, theme),
            Dialog::CanvasProperties(props) => canvas_properties_ui(ui, props, theme),
            Dialog::Convert(_, kind, name) => convert_ui(ui, kind, name, theme),
            Dialog::Message(_, paragraphs) => message_ui(ui, paragraphs, size, theme),
            Dialog::Gamut(tolerance, all) => gamut_ui(ui, tolerance, all, theme),
            Dialog::Webtoon(height, width) => webtoon_ui(ui, height, width, theme),
            Dialog::Timeline(fps, length) => timeline_ui(ui, fps, length, theme),
            Dialog::Shape3D(kind) => shape_3d_ui(ui, kind, theme),
            Dialog::Material(name) => material_ui(ui, name, theme),
            Dialog::Timelapse(export) => timelapse_ui(ui, export, theme),
            Dialog::Perspective(count) => perspective_ui(ui, count, theme),
            Dialog::Correction(_, filter, ..) => correction_ui(ui, filter, theme),
            Dialog::Canvas(canvas) => canvas_ui(ui, canvas, theme),
            Dialog::Resolution(resolution) => resolution_ui(ui, resolution, theme),
            Dialog::Shortcuts => shortcuts_ui(ui, cx, size, change_keys, theme),
            Dialog::Fonts(manager) => {
                if sue_wm::fonts::manage_ui(ui, manager, theme) {
                    Outcome::Ok
                } else {
                    Outcome::Open
                }
            }
        }
    }

    /// Puts back a dialog that stays open; a correction's or a filter's
    /// setting shows live on the canvas meanwhile.
    fn keep_open(&mut self, ctx: &egui::Context, cx: &mut AppletContext<'_>, dialog: Dialog) {
        if let Dialog::Correction(layer, filter, ..) = &dialog {
            Self::show_correction(cx, *layer, filter);
        }
        if let Dialog::Filter(filter) = &dialog
            && let Some(updated) = filter.show_preview(ctx, cx)
        {
            self.dialog = Some(Dialog::Filter(updated));
            ctx.request_repaint();
            return;
        }
        self.dialog = Some(dialog);
    }

    /// What OK (or Enter) does, for each kind of dialog.
    fn dialog_ok(&mut self, cx: &mut AppletContext<'_>, dialog: Dialog) {
        match dialog {
            Dialog::New(new) => self.new_ok(cx, &new),
            Dialog::AdvancedFill => self.advanced_fill_now(cx),
            Dialog::Filter(filter) => self.keep_filter(cx, &filter),
            Dialog::Pressure(pressure) => Self::pressure_ok(cx, &pressure),
            Dialog::Frame(frame) => self.add_frame_layer(cx, frame),
            Dialog::Text(layer, lettering) => self.put_text(cx, layer, &lettering),
            Dialog::Perspective(count) => self.make_perspective(cx, count),
            Dialog::Timelapse(export) => self.ask_export_timelapse(cx, export),
            Dialog::Material(name) => self.register_material(cx, &name),
            Dialog::SelectBy(3, width) => self.outline_selection(cx, width),
            Dialog::SelectBy(kind, amount) => self.select_by_ok(kind, amount),
            Dialog::Gamut(tolerance, all) => self.select_gamut(cx, tolerance, all),
            Dialog::Workspace(name) => self.workspace_ok(cx, &name),
            Dialog::Workspaces | Dialog::CommandBar(_) => self.save_extras(cx),
            Dialog::Print(settings, printing) => self.print_ok(cx, settings, printing),
            Dialog::Modifiers(chosen) => Self::modifiers_ok(cx, chosen),
            Dialog::PageSettings(settings) => self.page_settings_ok(cx, &settings),
            Dialog::StoryInfo(info) => Self::story_info_ok(cx, &info),
            Dialog::StoryEditor(texts) => Self::story_editor_ok(cx, texts),
            Dialog::Proof(profile) => self.proof_ok(profile),
            Dialog::CanvasProperties(props) => Self::canvas_properties_ok(cx, &props),
            Dialog::Convert(layer, kind, name) => self.convert_ok(cx, layer, kind, &name),
            Dialog::Webtoon(height, width) => self.ask_export_webtoon(cx, height, width),
            Dialog::Shape3D(kind) => self.shape_3d_ok(cx, kind),
            Dialog::Timeline(fps, length) => self.timeline_ok(cx, fps, length),
            Dialog::Onion(onion) => self.onion_ok(cx, &onion),
            Dialog::Correction(layer, filter, before, _) => {
                Self::correction_ok(cx, layer, &filter, &before);
            }
            Dialog::Canvas(canvas) => self.canvas_ok(cx, &canvas),
            Dialog::Resolution(resolution) => self.resolution_ok(cx, &resolution),
            Dialog::Message(..)
            | Dialog::Shortcuts
            | Dialog::Pages(_)
            | Dialog::Comps(_)
            | Dialog::Exposure
            | Dialog::Fonts(_) => {}
        }
    }

    /// What Cancel (or Escape, or closing its window) does, for each kind
    /// of dialog.
    fn dialog_cancel(&mut self, cx: &mut AppletContext<'_>, dialog: Dialog) {
        match dialog {
            Dialog::Filter(filter) => self.filter_cancel(cx, &filter),
            Dialog::Correction(layer, _, before, made) => {
                Self::correction_cancel(cx, layer, &before, made);
            }
            Dialog::Workspaces | Dialog::CommandBar(_) => self.save_extras(cx),
            Dialog::New(_)
            | Dialog::AdvancedFill
            | Dialog::Pressure(_)
            | Dialog::Pages(_)
            | Dialog::Comps(_)
            | Dialog::Exposure
            | Dialog::Frame(_)
            | Dialog::Canvas(_)
            | Dialog::Resolution(_)
            | Dialog::Text(..)
            | Dialog::Perspective(_)
            | Dialog::Onion(_)
            | Dialog::Timelapse(_)
            | Dialog::Material(_)
            | Dialog::SelectBy(..)
            | Dialog::Gamut(..)
            | Dialog::Webtoon(..)
            | Dialog::Timeline(..)
            | Dialog::Shape3D(_)
            | Dialog::Workspace(_)
            | Dialog::Message(..)
            | Dialog::Print(..)
            | Dialog::Modifiers(_)
            | Dialog::CanvasProperties(_)
            | Dialog::Proof(_)
            | Dialog::PageSettings(_)
            | Dialog::StoryInfo(_)
            | Dialog::StoryEditor(_)
            | Dialog::Convert(..)
            | Dialog::Shortcuts
            | Dialog::Fonts(_) => {}
        }
    }
}

/// OK and Cancel at the bottom right.
fn ok_cancel(ui: &mut Ui, outcome: &mut Outcome) {
    ui.with_layout(Layout::right_to_left(Align::Max), |ui| {
        if ui.button("OK").clicked() {
            *outcome = Outcome::Ok;
        }
        if ui.button(tr!("cancel")).clicked() {
            *outcome = Outcome::Cancel;
        }
    });
}

fn label(ui: &mut Ui, text: &str, theme: &Theme) {
    ui.label(RichText::new(text).color(theme.muted));
}

/// A label right-aligned in a cell `width` wide, as Clip Studio Paint
/// sets its dialogs' labels.
fn right_label(ui: &mut Ui, text: &str, width: f32, theme: &Theme) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 22.0), Sense::hover());
    ui.painter().text(
        rect.right_center(),
        egui::Align2::RIGHT_CENTER,
        format!("{}:", text.trim_end_matches(':')),
        theme.font(theme.ui_size),
        theme.text,
    );
}

/// A section's name with a line running to the right of it.
fn section(ui: &mut Ui, text: &str, theme: &Theme) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(text).color(theme.text));
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 16.0), Sense::hover());
        ui.painter()
            .hline(rect.x_range(), rect.center().y, theme.hairline());
    });
}

/// A page to read: its paragraphs, headings and links.
pub(super) fn message_ui(
    ui: &mut Ui,
    paragraphs: &[String],
    size: egui::Vec2,
    theme: &Theme,
) -> Outcome {
    let mut outcome = Outcome::Open;
    egui::ScrollArea::vertical()
        .max_height(size.y - 110.0)
        .show(ui, |ui| {
            for paragraph in paragraphs.iter() {
                if paragraph.starts_with("https://") {
                    ui.hyperlink(paragraph.as_str());
                } else if let Some(heading) = paragraph.strip_prefix("# ") {
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(heading)
                            .size(theme.ui_size)
                            .color(theme.text)
                            .strong(),
                    );
                } else {
                    ui.label(
                        RichText::new(paragraph.as_str())
                            .size(theme.small_size + 0.5)
                            .color(theme.text),
                    );
                }
            }
        });
    ui.add_space(8.0);
    if ui.button(tr!("close")).clicked() {
        outcome = Outcome::Ok;
    }
    outcome
}
