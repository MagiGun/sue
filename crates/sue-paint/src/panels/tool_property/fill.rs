//! Tool Property for Fill, Auto Select and Gradient, and for a gradient
//! layer while the Gradient or Object tool is on it.

use egui::Ui;
use sue_wm::Theme;
use sue_wm::icons::Icon;
use sue_wm::ui::{self, Density, Space, Tip};

use super::select::select_ops;
use super::{Category, Rows};
use crate::brush::AntiAlias;
use crate::fill::{Exclude, Refer, Target};
use crate::gradient::{Edge, MIN_ASPECT, Shape};
use crate::gradient_bar::{GradientLibrary, gradient_bar, stop_ui};
use crate::gradient_map::GradientMap;
use crate::select::Op;
use crate::tools::{CLOSE_AND_FILL, CONTOUR_FILL, SubTool, Tool};

/// The choices of Refer multiple: every way of looking at more than the
/// editing layer, in Tool Property's order.
const MULTIPLE: [Refer; 4] = [Refer::All, Refer::Selected, Refer::Folder, Refer::Reference];

/// What Close and fill's Target color offers: which of the closed areas
/// fill, judged on the editing layer.
pub(crate) const CLOSE_TARGETS: [Target; 7] = [
    Target::All,
    Target::Transparent,
    Target::NotTransparent,
    Target::Black,
    Target::NotBlack,
    Target::WhiteOrClear,
    Target::NotWhiteOrClear,
];

/// Fill and Auto Select, in Clip Studio Paint's order: Auto Select's
/// selection mode (Close and fill's Target color), then Apply to
/// connected pixels only, Close gap, Tolerance, Area scaling, Refer
/// multiple with what it leaves out, Anti-aliasing, and Fill's opacity.
pub(super) fn fill_properties(
    ui: &mut Ui,
    rows: &mut Rows<'_>,
    tool: Tool,
    sub: &mut SubTool,
    select_op: &mut Op,
) {
    if tool == Tool::AutoSelect {
        select_ops(ui, rows, select_op);
    }
    if tool == Tool::Fill && sub.mode == CLOSE_AND_FILL {
        let names: Vec<String> = CLOSE_TARGETS
            .iter()
            .map(|target| crate::dialogs::target_label(*target))
            .collect();
        let mut chosen = CLOSE_TARGETS
            .iter()
            .position(|target| *target == sub.close_target)
            .unwrap_or(0);
        if rows.choice(
            ui,
            "close-target",
            Category::Area,
            &tr!("target-color"),
            &mut chosen,
            &names,
        ) {
            sub.close_target = CLOSE_TARGETS[chosen];
        }
    }
    rows.check(
        ui,
        "connected-only",
        Category::Area,
        &tr!("apply-to-connected-pixels-only"),
        Some(&tr!("apply-to-connected-pixels-only-hint")),
        &mut sub.connected_only,
    );
    area_rows(ui, rows, sub);
    refer_rows(ui, rows, sub);
    let levels: Vec<String> = AntiAlias::ALL.iter().map(|level| level.label()).collect();
    let mut level = AntiAlias::ALL
        .iter()
        .position(|found| *found == sub.area_aa)
        .unwrap_or(0);
    if rows.choice(
        ui,
        "area-anti-aliasing",
        Category::AntiAliasing,
        &tr!("anti-aliasing"),
        &mut level,
        &levels,
    ) {
        sub.area_aa = AntiAlias::ALL[level];
    }
    if tool == Tool::Fill {
        rows.percent(
            ui,
            "fill-opacity",
            Category::Ink,
            &tr!("opacity"),
            &mut sub.tip.opacity,
            0.0,
            100.0,
        );
    }
}

/// Close gap, Tolerance and Area scaling: how an area is found.
fn area_rows(ui: &mut Ui, rows: &mut Rows<'_>, sub: &mut SubTool) {
    let gaps = [
        tr!("none"),
        tr!("weak"),
        tr!("medium"),
        tr!("strong"),
        tr!("max"),
    ];
    let mut gap = (sub.gap as usize).min(gaps.len() - 1);
    if rows.choice(
        ui,
        "close-gap",
        Category::Area,
        &tr!("close-gap"),
        &mut gap,
        &gaps,
    ) {
        sub.gap = gap as u32;
    }
    rows.percent(
        ui,
        "tolerance",
        Category::Area,
        &tr!("tolerance"),
        &mut sub.tolerance,
        0.0,
        100.0,
    );
    let mut spread = sub.spread as f32;
    if rows.slider(
        ui,
        "area-scaling",
        Category::Area,
        &tr!("area-scaling"),
        &mut spread,
        -20.0..=20.0,
        &tr!("px"),
        false,
    ) {
        sub.spread = spread.round() as i32;
    }
}

/// Refer multiple: whether more than the editing layer is looked at,
/// which layers, and the five kinds of layer it can leave out, as a row
/// of toggles (lit: left out).
fn refer_rows(ui: &mut Ui, rows: &mut Rows<'_>, sub: &mut SubTool) {
    rows.check(
        ui,
        "refer-multiple",
        Category::Reference,
        &tr!("refer-multiple"),
        Some(&tr!("refer-multiple-hint")),
        &mut sub.refer_multiple,
    );
    if !sub.refer_multiple {
        return;
    }
    let names: Vec<String> = MULTIPLE.iter().map(|refer| refer.label()).collect();
    let mut chosen = MULTIPLE
        .iter()
        .position(|refer| *refer == sub.refer)
        .unwrap_or(0);
    if rows.choice(
        ui,
        "refer-layers",
        Category::Reference,
        &tr!("refer"),
        &mut chosen,
        &names,
    ) {
        sub.set_referred(MULTIPLE[chosen]);
    }
    let mut exclude = sub.exclude;
    rows.row(ui, "exclude", Category::Reference, |ui, theme, enabled| {
        exclude_row(ui, &mut exclude, enabled, theme);
    });
    sub.exclude = exclude;
}

/// The Exclude row: draft, text, editing, paper and locked layers, each
/// a toggle with its name in its tooltip.
fn exclude_row(ui: &mut Ui, exclude: &mut Exclude, enabled: bool, theme: &Theme) {
    let Exclude {
        drafts,
        text,
        paper,
        locked,
        editing,
    } = exclude;
    let toggles: [(&str, Icon, String, &mut bool); 5] = [
        (
            "drafts",
            Icon::LayerDraft,
            tr!("exclude-draft-layers"),
            drafts,
        ),
        ("text", Icon::ToolText, tr!("exclude-text-layers"), text),
        ("editing", Icon::Edit, tr!("exclude-editing-layer"), editing),
        (
            "paper",
            Icon::LayerPaper,
            tr!("exclude-paper-layers"),
            paper,
        ),
        ("locked", Icon::Lock, tr!("exclude-locked-layers"), locked),
    ];
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme.space(Space::S1);
        super::name_cell(ui, &tr!("exclude"), super::name_width(theme), theme);
        for (salt, icon, name, on) in toggles {
            let response = ui::icon_button(("sue-paint-exclude", salt), icon, &name)
                .selected(*on)
                .enabled(enabled)
                .show(ui, theme);
            if response.clicked() {
                *on = !*on;
            }
        }
    });
}

/// What the gradient rows need besides the sub tool: the gradient sets,
/// the stop being edited, and the main, sub and drawing colors.
pub(super) struct GradientParts<'a> {
    pub library: &'a mut GradientLibrary,
    pub stop: &'a mut usize,
    pub colors: [[u8; 3]; 3],
}

/// Gradient: the bar of stops and the sets it can come from, then the
/// shape, the edge process, Step of angle, Where to create, opacity and
/// blending mode; Contour line fill's sub tools take the area rows of
/// Fill in place of the shape.
pub(super) fn gradient_properties(
    ui: &mut Ui,
    rows: &mut Rows<'_>,
    sub: &mut SubTool,
    parts: GradientParts<'_>,
) {
    let GradientParts {
        library,
        stop,
        colors,
    } = parts;
    bar_rows(ui, rows, &mut sub.gradient, stop, colors, library);
    let contour = sub.mode == CONTOUR_FILL;
    if contour {
        rows.slider(
            ui,
            "contour-width",
            Category::Operation,
            &tr!("contour-width"),
            &mut sub.contour_width,
            0.0..=200.0,
            &tr!("px"),
            false,
        );
    } else {
        shape_rows(ui, rows, &mut sub.gradient_shape, &mut sub.gradient_aspect);
    }
    edge_row(ui, rows, &mut sub.gradient_edge);
    if contour {
        area_rows(ui, rows, sub);
        refer_rows(ui, rows, sub);
    } else {
        rows.slider(
            ui,
            "gradient-step-angle",
            Category::Operation,
            &tr!("step-of-angle"),
            &mut sub.angle_step,
            0.0..=90.0,
            &tr!("degrees-short"),
            false,
        );
        let mut place = usize::from(sub.new_layer);
        if rows.choice(
            ui,
            "gradient-where",
            Category::Operation,
            &tr!("where-to-create"),
            &mut place,
            &[tr!("editing-layer"), tr!("new-gradient-layer-2")],
        ) {
            sub.new_layer = place == 1;
        }
    }
    rows.percent(
        ui,
        "gradient-opacity",
        Category::Ink,
        &tr!("opacity"),
        &mut sub.tip.opacity,
        0.0,
        100.0,
    );
    let modes: Vec<String> = sue_model::BlendMode::ALL
        .iter()
        .map(|mode| sue_lang::translate("model", mode.label()))
        .collect();
    let mut chosen = sue_model::BlendMode::ALL
        .iter()
        .position(|mode| *mode == sub.blend)
        .unwrap_or(0);
    if rows.choice(
        ui,
        "gradient-blend",
        Category::Ink,
        &tr!("blending-mode"),
        &mut chosen,
        &modes,
    ) {
        sub.blend = sue_model::BlendMode::ALL[chosen];
    }
    if !contour {
        rows.hint(ui, &tr!("gradient-drag-hint"));
    }
}

/// The gradient of a gradient layer, for Tool Property while the
/// Gradient or Object tool is on it: its bar, shape and edge process.
/// Returns `true` when something changed.
pub(super) fn gradient_layer_properties(
    ui: &mut Ui,
    rows: &mut Rows<'_>,
    gradient: &mut crate::doc::Gradient,
    stop: &mut usize,
    colors: [[u8; 3]; 3],
    library: &mut GradientLibrary,
) -> bool {
    let before = *gradient;
    rows.hint(ui, &tr!("gradient-layer-hint"));
    let look = &mut gradient.look;
    bar_rows(ui, rows, &mut look.map, stop, colors, library);
    shape_rows(ui, rows, &mut look.shape, &mut look.aspect);
    edge_row(ui, rows, &mut look.edge);
    *gradient != before
}

/// The bar of stops with the stop's settings, and the row that takes a
/// gradient from a set or keeps this one in the person's own set.
fn bar_rows(
    ui: &mut Ui,
    rows: &mut Rows<'_>,
    map: &mut GradientMap,
    stop: &mut usize,
    colors: [[u8; 3]; 3],
    library: &mut GradientLibrary,
) {
    // The stops that follow a drawing color show it now.
    *map = map.with_drawing(colors[0], colors[1], colors[2]);
    rows.row(ui, "gradient-bar", Category::Ink, |ui, theme, enabled| {
        ui.add_enabled_ui(enabled, |ui| {
            *stop = (*stop).min(map.stops().len().saturating_sub(1));
            gradient_bar(ui, map, stop, theme);
            stop_ui(ui, map, stop, colors, theme);
        });
    });
    rows.row(ui, "gradient-set", Category::Ink, |ui, theme, enabled| {
        set_row(ui, map, library, colors, enabled, theme);
    });
}

/// A gradient from any set (its stops copied in), and Keep in My
/// Gradients, which writes this one to the person's own set file.
fn set_row(
    ui: &mut Ui,
    map: &mut GradientMap,
    library: &mut GradientLibrary,
    colors: [[u8; 3]; 3],
    enabled: bool,
    theme: &Theme,
) {
    let mut names = vec![tr!("gradient-from-a-set")];
    let mut found = Vec::new();
    for (set, (_, gradients)) in library.sets.iter().enumerate() {
        for (index, gradient) in gradients.gradients.iter().enumerate() {
            names.push(tr!(
                "gradient-in-set",
                name = gradient.name.clone(),
                set = gradients.name.clone()
            ));
            found.push((set, index));
        }
    }
    ui.horizontal(|ui| {
        let side = Density::of(ui).icon_button();
        let room = (ui.available_width() - side - theme.space(Space::S2)).max(side);
        let mut chosen = 0;
        let label = tr!("gradient-set");
        let response = ui::input::dropdown("sue-paint-tool-gradient-set", &mut chosen, &names)
            .label(&label)
            .enabled(enabled)
            .width(room)
            .show(ui, theme);
        ui::tooltip(response, &Tip::new(&label), theme);
        if chosen > 0
            && let Some((set, index)) = found.get(chosen - 1).copied()
            && let Some(gradient) = library
                .sets
                .get(set)
                .and_then(|(_, gradients)| gradients.gradients.get(index))
        {
            *map = gradient.map.with_drawing(colors[0], colors[1], colors[2]);
            library.set = set;
            library.picked = Some(index);
        }
        let keep = tr!("gradient-keep");
        let response = ui::icon_button("sue-paint-tool-gradient-keep", Icon::Plus, &keep)
            .enabled(enabled)
            .show(ui, theme);
        if response.clicked() {
            library.add(tr!("gradient-new-name"), *map);
        }
    });
    if let Some(text) = &library.note {
        ui.label(
            egui::RichText::new(text)
                .size(theme.text_size(sue_wm::ui::TextRole::Caption))
                .color(theme.text_muted()),
        );
    }
}

/// The shape, and an Ellipse's aspect.
fn shape_rows(ui: &mut Ui, rows: &mut Rows<'_>, shape: &mut Shape, aspect: &mut f32) {
    let names = [tr!("linear"), tr!("circle"), tr!("ellipse")];
    let mut chosen = Shape::ALL
        .iter()
        .position(|found| found == shape)
        .unwrap_or(0);
    if rows.choice(
        ui,
        "gradient-shape",
        Category::Operation,
        &tr!("shape-2"),
        &mut chosen,
        &names,
    ) {
        *shape = Shape::ALL[chosen];
    }
    if *shape == Shape::Ellipse {
        let mut percent = *aspect * 100.0;
        if rows.slider(
            ui,
            "gradient-aspect",
            Category::Operation,
            &tr!("ellipse-aspect"),
            &mut percent,
            MIN_ASPECT * 100.0..=100.0,
            " %",
            false,
        ) {
            *aspect = (percent / 100.0).clamp(MIN_ASPECT, 1.0);
        }
    }
}

/// The edge process: what happens before the start and past the end.
fn edge_row(ui: &mut Ui, rows: &mut Rows<'_>, edge: &mut Edge) {
    let names = [
        tr!("edge-do-not-repeat"),
        tr!("edge-repeat"),
        tr!("edge-reverse"),
        tr!("edge-do-not-draw"),
    ];
    let mut chosen = Edge::ALL
        .iter()
        .position(|found| found == edge)
        .unwrap_or(0);
    if rows.choice(
        ui,
        "gradient-edge",
        Category::Operation,
        &tr!("edge-process"),
        &mut chosen,
        &names,
    ) {
        *edge = Edge::ALL[chosen];
    }
}
