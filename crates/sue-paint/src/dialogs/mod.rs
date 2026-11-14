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

use crate::doc;
use crate::filters::Filter;
use crate::studio::SuePaint;
use sue_rice::{RiceValue, SetRiceValue};

use anim::onion_ui;
use canvas::{canvas_ui, resolution_ui};
use fill::advanced_fill_ui;
use filter::{filter_params, filter_ui};
use frame::frame_ui;
use new::new_ui;
use pressure::pressure_ui;
use text::text_ui;

pub(crate) use canvas::{CanvasDialog, CanvasProperties, ResolutionDialog};
pub(crate) use comps::CompsDialog;
pub(crate) use fill::AdvancedFill;
pub(crate) use filter::FilterDialog;
#[cfg(test)]
pub(crate) use new::SIZES;
pub(crate) use new::{CanvasPreset, NEW_PRESET_TARGET, NewDialog, Project, new_presets};
pub(crate) use pressure::{DEFAULT_PEN_PRESSURE, PEN_PRESSURE, PressureDialog};
pub(crate) use settings::{GESTURES, MODIFIERS};
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
    /// File, Preferences: Sue Paint's own, page by page.
    Preferences(crate::prefs::Preferences),
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Open,
    Cancel,
    Ok,
}

impl SuePaint {
    /// Draws the open dialog, if any, over the studio.
    pub(crate) fn dialog_ui(
        &mut self,
        ctx: &egui::Context,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
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
        let (title, size) = match &dialog {
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
            Dialog::Preferences(_) => (tr!("preferences"), vec2(780.0, 540.0)),
            Dialog::AdvancedFill => (tr!("advanced-fill"), vec2(520.0, 470.0)),
            Dialog::CommandBar(_) => (tr!("command-bar-settings"), vec2(420.0, 460.0)),
            Dialog::CanvasProperties(_) => (tr!("canvas-properties"), vec2(380.0, 230.0)),
            Dialog::Proof(_) => (tr!("color-profile-preview-settings"), vec2(400.0, 230.0)),
            Dialog::PageSettings(_) => (tr!("change-page-settings"), vec2(380.0, 270.0)),
            Dialog::StoryInfo(_) => (tr!("change-story-information"), vec2(420.0, 360.0)),
            Dialog::StoryEditor(_) => (tr!("story-editor"), vec2(520.0, 520.0)),
            Dialog::Convert(..) => (tr!("convert-layer"), vec2(360.0, 190.0)),
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
        };
        let title = title.to_owned();
        let size = size.min(screen.size() - vec2(16.0, 16.0));
        let at = screen.center() - size / 2.0;
        let mut outcome = Outcome::Open;
        let mut change_keys = false;
        // Every command can be given a key, but the list shows the ones
        // that have one, as set now.
        let keys = matches!(dialog, Dialog::Shortcuts).then(|| {
            let section = crate::studio::KEYS_SECTION;
            let table = crate::studio::shortcut_table();
            let bound: Vec<sue_wm::shortcuts::Shortcut> = table
                .iter()
                .zip(sue_wm::shortcuts::resolve(cx.rice(), section, table))
                .filter(|(_, chords)| !chords.is_empty())
                .map(|(shortcut, _)| shortcut.clone())
                .collect();
            sue_wm::shortcuts::rows(cx.rice(), section, &bound)
        });
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
                    match &mut dialog {
                        Dialog::New(new) => {
                            new.own.clone_from(&self.canvas_presets);
                            outcome = new_ui(ui, new, theme);
                            if let Some(register) = new.preset_action.take() {
                                self.canvas_preset(cx, new, register);
                            }
                        }
                        Dialog::AdvancedFill => {
                            let (main, sub) = (self.main, self.sub);
                            outcome =
                                advanced_fill_ui(ui, &mut self.advanced_fill, main, sub, theme);
                        }
                        Dialog::Filter(filter) => outcome = filter_ui(ui, filter, theme),
                        Dialog::Pressure(pressure) => outcome = pressure_ui(ui, pressure, theme),
                        Dialog::Pages(pages) => outcome = self.pages_ui(ui, pages, cx, theme),
                        Dialog::Comps(comps) => outcome = self.comps_ui(ui, comps, cx, theme),
                        Dialog::Exposure => outcome = self.exposure_ui(ui, cx, theme),
                        Dialog::Frame(frame) => outcome = frame_ui(ui, frame, theme),
                        Dialog::Text(_, lettering) => outcome = text_ui(ui, lettering, theme),
                        Dialog::Onion(onion) => outcome = onion_ui(ui, onion, theme),
                        Dialog::SelectBy(kind, amount) => {
                            ui.horizontal(|ui| {
                                label(
                                    ui,
                                    &if *kind == 2 {
                                        tr!("blur-width")
                                    } else {
                                        tr!("width")
                                    },
                                    theme,
                                );
                                ui.add(egui::Slider::new(amount, 1.0..=100.0).suffix(tr!("px")));
                            });
                            ok_cancel(ui, &mut outcome);
                        }
                        Dialog::Workspace(name) => {
                            ui.horizontal(|ui| {
                                label(ui, &tr!("name"), theme);
                                ui.text_edit_singleline(name);
                            });
                            ok_cancel(ui, &mut outcome);
                        }
                        Dialog::Workspaces => {
                            let mut chosen = None;
                            let mut gone = None;
                            egui::ScrollArea::vertical()
                                .max_height(220.0)
                                .show(ui, |ui| {
                                    for (index, space) in self.workspaces.iter_mut().enumerate() {
                                        ui.horizontal(|ui| {
                                            ui.text_edit_singleline(&mut space.name);
                                            if ui.button(tr!("use")).clicked() {
                                                chosen = Some(index);
                                            }
                                            if ui.button(tr!("delete")).clicked() {
                                                gone = Some(index);
                                            }
                                        });
                                    }
                                    if self.workspaces.is_empty() {
                                        ui.label(
                                            RichText::new(tr!(
                                                "no-workspaces-yet-window-workspace-register"
                                            ))
                                            .size(theme.small_size)
                                            .color(theme.muted),
                                        );
                                    }
                                });
                            if let Some(index) = chosen {
                                self.use_workspace(index);
                            }
                            if let Some(index) = gone {
                                self.workspaces.remove(index);
                            }
                            ui.add_space(8.0);
                            if ui.button(tr!("close")).clicked() {
                                outcome = Outcome::Ok;
                            }
                        }
                        Dialog::Print(settings, printing) => {
                            ui.horizontal(|ui| {
                                label(ui, &tr!("paper"), theme);
                                egui::ComboBox::from_id_salt("sue-paint-paper")
                                    .selected_text(crate::localize(
                                        crate::print::PAPERS[settings.paper].0,
                                    ))
                                    .show_ui(ui, |ui| {
                                        for (index, (name, ..)) in
                                            crate::print::PAPERS.iter().enumerate()
                                        {
                                            ui.selectable_value(
                                                &mut settings.paper,
                                                index,
                                                crate::localize(name),
                                            );
                                        }
                                    });
                            });
                            ui.horizontal(|ui| {
                                label(ui, &tr!("orientation"), theme);
                                ui.radio_value(&mut settings.orientation, 0, tr!("auto"));
                                ui.radio_value(&mut settings.orientation, 1, tr!("portrait"));
                                ui.radio_value(&mut settings.orientation, 2, tr!("landscape"));
                            });
                            ui.horizontal(|ui| {
                                label(ui, &tr!("size"), theme);
                                ui.radio_value(&mut settings.size, 0, tr!("fit-to-page"));
                                ui.radio_value(&mut settings.size, 1, tr!("print-size"));
                                ui.radio_value(&mut settings.size, 2, tr!("scale"));
                            });
                            if settings.size == 2 {
                                ui.add(
                                    egui::Slider::new(&mut settings.percent, 1.0..=400.0)
                                        .suffix(" %"),
                                );
                            }
                            ui.horizontal(|ui| {
                                label(ui, &tr!("margin"), theme);
                                ui.add(
                                    egui::Slider::new(&mut settings.margin, 0.0..=50.0)
                                        .suffix(tr!("mm")),
                                );
                            });
                            ui.checkbox(&mut settings.all_pages, tr!("every-page-of-the-story"));
                            if *printing {
                                ui.label(
                                    RichText::new(tr!("pages-open-as-pdf-your-viewer"))
                                        .size(theme.small_size)
                                        .color(theme.muted),
                                );
                            }
                            ok_cancel(ui, &mut outcome);
                        }
                        Dialog::Preferences(prefs) => match prefs.ui(ui, theme) {
                            Some(true) => outcome = Outcome::Ok,
                            Some(false) => outcome = Outcome::Cancel,
                            None => {}
                        },
                        Dialog::Modifiers(chosen) => {
                            for ((key, name, choices), pick) in
                                MODIFIERS.iter().zip(chosen.iter_mut())
                            {
                                // Each choice reads as Settings names it.
                                let choice_text = |choice: &str| {
                                    sue_lang::text_or(
                                        "rice",
                                        &format!("{}.{choice}", key.replace('.', "-")),
                                        &choice.replace('_', " "),
                                    )
                                };
                                ui.horizontal(|ui| {
                                    label(ui, &crate::localize(name), theme);
                                    egui::ComboBox::from_id_salt(("sue-paint-modifier", *name))
                                        .selected_text(choice_text(
                                            choices[(*pick).min(choices.len() - 1)],
                                        ))
                                        .show_ui(ui, |ui| {
                                            for (index, choice) in choices.iter().enumerate() {
                                                ui.selectable_value(
                                                    pick,
                                                    index,
                                                    choice_text(choice),
                                                );
                                            }
                                        });
                                });
                            }
                            ui.label(
                                RichText::new(tr!("space-shift-turns-view-ctrl-alt"))
                                    .size(theme.small_size)
                                    .color(theme.muted),
                            );
                            ok_cancel(ui, &mut outcome);
                        }
                        Dialog::CommandBar(adding) => {
                            egui::ScrollArea::vertical()
                                .max_height(size.y - 150.0)
                                .show(ui, |ui| {
                                    for group in crate::panels::BAR_GROUPS {
                                        for (_, command, tip) in group.iter() {
                                            let mut shown = !self.bar_hidden.contains(*command);
                                            if ui
                                                .checkbox(&mut shown, crate::localize(tip))
                                                .changed()
                                            {
                                                if shown {
                                                    self.bar_hidden.remove(*command);
                                                } else {
                                                    self.bar_hidden.insert((*command).to_owned());
                                                }
                                            }
                                        }
                                        ui.separator();
                                    }
                                    let mut gone = None;
                                    for (index, command) in self.bar_extra.iter().enumerate() {
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                crate::menus::label_of(&self.menus, command)
                                                    .unwrap_or(command),
                                            );
                                            if ui.small_button(tr!("remove")).clicked() {
                                                gone = Some(index);
                                            }
                                        });
                                    }
                                    if let Some(index) = gone {
                                        self.bar_extra.remove(index);
                                    }
                                });
                            ui.horizontal(|ui| {
                                label(ui, &tr!("add"), theme);
                                let picked = crate::menus::label_of(&self.menus, adding)
                                    .unwrap_or(&tr!("a-menu-command"))
                                    .to_owned();
                                egui::ComboBox::from_id_salt("sue-paint-bar-add")
                                    .selected_text(picked)
                                    .height(320.0)
                                    .show_ui(ui, |ui| {
                                        for (label, command) in crate::menus::commands(&self.menus)
                                        {
                                            ui.selectable_value(adding, command, label);
                                        }
                                    });
                                if ui.button(tr!("add-2")).clicked()
                                    && !adding.is_empty()
                                    && !self.bar_extra.contains(adding)
                                {
                                    self.bar_extra.push(adding.clone());
                                }
                            });
                            ui.add_space(6.0);
                            if ui.button(tr!("close")).clicked() {
                                outcome = Outcome::Ok;
                            }
                        }
                        Dialog::PageSettings(settings) => {
                            ui.horizontal(|ui| {
                                label(ui, &tr!("size"), theme);
                                ui.add(
                                    egui::DragValue::new(&mut settings.width)
                                        .range(1..=10_000)
                                        .suffix(tr!("px")),
                                );
                                ui.label("by");
                                ui.add(
                                    egui::DragValue::new(&mut settings.height)
                                        .range(1..=10_000)
                                        .suffix(tr!("px")),
                                );
                            });
                            ui.horizontal(|ui| {
                                label(ui, &tr!("resolution"), theme);
                                ui.add(
                                    egui::DragValue::new(&mut settings.dpi)
                                        .range(1.0..=2400.0)
                                        .suffix(tr!("dpi")),
                                );
                            });
                            ui.horizontal(|ui| {
                                label(ui, &tr!("bleed"), theme);
                                ui.add(
                                    egui::DragValue::new(&mut settings.bleed)
                                        .range(0.0..=50.0)
                                        .suffix(tr!("mm")),
                                );
                            });
                            ui.horizontal(|ui| {
                                label(ui, &tr!("inner-border"), theme);
                                ui.add(
                                    egui::DragValue::new(&mut settings.inner)
                                        .range(0.0..=100.0)
                                        .suffix(tr!("mm")),
                                );
                            });
                            ui.label(
                                RichText::new(tr!("every-page-story-changes-each-keeps"))
                                    .size(theme.small_size)
                                    .color(theme.muted),
                            );
                            ok_cancel(ui, &mut outcome);
                        }
                        Dialog::StoryInfo(info) => {
                            ui.horizontal(|ui| {
                                label(ui, &tr!("title"), theme);
                                ui.text_edit_singleline(&mut info.title);
                            });
                            ui.horizontal(|ui| {
                                label(ui, &tr!("author"), theme);
                                ui.text_edit_singleline(&mut info.author);
                            });
                            ui.horizontal(|ui| {
                                label(ui, &tr!("first-page-number"), theme);
                                ui.add(egui::DragValue::new(&mut info.first).range(0..=9_999));
                            });
                            ui.horizontal(|ui| {
                                label(ui, &tr!("binding"), theme);
                                ui.radio_value(
                                    &mut info.right_bound,
                                    false,
                                    tr!("left-left-to-right"),
                                );
                                ui.radio_value(&mut info.right_bound, true, tr!("right-manga"));
                            });
                            ui.horizontal(|ui| {
                                label(ui, &tr!("first-page-place"), theme);
                                ui.radio_value(&mut info.paired, false, tr!("start-page-alone"));
                                ui.radio_value(&mut info.paired, true, tr!("start-page-paired"));
                            });
                            ui.separator();
                            // Clip Studio Paint's folio: page numbers on the pages.
                            ui.checkbox(&mut info.folio.show, tr!("show-folio"));
                            ui.add_enabled_ui(info.folio.show, |ui| {
                                ui.horizontal(|ui| {
                                    label(ui, &tr!("size-2"), theme);
                                    ui.add(
                                        egui::DragValue::new(&mut info.folio.size)
                                            .range(4.0..=72.0)
                                            .suffix(" pt"),
                                    );
                                });
                                ui.horizontal(|ui| {
                                    label(ui, &tr!("folio-position"), theme);
                                    ui.radio_value(
                                        &mut info.folio.center,
                                        false,
                                        tr!("folio-outside"),
                                    );
                                    ui.radio_value(
                                        &mut info.folio.center,
                                        true,
                                        tr!("folio-center"),
                                    );
                                });
                            });
                            ok_cancel(ui, &mut outcome);
                        }
                        Dialog::StoryEditor(texts) => {
                            egui::ScrollArea::vertical()
                                .max_height(size.y - 120.0)
                                .show(ui, |ui| {
                                    let mut last_page = 0;
                                    for (number, _, _, words) in texts.iter_mut() {
                                        if *number != last_page {
                                            last_page = *number;
                                            ui.add_space(4.0);
                                            ui.label(
                                                RichText::new(tr!("page", number = number))
                                                    .color(theme.text)
                                                    .strong(),
                                            );
                                        }
                                        ui.add(
                                            egui::TextEdit::multiline(words)
                                                .desired_rows(2)
                                                .desired_width(f32::INFINITY),
                                        );
                                    }
                                });
                            ok_cancel(ui, &mut outcome);
                        }
                        Dialog::Proof(profile) => {
                            ui.label(
                                RichText::new(tr!("how-picture-would-print-shown-while"))
                                    .size(theme.small_size)
                                    .color(theme.muted),
                            );
                            for (index, (name, ..)) in crate::proof::PROFILES.iter().enumerate() {
                                ui.radio_value(profile, index, crate::localize(name));
                            }
                            ok_cancel(ui, &mut outcome);
                        }
                        Dialog::CanvasProperties(props) => {
                            ui.horizontal(|ui| {
                                label(ui, &tr!("name"), theme);
                                ui.text_edit_singleline(&mut props.name);
                            });
                            ui.horizontal(|ui| {
                                label(ui, &tr!("resolution"), theme);
                                ui.add(
                                    egui::DragValue::new(&mut props.dpi)
                                        .range(1.0..=2400.0)
                                        .suffix(tr!("dpi")),
                                );
                            });
                            if props.paper.is_some() {
                                ui.horizontal(|ui| {
                                    ui.checkbox(&mut props.paper_shown, tr!("paper-2"));
                                    ui.color_edit_button_srgb(&mut props.paper_color);
                                });
                            }
                            ok_cancel(ui, &mut outcome);
                        }
                        Dialog::Convert(_, kind, name) => {
                            ui.horizontal(|ui| {
                                label(ui, &tr!("name"), theme);
                                ui.text_edit_singleline(name);
                            });
                            ui.horizontal(|ui| {
                                label(ui, &tr!("type"), theme);
                                ui.radio_value(kind, 0, tr!("raster-layer"));
                                ui.radio_value(kind, 1, tr!("vector-layer"));
                            });
                            ui.label(
                                RichText::new(tr!("vector-pixels-are-traced-lines-down"))
                                    .size(theme.small_size)
                                    .color(theme.muted),
                            );
                            ok_cancel(ui, &mut outcome);
                        }
                        Dialog::Message(_, paragraphs) => {
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
                        }
                        Dialog::Gamut(tolerance, all) => {
                            ui.label(
                                RichText::new(tr!("selects-colors-near-drawing-color"))
                                    .size(theme.small_size)
                                    .color(theme.muted),
                            );
                            ui.horizontal(|ui| {
                                label(ui, &tr!("color-margin"), theme);
                                let mut percent = *tolerance * 100.0;
                                if ui
                                    .add(egui::Slider::new(&mut percent, 0.0..=100.0).suffix(" %"))
                                    .changed()
                                {
                                    *tolerance = percent / 100.0;
                                }
                            });
                            ui.checkbox(all, tr!("refer-to-all-layers"));
                            ok_cancel(ui, &mut outcome);
                        }
                        Dialog::Webtoon(height, width) => {
                            egui::Grid::new("sue-paint-webtoon")
                                .num_columns(2)
                                .spacing(vec2(12.0, 8.0))
                                .show(ui, |ui| {
                                    label(ui, &tr!("split-every"), theme);
                                    ui.add(
                                        egui::DragValue::new(height)
                                            .range(200..=20000)
                                            .suffix(tr!("px")),
                                    );
                                    ui.end_row();
                                    label(ui, &tr!("output-width"), theme);
                                    ui.add(
                                        egui::DragValue::new(width)
                                            .range(100..=4000)
                                            .suffix(tr!("px")),
                                    );
                                    ui.end_row();
                                });
                            ui.label(
                                RichText::new(tr!("tall-canvas-cut-into-numbered-pictures"))
                                    .size(theme.small_size)
                                    .color(theme.muted),
                            );
                            ok_cancel(ui, &mut outcome);
                        }
                        Dialog::Timeline(fps, length) => {
                            egui::Grid::new("sue-paint-timeline-edit")
                                .num_columns(2)
                                .spacing(vec2(12.0, 8.0))
                                .show(ui, |ui| {
                                    label(ui, &tr!("frame-rate-2"), theme);
                                    ui.add(
                                        egui::DragValue::new(fps).range(1..=120).suffix(tr!("fps")),
                                    );
                                    ui.end_row();
                                    label(ui, &tr!("playback-time"), theme);
                                    ui.add(
                                        egui::DragValue::new(length)
                                            .range(1..=100_000)
                                            .suffix(tr!("frames")),
                                    );
                                    ui.end_row();
                                });
                            ok_cancel(ui, &mut outcome);
                        }
                        Dialog::Shape3D(kind) => {
                            label(ui, &tr!("shape"), theme);
                            for (index, name) in crate::three::SHAPES.iter().enumerate() {
                                ui.radio_value(kind, index, crate::localize(name));
                            }
                            ui.label(
                                RichText::new(tr!("file-import-3d-data-brings-models"))
                                    .size(theme.small_size)
                                    .color(theme.muted),
                            );
                            ok_cancel(ui, &mut outcome);
                        }
                        Dialog::Material(name) => {
                            ui.horizontal(|ui| {
                                label(ui, &tr!("material-name"), theme);
                                ui.add(egui::TextEdit::singleline(name).desired_width(200.0));
                            });
                            ui.label(
                                RichText::new(tr!("current-layer-goes-into-material-palette"))
                                    .size(theme.small_size)
                                    .color(theme.muted),
                            );
                            ok_cancel(ui, &mut outcome);
                        }
                        Dialog::Timelapse(export) => {
                            egui::Grid::new("sue-paint-timelapse-export")
                                .num_columns(2)
                                .spacing(vec2(12.0, 8.0))
                                .show(ui, |ui| {
                                    label(ui, &tr!("long-side"), theme);
                                    egui::ComboBox::from_id_salt("sue-paint-timelapse-side")
                                        .selected_text(tr!("px-2", side = export.side))
                                        .show_ui(ui, |ui| {
                                            for side in [480, 720, 1080, 1280, 1920] {
                                                ui.selectable_value(
                                                    &mut export.side,
                                                    side,
                                                    tr!("px-2", side = side),
                                                );
                                            }
                                        });
                                    ui.end_row();
                                    label(ui, &tr!("length"), theme);
                                    let mut fit = export.seconds.is_some();
                                    ui.vertical(|ui| {
                                        if ui
                                            .radio_value(
                                                &mut fit,
                                                true,
                                                tr!("fit-to-specified-length"),
                                            )
                                            .changed()
                                            || ui
                                                .radio_value(
                                                    &mut fit,
                                                    false,
                                                    tr!("export-all-frames"),
                                                )
                                                .changed()
                                        {
                                            export.seconds = fit.then_some(30.0);
                                        }
                                        if let Some(seconds) = export.seconds.as_mut() {
                                            ui.add(
                                                egui::DragValue::new(seconds)
                                                    .range(1.0..=600.0)
                                                    .suffix(" s"),
                                            );
                                        }
                                    });
                                    ui.end_row();
                                });
                            ui.label(
                                RichText::new(tr!("motion-jpeg-avi-video-animated-gif"))
                                    .size(theme.small_size)
                                    .color(theme.muted),
                            );
                            ok_cancel(ui, &mut outcome);
                        }
                        Dialog::Perspective(count) => {
                            ui.horizontal(|ui| {
                                label(ui, &tr!("type"), theme);
                                ui.vertical(|ui| {
                                    for (points, name) in [
                                        (1, tr!("n-1-point-perspective")),
                                        (2, tr!("n-2-point-perspective")),
                                        (3, tr!("n-3-point-perspective")),
                                    ] {
                                        ui.radio_value(count, points, name);
                                    }
                                });
                            });
                            ui.label(
                                RichText::new(tr!("strokes-run-toward-vanishing-points-while"))
                                    .size(theme.small_size)
                                    .color(theme.muted),
                            );
                            ok_cancel(ui, &mut outcome);
                        }
                        Dialog::Correction(_, filter, ..) => {
                            filter_params(ui, filter, theme);
                            ui.label(
                                RichText::new(tr!("changes-every-layer-under-one"))
                                    .size(theme.small_size)
                                    .color(theme.muted),
                            );
                            ok_cancel(ui, &mut outcome);
                        }
                        Dialog::Canvas(canvas) => outcome = canvas_ui(ui, canvas, theme),
                        Dialog::Resolution(resolution) => {
                            outcome = resolution_ui(ui, resolution, theme)
                        }
                        Dialog::Shortcuts => {
                            egui::ScrollArea::vertical()
                                .max_height(size.y - 90.0)
                                .show(ui, |ui| {
                                    let keys = keys.as_deref().unwrap_or_default();
                                    sue_wm::shortcuts::grid_ui(ui, "sue-paint-keys", keys, theme);
                                    ui.add_space(12.0);
                                    ui.label(
                                        RichText::new(tr!("with-the-pen-and-mouse"))
                                            .color(theme.text)
                                            .strong(),
                                    );
                                    let gestures: Vec<(String, String)> = GESTURES
                                        .iter()
                                        .map(|(what, how)| {
                                            (crate::localize(what), crate::localize(how))
                                        })
                                        .collect();
                                    sue_wm::shortcuts::grid_ui(
                                        ui,
                                        "sue-paint-gestures",
                                        &gestures,
                                        theme,
                                    );
                                });
                            ui.with_layout(Layout::right_to_left(Align::Max), |ui| {
                                if ui.button(tr!("close")).clicked() {
                                    outcome = Outcome::Cancel;
                                }
                                if ui
                                    .button(tr!("change-shortcuts-ellipsis"))
                                    .on_hover_text(tr!("settings-keymaps-any-number-keys-each"))
                                    .clicked()
                                {
                                    change_keys = true;
                                    outcome = Outcome::Cancel;
                                }
                            });
                        }
                    }
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
        match (dialog, outcome) {
            (dialog, Outcome::Open) => {
                if let Dialog::Correction(layer, filter, ..) = &dialog
                    && let Some(text) = filter.to_text()
                    && let Ok(entity) = cx.world.arena.get_by_id(*layer)
                    && entity
                        .property(doc::CORRECTION_FILTER)
                        .and_then(sue_core::Value::as_str)
                        != Some(text.as_str())
                    && let Ok(entity) = cx.world.arena.get_by_id_mut(*layer)
                {
                    // The setting shows live while the dialog is open.
                    entity.properties_mut().set(doc::CORRECTION_FILTER, text);
                }
                if let Dialog::Filter(filter) = &dialog {
                    let pointer_down = ctx.input(|input| input.pointer.any_down());
                    let wanted = if filter.preview {
                        Some(filter.filter)
                    } else {
                        None
                    };
                    let cheap = !matches!(
                        filter.filter,
                        Filter::GaussianBlur { .. }
                            | Filter::Sharpen { .. }
                            | Filter::Mosaic { .. }
                            | Filter::LineExtraction { .. }
                            | Filter::MotionBlur { .. }
                            | Filter::RadialBlur { .. }
                            | Filter::ZoomBlur { .. }
                            | Filter::LineWidth { .. }
                            | Filter::Wave { .. }
                            | Filter::Pinch { .. }
                            | Filter::Polar { .. }
                            | Filter::Ripple { .. }
                            | Filter::Twirl { .. }
                            | Filter::ZigZag { .. }
                            | Filter::Crystallize { .. }
                            | Filter::JpegNoise { .. }
                            | Filter::Quick { .. }
                            | Filter::SmartSmooth { .. }
                            | Filter::Fisheye { .. }
                            | Filter::Aberration { .. }
                            | Filter::Glow { .. }
                            | Filter::DropShadow { .. }
                            | Filter::Inflate { .. }
                            | Filter::Retro { .. }
                            | Filter::PerlinNoise { .. }
                            | Filter::Unsharp { .. }
                            | Filter::Dust { .. }
                    );
                    if wanted != filter.shown && (cheap || !pointer_down) {
                        let pixels = match wanted {
                            Some(filter_now) => filter.result(filter_now),
                            None => filter.before.clone(),
                        };
                        filter.show(cx, &pixels);
                        let mut updated = filter.clone();
                        updated.shown = wanted;
                        self.dialog = Some(Dialog::Filter(updated));
                        ctx.request_repaint();
                        return;
                    }
                }
                self.dialog = Some(dialog);
            }
            (Dialog::Filter(filter), Outcome::Cancel) => {
                if filter.shown.is_some() {
                    filter.show(cx, &filter.before);
                }
                if filter.merged {
                    // Take back the merged layer made for it.
                    self.run("edit.undo", cx);
                }
            }
            (Dialog::Filter(filter), Outcome::Ok) => self.keep_filter(cx, &filter),
            (Dialog::Frame(frame), Outcome::Ok) => self.add_frame_layer(cx, frame),
            (Dialog::Text(layer, lettering), Outcome::Ok) => self.put_text(cx, layer, &lettering),
            (Dialog::Perspective(count), Outcome::Ok) => self.make_perspective(cx, count),
            (Dialog::Timelapse(export), Outcome::Ok) => self.ask_export_timelapse(cx, export),
            (Dialog::Material(name), Outcome::Ok) => self.register_material(cx, &name),
            (Dialog::SelectBy(3, width), Outcome::Ok) => self.outline_selection(cx, width),
            (Dialog::SelectBy(kind, amount), Outcome::Ok) => {
                if let Some(doc) = self.doc_mut()
                    && let Some(selection) = doc.selection.clone()
                {
                    let changed = match kind {
                        0 => selection.expanded(amount),
                        1 => selection.shrunk(amount),
                        _ => selection.blurred(amount),
                    };
                    doc.set_selection(Some(changed));
                }
            }
            (Dialog::Gamut(tolerance, all), Outcome::Ok) => self.select_gamut(cx, tolerance, all),
            (Dialog::Workspace(name), Outcome::Ok) => {
                let name = if name.trim().is_empty() {
                    tr!("workspace")
                } else {
                    name.trim().to_owned()
                };
                let space = self.workspace_now(&name);
                match self.workspaces.iter().position(|known| known.name == name) {
                    Some(index) => self.workspaces[index] = space,
                    None => self.workspaces.push(space),
                }
                self.save_extras(cx);
            }
            (Dialog::Workspaces, _) => self.save_extras(cx),
            (Dialog::Message(..) | Dialog::Print(..) | Dialog::Modifiers(_), Outcome::Cancel) => {}
            (Dialog::Message(..), _) => {}
            (Dialog::Print(settings, printing), _) => {
                self.print = settings;
                if printing {
                    self.print_now(cx);
                }
            }
            (Dialog::Modifiers(chosen), _) => {
                for ((key, _, choices), pick) in MODIFIERS.iter().zip(chosen) {
                    let value = choices[pick.min(choices.len() - 1)].to_owned();
                    if let Err(error) =
                        cx.edit_rice(SetRiceValue::new(*key, RiceValue::Text(value)))
                    {
                        cx.fail(tr!("cannot-keep-the-modifier-keys", error = error));
                    }
                }
            }
            (Dialog::CommandBar(_), _) => self.save_extras(cx),
            (Dialog::Preferences(_), Outcome::Cancel) => {}
            (Dialog::Preferences(prefs), _) => {
                for (key, value) in prefs.changes() {
                    if cx.rice().get(key).as_ref() == Some(&value) {
                        continue;
                    }
                    if let Err(error) = cx.edit_rice(SetRiceValue::new(key, value)) {
                        cx.fail(tr!("cannot-keep-the-preferences", error = error));
                    }
                }
            }
            (
                Dialog::CanvasProperties(_)
                | Dialog::Convert(..)
                | Dialog::Proof(_)
                | Dialog::PageSettings(_)
                | Dialog::StoryInfo(_)
                | Dialog::StoryEditor(_),
                Outcome::Cancel,
            ) => {}
            (Dialog::PageSettings(settings), _) => {
                match crate::story::page_settings(
                    &cx.world.arena,
                    settings.story,
                    [settings.width.max(1), settings.height.max(1)],
                    settings.dpi,
                    settings.bleed,
                    settings.inner,
                ) {
                    Ok(edit) => {
                        Self::apply(cx, edit);
                        for doc in &mut self.docs {
                            doc.view.fitted = false;
                        }
                    }
                    Err(error) => cx.fail(error),
                }
            }
            (Dialog::StoryInfo(info), _) => {
                let mut edit = crate::doc::Edit::new(tr!("change-story-information"))
                    .then(sue_core::txn::SetProperty::new(
                        info.story,
                        crate::story::AUTHOR,
                        info.author.clone(),
                    ))
                    .then(sue_core::txn::SetProperty::new(
                        info.story,
                        crate::story::FIRST_NUMBER,
                        info.first,
                    ))
                    .then(sue_core::txn::SetProperty::new(
                        info.story,
                        crate::story::BINDING,
                        if info.right_bound { "right" } else { "left" },
                    ))
                    .then(sue_core::txn::SetProperty::new(
                        info.story,
                        crate::story::START,
                        if info.paired { "paired" } else { "alone" },
                    ))
                    .then(sue_core::txn::SetProperty::new(
                        info.story,
                        crate::story::FOLIO,
                        info.folio.show,
                    ))
                    .then(sue_core::txn::SetProperty::new(
                        info.story,
                        crate::story::FOLIO_SIZE,
                        f64::from(info.folio.size),
                    ))
                    .then(sue_core::txn::SetProperty::new(
                        info.story,
                        crate::story::FOLIO_PLACE,
                        if info.folio.center {
                            "center"
                        } else {
                            "outside"
                        },
                    ));
                if !info.title.trim().is_empty() {
                    edit = edit.then(sue_core::txn::Rename::new(info.story, info.title.trim()));
                }
                Self::apply(cx, edit);
            }
            (Dialog::StoryEditor(texts), _) => {
                let mut edit = crate::doc::Edit::new(tr!("story-editor"));
                let mut changed = false;
                for (_, page, layer, words) in texts {
                    let arena = &cx.world.arena;
                    let same = arena
                        .get_by_id(layer)
                        .ok()
                        .and_then(crate::lettering::Lettering::of)
                        .is_some_and(|lettering| lettering.content == words);
                    if same {
                        continue;
                    }
                    let Some([width, height]) = arena
                        .get_by_id(page)
                        .ok()
                        .and_then(sue_model::picture::size)
                    else {
                        continue;
                    };
                    if let Some(step) =
                        crate::lettering::retext(arena, layer, &words, width, height)
                    {
                        edit = edit.then(step);
                        changed = true;
                    }
                }
                if changed {
                    Self::apply(cx, edit);
                }
            }
            (Dialog::Proof(profile), _) => {
                self.proof_profile = profile;
                self.proof_on = true;
            }
            (Dialog::CanvasProperties(props), _) => {
                let mut edit = crate::doc::Edit::new(tr!("canvas-properties")).then(
                    sue_core::txn::SetProperty::new(
                        props.picture,
                        sue_model::picture::DPI,
                        props.dpi,
                    ),
                );
                let name = props.name.trim();
                if !name.is_empty() {
                    edit = edit.then(sue_core::txn::Rename::new(props.picture, name));
                }
                if let Some(paper) = props.paper {
                    let [r, g, b] = props.paper_color;
                    edit = edit
                        .then(sue_core::txn::SetProperty::new(
                            paper,
                            sue_model::picture::COLOR,
                            sue_core::Color::srgb8(r, g, b),
                        ))
                        .then(sue_core::txn::SetProperty::new(
                            paper,
                            sue_model::picture::VISIBLE,
                            props.paper_shown,
                        ));
                }
                Self::apply(cx, edit);
            }
            (Dialog::Convert(layer, kind, name), _) => {
                let Some(picture) = self.picture() else {
                    return;
                };
                let arena = &cx.world.arena;
                let vector = crate::vector::is_vector(arena, layer);
                let made = match (kind, vector) {
                    (1, false) => crate::doc::vectorize(arena, picture, layer).map(Some),
                    (0, true) => crate::doc::rasterize(arena, picture, layer).map(Some),
                    _ => Ok(None),
                };
                match made {
                    Ok(Some((mut edit, id))) => {
                        if !name.trim().is_empty() {
                            edit = edit.then(sue_core::txn::Rename::new(id, name.trim()));
                        }
                        if Self::apply(cx, edit)
                            && let Some(doc) = self.doc_mut()
                        {
                            doc.layer = Some(id);
                        }
                    }
                    Ok(None) => {
                        if !name.trim().is_empty() {
                            Self::apply(
                                cx,
                                crate::doc::Edit::new(tr!("rename-layer"))
                                    .then(sue_core::txn::Rename::new(layer, name.trim())),
                            );
                        }
                    }
                    Err(error) => cx.fail(error),
                }
            }
            (Dialog::Webtoon(height, width), Outcome::Ok) => {
                self.ask_export_webtoon(cx, height, width)
            }
            (Dialog::Shape3D(kind), Outcome::Ok) => {
                let name = crate::three::SHAPES[kind.min(crate::three::SHAPES.len() - 1)];
                let model = sue_core::Entity::empty(crate::localize(name), sue_model::class::MODEL)
                    .with_ext(crate::three::shape(kind));
                self.add_3d_layer(cx, name, model);
            }
            (Dialog::Timeline(fps, length), Outcome::Ok) => {
                if let Some(picture) = self.picture()
                    && let Some(edit) =
                        crate::anim::edit_timeline(&cx.world.arena, picture, fps, length)
                {
                    Self::apply(cx, edit);
                }
            }
            (Dialog::Onion(onion), Outcome::Ok) => {
                if let Some(picture) = self.picture() {
                    let edit = doc::Edit::new(tr!("onion-skin-settings")).then(
                        sue_core::txn::SetProperty::new(
                            picture,
                            crate::anim::ONION,
                            onion.to_text(),
                        ),
                    );
                    if Self::apply(cx, edit) {
                        // The settings are for seeing it.
                        self.onion = true;
                    }
                }
            }
            (Dialog::Correction(layer, filter, before, made), outcome) => {
                let text = filter.to_text().unwrap_or_default();
                // The setting shows live while the dialog is open.
                let set_now = |cx: &mut AppletContext<'_>, text: &str| {
                    if let Ok(entity) = cx.world.arena.get_by_id_mut(layer) {
                        entity
                            .properties_mut()
                            .set(doc::CORRECTION_FILTER, text.to_owned());
                    }
                };
                match outcome {
                    Outcome::Open => {}
                    Outcome::Ok => {
                        // Back as it was, then the change as one undo step.
                        set_now(cx, &before);
                        if text != before {
                            Self::apply(cx, doc::set(layer, doc::CORRECTION_FILTER, text));
                        }
                    }
                    Outcome::Cancel => {
                        set_now(cx, &before);
                        if made {
                            let _ = cx.undo(false);
                        }
                    }
                }
            }
            (Dialog::Canvas(canvas), Outcome::Ok) => {
                if let Some(picture) = self.picture() {
                    match doc::resize_canvas(&cx.world.arena, picture, canvas.size, canvas.shift())
                    {
                        Ok(edit) => {
                            Self::apply(cx, edit);
                            if let Some(doc) = self.doc_mut() {
                                doc.set_selection(None);
                            }
                        }
                        Err(error) => cx.fail(error),
                    }
                }
            }
            (Dialog::Resolution(resolution), Outcome::Ok) => {
                if let Some(picture) = self.picture() {
                    match doc::resample(&cx.world.arena, picture, resolution.size, resolution.dpi) {
                        Ok(edit) => {
                            Self::apply(cx, edit);
                            if let Some(doc) = self.doc_mut() {
                                doc.set_selection(None);
                            }
                        }
                        Err(error) => cx.fail(error),
                    }
                }
            }
            (Dialog::AdvancedFill, Outcome::Ok) => self.advanced_fill_now(cx),
            (Dialog::AdvancedFill, Outcome::Cancel) => {}
            (Dialog::New(new), Outcome::Ok) => {
                let spec = new.spec();
                match new.project {
                    Project::Illustration | Project::Webtoon => self.make_picture(cx, &spec),
                    Project::Comic | Project::Fanzine | Project::ComicAll => {
                        self.make_story(cx, &spec, new.pages as usize, new.right_bound);
                    }
                    Project::Animation => self.make_animation(cx, &spec, new.frames, new.fps),
                }
            }
            (Dialog::Pressure(pressure), Outcome::Ok) => {
                let text = pressure.curve.to_string();
                match cx.edit_rice(SetRiceValue::new(PEN_PRESSURE, RiceValue::Text(text))) {
                    Ok(_) => cx.notify(tr!("pen-pressure-adjusted-every-tool")),
                    Err(error) => cx.fail(tr!("cannot-keep-the-pen-pressure", error = error)),
                }
            }
            (
                Dialog::New(_)
                | Dialog::Shortcuts
                | Dialog::Pressure(_)
                | Dialog::Pages(_)
                | Dialog::Comps(_)
                | Dialog::Exposure
                | Dialog::Frame(_)
                | Dialog::Canvas(_)
                | Dialog::Resolution(_)
                | Dialog::Text(..),
                Outcome::Cancel | Outcome::Ok,
            )
            | (
                Dialog::Perspective(_)
                | Dialog::Onion(_)
                | Dialog::Timelapse(_)
                | Dialog::Material(_)
                | Dialog::SelectBy(..)
                | Dialog::Gamut(..)
                | Dialog::Webtoon(..)
                | Dialog::Timeline(..)
                | Dialog::Shape3D(_)
                | Dialog::Workspace(_),
                Outcome::Cancel,
            ) => {}
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
