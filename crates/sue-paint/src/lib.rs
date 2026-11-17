//! # sue-paint: Sue Paint, the painting and illustration studio
//!
//! Sue Paint fills Sue's page with a painting workspace: its menus in Sue's
//! top bar after the Sue button, a command bar, the tool palette with sub tools and their properties, a
//! color wheel, color sets, brush size sliders, tabs of open canvases, a
//! navigator, a layer palette and a timeline. Menu items whose feature is
//! not built yet are grayed out, so the menus show where it is heading.
//!
//! It follows the backbone ([`sue_core`], [`sue_model`]):
//!
//! - A picture is the shared layered-picture shape of
//!   [`sue_model::picture`]: an entity in the arena with its layers as
//!   ordered children, so it saves inside the `.sue` project with
//!   everything else and other features can read it.
//! - Every change goes through the global undo history: brush strokes,
//!   fills and filters as [`sue_core::txn::RasterEdit`]s holding only the
//!   tiles they touched, layer changes as the backbone's generic edits.
//! - Layers blend with the shared [`sue_model::BlendMode`]s, so a picture
//!   looks the same wherever Sue shows it.
//! - Opening and exporting go through the format registry of `sue-io`.
//!
//! The brush engine ([`brush`]), compositing ([`composite`]), selections
//! ([`select`]), fills ([`fill`]) and filters ([`filters`]) are plain
//! functions over pixels with their own tests, in the picture engine
//! ([`sue_picture`], re-exported here); the studio ([`SuePaint`]) ties
//! them to the interface.

/// Text from the language database's `paint` namespace (Sue Paint).
macro_rules! tr {
    ($($arg:tt)*) => { sue_lang::tr!("paint", $($arg)*) };
}

/// `text`, kept in English in a table or in data (a tool's default name),
/// in the language in use; a name someone typed stays theirs.
pub(crate) fn localize(text: &str) -> String {
    sue_lang::translate("paint", text)
}

mod bar_items;
mod brush_rows;
mod canvas;
pub mod clip_io;
mod colorsets;
mod comic;
mod commands;
mod dialogs;
mod dock;
mod extras;
mod gradient_bar;
pub mod grid;
pub mod hub;
mod icons;
mod launchers;
mod layout_style;
mod lettering;
mod material_props;
pub mod materials;
mod menus;
mod mobile;
mod modifier_keys;
mod pages;
mod panels;
mod phone;
mod prefs;
mod scan;
pub mod simple;
mod studio;
pub mod studio_mode;
mod subview;
mod transform_ui;
pub mod view;

// The picture engine moved to `sue-picture`; these keep every
// `crate::brush::Tip` and `sue_paint::composite::flatten` path working.
pub use sue_picture::{
    anim, blend, brush, camera, color, colorize, composite, correct, decoration, doc, effects,
    figure, fill, filters, gradient, gradient_map, hand_scan, manga, model3d, print, proof, psd_io, quick,
    rulers, select, story, templates, three, timelapse, tools, transform, vector, warp,
};
// Private in Sue Paint before the move, so private here still.
use sue_picture::{comps, smart};

pub use bar_items::bar_button_rect;
pub use commands::registry;
pub use dialogs::hand_scanner;
pub use launchers::shown_buttons;
pub use menus::menus;
pub use studio::{
    ID, StrokeSeen, SuePaint, ViewSeen, canvas_probed, export_at_once, hold_export_starts,
    hold_exports_to, last_stroke, last_view, mirror, probe_canvas,
};

use sue_core::EntityId;
use sue_wm::{AppletContext, ArenaEdit};

use crate::brush::Tip;
use crate::doc::NewPicture;

/// Paints a stroke through `points` on `layer` with the brush engine and
/// records it in the global history, the way a pen would: pressing in
/// and out along the way.
fn paint_path(
    cx: &mut AppletContext<'_>,
    layer: EntityId,
    tip: Tip,
    rgb: [f32; 3],
    points: &[[f32; 2]],
    label: &str,
) {
    let mut line = crate::vector::figure_engine_line(points, tip, [rgb, rgb], false);
    if let Some(engine) = line.engine.as_mut() {
        engine.device = sue_brush::input::Device::Pen;
        let count = engine.pen.len().max(2) - 1;
        for (index, part) in engine.pen.iter_mut().enumerate() {
            let t = index as f32 / count as f32;
            part.pressure = Some((t * std::f32::consts::PI).sin().max(0.2));
        }
    }
    let Some(before) = doc::raster(&cx.world.arena, layer).cloned() else {
        return;
    };
    let Some(raster) = doc::raster_mut(&mut cx.world.arena, layer) else {
        return;
    };
    let rects = line.paint_into(raster, false, None);
    if let Some(raster) = doc::raster(&cx.world.arena, layer) {
        let edit = crate::brush::engine::edit_between(layer, label, &before, raster, &rects);
        if !edit.is_empty() {
            cx.record(ArenaEdit(edit));
        }
    }
}

/// Creates a sample illustration of `width` by `height` (a soft colored
/// layer under an inked one), painted with the real brushes, every step
/// undoable. Used by the snapshots and the self-check. Returns the picture.
pub fn sample_picture(cx: &mut AppletContext<'_>, width: u32, height: u32) -> Option<EntityId> {
    let (edit, picture) = doc::create(&NewPicture {
        name: "Sample".to_owned(),
        width,
        height,
        ..NewPicture::default()
    })
    .ok()?;
    cx.apply(ArenaEdit(edit)).ok()?;
    let ink = doc::layers(&cx.world.arena, picture).last()?.id;
    let (spawn, color_layer) =
        doc::add_raster_layer(&cx.world.arena, picture, None, "Color".to_owned(), None).ok()?;
    cx.apply(ArenaEdit(spawn)).ok()?;
    // Keep the ink on top.
    if let Some(order) = doc::reorder(&cx.world.arena, picture, color_layer, 1) {
        cx.apply(ArenaEdit(order)).ok()?;
    }
    let (w, h) = (width as f32, height as f32);
    // The circles the ink draws (center, and radius as a share of the
    // width), each filled with flat color up to the middle of its ring:
    // a hard round brush a little over half the circle across, led once
    // around a path a little under half the circle out, covers it from
    // the middle to the ink's center line, so the color meets the line
    // with no pale gap inside it and no halo outside it.
    let circles = [
        ([0.34, 0.45], 0.1, [0.95, 0.55, 0.35]),
        ([0.62, 0.38], 0.08, [0.35, 0.6, 0.95]),
        ([0.5, 0.66], 0.09, [0.55, 0.85, 0.45]),
    ];
    for (center, radius, rgb) in circles {
        let radius = w * radius;
        let fill = Tip {
            size: radius * 1.08,
            hardness: 1.0,
            size_by: crate::brush::Dynamics::NONE,
            opacity_by: crate::brush::Dynamics::NONE,
            ..Tip::default()
        };
        let orbit = radius * 0.46;
        let points: Vec<[f32; 2]> = (0..=48)
            .map(|step| {
                let t = step as f32 / 48.0 * std::f32::consts::TAU;
                [
                    w * center[0] + t.cos() * orbit,
                    h * center[1] + t.sin() * orbit,
                ]
            })
            .collect();
        paint_path(cx, color_layer, fill, rgb, &points, "Fill");
    }
    let pen = Tip {
        size: (w / 160.0).max(2.0),
        ..Tip::default()
    };
    let wave: Vec<[f32; 2]> = (0..=96)
        .map(|step| {
            let t = step as f32 / 96.0;
            [
                w * (0.12 + 0.76 * t),
                h * (0.8 - 0.06 * (t * std::f32::consts::TAU * 2.0).sin()),
            ]
        })
        .collect();
    paint_path(cx, ink, pen, [0.08, 0.08, 0.1], &wave, "G-pen");
    for (center, radius) in [
        ([0.34, 0.45], 0.1),
        ([0.62, 0.38], 0.08),
        ([0.5, 0.66], 0.09),
    ] {
        let ring: Vec<[f32; 2]> = (0..=72)
            .map(|step| {
                let t = step as f32 / 72.0 * std::f32::consts::TAU;
                [
                    w * center[0] + t.cos() * w * radius,
                    h * center[1] + t.sin() * w * radius,
                ]
            })
            .collect();
        paint_path(cx, ink, pen, [0.08, 0.08, 0.1], &ring, "G-pen");
    }
    Some(picture)
}

#[cfg(test)]
mod language_tests {
    /// Every name Sue Paint keeps in English in its tables and in what it
    /// saves has an English message, so `localize` shows it in the
    /// language in use.
    #[test]
    fn every_name_kept_in_english_is_in_the_language_database() {
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        let mut names: Vec<String> = Vec::new();
        for tool in crate::tools::Tool::ALL {
            names.extend(tool.sub_tools().into_iter().map(|sub| sub.name));
        }
        names.extend(
            crate::materials::library(None)
                .into_iter()
                .map(|material| material.name),
        );
        names.extend(
            crate::materials::MaterialKinds::builtin()
                .iter()
                .map(|kind| kind.name.clone()),
        );
        // The 3D object's angles, the body types and the body parts of
        // the body shape chart.
        names.extend(
            crate::three::ANGLES
                .iter()
                .map(|(name, _)| (*name).to_owned()),
        );
        names.extend(
            crate::figure::Build::ALL
                .iter()
                .map(|build| build.name().to_owned()),
        );
        names.extend(crate::figure::PARTS.iter().map(|part| (*part).to_owned()));
        let templates = crate::templates::CanvasTemplates::builtin();
        names.extend(templates.categories().iter().map(|(_, name)| name.clone()));
        names.extend(
            templates
                .categories()
                .iter()
                .flat_map(|(id, _)| templates.of(id))
                .map(|template| template.name.clone())
                .filter(|name| !name.is_empty()),
        );
        let tables: [&[&str]; 7] = [
            &crate::anim::Onion::MODES,
            &crate::vector::Erase::NAMES,
            &crate::rulers::Special::NAMES,
            &crate::correct::Widen::NAMES,
            &crate::effects::EXPRESSIONS,
            &crate::three::STYLES,
            &crate::three::SHAPES,
        ];
        names.extend(
            tables
                .iter()
                .flat_map(|table| table.iter().map(|&name| name.to_owned())),
        );
        names.extend(crate::print::PAPERS.iter().map(|paper| paper.0.to_owned()));
        names.extend(
            crate::manga::LAYOUTS
                .iter()
                .map(|layout| layout.0.to_owned()),
        );
        names.extend(
            crate::proof::PROFILES
                .iter()
                .map(|profile| profile.0.to_owned()),
        );
        names.extend(crate::dialogs::SIZES.iter().map(|size| size.0.to_owned()));
        names.extend(
            crate::dialogs::MODIFIERS
                .iter()
                .map(|modifier| modifier.1.to_owned()),
        );
        for (what, how) in crate::dialogs::GESTURES {
            names.push(what.to_owned());
            names.push(how.to_owned());
        }
        let missing: Vec<String> = names
            .into_iter()
            .filter(|name| !sue_lang::has_english("paint", name))
            .collect();
        assert!(missing.is_empty(), "paint.ftl lacks: {missing:?}");
    }

    /// The blend modes Paint offers read from model.ftl.
    #[test]
    fn every_blend_mode_is_in_the_language_database() {
        let missing: Vec<&str> = sue_model::BlendMode::ALL
            .iter()
            .map(|mode| mode.label())
            .filter(|label| !sue_lang::has_english("model", label))
            .collect();
        assert!(missing.is_empty(), "model.ftl lacks: {missing:?}");
    }
}

/// Old files still load: the golden files of `tests/golden` and the tests
/// that read them. The file lives beside its fixtures and is compiled
/// here, inside the crate, because half of what it loads is read by
/// private code (`autotests` is off in Cargo.toml for that reason).
#[cfg(test)]
#[path = "../tests/golden.rs"]
mod golden;
