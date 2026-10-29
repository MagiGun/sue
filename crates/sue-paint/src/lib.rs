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
//! functions over pixels with their own tests; the studio ([`SuePaint`])
//! ties them to the interface.

/// Text from the language database's `paint` namespace (Sue Paint).
macro_rules! tr {
    ($($arg:tt)*) => { sue_lang::tr!("paint", $($arg)*) };
}

/// `text`, kept in English in a table or in data (a tool's default name),
/// in the language in use; a name someone typed stays theirs.
pub(crate) fn localize(text: &str) -> String {
    sue_lang::translate("paint", text)
}

pub mod anim;
mod anim_ui;
pub mod blend;
pub mod brush;
pub mod camera;
mod canvas;
pub mod color;
mod colorsets;
mod comic;
pub mod composite;
pub mod correct;
mod dialogs;
pub mod doc;
mod dock;
pub mod effects;
mod extras;
pub mod fill;
pub mod filters;
mod icons;
mod lettering;
pub mod manga;
pub mod materials;
mod menus;
mod pages;
mod panels;
mod phone;
pub mod print;
pub mod proof;
pub mod psd_io;
pub mod rulers;
pub mod select;
pub mod story;
mod studio;
mod subview;
pub mod three;
pub mod timelapse;
pub mod tools;
pub mod transform;
mod transform_ui;
pub mod vector;
pub mod view;
pub mod warp;

pub use menus::menus;
pub use studio::{ID, SuePaint, mirror};

use sue_core::EntityId;
use sue_wm::{AppletContext, ArenaEdit};

use crate::brush::{Stroke, Tip};
use crate::doc::NewPicture;

/// Paints a stroke through `points` on `layer` in place and records it in
/// the global history, the way a pen would.
fn paint_path(
    cx: &mut AppletContext<'_>,
    layer: EntityId,
    tip: Tip,
    rgb: [f32; 3],
    points: &[[f32; 2]],
    label: &str,
) {
    let mut stroke = Stroke::new(layer, tip, rgb, false);
    if let Some(raster) = doc::raster_mut(&mut cx.world.arena, layer) {
        for (index, point) in points.iter().enumerate() {
            let t = index as f32 / points.len().max(1) as f32;
            stroke.to(
                raster,
                *point,
                (t * std::f32::consts::PI).sin().max(0.2),
                None,
            );
        }
    }
    if let Some(raster) = doc::raster(&cx.world.arena, layer) {
        let edit = stroke.finish(raster, label);
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
    let soft = Tip {
        size: w * 0.18,
        hardness: 0.0,
        build_up: true,
        flow: 0.12,
        size_by: crate::brush::Dynamics::NONE,
        ..Tip::default()
    };
    for (center, rgb) in [
        ([0.34, 0.45], [0.95, 0.55, 0.35]),
        ([0.62, 0.38], [0.35, 0.6, 0.95]),
        ([0.5, 0.66], [0.55, 0.85, 0.45]),
    ] {
        let points: Vec<[f32; 2]> = (0..24)
            .map(|step| {
                let t = step as f32 / 24.0 * std::f32::consts::TAU;
                [
                    w * center[0] + t.cos() * w * 0.06,
                    h * center[1] + t.sin() * h * 0.06,
                ]
            })
            .collect();
        paint_path(cx, color_layer, soft, rgb, &points, "Airbrush");
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
    paint_path(cx, ink, pen, [0.08, 0.08, 0.1], &wave, "G-Pen");
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
        paint_path(cx, ink, pen, [0.08, 0.08, 0.1], &ring, "G-Pen");
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
        let tables: [&[&str]; 8] = [
            &crate::anim::Onion::MODES,
            &crate::vector::Erase::NAMES,
            &crate::rulers::Special::NAMES,
            &crate::correct::Widen::NAMES,
            &crate::materials::Kind::NAMES,
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
        for group in crate::panels::BAR_GROUPS {
            names.extend(group.iter().map(|(_, _, tip)| (*tip).to_owned()));
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
