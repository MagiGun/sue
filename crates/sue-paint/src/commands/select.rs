//! The Select menu's commands: one entry each, with its own `run` and
//! `state`, its label, default keys, icon and flags.

use sue_core::txn::SetProperty;
use sue_core::Value;
use sue_model::picture;
use sue_wm::{AppletContext, Call, Command, Facts, Flags, ItemState};

use super::{flag, when};
use crate::dialogs::Dialog;
use crate::doc;
use crate::select::{Op, Selection};
use crate::studio::{SELECTION_LAYER, SuePaint};
use crate::tools::Tool;

/// The commands of this group that Auto Actions may record: all of them.
/// Auto Actions keep the name and run it again later.
pub(super) const RECORDABLE: &[&str] = &[
    "select.from_layer",
    "select.all",
    "select.none",
    "select.again",
    "select.invert",
    "select.expand",
    "select.shrink",
    "select.feather",
    "select.gamut",
    "select.add_layer",
    "select.subtract_layer",
    "select.to_layer",
    "select.from_selection_layer",
    "select.quick_mask",
    "select.launcher",
    "select.vectors_overlapping",
    "select.vectors_within",
];

/// The commands of this group that leave the canvas (quit, print, open a
/// system dialog or a browser): none. Expand, Shrink, Blur Border and
/// Color Gamut open a dialog of the studio's own, which Esc closes.
pub(super) const LEAVES_CANVAS: &[&str] = &[];

/// `true` when `a` and `b` are one name (a `const` comparison).
const fn same(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut at = 0;
    while at < a.len() {
        if a[at] != b[at] {
            return false;
        }
        at += 1;
    }
    true
}

/// `true` when `list` holds `name`.
const fn holds(list: &[&str], name: &str) -> bool {
    let mut at = 0;
    while at < list.len() {
        if same(list[at], name) {
            return true;
        }
        at += 1;
    }
    false
}

/// The flags the lists above give `name`.
const fn flags_of(name: &str) -> Flags {
    let mut flags = Flags::NONE;
    if holds(RECORDABLE, name) {
        flags = flags.with(Flags::RECORDABLE);
    }
    if holds(LEAVES_CANVAS, name) {
        flags = flags.with(Flags::LEAVES_CANVAS);
    }
    flags
}

/// A command of this group: its name, label, icon, run and state, with
/// the flags of the lists above.
const fn c(
    name: &'static str,
    label: &'static str,
    icon: &'static str,
    run: fn(&mut SuePaint, &mut AppletContext<'_>, Call<'_>),
    state: fn(&SuePaint, &Facts<'_>, Call<'_>) -> ItemState,
) -> Command<SuePaint> {
    Command::new(name, label, "select", run)
        .icon(icon)
        .state(state)
        .with(flags_of(name))
}

pub(super) const COMMANDS: &[Command<SuePaint>] = &[
    c(
        "select.from_layer",
        "create-selection-from-layer",
        "layers",
        from_layer,
        raster_layer,
    ),
    c("select.all", "select-all", "select-all", all, canvas_open).keys("Ctrl+A"),
    c("select.none", "deselect", "deselect", none, has_selection).keys("Ctrl+D"),
    c("select.again", "reselect", "reload", again, had_selection).keys("Ctrl+Shift+D"),
    c(
        "select.invert",
        "invert-selected-area-2",
        "select-invert",
        invert,
        has_selection,
    )
    .keys("Ctrl+Shift+I or Shift+F7"),
    c(
        "select.expand",
        "expand-selected-area-ellipsis",
        "select-expand",
        expand,
        has_selection,
    ),
    c(
        "select.shrink",
        "shrink-selected-area-ellipsis",
        "select-shrink",
        shrink,
        has_selection,
    ),
    c(
        "select.feather",
        "blur-border-ellipsis",
        "tool-blend",
        feather,
        has_selection,
    ),
    c(
        "select.gamut",
        "select-color-gamut-ellipsis",
        "tool-auto-select",
        gamut,
        canvas_open,
    ),
    c(
        "select.add_layer",
        "add-selection",
        "plus",
        add_layer,
        raster_layer,
    ),
    c(
        "select.subtract_layer",
        "remove-selection",
        "minus",
        subtract_layer,
        raster_layer,
    ),
    c(
        "select.to_layer",
        "convert-to-selection-layer",
        "layer-new",
        to_layer,
        has_selection,
    ),
    c(
        "select.from_selection_layer",
        "convert-selection-layer-to-selection",
        "tool-select",
        from_selection_layer,
        on_selection_layer,
    ),
    c(
        "select.quick_mask",
        "quick-mask",
        "layer-mask",
        quick_mask,
        quick_mask_on,
    ),
    c(
        "select.launcher",
        "selection-launcher",
        "palette-command-bar",
        launcher,
        launcher_shown,
    ),
    c(
        "select.vectors_overlapping",
        "select-overlapping-vectors",
        "tool-operation",
        vectors_overlapping,
        vectors_and_selection,
    ),
    c(
        "select.vectors_within",
        "select-vectors-within-area",
        "tool-operation",
        vectors_within,
        vectors_and_selection,
    ),
];

// What each item looks like now.

/// While a canvas is open.
fn canvas_open(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(paint.doc().is_some())
}

/// While the canvas has a selection.
fn has_selection(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(paint.doc().is_some_and(|doc| doc.selection.is_some()))
}

/// While there is a selection to bring back.
fn had_selection(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(paint.doc().is_some_and(|doc| doc.last_selection.is_some()))
}

/// While the current layer is a raster layer.
fn raster_layer(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(
        paint
            .layer_info(facts.arena)
            .is_some_and(|info| info.kind == picture::LayerKind::Raster),
    )
}

/// While the current layer is a selection layer.
fn on_selection_layer(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(
        paint
            .layer_info(facts.arena)
            .and_then(|info| facts.arena.get_by_id(info.id).ok())
            .is_some_and(|entity| {
                entity.property(SELECTION_LAYER).and_then(Value::as_bool) == Some(true)
            }),
    )
}

/// Checked while Quick Mask is on.
fn quick_mask_on(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    paint
        .doc()
        .map_or(ItemState::off(), |doc| flag(doc.quick_mask.is_some()))
}

/// Checked while the selection launcher shows.
fn launcher_shown(paint: &SuePaint, _: &Facts<'_>, _: Call<'_>) -> ItemState {
    flag(paint.launcher)
}

/// While the current layer is a vector layer and the canvas has a
/// selection.
fn vectors_and_selection(paint: &SuePaint, facts: &Facts<'_>, _: Call<'_>) -> ItemState {
    when(
        paint.doc().is_some_and(|doc| doc.selection.is_some())
            && paint
                .layer()
                .is_some_and(|layer| crate::vector::is_vector(facts.arena, layer)),
    )
}

// What each command does.

/// Select All: the whole canvas.
fn all(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let all = paint.doc().and_then(|doc| {
        let [w, h] = picture::size(cx.world.arena.get_by_id(doc.picture).ok()?)?;
        Some(Selection::all(w, h))
    });
    if let Some(doc) = paint.doc_mut() {
        doc.set_selection(all);
    }
}

/// Deselect.
fn none(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    if let Some(doc) = paint.doc_mut() {
        doc.set_selection(None);
    }
}

/// Reselect: the selection before the last change comes back.
fn again(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    if let Some(doc) = paint.doc_mut() {
        let again = doc.last_selection.clone();
        doc.set_selection(again);
    }
}

/// Invert Selected Area.
fn invert(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    if let Some(doc) = paint.doc_mut()
        && let Some(mut selection) = doc.selection.clone()
    {
        selection.invert();
        doc.set_selection(Some(selection));
    }
}

/// Opens the dialog that asks by how much (`kind` 0 expands, 1 shrinks, 2
/// blurs the border), when there is a selection.
fn select_by(paint: &mut SuePaint, kind: u8) {
    if paint.doc().is_some_and(|doc| doc.selection.is_some()) {
        paint.dialog = Some(Dialog::SelectBy(kind, 4.0));
    }
}

/// Expand Selected Area...
fn expand(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    select_by(paint, 0);
}

/// Shrink Selected Area...
fn shrink(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    select_by(paint, 1);
}

/// Blur Border...
fn feather(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    select_by(paint, 2);
}

/// Select Color Gamut...
fn gamut(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    if paint.picture().is_some() {
        paint.dialog = Some(Dialog::Gamut(0.1, false));
    }
}

/// Adds the current layer's pixels to the selection, or takes them away.
fn layer_pixels(paint: &mut SuePaint, cx: &AppletContext<'_>, op: Op) {
    let Some(raster) = paint
        .layer()
        .and_then(|layer| doc::raster(&cx.world.arena, layer))
    else {
        return;
    };
    let from = Selection::from_alpha(raster);
    if let Some(doc) = paint.doc_mut() {
        let [width, height] = from.size();
        let mut selection = doc
            .selection
            .clone()
            .unwrap_or_else(|| Selection::new(width, height));
        selection.apply_mask(from.mask(), op);
        doc.set_selection(Some(selection));
    }
}

/// Selection from Layer, Add Selection.
fn add_layer(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    layer_pixels(paint, cx, Op::Add);
}

/// Selection from Layer, Remove Selection.
fn subtract_layer(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    layer_pixels(paint, cx, Op::Subtract);
}

/// Convert to Selection Layer: the selection in green on a new layer,
/// left out of exports; Convert Selection Layer to Selection takes it
/// back. One undo step.
fn to_layer(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let (Some(picture), Some(selection)) = (
        paint.picture(),
        paint.doc().and_then(|doc| doc.selection.clone()),
    ) else {
        return;
    };
    let [width, height] = selection.size();
    let pixels: Vec<u8> = selection
        .mask()
        .iter()
        .flat_map(|alpha| [60, 200, 90, *alpha])
        .collect();
    let Ok(raster) =
        sue_core::RasterTile2D::from_bytes(width, height, sue_core::PixelFormat::Rgba8, pixels)
    else {
        return;
    };
    match doc::add_layer_pixels(
        &cx.world.arena,
        picture,
        paint.layer(),
        tr!("selection"),
        raster,
    ) {
        Ok((spawn, id)) => {
            let edit = doc::Edit::new(tr!("convert-to-selection-layer"))
                .then(spawn)
                .then(SetProperty::new(id, SELECTION_LAYER, true))
                .then(SetProperty::new(id, picture::DRAFT, true))
                .then(SetProperty::new(
                    id,
                    picture::OPACITY,
                    cx.rice()
                        .get_i64("sue_paint.selection_layer_opacity")
                        .unwrap_or(50)
                        .clamp(0, 100) as f64
                        / 100.0,
                ));
            if SuePaint::apply(cx, edit)
                && let Some(doc) = paint.doc_mut()
            {
                doc.layer = Some(id);
            }
        }
        Err(error) => cx.fail(error),
    }
}

/// Convert Selection Layer to Selection.
fn from_selection_layer(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    if let Some(raster) = paint
        .layer()
        .and_then(|layer| doc::raster(&cx.world.arena, layer))
    {
        let selection = Selection::from_alpha(raster);
        if let Some(doc) = paint.doc_mut() {
            doc.set_selection(Some(selection));
        }
    }
}

/// Quick Mask, on and off (one undo step each way).
fn quick_mask(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    paint.toggle_quick_mask(cx);
}

/// View, Selection Launcher, shown or hidden.
fn launcher(paint: &mut SuePaint, _: &mut AppletContext<'_>, _: Call<'_>) {
    paint.launcher = !paint.launcher;
}

/// Layer, Selection from Layer, Create Selection: the current layer's
/// pixels where they lie on the canvas.
fn from_layer(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    let arena = &cx.world.arena;
    let Some(info) = paint.layer_info(arena) else {
        return;
    };
    let Some(size) = paint
        .picture()
        .and_then(|picture| arena.get_by_id(picture).ok())
        .and_then(picture::size)
    else {
        return;
    };
    let selection = doc::raster(arena, info.id).map(|raster| {
        let alpha = Selection::from_alpha(raster);
        let mut mask = vec![0_u8; size[0] as usize * size[1] as usize];
        let [ox, oy] = info.style.offset;
        for y in 0..raster.height() {
            for x in 0..raster.width() {
                let (px, py) = (i64::from(x) + ox, i64::from(y) + oy);
                if px >= 0 && py >= 0 && px < i64::from(size[0]) && py < i64::from(size[1]) {
                    mask[(py as u32 * size[0] + px as u32) as usize] =
                        (alpha.at(x, y) * 255.0).round() as u8;
                }
            }
        }
        Selection::from_mask(size[0], size[1], mask)
    });
    if let Some(doc) = paint.doc_mut() {
        doc.set_selection(selection.flatten());
    }
}

/// Select Overlapping Vectors: the lines of the current vector layer that
/// touch the selected area are picked for the Object tool.
fn vectors_overlapping(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    pick_vectors(paint, cx, Reach::Overlapping);
}

/// Select Vectors Within Area: the lines of the current vector layer that
/// lie wholly inside the selected area are picked for the Object tool.
fn vectors_within(paint: &mut SuePaint, cx: &mut AppletContext<'_>, _: Call<'_>) {
    pick_vectors(paint, cx, Reach::Within);
}

/// How much of a line must be in the selected area for it to be picked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reach {
    /// Any point of its path.
    Overlapping,
    /// Every point of its path.
    Within,
}

/// Picks the lines of the current vector layer the selected area reaches
/// as `reach` says, and takes up the Object tool, which shows them, moves
/// them and deletes them. The selection stays; nothing in the document
/// changes, so there is nothing to undo.
fn pick_vectors(paint: &mut SuePaint, cx: &mut AppletContext<'_>, reach: Reach) {
    let arena = &cx.world.arena;
    let Some(info) = paint.layer_info(arena) else {
        return;
    };
    if !crate::vector::is_vector(arena, info.id) {
        cx.notify(tr!("pick-a-vector-layer-first"));
        return;
    }
    let Some(selection) = paint.doc().and_then(|doc| doc.selection.clone()) else {
        return;
    };
    let lines = crate::vector::lines(&crate::vector::geometry(arena, info.id));
    let offset = [info.style.offset[0] as f32, info.style.offset[1] as f32];
    let paths: Vec<Vec<[f32; 2]>> = lines
        .iter()
        .map(|line| {
            line.samples
                .iter()
                .map(|sample| [sample.at[0] + offset[0], sample.at[1] + offset[1]])
                .collect()
        })
        .collect();
    let picked = reached(&selection, &paths, reach);
    if picked.is_empty() {
        cx.notify(tr!("no-vector-line-in-the-selected-area"));
    }
    take_up_object(paint);
    paint.picked_lines = Some((info.id, picked));
}

/// The Operation tool with its Object sub tool, the one that works on
/// picked vector lines.
fn take_up_object(paint: &mut SuePaint) {
    paint.toolbox.pick(Tool::Operation);
    if let Some(object) = paint
        .toolbox
        .subs(Tool::Operation)
        .iter()
        .position(|sub| sub.mode == 0)
    {
        paint.toolbox.pick_sub(Tool::Operation, object);
    }
}

/// The indices of the `paths` (in canvas pixels) that `selection` reaches
/// as `reach` says. A path is followed point by point at most half a
/// pixel apart, so a line that only crosses a thin selected area between
/// two of its samples is found too; a point counts as selected where the
/// selection is at least half on.
fn reached(selection: &Selection, paths: &[Vec<[f32; 2]>], reach: Reach) -> Vec<usize> {
    let [width, height] = selection.size();
    let selected = |[x, y]: [f32; 2]| {
        let (x, y) = (x.floor(), y.floor());
        x >= 0.0
            && y >= 0.0
            && x < width as f32
            && y < height as f32
            && selection.at(x as u32, y as u32) >= 0.5
    };
    paths
        .iter()
        .enumerate()
        .filter(|(_, path)| {
            let mut points = along(path);
            match reach {
                Reach::Overlapping => points.any(selected),
                Reach::Within => !path.is_empty() && points.all(selected),
            }
        })
        .map(|(index, _)| index)
        .collect()
}

/// The points of `path`, with points put in between so that no two in a
/// row are more than half a pixel apart.
fn along(path: &[[f32; 2]]) -> impl Iterator<Item = [f32; 2]> + '_ {
    let first = path.first().copied().into_iter();
    first.chain(path.windows(2).flat_map(|pair| {
        let ([x0, y0], [x1, y1]) = (pair[0], pair[1]);
        let length = (x1 - x0).hypot(y1 - y0);
        let steps = (length * 2.0).ceil().max(1.0) as usize;
        (1..=steps).map(move |step| {
            let t = step as f32 / steps as f32;
            [x0 + (x1 - x0) * t, y0 + (y1 - y0) * t]
        })
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 20 by 10 selection with the box from (5, 2) to (15, 8) selected.
    fn boxed() -> Selection {
        let mut selection = Selection::new(20, 10);
        selection.rect([5, 2, 15, 8], Op::Add);
        selection
    }

    #[test]
    fn overlapping_takes_a_line_that_touches_within_one_wholly_inside() {
        let selection = boxed();
        let paths = vec![
            // Wholly inside.
            vec![[6.0, 4.0], [12.0, 4.0]],
            // In and out.
            vec![[1.0, 5.0], [10.0, 5.0]],
            // Wholly outside.
            vec![[0.5, 0.5], [19.0, 0.5]],
            // Its two samples outside, its middle across the box.
            vec![[0.0, 6.0], [19.5, 6.0]],
            // A single point inside.
            vec![[8.0, 3.0]],
            // No samples at all.
            Vec::new(),
        ];
        assert_eq!(
            reached(&selection, &paths, Reach::Overlapping),
            [0, 1, 3, 4]
        );
        assert_eq!(reached(&selection, &paths, Reach::Within), [0, 4]);
    }

    #[test]
    fn a_line_off_the_canvas_is_never_within() {
        let selection = Selection::all(10, 10);
        let paths = vec![vec![[2.0, 2.0], [14.0, 2.0]], vec![[-3.0, 5.0], [5.0, 5.0]]];
        assert_eq!(reached(&selection, &paths, Reach::Within), Vec::<usize>::new());
        assert_eq!(reached(&selection, &paths, Reach::Overlapping), [0, 1]);
    }

    #[test]
    fn a_path_is_followed_at_most_half_a_pixel_apart() {
        let points: Vec<[f32; 2]> = along(&[[0.0, 0.0], [3.0, 0.0], [3.0, 1.0]]).collect();
        assert_eq!(points.first(), Some(&[0.0, 0.0]));
        assert_eq!(points.last(), Some(&[3.0, 1.0]));
        assert_eq!(points.len(), 1 + 6 + 2);
        for pair in points.windows(2) {
            let gap = (pair[1][0] - pair[0][0]).hypot(pair[1][1] - pair[0][1]);
            assert!(gap <= 0.5 + 1e-6, "{pair:?}");
        }
    }

    #[test]
    fn the_lists_name_commands_of_this_group_and_set_their_flags() {
        let names: Vec<&str> = COMMANDS.iter().map(|command| command.name).collect();
        for name in RECORDABLE.iter().chain(LEAVES_CANVAS) {
            assert!(names.contains(name), "{name} is no Select command");
        }
        for command in COMMANDS {
            assert_eq!(
                command.flags.has(Flags::RECORDABLE),
                RECORDABLE.contains(&command.name),
                "{}",
                command.name
            );
            assert_eq!(
                command.flags.has(Flags::LEAVES_CANVAS),
                LEAVES_CANVAS.contains(&command.name),
                "{}",
                command.name
            );
            assert_eq!(command.group, "select");
        }
        assert!(!same("select.all", "select.al"));
        assert!(!same("select.all", "select.alm"));
        assert!(same("select.all", "select.all"));
    }

    #[test]
    fn every_entry_has_a_label_and_an_icon_of_the_shared_set() {
        let _english = sue_lang::scope(sue_lang::ENGLISH);
        for command in COMMANDS {
            assert!(
                sue_wm::icons::Icon::from_name(command.icon).is_some(),
                "{}: no icon {:?}",
                command.name,
                command.icon
            );
            assert!(
                sue_lang::has("paint", command.label),
                "{}: {}",
                command.name,
                command.label
            );
        }
        let label = |name: &str| super::super::registry().label(name);
        assert_eq!(
            label("select.vectors_overlapping").as_deref(),
            Some("Select Overlapping Vectors")
        );
        assert_eq!(
            label("select.vectors_within").as_deref(),
            Some("Select Vectors Within Area")
        );
    }

    #[test]
    fn picking_lines_takes_up_the_object_tool() {
        let mut paint = SuePaint::default();
        paint.toolbox.pick(Tool::Operation);
        paint.toolbox.pick_sub(Tool::Operation, 1);
        paint.toolbox.pick(Tool::Pen);
        take_up_object(&mut paint);
        assert_eq!(paint.toolbox.tool(), Tool::Operation);
        let sub = paint.toolbox.picked(Tool::Operation);
        assert_eq!(paint.toolbox.subs(Tool::Operation)[sub].mode, 0);
        assert!(paint.picked_lines.is_none(), "nothing picked yet");
    }
}
