//! Sue Paint wears the theme preset Sue wears: every surface (menus, the
//! command bar, the dock and palette frames, every palette, dialogs, the
//! canvas surround, the timeline and the status) changes with it, and its
//! text and icons stay readable in each preset.

use super::*;
use egui::{Color32, Rect, Shape};
use sue_core::ReversibleTransaction;
use sue_wm::ui::tokens::contrast;

/// Where an opaque fill shows.
enum Area {
    Rect(Rect),
    Disc(Pos2, f32, Rect),
}

impl Area {
    fn contains(&self, at: Pos2) -> bool {
        match self {
            Self::Rect(rect) => rect.contains(at),
            Self::Disc(center, radius, clip) => clip.contains(at) && center.distance(at) <= *radius,
        }
    }

    /// Small enough to be a color chip: a swatch of the color set, the
    /// main and sub color, the paper color of the New dialog.
    fn chip_sized(&self) -> bool {
        let size = match self {
            Self::Rect(rect) => rect.size(),
            Self::Disc(_, radius, _) => vec2(*radius, *radius) * 2.0,
        };
        size.x <= CHIP && size.y <= CHIP
    }
}

/// The largest side of a color chip.
const CHIP: f32 = 64.0;

/// The colors of the picture's own black and white ink (the main and sub
/// color, and the color set's swatches), which are the same in every
/// preset.
const INK: [Color32; 2] = [Color32::BLACK, Color32::WHITE];

/// What one frame paints, in paint order.
#[derive(Default)]
struct Painted {
    /// Opaque rectangles and discs, and their fill.
    fills: Vec<(usize, Area, Color32)>,
    /// Text: what it says, where, and its color.
    texts: Vec<(usize, String, Rect, Color32)>,
    /// Icons (small textured meshes): where, and their tint.
    icons: Vec<(usize, Rect, Color32)>,
    /// Pictures (textured meshes larger than an icon): document content.
    pictures: Vec<(usize, Rect)>,
}

/// The largest side of an icon; a larger picture is document content.
const ICON: f32 = 64.0;

impl Painted {
    fn of(output: &egui::FullOutput) -> Self {
        let mut painted = Self::default();
        let mut order = 0;
        for clipped in &output.shapes {
            let clip = clipped.clip_rect;
            let mut look = |shape: &Shape| {
                order += 1;
                painted.take(order, clip, shape);
            };
            match &clipped.shape {
                Shape::Vec(shapes) => shapes.iter().for_each(&mut look),
                shape => look(shape),
            }
        }
        painted
    }

    fn take(&mut self, order: usize, clip: Rect, shape: &Shape) {
        match shape {
            Shape::Rect(rect) if rect.fill.a() == 255 && rect.brush.is_none() => {
                let shown = rect.rect.intersect(clip);
                if shown.is_positive() {
                    self.fills.push((order, Area::Rect(shown), rect.fill));
                }
            }
            Shape::Circle(circle) if circle.fill.a() == 255 => {
                self.fills.push((
                    order,
                    Area::Disc(circle.center, circle.radius, clip),
                    circle.fill,
                ));
            }
            Shape::Text(text) => {
                let words = text.galley.text().trim().to_owned();
                let color = text.override_text_color.unwrap_or_else(|| {
                    let color = text
                        .galley
                        .job
                        .sections
                        .first()
                        .map_or(text.fallback_color, |section| section.format.color);
                    if color == Color32::PLACEHOLDER {
                        text.fallback_color
                    } else {
                        color
                    }
                });
                let rect = Rect::from_min_size(text.pos, text.galley.size());
                if !words.is_empty() && clip.contains(rect.center()) && color.a() == 255 {
                    self.texts.push((order, words, rect, color));
                }
            }
            Shape::Mesh(mesh) if mesh.texture_id != egui::TextureId::default() => {
                let bounds = mesh.calc_bounds();
                let shown = bounds.intersect(clip);
                if !shown.is_positive() {
                    return;
                }
                if bounds.width() > ICON || bounds.height() > ICON {
                    self.pictures.push((order, shown));
                } else if let Some(vertex) = mesh.vertices.first()
                    && vertex.color.a() == 255
                    && clip.contains(bounds.center())
                {
                    self.icons.push((order, bounds, vertex.color));
                }
            }
            _ => {}
        }
    }

    /// The opaque color under `at` before paint step `before`, or `None`
    /// where a picture lies above every fill.
    fn under(&self, at: Pos2, before: usize) -> Option<Color32> {
        self.fill_under(at, before).map(|(_, color)| color)
    }

    /// The fill under `at` before paint step `before`, as [`Self::under`]
    /// finds it, with where it shows.
    fn fill_under(&self, at: Pos2, before: usize) -> Option<(&Area, Color32)> {
        let fill = self
            .fills
            .iter()
            .rev()
            .find(|(order, area, _)| *order < before && area.contains(at))?;
        let covered = self
            .pictures
            .iter()
            .any(|(order, rect)| *order > fill.0 && *order < before && rect.contains(at));
        (!covered).then_some((&fill.1, fill.2))
    }
}

/// A theme preset Sue ships.
fn preset(id: &str) -> sue_rice::ThemePreset {
    sue_rice::builtin_presets()
        .into_iter()
        .find(|preset| preset.id == id)
        .unwrap_or_else(|| panic!("Sue has the preset {id}"))
}

/// The colors a preset fills surfaces with (never text or ink).
fn surface_colors(id: &str) -> Vec<Color32> {
    let shipped = sue_rice::RiceSchema::parse(sue_rice::DEFAULT_RICE).unwrap();
    let theme = sue_wm::Theme::from_rice(&shipped).with_preset(&preset(id));
    vec![
        theme.void,
        theme.panel,
        theme.pill,
        theme.hover,
        theme.active,
        theme.border,
        theme.selection,
        theme.grid,
    ]
}

/// What a person can have on screen in Sue Paint.
#[derive(Clone, Copy, Debug)]
enum Scene {
    /// A canvas, the palettes, the command bar, a timeline, the status.
    Canvas,
    /// The File menu open over it.
    Menu,
    /// The New dialog open over it.
    Dialog,
    /// The pencil chosen: Tool Property and Brush Size with a brush's
    /// controls, its sub tools in Sub Tool.
    Pencil,
    /// The Movie Export dialog open over it.
    Export,
}

/// Which palettes Sue Paint shows.
#[derive(Clone, Copy, Debug)]
enum Layout {
    /// The layout a test runtime keeps: Sub Tool, the color set and
    /// history, the layer list, the timeline.
    Kept,
    /// The layout style's own, as a first start shows it: Tool Property,
    /// Brush Size, the color wheel, Navigator and Layer Property.
    Fresh,
}

/// Every layout with the scenes looked at in it.
const LAYOUTS: [(Layout, &[Scene]); 2] = [
    (
        Layout::Kept,
        &[Scene::Canvas, Scene::Menu, Scene::Dialog, Scene::Export],
    ),
    (Layout::Fresh, &[Scene::Canvas, Scene::Pencil]),
];

/// Sue Paint in `layout` with a canvas and a timeline.
fn sue_paint(layout: Layout) -> PaintDriver {
    let mut paint = match layout {
        Layout::Kept => PaintDriver::new("paint-theme"),
        Layout::Fresh => PaintDriver::fresh("paint-theme-fresh"),
    };
    paint.new_timeline();
    paint.settle(10);
    paint
}

/// One frame of `scene` in Sue Paint while Sue wears the preset `id`,
/// and the theme Sue Paint wears then.
fn wearing(paint: &mut PaintDriver, id: &str, scene: Scene) -> (Painted, sue_wm::Theme) {
    let rice = &mut paint.runtime.engine.world.rice;
    preset(id).transaction(rice).apply(rice).unwrap();
    let escape = || key(egui::Key::Escape, egui::Modifiers::NONE);
    paint.step(vec![escape()]);
    paint.settle(20);
    match scene {
        Scene::Canvas => {}
        Scene::Menu => {
            paint.click_text("File");
        }
        Scene::Dialog => {
            paint.step(vec![key(egui::Key::N, egui::Modifiers::COMMAND)]);
        }
        Scene::Pencil => paint.command("tool.pick.pencil"),
        Scene::Export => paint.command("file.export.movie"),
    }
    paint.settle(40);
    let raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
        time: Some(paint.ctx.input(|input| input.time) + 0.016),
        ..egui::RawInput::default()
    };
    paint.now += Duration::from_millis(16);
    let runtime = &mut paint.runtime;
    let now = paint.now;
    let output = paint
        .ctx
        .run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now));
    let painted = Painted::of(&output);
    output.drop_without_applying_deltas();
    paint.step(vec![escape()]);
    if matches!(scene, Scene::Pencil) {
        paint.command("tool.operation");
    }
    paint.settle(20);
    let rice = &paint.runtime.engine.world.rice;
    let theme =
        sue_wm::Theme::from_rice(rice).for_studio(rice, "sue_paint", &sue_rice::builtin_presets());
    (painted, theme)
}

/// The points of a 16 point grid over the 1440 by 900 window.
fn grid() -> impl Iterator<Item = Pos2> {
    (0..56u8).flat_map(|row| {
        (0..90u8).map(move |column| {
            egui::pos2(8.0 + 16.0 * f32::from(column), 8.0 + 16.0 * f32::from(row))
        })
    })
}

/// The places where `a` and `b` fill Sue Paint's surfaces with the same
/// color. The document is the same in every preset, so places under a
/// picture do not count, and neither do color chips (the color set's
/// swatches, the main and sub color, the New dialog's paper color),
/// unless a chip wears a surface color of `presets` other than black or
/// white ink. Any larger fill counts whatever its color, so a literal
/// color in place of the theme's (a gray of an old palette table) fails
/// even when no preset uses it.
fn same_surfaces(a: &Painted, b: &Painted, presets: &[&str]) -> Vec<String> {
    let chrome: Vec<Color32> = presets.iter().flat_map(|id| surface_colors(id)).collect();
    grid()
        .filter_map(
            |at| match (a.fill_under(at, usize::MAX), b.fill_under(at, usize::MAX)) {
                (Some((area, one)), Some((_, other)))
                    if one == other
                        && (!area.chip_sized()
                            || (chrome.contains(&one) && !INK.contains(&one))) =>
                {
                    Some(format!("{at:?} {one:?}"))
                }
                _ => None,
            },
        )
        .collect()
}

/// The text with less than 4.5 to 1 against what lies under it (WCAG AA;
/// disabled text, which WCAG leaves out, needs the 3 to 1 of
/// `text_disabled`), and the icons with less than 3 to 1 (WCAG's non-text
/// contrast).
fn hard_to_read(painted: &Painted, theme: &sue_wm::Theme) -> Vec<String> {
    let texts = painted
        .texts
        .iter()
        .filter_map(|(order, words, rect, color)| {
            let under = painted.under(rect.center(), *order)?;
            let seen = contrast(*color, under);
            let disabled = *color == theme.text_disabled(under) && seen >= 3.0;
            (seen < 4.5 && !disabled)
                .then(|| format!("text {words:?} at {rect:?}: {color:?} on {under:?}, {seen:.2}"))
        });
    let icons = painted.icons.iter().filter_map(|(order, rect, color)| {
        let under = painted.under(rect.center(), *order)?;
        let seen = contrast(*color, under);
        (seen < 3.0).then(|| format!("icon at {rect:?}: {color:?} on {under:?}, {seen:.2}"))
    });
    texts.chain(icons).collect()
}

/// Every surface of Sue Paint changes between Void and Paper: none of
/// them keeps a color of its own (a palette table, a literal, the Gray it
/// used to ship in) in place of the theme's.
#[test]
fn no_surface_of_sue_paint_looks_the_same_in_void_and_paper() {
    for (layout, scenes) in LAYOUTS {
        let mut paint = sue_paint(layout);
        for &scene in scenes {
            let (void, _) = wearing(&mut paint, "void", scene);
            let (paper, _) = wearing(&mut paint, "paper", scene);
            showing(&void, layout, scene);
            let same = same_surfaces(&void, &paper, &["void", "paper", "gray"]);
            assert!(
                same.is_empty(),
                "{layout:?} {scene:?}: {} places are the same in Void and Paper: {same:#?}",
                same.len()
            );
            assert!(void.fills.len() > 100, "{layout:?} {scene:?}");
        }
        // The check sees a studio that wears its own colors: Sue Paint
        // set to Gray looks the same in both.
        paint
            .runtime
            .engine
            .world
            .rice
            .set(
                "sue_paint.theme",
                sue_rice::RiceValue::Text("gray".to_owned()),
            )
            .unwrap();
        let (void, _) = wearing(&mut paint, "void", Scene::Canvas);
        let (paper, _) = wearing(&mut paint, "paper", Scene::Canvas);
        let same = same_surfaces(&void, &paper, &["void", "paper", "gray"]);
        assert!(same.len() > 1000, "{layout:?}: {}", same.len());
    }
}

/// Fails unless `painted` shows what `scene` in `layout` is there to
/// show, so no check passes on a scene that did not open.
fn showing(painted: &Painted, layout: Layout, scene: Scene) {
    let words: &[&str] = match (layout, scene) {
        (Layout::Kept, Scene::Canvas) => &["Sub Tool", "Color Set", "Layer", "Timeline 1"],
        (Layout::Fresh, Scene::Canvas) => &["Tool Property", "Brush Size", "Layer Property"],
        (_, Scene::Menu) => &["Save As...", "Quit Sue"],
        (_, Scene::Dialog) => &["Resolution", "Paper color", "OK"],
        (_, Scene::Pencil) => &["Mechanical pencil", "Tool Property", "Brush Size"],
        (_, Scene::Export) => &["Movie Export Settings", "Frame rate", "OK"],
    };
    for word in words {
        assert!(
            painted.texts.iter().any(|(_, text, _, _)| text == word),
            "{layout:?} {scene:?} shows {word:?}"
        );
    }
}

/// In every preset Sue ships, Sue Paint's text reads at 4.5 to 1 and its
/// icons at 3 to 1 against what they lie on.
#[test]
fn sue_paints_text_and_icons_read_in_every_preset() {
    let mut found = Vec::new();
    for (layout, scenes) in LAYOUTS {
        let mut paint = sue_paint(layout);
        for preset in sue_rice::builtin_presets() {
            for &scene in scenes {
                let (painted, theme) = wearing(&mut paint, &preset.id, scene);
                showing(&painted, layout, scene);
                assert!(painted.icons.len() > 20, "{scene:?} paints its icons");
                found.extend(
                    hard_to_read(&painted, &theme)
                        .into_iter()
                        .map(|line| format!("{} {layout:?} {scene:?}: {line}", preset.id)),
                );
            }
        }
    }
    assert!(found.is_empty(), "{found:#?}");
}

/// Sue Paint ships wearing Sue's theme, and a settings file of 0.0.75
/// that kept its old default (Gray) takes the new one when it is read.
#[test]
fn sue_paint_follows_sues_theme_from_the_start() {
    let shipped = sue_rice::RiceSchema::parse(sue_rice::DEFAULT_RICE).unwrap();
    assert_eq!(
        shipped.get_str("sue_paint.theme"),
        Some(sue_wm::STUDIO_THEME_SUE)
    );
    let old = include_str!("../../../../../crates/sue-rice/rice/history/v6.suerice");
    assert!(old.contains("\ntheme = \"gray\""), "0.0.75 shipped Gray");
    let migrated = sue_rice::migrate(old).unwrap().unwrap();
    let schema = sue_rice::RiceSchema::parse(&migrated.text).unwrap();
    assert_eq!(
        schema.get_str("sue_paint.theme"),
        Some(sue_wm::STUDIO_THEME_SUE)
    );
    // A preset picked on purpose stays.
    let picked = old.replacen(
        "theme = \"gray\"                 #",
        "theme = \"paper\"                #",
        1,
    );
    assert_ne!(picked, old);
    let migrated = sue_rice::migrate(&picked).unwrap().unwrap();
    let schema = sue_rice::RiceSchema::parse(&migrated.text).unwrap();
    assert_eq!(schema.get_str("sue_paint.theme"), Some("paper"));
}
