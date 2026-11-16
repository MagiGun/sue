//! Sue Paint's dialogs: New (a canvas from presets, paper sizes and
//! units), the filter dialogs with a live preview, and the shortcut list.
//! One file per family; this one holds the open dialog, what it ends
//! with, the host that draws it, and the pieces the families share.
//!
//! The host is Sue's one dialog host (`sue_wm::ui::Dialog`): a title,
//! the body, and a footer whose buttons stand in Sue's one order. A
//! dialog is as tall as what it holds, up to the screen (it scrolls past
//! that), and as wide as its [`Width`] says, never wider than the screen.
//! Each dialog says what it needs in [`Dialog::look`]; there is no table
//! of fixed sizes.

mod anim;
mod canvas;
mod color;
mod colorize;
mod command_bar;
mod comps;
mod fill;
mod filter;
mod frame;
mod lines_tones;
mod modifiers;
mod new;
mod pressure;
mod settings;
mod story;
mod text;
mod timelapse;

use egui::{Response, Ui, vec2};
use sue_core::EntityId;
use sue_wm::ui::{Answer, Density, DialogHost, Space, TextRole, Tip, tooltip};
use sue_wm::{AppletContext, Theme};

use crate::filters::Filter;
use crate::studio::SuePaint;

pub(crate) use anim::AnimDialog;
use anim::{onion_ui, timeline_ui};
use canvas::{
    canvas_properties_ui, canvas_ui, convert_ui, grid_ui, material_ui, perspective_ui, print_ui,
    proof_ui, resolution_ui, shape_3d_ui,
};
use colorize::colorize_ui;
use fill::{gamut_ui, select_by_ui};
use frame::frame_ui;
use lines_tones::lines_tones_ui;
use modifiers::modifiers_ui;
use pressure::pressure_ui;
use settings::{shortcuts_ui, workspace_ui};
use story::{page_settings_ui, story_editor_ui, story_info_ui, webtoon_ui};
use text::text_ui;

pub(crate) use crate::gradient_bar::GradientLibrary;
pub(crate) use canvas::{
    CanvasDialog, CanvasProperties, ConvertDialog, PerspectiveDialog, ResolutionDialog,
};
pub(crate) use color::{ColorDialog, ColorTarget, cmyk_of, rgb_of_cmyk};
pub(crate) use colorize::ColorizeDialog;
pub(crate) use command_bar::{BarDialog, BarTarget};
pub(crate) use comps::CompsDialog;
pub(crate) use fill::{AdvancedFill, target_label};
pub(crate) use filter::FilterDialog;
pub(crate) use lines_tones::LinesTonesDialog;
pub(crate) use modifiers::ModifiersDialog;
#[cfg(test)]
pub(crate) use new::SIZES;
pub(crate) use new::{CanvasPreset, NEW_PRESET_TARGET, NewDialog, Project, new_presets};
pub(crate) use pressure::{DEFAULT_PEN_PRESSURE, PEN_PRESSURE, PressureDialog};
#[cfg(test)]
pub(crate) use settings::GESTURES;
pub(crate) use settings::MODIFIERS;
pub(crate) use story::{PageSettings, PagesDialog, StoryInfo};
pub(crate) use timelapse::TimelapseDialog;

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
    /// Create Perspective Ruler: how many vanishing points, and whether
    /// the ruler gets a new layer of its own.
    Perspective(PerspectiveDialog),
    /// Onion Skin Settings.
    Onion(crate::anim::Onion),
    /// Export Timelapse: the video's size, length and shape, and its
    /// preview.
    Timelapse(Box<TimelapseDialog>),
    /// Register Material, Image: its name.
    Material(String),
    /// Expand with round corners (0), Shrink (1), Blur the border of (2),
    /// Outline (3) or Expand with square corners (4) the selection, by
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
    /// Modifier Key Settings: what the keys, the wheel and the buttons do
    /// on the canvas.
    Modifiers(Box<ModifiersDialog>),
    /// Command Bar Settings, or Quick Access Settings.
    CommandBar(BarDialog),
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
    /// Convert Layer: the layer and what it becomes.
    Convert(ConvertDialog),
    /// File, Manage Fonts: Sue's shared font list.
    Fonts(sue_wm::fonts::FontManager),
    /// Convert to Lines and Tones: its settings and live preview.
    LinesTones(LinesTonesDialog),
    /// View, Grid Settings: the picture and its grid as set so far.
    Grid(EntityId, crate::grid::Grid),
    /// Layer, Ruler/Frame, Divide Frame Border Equally.
    DivideFrame(crate::comic::DivideSpec),
    /// Color Settings, Edit Color Sets, and the names of sets and colors.
    Color(ColorDialog),
    /// Deleting picked layer folders asks first: whether "Do not show
    /// again" is ticked.
    DeleteFolders(bool),
    /// New Layer, Tone: Simple Tone Settings.
    SimpleTone(crate::panels::SimpleTone),
    /// A dialog of the timeline (New Timeline, the animation exports,
    /// Audio Clip Settings and the rest).
    Anim(Box<AnimDialog>),
    /// Edit, Colorize: the line art, the hint layer and the blur.
    Colorize(ColorizeDialog),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Open,
    Cancel,
    Ok,
}

/// How wide a dialog is. Its height follows what it holds.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Width {
    /// A question with a field or two.
    Narrow,
    /// A form of a few rows.
    Form,
    /// A form with a picture, a list or two columns.
    Wide,
    /// Two columns of settings side by side.
    Double,
    /// As wide as a shared component asks.
    Points(f32),
}

impl Width {
    /// The width in points, from the spacing scale so a denser or looser
    /// theme scales every dialog alike.
    fn points(self, theme: &Theme) -> f32 {
        let unit = theme.space(Space::S9);
        match self {
            Self::Narrow => unit * 8.0,
            Self::Form => unit * 10.0,
            Self::Wide => unit * 13.0,
            Self::Double => unit * 18.0,
            Self::Points(points) => points,
        }
    }
}

/// The dialogs whose bodies fill the room they are given, before the
/// screen clamps them: 3D Preview for Binding's book, Modifier Key
/// Settings' rows, and Command Bar Settings' columns (its width, and the
/// room for its lists in units of the largest spacing step).
const BINDING_SIZE: egui::Vec2 = vec2(820.0, 560.0);
const MODIFIERS_SIZE: egui::Vec2 = vec2(600.0, 540.0);
const COMMAND_BAR_SIZE: egui::Vec2 = vec2(780.0, 540.0);
const COMMAND_BAR_ROOM: f32 = 13.0;
/// What a dialog's title and frame took of the fixed sizes dialogs had
/// before the host sized them by what they hold.
const TITLE_ROOM: f32 = 64.0;

/// The buttons in a dialog's footer.
#[derive(Clone, Debug, Default, PartialEq)]
struct Footer {
    accept: Option<String>,
    cancel: Option<String>,
    extra: Option<String>,
}

impl Footer {
    /// OK and Cancel.
    fn ok_cancel() -> Self {
        Self {
            accept: Some(tr!("ok")),
            cancel: Some(tr!("cancel")),
            extra: None,
        }
    }

    /// Close alone, for a dialog whose changes are made as they happen.
    fn close() -> Self {
        Self {
            cancel: Some(tr!("close")),
            ..Self::default()
        }
    }

    /// None: the body has its own buttons.
    fn none() -> Self {
        Self::default()
    }

    /// The same footer with an extra action at the leading edge.
    fn with_extra(mut self, label: String) -> Self {
        self.extra = Some(label);
        self
    }

    fn is_empty(&self) -> bool {
        self.accept.is_none() && self.cancel.is_none() && self.extra.is_none()
    }
}

/// What the host needs to know of a dialog before it draws the body.
struct Look {
    title: String,
    width: Width,
    footer: Footer,
    /// Enter belongs to the body (a text box of several lines, a pad).
    enter_is_the_bodys: bool,
    /// For a body that lays out a list in the room it has: how tall the
    /// room is meant to be. The dialog still follows what it draws.
    room: Option<f32>,
    /// For a body that fills whatever height it is given (a list over a
    /// sample): the height it gets, in points.
    fixed: Option<f32>,
}

impl Look {
    fn new(title: String, width: Width, footer: Footer) -> Self {
        Self {
            title,
            width,
            footer,
            enter_is_the_bodys: false,
            room: None,
            fixed: None,
        }
    }

    /// Gives the body exactly `points` of height.
    fn fixed(mut self, points: f32) -> Self {
        self.fixed = Some(points);
        self
    }

    fn enter_is_the_bodys(mut self) -> Self {
        self.enter_is_the_bodys = true;
        self
    }

    /// `units` of the spacing scale's largest step.
    fn room(mut self, units: f32) -> Self {
        self.room = Some(units);
        self
    }
}

impl Dialog {
    /// The dialog's title, width and footer.
    #[allow(clippy::too_many_lines, reason = "one arm per dialog")]
    fn look(&self) -> Look {
        use Width::{Double, Form, Narrow, Wide};
        let ok = Footer::ok_cancel;
        match self {
            Dialog::New(new) => Look::new(
                tr!("new-2"),
                if new.project == Project::Animation {
                    Double
                } else {
                    Wide
                },
                ok().with_extra(tr!("register-preset")),
            ),
            // The gallery keeps one title (and so one window) whatever look
            // is picked.
            Dialog::Filter(FilterDialog {
                filter: Filter::Quick { effect, .. },
                ..
            }) => Look::new(
                if *effect {
                    tr!("quick-effects")
                } else {
                    tr!("quick-filters")
                },
                Wide,
                Footer::none(),
            ),
            Dialog::Filter(filter) => Look::new(
                filter.filter.label(),
                Width::Points(filter::dialog_size(&filter.filter).x),
                Footer::none(),
            ),
            Dialog::Shortcuts => Look::new(tr!("keyboard-shortcuts"), Wide, Footer::none())
                .room(14.0)
                .enter_is_the_bodys(),
            // 3D Preview for Binding: the book fills what it is given and
            // has its own Close.
            Dialog::Pages(_) => Look::new(
                tr!("binding-preview-3d"),
                Width::Points(BINDING_SIZE.x),
                Footer::none(),
            )
            .fixed(BINDING_SIZE.y)
            .enter_is_the_bodys(),
            Dialog::Comps(_) => Look::new(tr!("layer-comps"), Form, Footer::close()),
            Dialog::Exposure => Look::new(tr!("exposure-sheet"), Wide, Footer::none())
                .room(14.0)
                .enter_is_the_bodys(),
            Dialog::Frame(_) => Look::new(tr!("new-frame-border-folder"), Form, ok()),
            Dialog::Canvas(_) => Look::new(tr!("change-canvas-size"), Narrow, ok()),
            Dialog::Resolution(_) => Look::new(tr!("change-image-resolution"), Narrow, ok()),
            Dialog::Text(..) => Look::new(tr!("text"), Wide, ok()).enter_is_the_bodys(),
            Dialog::Perspective(_) => Look::new(tr!("create-perspective-ruler"), Narrow, ok()),
            Dialog::Onion(_) => Look::new(tr!("onion-skin-settings"), Form, ok()),
            Dialog::Timelapse(_) => Look::new(tr!("export-timelapse"), Form, ok()),
            Dialog::Material(_) => Look::new(tr!("register-material"), Form, ok()),
            Dialog::SelectBy(kind, _) => Look::new(
                match kind {
                    0 | 4 => tr!("expand-selected-area-2"),
                    1 => tr!("shrink-selected-area-2"),
                    3 => tr!("outline-selection"),
                    _ => tr!("blur-border"),
                },
                Narrow,
                ok(),
            ),
            Dialog::Gamut(..) => Look::new(tr!("select-color-gamut"), Form, ok()),
            Dialog::Webtoon(..) => Look::new(tr!("export-webtoon"), Form, ok()),
            Dialog::Timeline(..) => Look::new(tr!("edit-timeline"), Narrow, ok()),
            Dialog::Shape3D(_) => Look::new(tr!("new-3d-layer"), Narrow, ok()),
            Dialog::Workspace(_) => Look::new(tr!("register-workspace"), Form, ok()),
            Dialog::Workspaces => Look::new(tr!("manage-workspaces"), Form, Footer::none()),
            Dialog::Message(title, _) => Look::new(title.clone(), Wide, Footer::close()).room(11.5),
            Dialog::Print(_, true) => Look::new(tr!("print"), Form, ok()),
            Dialog::Print(_, false) => Look::new(tr!("print-settings"), Form, ok()),
            // Its rows fill the dialog down to Reset and the note.
            Dialog::Modifiers(_) => Look::new(
                tr!("modifier-key-settings"),
                Width::Points(MODIFIERS_SIZE.x),
                ok(),
            )
            .fixed(MODIFIERS_SIZE.y),
            Dialog::AdvancedFill => Look::new(tr!("advanced-fill"), Wide, ok()),
            Dialog::CommandBar(bar) => Look::new(
                bar.title(),
                Width::Points(COMMAND_BAR_SIZE.x),
                Footer::none(),
            )
            .room(COMMAND_BAR_ROOM)
            .enter_is_the_bodys(),
            Dialog::CanvasProperties(_) => Look::new(tr!("canvas-properties"), Form, ok()),
            Dialog::Proof(_) => Look::new(tr!("color-profile-preview-settings"), Form, ok()),
            Dialog::PageSettings(_) => Look::new(tr!("change-page-settings"), Form, ok()),
            Dialog::StoryInfo(_) => Look::new(tr!("change-story-information"), Form, ok()),
            Dialog::StoryEditor(_) => Look::new(tr!("story-editor"), Wide, ok())
                .room(13.0)
                .enter_is_the_bodys(),
            Dialog::Convert(_) => Look::new(tr!("convert-layer"), Form, ok()),
            Dialog::LinesTones(_) => Look::new(tr!("convert-to-lines-and-tones"), Wide, ok()),
            Dialog::Grid(..) => Look::new(tr!("grid-settings"), Narrow, ok()),
            // These draw their own buttons.
            Dialog::DivideFrame(_) => {
                Look::new(tr!("divide-frame-border-equally"), Form, Footer::none())
            }
            Dialog::Color(color) => {
                let (title, size) = color.title_and_size();
                let look = Look::new(title, Width::Points(size.x), Footer::none());
                // Edit Color Sets lays its list out in the room it is
                // given: it gets its old height, less the title.
                if matches!(color, ColorDialog::EditSets(..)) {
                    look.fixed(size.y - TITLE_ROOM)
                } else {
                    look
                }
            }
            Dialog::DeleteFolders(_) => Look::new(
                tr!("delete-layer-folders"),
                Width::Points(crate::panels::DELETE_FOLDERS_SIZE.x),
                Footer::none(),
            ),
            Dialog::SimpleTone(_) => Look::new(
                tr!("simple-tone-settings"),
                Width::Points(crate::panels::SIMPLE_TONE_SIZE.x),
                Footer::none(),
            ),
            Dialog::Anim(dialog) => Look::new(
                dialog.title(),
                Width::Points(dialog.size().x),
                if matches!(**dialog, AnimDialog::StopTimelapse(..)) {
                    Footer::none()
                } else {
                    ok()
                },
            ),
            Dialog::Fonts(_) => Look::new(
                tr!("manage-fonts"),
                Width::Points(sue_wm::fonts::MANAGE_SIZE.x),
                Footer::none(),
            )
            .fixed(sue_wm::fonts::MANAGE_SIZE.y)
            .enter_is_the_bodys(),
            Dialog::Colorize(_) => Look::new(tr!("colorize"), Form, Footer::none()),
            // Its body draws its own OK and Cancel, by its preview.
            Dialog::Correction(_, filter, ..) => Look::new(
                filter.label(),
                Width::Points(filter::correction_size(filter).x),
                Footer::none(),
            ),
            Dialog::Pressure(_) => Look::new(
                tr!("adjust-pen-pressure"),
                Wide,
                ok().with_extra(tr!("reset-to-default")),
            )
            .enter_is_the_bodys(),
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
        let look = dialog.look();
        let width = look.width.points(theme);
        let room = vec2(
            width,
            look.room
                .map_or(f32::INFINITY, |units| units * theme.space(Space::S9)),
        );
        let mut change_keys = false;
        // A window of its own where the system gives Sue more than one,
        // as Clip Studio Paint's dialogs: it can be moved anywhere,
        // outside Sue's window too.
        let mut host = sue_wm::ui::Dialog::new(
            ("sue-paint-dialog", look.title.as_str()),
            &look.title,
            theme,
        )
        .host(DialogHost::Window)
        .width(width);
        if let Some(label) = &look.footer.accept {
            host = host.accept(label.clone());
        }
        if let Some(label) = &look.footer.extra {
            host = host.extra(label.clone());
        }
        if let Some(label) = &look.footer.cancel {
            host = host.cancel_as(label.clone());
        }
        if let Some(points) = look.fixed {
            // Never taller than the window less the dialog's own frame.
            let room = ctx.content_rect().height() - theme.space(Space::S9) * 4.0;
            host = host.height(points.min(room).max(theme.space(Space::S9)));
        }
        if look.enter_is_the_bodys {
            host = host.enter_is_the_bodys();
        }
        if look.footer.is_empty() {
            // The body's own buttons are the way out (Escape and the
            // window's close box still cancel).
            host = host.no_close_button();
        }
        let shown = host.show(ctx, |ui| {
            self.dialog_body(ui, &mut dialog, cx, theme, room, &mut change_keys)
        });
        let mut outcome = shown.inner;
        match shown.answer {
            Some(Answer::Accept) => outcome = Outcome::Ok,
            Some(Answer::Cancel) => outcome = Outcome::Cancel,
            Some(Answer::Extra) => {
                self.dialog_extra(cx, &mut dialog);
                outcome = Outcome::Open;
            }
            Some(Answer::Alternate) | None => {}
        }
        // A dialog whose body has its own buttons still takes Enter as
        // OK when nothing holds the keys.
        if outcome == Outcome::Open
            && shown.answer.is_none()
            && look.footer.is_empty()
            && !look.enter_is_the_bodys
            && !ctx.text_edit_focused()
            && ctx.memory(|memory| memory.focused().is_none())
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
        {
            outcome = Outcome::Ok;
        }
        if change_keys {
            cx.request(sue_wm::ShellRequest::OpenSettings(Some(
                "keymaps".to_owned(),
            )));
        }
        match outcome {
            Outcome::Open => self.keep_open(ctx, cx, dialog),
            Outcome::Ok => self.dialog_ok(cx, dialog),
            Outcome::Cancel => self.dialog_cancel(cx, dialog),
        }
    }

    /// The open dialog's body: one function per kind of dialog, each
    /// answering whether the dialog stays open, is kept or is cancelled.
    /// `room` is the width the body has and, for a body that lays out a
    /// list, the height meant for it.
    fn dialog_body(
        &mut self,
        ui: &mut Ui,
        dialog: &mut Dialog,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
        room: egui::Vec2,
        change_keys: &mut bool,
    ) -> Outcome {
        match dialog {
            Dialog::New(new) => self.new_dialog_ui(ui, new, cx, theme),
            Dialog::AdvancedFill => self.advanced_fill_dialog_ui(ui, theme),
            Dialog::Filter(filter) => self.filter_dialog_ui(ui, filter, cx, theme),
            Dialog::Pressure(pressure) => pressure_ui(ui, pressure, theme),
            Dialog::Pages(preview) => self.binding_ui(ui, preview, cx, theme),
            Dialog::Comps(comps) => self.comps_ui(ui, comps, cx, theme),
            Dialog::Exposure => self.exposure_ui(ui, cx, theme),
            Dialog::Frame(frame) => frame_ui(ui, frame, theme),
            Dialog::Text(_, lettering) => text_ui(ui, lettering, self.main, self.sub, theme),
            Dialog::Onion(onion) => onion_ui(ui, onion, theme),
            Dialog::SelectBy(kind, amount) => select_by_ui(ui, kind, amount, theme),
            Dialog::Workspace(name) => workspace_ui(ui, name, theme),
            Dialog::Workspaces => self.workspaces_ui(ui, theme),
            Dialog::Print(settings, printing) => print_ui(ui, settings, *printing, theme),
            Dialog::Modifiers(chosen) => modifiers_ui(ui, chosen, theme),
            Dialog::CommandBar(bar) => self.command_bar_ui(ui, bar, cx, room, theme),
            Dialog::PageSettings(settings) => page_settings_ui(ui, settings, theme),
            Dialog::StoryInfo(info) => story_info_ui(ui, info, theme),
            Dialog::StoryEditor(texts) => story_editor_ui(ui, texts, room, theme),
            Dialog::Proof(profile) => proof_ui(ui, profile, theme),
            Dialog::CanvasProperties(props) => canvas_properties_ui(ui, props, theme),
            Dialog::Convert(convert) => convert_ui(ui, convert, theme),
            Dialog::LinesTones(lines) => lines_tones_ui(ui, lines, theme),
            Dialog::Grid(_, grid) => grid_ui(ui, grid, theme),
            Dialog::Message(_, paragraphs) => message_ui(ui, paragraphs, room, theme),
            Dialog::Gamut(tolerance, all) => gamut_ui(ui, tolerance, all, theme),
            Dialog::Webtoon(height, width) => webtoon_ui(ui, height, width, theme),
            Dialog::DivideFrame(spec) => crate::comic::divide_frame_ui(ui, spec, theme),
            Dialog::Timeline(fps, length) => timeline_ui(ui, fps, length, theme),
            Dialog::Shape3D(kind) => shape_3d_ui(ui, kind, theme),
            Dialog::Material(name) => material_ui(ui, name, theme),
            Dialog::Timelapse(dialog) => dialog.ui(ui, theme),
            Dialog::Perspective(perspective) => perspective_ui(ui, perspective, theme),
            Dialog::Correction(_, filter, ..) => self.correction_dialog_ui(ui, filter, cx, theme),
            Dialog::Colorize(colorize) => colorize_ui(ui, colorize, theme),
            Dialog::Canvas(canvas) => canvas_ui(ui, canvas, theme),
            Dialog::Resolution(resolution) => resolution_ui(ui, resolution, theme),
            Dialog::Shortcuts => shortcuts_ui(ui, cx, room, change_keys, theme),
            Dialog::DeleteFolders(quiet) => Self::delete_folders_ui(ui, quiet, theme),
            Dialog::SimpleTone(tone) => Self::simple_tone_ui(ui, tone, theme),
            Dialog::Anim(dialog) => dialog.ui(ui, theme),
            Dialog::Color(color) => self.color_dialog_ui(ui, color, theme),
            Dialog::Fonts(manager) => {
                if sue_wm::fonts::manage_ui(ui, manager, theme) {
                    Outcome::Ok
                } else {
                    Outcome::Open
                }
            }
        }
    }

    /// What the extra button at the footer's leading edge does: it
    /// changes the dialog and leaves it open.
    fn dialog_extra(&mut self, cx: &mut AppletContext<'_>, dialog: &mut Dialog) {
        match dialog {
            Dialog::New(new) => self.canvas_preset(cx, new, true),
            Dialog::Pressure(pressure) => pressure.reset(),
            _ => {}
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
            Dialog::Perspective(perspective) => self.perspective_ok(cx, perspective),
            Dialog::Timelapse(dialog) => self.ask_export_timelapse(cx, dialog.export),
            Dialog::Material(name) => self.register_material(cx, &name),
            Dialog::SelectBy(3, width) => self.outline_selection(cx, width),
            Dialog::SelectBy(kind, amount) => self.select_by_ok(kind, amount),
            Dialog::Gamut(tolerance, all) => self.select_gamut(cx, tolerance, all),
            Dialog::Workspace(name) => self.workspace_ok(cx, &name),
            Dialog::Workspaces | Dialog::CommandBar(_) => self.save_extras(cx),
            Dialog::Print(settings, printing) => self.print_ok(cx, settings, printing),
            Dialog::Modifiers(chosen) => self.keep_modifier_map(chosen.map, cx),
            Dialog::PageSettings(settings) => self.page_settings_ok(cx, &settings),
            Dialog::StoryInfo(info) => Self::story_info_ok(cx, &info),
            Dialog::StoryEditor(texts) => Self::story_editor_ok(cx, texts),
            Dialog::Proof(profile) => self.proof_ok(profile),
            Dialog::CanvasProperties(props) => Self::canvas_properties_ok(cx, &props),
            Dialog::Convert(convert) => self.convert_ok(cx, &convert),
            Dialog::LinesTones(lines) => self.lines_tones_ok(cx, &lines),
            Dialog::Grid(picture, grid) => Self::grid_ok(cx, picture, grid),
            Dialog::Webtoon(height, width) => self.ask_export_webtoon(cx, height, width),
            Dialog::DivideFrame(spec) => self.divide_frame_ok(cx, &spec),
            Dialog::Shape3D(kind) => self.shape_3d_ok(cx, kind),
            Dialog::Timeline(fps, length) => self.timeline_ok(cx, fps, length),
            Dialog::Onion(onion) => self.onion_ok(cx, &onion),
            Dialog::Correction(layer, filter, before, made) => {
                Self::correction_ok(cx, layer, &filter, &before, made);
            }
            Dialog::Canvas(canvas) => self.canvas_ok(cx, &canvas),
            Dialog::Resolution(resolution) => self.resolution_ok(cx, &resolution),
            Dialog::DeleteFolders(quiet) => self.delete_folders_ok(cx, quiet),
            Dialog::SimpleTone(tone) => self.simple_tone_ok(cx, &tone),
            Dialog::Anim(dialog) => self.anim_dialog_ok(cx, *dialog),
            Dialog::Color(color) => self.color_dialog_ok(cx, color),
            Dialog::Colorize(colorize) => self.colorize_ok(cx, &colorize),
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
            | Dialog::Convert(_)
            | Dialog::LinesTones(_)
            | Dialog::Grid(..)
            | Dialog::Shortcuts
            | Dialog::Fonts(_)
            | Dialog::DivideFrame(_)
            | Dialog::DeleteFolders(_)
            | Dialog::SimpleTone(_)
            | Dialog::Anim(_)
            | Dialog::Colorize(_) => {}
            Dialog::Color(color) => self.color_dialog_cancel(cx, color),
        }
    }
}

/// Where a body's OK and Cancel stood. The host now draws them in the
/// dialog's footer, in Sue's one order, for every dialog whose
/// [`Dialog::look`] gives it OK and Cancel; a body that still calls this
/// draws nothing more. The call goes when the body's own file is next
/// edited.
fn ok_cancel(_ui: &mut Ui, _outcome: &mut Outcome) {}

/// A row of buttons standing at the right, as tall as a control. A
/// layout that aligns to the bottom would take every point the dialog
/// host offers, and the host makes a dialog as tall as what it holds:
/// such a dialog would grow to the screen's height.
pub(crate) fn button_row<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let height = Density::of(ui).control_height();
    ui.allocate_ui_with_layout(
        vec2(ui.available_width(), height),
        egui::Layout::right_to_left(egui::Align::Center),
        add,
    )
    .inner
}

/// A label as a form row, a tooltip or a field's name shows it: without
/// the colon some of the older labels end with.
fn bare(text: String) -> String {
    let kept = text.trim_end_matches([':', '\u{FF1A}', ' ']);
    if kept.len() == text.len() {
        text
    } else {
        kept.to_owned()
    }
}

/// A muted label before a control.
fn label(ui: &mut Ui, text: &str, theme: &Theme) {
    ui.label(
        egui::RichText::new(text)
            .font(theme.text_font(TextRole::Body))
            .color(theme.text_muted()),
    );
}

/// A line of help under a group of controls, in the caption size.
fn note(ui: &mut Ui, text: &str, theme: &Theme) {
    ui.add(
        egui::Label::new(
            egui::RichText::new(text)
                .font(theme.text_font(TextRole::Caption))
                .color(theme.text_muted()),
        )
        .wrap(),
    );
}

/// Gives `response` a tooltip with `name`, and with `keys` when a key
/// runs it.
fn tip(response: Response, name: &str, keys: &str, theme: &Theme) -> Response {
    tooltip(response, &Tip::new(name).shortcut(keys), theme)
}

/// The height of one row of controls.
fn row_height(ui: &Ui) -> f32 {
    Density::of(ui).control_height()
}

/// A page to read: its paragraphs, headings and links.
pub(super) fn message_ui(
    ui: &mut Ui,
    paragraphs: &[String],
    room: egui::Vec2,
    theme: &Theme,
) -> Outcome {
    egui::ScrollArea::vertical()
        .max_height(room.y)
        .show(ui, |ui| {
            for paragraph in paragraphs {
                if paragraph.starts_with("https://") {
                    ui.hyperlink(paragraph.as_str());
                } else if let Some(heading) = paragraph.strip_prefix("# ") {
                    ui.add_space(theme.space(Space::S3));
                    ui.label(
                        egui::RichText::new(heading)
                            .font(theme.text_font(TextRole::Body))
                            .color(theme.text)
                            .strong(),
                    );
                } else {
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(paragraph.as_str())
                                .font(theme.text_font(TextRole::Body))
                                .color(theme.text),
                        )
                        .wrap(),
                    );
                }
            }
        });
    Outcome::Open
}
