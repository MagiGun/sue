//! The gallery: every part of Sue's design system on one page, live, in
//! the theme, the scale and the density it is drawn at. It is where a
//! token or a component is looked at before a studio uses it.
//!
//! It is hidden: no run of Sue lists it, except one started with
//! `sue --gallery`, which shows it under Tools in the Hub. The snapshot
//! scenes 29 (`snapshot/scenes/gallery.rs`) draw it in every preset, at
//! 1.5 and 2.0 pixels per point and in each density, with the focus ring
//! showing.
//!
//! The words on the page are the names of tokens and components as the
//! code spells them, and numbers: nothing here is interface text, as on
//! the icon sheets of `sue_wm::icons::sheet`.
//!
//! It grows with the component library: a component of `sue_wm::ui` gets
//! a [`Section`] in [`COLUMNS`] when it lands.

use std::sync::atomic::{AtomicBool, Ordering};

use egui::{Color32, Frame, Grid, RichText, Sense, Ui, vec2};
use sue_wm::icons::{self, IconStyle, REFERENCE};
use sue_wm::ui::{Density, Elevation, Radius, Space, TextRole, contrast, focus_ring};
use sue_wm::{AppletContext, AppletManifest, AppletRole, Launchpad, SueApplet, Theme, WmError};

/// The gallery's applet id.
pub const ID: &str = "sue-gallery";

/// Whether this run lists the gallery (`sue --gallery`).
static LISTED: AtomicBool = AtomicBool::new(false);

/// Lists the gallery in this run: under Tools in the Hub, and in the
/// command palette. Called before Sue boots.
pub fn list_in_this_run() {
    LISTED.store(true, Ordering::Relaxed);
}

/// Installs the gallery into `launchpad` when this run lists it.
///
/// # Errors
/// If an applet with the gallery's id is installed already.
pub fn install(launchpad: &mut Launchpad) -> Result<(), WmError> {
    if LISTED.load(Ordering::Relaxed) {
        launchpad.register(|| Box::new(Gallery::default()))?;
    }
    Ok(())
}

/// The state of the live controls on the page, so they can be used.
#[derive(Clone, Debug, PartialEq)]
pub struct Demo {
    check: bool,
    radio: usize,
    selected: usize,
    slider: f32,
    value: f32,
    text: String,
    combo: usize,
    segment: usize,
}

impl Default for Demo {
    fn default() -> Self {
        Self {
            check: true,
            radio: 0,
            selected: 0,
            slider: 0.6,
            value: 42.0,
            text: "Text".to_owned(),
            combo: 0,
            segment: 1,
        }
    }
}

/// One card of the page.
pub struct Section {
    /// Its title: the name of what it shows, as the code spells it.
    pub name: &'static str,
    /// Draws its content.
    pub show: fn(&mut Ui, &Theme, Density, &mut Demo),
}

/// The page's cards, column by column: the tokens first, then the
/// controls. A component of `sue_wm::ui` joins the third column.
pub const COLUMNS: [&[Section]; 3] = [
    &[
        Section {
            name: "color roles",
            show: colors,
        },
        Section {
            name: "TextRole",
            show: type_scale,
        },
    ],
    &[
        Section {
            name: "Radius",
            show: radii,
        },
        Section {
            name: "Space",
            show: spacing,
        },
        Section {
            name: "Elevation",
            show: elevation,
        },
        Section {
            name: "Density",
            show: density_sizes,
        },
        Section {
            name: "sue_wm::icons",
            show: icon_row,
        },
    ],
    &[
        Section {
            name: "egui controls",
            show: controls,
        },
        Section {
            name: "sue_wm::widgets",
            show: widgets,
        },
    ],
];

/// Draws the whole page at `density`.
pub fn show(ui: &mut Ui, theme: &Theme, density: Density, demo: &mut Demo) {
    density.scope(ui, |ui| {
        ui.spacing_mut().interact_size.y = density.control_height();
        ui.spacing_mut().item_spacing = vec2(theme.space(Space::S4), theme.space(Space::S3));
        ui.label(
            RichText::new("Gallery")
                .font(theme.text_font(TextRole::Display))
                .color(theme.text),
        );
        ui.label(
            RichText::new(format!(
                "Density::{density:?}, {} px per point",
                ui.ctx().pixels_per_point()
            ))
            .font(theme.text_font(TextRole::Caption))
            .color(theme.text_muted()),
        );
        ui.add_space(theme.space(Space::S4));
        ui.columns(COLUMNS.len(), |columns| {
            for (ui, sections) in columns.iter_mut().zip(COLUMNS) {
                for section in sections {
                    card(ui, theme, density, section, demo);
                    ui.add_space(theme.space(Space::S5));
                }
            }
        });
    });
}

/// One section on a card.
fn card(ui: &mut Ui, theme: &Theme, density: Density, section: &Section, demo: &mut Demo) {
    Frame::new()
        .fill(theme.surface(1))
        .stroke(egui::Stroke::new(theme.border_width, theme.border))
        .corner_radius(theme.corner(Radius::Lg))
        .inner_margin(egui::Margin::same(
            theme.card_padding().clamp(0.0, 127.0) as i8
        ))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new(section.name)
                    .font(theme.text_font(TextRole::Title))
                    .color(theme.text),
            );
            ui.add_space(theme.space(Space::S3));
            (section.show)(ui, theme, density, demo);
        });
}

/// A small muted name beside what it names.
fn name(ui: &mut Ui, theme: &Theme, text: &str) {
    ui.label(
        RichText::new(text)
            .font(theme.text_font(TextRole::Caption))
            .color(theme.text_muted()),
    );
}

/// `#rrggbb`.
fn hex(color: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", color.r(), color.g(), color.b())
}

// ----- Tokens ----------------------------------------------------------------

/// Every color role, with its contrast on the card it stands on.
fn colors(ui: &mut Ui, theme: &Theme, density: Density, _demo: &mut Demo) {
    let ground = theme.surface(1);
    let roles = [
        ("void", theme.void),
        ("panel", theme.panel),
        ("surface(2)", theme.surface(2)),
        ("pill", theme.pill),
        ("fill_hover", theme.fill_hover()),
        ("fill_pressed", theme.fill_pressed()),
        ("fill_selected", theme.fill_selected()),
        ("border", theme.border),
        ("border_strong", theme.border_strong()),
        ("focus", theme.focus()),
        ("text", theme.text),
        ("text_muted", theme.text_muted()),
        ("text_disabled", theme.text_disabled(ground)),
        ("selection", theme.selection),
        ("danger", theme.danger()),
        ("warning", theme.warning()),
        ("success", theme.success()),
        ("info", theme.info()),
        ("link", theme.link()),
    ];
    let swatch = vec2(density.icon_button(), theme.text_size(TextRole::Body));
    Grid::new("gallery-colors")
        .num_columns(4)
        .min_row_height(swatch.y)
        .spacing(vec2(theme.space(Space::S4), theme.space(Space::S3)))
        .show(ui, |ui| {
            for (role, color) in roles {
                let (rect, _) = ui.allocate_exact_size(swatch, Sense::hover());
                ui.painter()
                    .rect_filled(rect, theme.radius(Radius::Xs), color);
                ui.painter().rect_stroke(
                    rect,
                    theme.radius(Radius::Xs),
                    egui::Stroke::new(theme.border_width, theme.border),
                    egui::StrokeKind::Inside,
                );
                ui.label(RichText::new(role).color(theme.text));
                name(ui, theme, &hex(color));
                name(ui, theme, &format!("{:.1}:1", contrast(color, ground)));
                ui.end_row();
            }
        });
}

/// The type scale, each step in its own size.
fn type_scale(ui: &mut Ui, theme: &Theme, _density: Density, _demo: &mut Demo) {
    for role in TextRole::ALL {
        ui.label(
            RichText::new(format!("{role:?} {}", theme.text_size(role)))
                .font(theme.text_font(role))
                .color(theme.text),
        );
    }
}

/// The corner radius steps, a square each.
fn radii(ui: &mut Ui, theme: &Theme, density: Density, _demo: &mut Demo) {
    let side = density.icon_button();
    ui.horizontal_wrapped(|ui| {
        for step in [
            Radius::Xs,
            Radius::Sm,
            Radius::Md,
            Radius::Lg,
            Radius::Xl,
            Radius::Full,
        ] {
            ui.vertical(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
                ui.painter()
                    .rect_filled(rect, theme.radius(step), theme.surface(2));
                ui.painter().rect_stroke(
                    rect,
                    theme.radius(step),
                    egui::Stroke::new(theme.border_width, theme.border_strong()),
                    egui::StrokeKind::Inside,
                );
                name(ui, theme, &format!("{step:?}"));
            });
        }
    });
}

/// The spacing scale, a bar each.
fn spacing(ui: &mut Ui, theme: &Theme, _density: Density, _demo: &mut Demo) {
    Grid::new("gallery-space")
        .num_columns(2)
        .min_row_height(theme.text_size(TextRole::Caption))
        .spacing(vec2(theme.space(Space::S4), theme.space(Space::S1)))
        .show(ui, |ui| {
            for step in Space::ALL {
                name(ui, theme, &format!("{step:?} {}", theme.space(step)));
                let (rect, _) = ui.allocate_exact_size(
                    vec2(theme.space(step), theme.space(Space::S3)),
                    Sense::hover(),
                );
                ui.painter().rect_filled(rect, 0.0, theme.text_muted());
                ui.end_row();
            }
        });
}

/// The elevation steps, a floating card each.
fn elevation(ui: &mut Ui, theme: &Theme, density: Density, _demo: &mut Demo) {
    let side = density.icon_button() * 1.5;
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = theme.space(Space::S6);
        for level in Elevation::ALL {
            ui.vertical(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
                let corner = theme.corner(Radius::Lg);
                ui.painter()
                    .add(theme.elevation(level).as_shape(rect, corner));
                ui.painter().rect_filled(rect, corner, theme.surface(2));
                name(ui, theme, &format!("{level:?}"));
            });
        }
    });
}

/// The sizes the density gives controls, a square each.
fn density_sizes(ui: &mut Ui, theme: &Theme, density: Density, _demo: &mut Demo) {
    let sizes = [
        ("control_height", density.control_height()),
        ("icon_button", density.icon_button()),
        ("icon", density.icon()),
        ("row", density.row()),
        ("tab_strip", density.tab_strip()),
        ("min_hit", density.min_hit()),
    ];
    // Every square stands on one line, so their names line up too.
    let tallest = sizes.iter().map(|(_, side)| *side).fold(0.0, f32::max);
    ui.horizontal_wrapped(|ui| {
        for (size_name, side) in sizes {
            ui.vertical(|ui| {
                let (cell, _) = ui.allocate_exact_size(vec2(side, tallest), Sense::hover());
                let rect = egui::Rect::from_min_max(
                    egui::pos2(cell.left(), cell.bottom() - side),
                    cell.max,
                );
                ui.painter().rect_stroke(
                    rect,
                    theme.radius(Radius::Xs),
                    egui::Stroke::new(theme.border_width, theme.border_strong()),
                    egui::StrokeKind::Inside,
                );
                name(ui, theme, &format!("{size_name} {side}"));
            });
        }
    });
}

// ----- Controls --------------------------------------------------------------

/// egui's own controls as the theme dresses them, each with the focus
/// ring of `sue_wm::ui::focus`.
fn controls(ui: &mut Ui, theme: &Theme, _density: Density, demo: &mut Demo) {
    let ring = theme.radius(Radius::Md);
    Grid::new("gallery-controls")
        .num_columns(2)
        .spacing(vec2(theme.space(Space::S5), theme.space(Space::S3)))
        .show(ui, |ui| {
            name(ui, theme, "Button");
            ui.horizontal(|ui| {
                let response = ui.button("Button");
                focus_ring(ui, &response, ring, theme);
                ui.add_enabled(false, egui::Button::new("Disabled"));
            });
            ui.end_row();

            name(ui, theme, "selectable_value");
            ui.horizontal(|ui| {
                for (index, label) in ["One", "Two", "Three"].into_iter().enumerate() {
                    let response = ui.selectable_value(&mut demo.selected, index, label);
                    focus_ring(ui, &response, ring, theme);
                }
            });
            ui.end_row();

            name(ui, theme, "checkbox");
            let response = ui.checkbox(&mut demo.check, "Checkbox");
            focus_ring(ui, &response, ring, theme);
            ui.end_row();

            name(ui, theme, "radio_value");
            ui.horizontal(|ui| {
                for (index, label) in ["Radio", "Other"].into_iter().enumerate() {
                    let response = ui.radio_value(&mut demo.radio, index, label);
                    focus_ring(ui, &response, ring, theme);
                }
            });
            ui.end_row();

            name(ui, theme, "Slider");
            let response = ui.add(egui::Slider::new(&mut demo.slider, 0.0..=1.0));
            focus_ring(ui, &response, ring, theme);
            ui.end_row();

            name(ui, theme, "DragValue");
            let response = ui.add(egui::DragValue::new(&mut demo.value));
            focus_ring(ui, &response, ring, theme);
            ui.end_row();

            name(ui, theme, "TextEdit");
            let response = ui.text_edit_singleline(&mut demo.text);
            focus_ring(ui, &response, ring, theme);
            ui.end_row();

            name(ui, theme, "ComboBox");
            let options = ["First", "Second", "Third"];
            let response = egui::ComboBox::from_id_salt("gallery-combo")
                .selected_text(options[demo.combo])
                .show_ui(ui, |ui| {
                    for (index, option) in options.into_iter().enumerate() {
                        ui.selectable_value(&mut demo.combo, index, option);
                    }
                })
                .response;
            focus_ring(ui, &response, ring, theme);
            ui.end_row();

            name(ui, theme, "ProgressBar");
            ui.add(egui::ProgressBar::new(demo.slider));
            ui.end_row();

            name(ui, theme, "link");
            let response = ui.link("Link");
            focus_ring(ui, &response, ring, theme);
            ui.end_row();
        });
}

/// The shared widgets of `sue_wm::widgets` that exist before the
/// component library.
fn widgets(ui: &mut Ui, theme: &Theme, _density: Density, demo: &mut Demo) {
    let ring = theme.radius(Radius::Md);
    ui.horizontal(|ui| {
        name(ui, theme, "pill_button");
        let response = sue_wm::widgets::pill_button(ui, "Chosen", true, theme);
        focus_ring(ui, &response, ring, theme);
        let response = sue_wm::widgets::pill_button(ui, "Pill", false, theme);
        focus_ring(ui, &response, ring, theme);
    });
    ui.horizontal(|ui| {
        name(ui, theme, "segmented");
        let options = ["Left", "Middle", "Right"].map(str::to_owned);
        if let Some(picked) = sue_wm::widgets::segmented(ui, &options, Some(demo.segment), theme) {
            demo.segment = picked;
        }
    });
    sue_wm::widgets::heading(ui, "heading", theme);
}

/// The reference icons at the density's icon size, line and solid.
fn icon_row(ui: &mut Ui, theme: &Theme, density: Density, _demo: &mut Demo) {
    let side = density.icon();
    for style in IconStyle::ALL {
        ui.horizontal_wrapped(|ui| {
            for icon in REFERENCE {
                let (rect, _) = ui.allocate_exact_size(vec2(side, side), Sense::hover());
                icons::paint_styled(ui.painter(), rect, icon, style, theme.text);
            }
            name(ui, theme, &format!("{style:?} {side}"));
        });
    }
}

// ----- The applet ------------------------------------------------------------

/// The gallery as a tool: the page, with a switch of density over it.
#[derive(Default)]
pub struct Gallery {
    density: Density,
    demo: Demo,
}

impl SueApplet for Gallery {
    fn manifest(&self) -> AppletManifest {
        let mut manifest = AppletManifest::new(ID, "Gallery", AppletRole::Tool, "G");
        manifest.summary = "Sue's design system on one page.".to_owned();
        manifest.order = i32::MAX;
        manifest
    }

    fn page_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        let theme = cx.theme.clone();
        Frame::new()
            .fill(theme.void)
            .inner_margin(egui::Margin::same(
                theme.dialog_margin().clamp(0.0, 127.0) as i8
            ))
            .show(ui, |ui| {
                let options = Density::ALL.map(|density| format!("{density:?}"));
                let chosen = Density::ALL.iter().position(|d| *d == self.density);
                if let Some(picked) = sue_wm::widgets::segmented(ui, &options, chosen, &theme) {
                    self.density = Density::ALL[picked];
                }
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| show(ui, &theme, self.density, &mut self.demo));
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Draws one frame of the page in `ctx` with `events`, and returns
    /// how many shapes it painted.
    fn draw(
        ctx: &egui::Context,
        theme: &Theme,
        density: Density,
        demo: &mut Demo,
        events: Vec<egui::Event>,
    ) -> usize {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(1440.0, 900.0),
            )),
            events,
            ..egui::RawInput::default()
        };
        let output = ctx.run_ui(raw, |ui| show(ui, theme, density, demo));
        let shapes = output.shapes.len();
        output.drop_without_applying_deltas();
        shapes
    }

    /// Sue's theme as it ships.
    fn shipped() -> Theme {
        let mut rice = sue_rice::RiceSchema::parse(sue_rice::DEFAULT_RICE).unwrap();
        sue_wm::register_rice(&mut rice).unwrap();
        Theme::from_rice(&rice)
    }

    fn tab() -> egui::Event {
        egui::Event::Key {
            key: egui::Key::Tab,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn the_gallery_is_listed_only_in_a_gallery_run() {
        let launchpad = crate::applets::launchpad().unwrap();
        assert!(
            launchpad
                .manifests()
                .all(|manifest| manifest.id.as_str() != ID),
            "a plain run does not list the gallery"
        );
        let mut launchpad = Launchpad::new();
        launchpad.register(|| Box::new(Gallery::default())).unwrap();
        let manifest = launchpad.manifests().next().unwrap();
        assert_eq!(manifest.role, AppletRole::Tool);
        assert!(
            manifest.icon.chars().all(|c| c.is_ascii_alphanumeric()),
            "the gallery uses no font character as an icon"
        );
    }

    #[test]
    fn the_page_draws_in_every_preset_and_density() {
        let base = shipped();
        for preset in sue_rice::builtin_presets() {
            let theme = base.with_preset(&preset);
            for density in Density::ALL {
                let ctx = egui::Context::default();
                theme.apply(&ctx, None, None);
                let mut demo = Demo::default();
                let shapes = draw(&ctx, &theme, density, &mut demo, Vec::new());
                assert!(shapes > 0, "{} {density:?}", preset.id);
                assert_eq!(demo, Demo::default(), "drawing changes nothing");
            }
        }
    }

    #[test]
    fn tab_puts_a_visible_focus_ring_on_the_first_control() {
        let theme = shipped();
        let ctx = egui::Context::default();
        theme.apply(&ctx, None, None);
        let mut demo = Demo::default();
        draw(&ctx, &theme, Density::Regular, &mut demo, Vec::new());
        draw(&ctx, &theme, Density::Regular, &mut demo, vec![tab()]);
        draw(&ctx, &theme, Density::Regular, &mut demo, Vec::new());
        let focused = ctx.memory(|memory| memory.focused());
        assert!(focused.is_some(), "Tab gives a control focus");
        assert!(sue_wm::ui::focus_visible(&ctx), "and the focus shows");
    }

    #[test]
    fn denser_pages_are_shorter() {
        let theme = shipped();
        let height = |density: Density| {
            let ctx = egui::Context::default();
            theme.apply(&ctx, None, None);
            let mut demo = Demo::default();
            let mut used = 0.0;
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    vec2(1440.0, 900.0),
                )),
                ..egui::RawInput::default()
            };
            ctx.run_ui(raw, |ui| {
                show(ui, &theme, density, &mut demo);
                used = ui.min_rect().height();
            })
            .drop_without_applying_deltas();
            used
        };
        let [compact, regular, touch] = Density::ALL.map(height);
        assert!(
            compact < regular && regular < touch,
            "{compact} {regular} {touch}"
        );
    }
}
