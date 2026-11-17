//! Phase 6, "Brush settings in Tool Property (FG-4)": the full guide's
//! brush rows, each followed in the running Sue Paint and named after
//! its row (`fg_` and the time, or `fgv` and the id). Every step drives
//! real frames: the rows of Tool Property and Sub Tool Detail are found
//! by their names and clicked or typed into, strokes are dragged on the
//! canvas, and the pixels the layers hold (or the brush file Sue Paint
//! keeps) are read after.

use super::*;
use egui::{Event, Key, Modifiers, PointerButton};
use sue_core::EntityId;

// ----- Driving ---------------------------------------------------------------

fn open(tag: &str) -> PaintDriver {
    let mut driver = PaintDriver::new(&format!("p6-rows-{tag}"));
    driver.command("view.actual");
    driver.settle(4);
    driver
}

/// A press at the first of `points`, `between` moves on the way to each
/// of the rest, and the release at the last.
fn path(driver: &mut PaintDriver, points: &[Pos2], between: usize) {
    let button = |pos, pressed| Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    let first = points[0];
    driver.step(vec![Event::PointerMoved(first)]);
    driver.step(vec![button(first, true)]);
    for pair in points.windows(2) {
        for index in 1..=between.max(1) {
            let at = pair[0] + (pair[1] - pair[0]) * (index as f32 / between.max(1) as f32);
            driver.step(vec![Event::PointerMoved(at)]);
        }
    }
    let last = *points.last().unwrap();
    driver.step(vec![button(last, false)]);
    driver.settle(4);
}

fn stroke(driver: &mut PaintDriver, from: Pos2, to: Pos2) {
    path(driver, &[from, to], 8);
}

fn run(driver: &mut PaintDriver, command: &str) {
    driver.command(command);
    driver.settle(4);
}

/// The sub tool `name` of the tool whose id is `tool`.
fn pick(driver: &mut PaintDriver, tool: &str, name: &str) {
    run(driver, &format!("tool.pick.{tool}"));
    run(
        driver,
        &format!("subtool.{tool}.{}", name.replace(' ', "_")),
    );
}

/// Every widget with a name the next frame draws, and where.
fn widgets(driver: &mut PaintDriver) -> Vec<(String, egui::Rect)> {
    driver.ctx.enable_accesskit();
    let raw = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
        time: Some(driver.ctx.input(|input| input.time) + 0.016),
        ..egui::RawInput::default()
    };
    driver.now += Duration::from_millis(16);
    let runtime = &mut driver.runtime;
    let now = driver.now;
    let output = driver
        .ctx
        .run_ui(raw, |ui| runtime.ui(ui, &mut NoTextures, now));
    driver.texts = text_rects(&output);
    let mut found = Vec::new();
    if let Some(update) = &output.platform_output.accesskit_update {
        for (_, node) in &update.nodes {
            if let (Some(label), Some(bounds)) = (node.label(), node.bounds()) {
                found.push((
                    label.to_owned(),
                    egui::Rect::from_min_max(
                        egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                        egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                    ),
                ));
            }
        }
    }
    output.drop_without_applying_deltas();
    found
}

fn click(driver: &mut PaintDriver, at: Pos2) {
    path(driver, &[at], 1);
}

/// Where Sub Tool Detail is, while it is open: from its title to its
/// footer's buttons.
fn detail_area(driver: &PaintDriver) -> Option<egui::Rect> {
    let title = driver.rect_of("Sub Tool Detail")?;
    let footer = driver.rect_of("Show Categories in Tool Property")?;
    let footer = footer.union(
        driver
            .rect_of("Save all settings as default")
            .unwrap_or(footer),
    );
    Some(egui::Rect::from_min_max(
        egui::pos2(title.left() - 60.0, title.top()),
        egui::pos2(
            footer.right().max(title.left() + 560.0) + 120.0,
            footer.bottom(),
        ),
    ))
}

/// The rightmost widget named `name` (inside Sub Tool Detail while it is
/// open): a row's number field (its slider and its field share the
/// row's name), or the row's one control.
fn control(driver: &mut PaintDriver, name: &str) -> egui::Rect {
    let all = widgets(driver);
    let area = detail_area(driver);
    all.iter()
        .filter(|(label, rect)| {
            label == name && area.is_none_or(|area| area.contains(rect.center()))
        })
        .map(|(_, rect)| *rect)
        .max_by(|a, b| a.right().total_cmp(&b.right()))
        .or_else(|| {
            // A row whose control carries no name of its own: what is
            // drawn just right of the row's name, on its line.
            let label = driver
                .texts
                .iter()
                .find(|(text, rect)| {
                    text == name && area.is_none_or(|area| area.contains(rect.center()))
                })
                .map(|(_, rect)| *rect)?;
            driver
                .texts
                .iter()
                .filter(|(_, rect)| {
                    (rect.center().y - label.center().y).abs() < 8.0 && rect.left() > label.right()
                })
                .map(|(_, rect)| *rect)
                .min_by(|a, b| a.left().total_cmp(&b.left()))
        })
        .unwrap_or_else(|| {
            let names: Vec<&String> = all.iter().map(|(label, _)| label).collect();
            panic!("{name:?} among {names:?}; drawn {:?}", driver.texts)
        })
}

/// Types `value` into the number field of the row named `name`.
fn set(driver: &mut PaintDriver, name: &str, value: &str) {
    let rect = control(driver, name);
    super::paint_anim::type_into(driver, rect, value);
    driver.settle(3);
}

/// `true` when the row named `name` shows `option`.
fn showing(driver: &mut PaintDriver, name: &str, option: &str) -> bool {
    let rect = control(driver, name);
    driver
        .texts
        .iter()
        .any(|(text, at)| text == option && rect.contains(at.center()))
}

/// Picks option `index` (named `option`) of the drop-down row named
/// `name`, as a person does: the list opened with a click, then Home,
/// Down to it and Enter, or else a click on it in the open list.
/// Asserts the row then shows it.
fn choose(driver: &mut PaintDriver, name: &str, index: usize, option: &str) {
    let tap = |driver: &mut PaintDriver, k: Key| {
        driver.step(vec![key(k, Modifiers::NONE)]);
        driver.step(vec![Event::Key {
            key: k,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: Modifiers::NONE,
        }]);
    };
    let rect = control(driver, name);
    click(driver, rect.center());
    driver.settle(2);
    tap(driver, Key::Home);
    for _ in 0..index {
        tap(driver, Key::ArrowDown);
    }
    tap(driver, Key::Enter);
    driver.settle(4);
    if !showing(driver, name, option) {
        widgets(driver);
        let rect = control(driver, name);
        if !driver
            .texts
            .iter()
            .any(|(text, at)| text == option && !rect.contains(at.center()))
        {
            click(driver, rect.center());
            driver.settle(2);
        }
        let shown = driver
            .texts
            .iter()
            .rev()
            .find(|(text, at)| text == option && !rect.contains(at.center()))
            .map(|(_, at)| at.center());
        if let Some(at) = shown {
            click(driver, at);
            driver.settle(3);
        }
    }
    assert!(
        showing(driver, name, option),
        "{name} shows {option}: {:?}",
        driver.texts
    );
}

/// A click on the text `text` drawn last (over the rest).
fn click_last(driver: &mut PaintDriver, text: &str) {
    widgets(driver);
    let at = driver
        .texts
        .iter()
        .rev()
        .find(|(found, _)| found == text)
        .map(|(_, rect)| rect.center())
        .unwrap_or_else(|| panic!("{text:?} on screen: {:?}", driver.texts));
    click(driver, at);
    driver.settle(3);
}

/// The categories of Sub Tool Detail's list, in its order
/// (`Category::ALL`).
const CATEGORIES: [&str; 20] = [
    "Brush Size",
    "Ink",
    "Anti-aliasing",
    "Brush Shape",
    "Brush Tip",
    "Spraying Effect",
    "Stroke",
    "Texture",
    "Watercolor edge",
    "Dual brush",
    "Starting and Ending",
    "Erase",
    "Color Jitter",
    "Correction",
    "Anti-overflow",
    "Operation",
    "Reference",
    "Area",
    "Selection",
    "Text",
];

/// Opens Sub Tool Detail on its category named `category`: a click on it
/// in the list where the list shows it, else as a script names it
/// (`detail/<index>`), the list being longer than the palette is high.
fn detail(driver: &mut PaintDriver, category: &str) {
    widgets(driver);
    if driver.rect_of("Show Categories in Tool Property").is_none() {
        run(driver, "sub_tool.detail");
    }
    driver.settle(4);
    widgets(driver);
    let area = detail_area(driver).expect("Sub Tool Detail is open");
    let listed = driver
        .texts
        .iter()
        .rev()
        .find(|(text, rect)| text == category && area.contains(rect.center()))
        .map(|(_, rect)| rect.center());
    match listed {
        Some(at) => click(driver, at),
        None => {
            let index = CATEGORIES
                .iter()
                .position(|name| *name == category)
                .unwrap_or_else(|| panic!("no category {category}"));
            let id = AppletId::new(sue_paint::ID).unwrap();
            let applet = driver.runtime.applets.get_mut(&id).expect("Sue Paint");
            assert!(applet.navigate(&format!("detail/{index}")), "{category}");
        }
    }
    driver.settle(4);
}

/// Closes Sub Tool Detail, so the canvas is free to draw on.
fn close_detail(driver: &mut PaintDriver) {
    widgets(driver);
    if driver.rect_of("Show Categories in Tool Property").is_some() {
        run(driver, "window.sub_tool_detail");
    }
    driver.settle(4);
}

fn shows(driver: &mut PaintDriver, text: &str) -> bool {
    widgets(driver);
    driver.texts.iter().any(|(found, _)| found == text)
}

// ----- Reading the canvas ----------------------------------------------------

fn picture(driver: &PaintDriver) -> EntityId {
    driver
        .runtime
        .applets
        .get(&AppletId::new(sue_paint::ID).unwrap())
        .and_then(|applet| applet.focus())
        .expect("a picture is open")
}

fn arena(driver: &PaintDriver) -> &sue_core::Arena {
    &driver.runtime.engine.world.arena
}

fn top(driver: &PaintDriver) -> EntityId {
    sue_paint::doc::layers(arena(driver), picture(driver))
        .last()
        .expect("a layer")
        .id
}

fn new_layer(driver: &mut PaintDriver) -> EntityId {
    run(driver, "layer.new");
    top(driver)
}

fn size(driver: &PaintDriver) -> [u32; 2] {
    sue_model::picture::size(arena(driver).get_by_id(picture(driver)).unwrap()).unwrap()
}

fn pixel(driver: &PaintDriver, layer: EntityId, p: [i64; 2]) -> [u8; 4] {
    let Some(raster) = sue_paint::doc::raster(arena(driver), layer) else {
        return [0; 4];
    };
    let (x, y) = (p[0], p[1]);
    if x < 0 || y < 0 || x >= i64::from(raster.width()) || y >= i64::from(raster.height()) {
        return [0; 4];
    }
    let at = ((y * i64::from(raster.width()) + x) * 4) as usize;
    let bytes = raster.bytes().unwrap();
    [bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]
}

fn fill_with(driver: &mut PaintDriver, layer: EntityId, rgba: impl Fn(u32, u32) -> [u8; 4]) {
    let arena = &mut driver.runtime.engine.world.arena;
    let raster = sue_paint::doc::raster_mut(arena, layer).expect("pixels");
    let width = raster.width();
    for (i, p) in raster.bytes_mut().expect("bytes").chunks_mut(4).enumerate() {
        p.copy_from_slice(&rgba(i as u32 % width, i as u32 / width));
    }
    driver.settle(2);
}

fn middle(driver: &PaintDriver) -> Pos2 {
    paint_middle(&driver.runtime)
}

/// The picture pixel under screen point `at` (at 100 %).
fn under(driver: &PaintDriver, at: Pos2) -> [i64; 2] {
    let [w, h] = size(driver);
    let d = at - middle(driver);
    [
        (w as f32 / 2.0 + d.x).floor() as i64,
        (h as f32 / 2.0 + d.y).floor() as i64,
    ]
}

/// The alphas of `layer` over the box of picture pixels `x0..x1` by
/// `y0..y1`.
fn alphas(
    driver: &PaintDriver,
    layer: EntityId,
    [x0, y0]: [i64; 2],
    [x1, y1]: [i64; 2],
) -> Vec<u8> {
    (y0..y1)
        .flat_map(|y| (x0..x1).map(move |x| (x, y)))
        .map(|(x, y)| pixel(driver, layer, [x, y])[3])
        .collect()
}

/// How many pixels of `layer` hold paint.
fn painted(driver: &PaintDriver, layer: EntityId) -> usize {
    sue_paint::doc::raster(arena(driver), layer)
        .and_then(sue_core::RasterTile2D::bytes)
        .map_or(0, |bytes| bytes.chunks(4).filter(|p| p[3] > 0).count())
}

/// The kept brush file of the driver's data folder, read.
fn kept_brushes(driver: &PaintDriver) -> sue_core::Arena {
    let file = driver
        .root
        .join("data")
        .join("paint")
        .join("sub-tool-brushes.sue");
    let bytes = std::fs::read(&file).unwrap_or_else(|error| panic!("{file:?}: {error}"));
    sue_io::brushes::library::from_file(&bytes).expect("a brush file")
}

/// The brushes of the kept file's set `set`, by entity name.
fn kept_set(driver: &PaintDriver, set: &str) -> Vec<(String, sue_model::BrushPreset)> {
    use sue_io::brushes::library;
    let arena = kept_brushes(driver);
    let Some(found) = library::sets(&arena)
        .into_iter()
        .find(|id| arena.get_by_id(*id).is_ok_and(|e| e.name() == set))
    else {
        return Vec::new();
    };
    library::brushes(&arena, found)
        .into_iter()
        .filter_map(|id| {
            Some((
                arena.get_by_id(id).ok()?.name().to_owned(),
                library::preset(&arena, id)?.clone(),
            ))
        })
        .collect()
}

// ----- 00:28:16: the brush's own blending mode -------------------------------

/// 00:28:16: a pen's own blending mode, set in Sub Tool Detail's Ink:
/// black in Lighten over gray leaves the gray, while Normal lays black.
/// The brush file keeps the mode.
#[test]
fn fg_002816_a_brush_paints_in_its_own_blending_mode() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("blend");
    let layer = new_layer(&mut driver);
    fill_with(&mut driver, layer, |_, _| [128, 128, 128, 255]);
    pick(&mut driver, "pen", "G-pen");
    let m = middle(&driver);
    stroke(&mut driver, m + vec2(-80.0, -40.0), m + vec2(80.0, -40.0));
    let normal = pixel(&driver, layer, under(&driver, m + vec2(0.0, -40.0)));
    assert!(normal[0] < 40, "Normal lays black: {normal:?}");
    detail(&mut driver, "Ink");
    choose(&mut driver, "Blending mode", 7, "Lighten");
    close_detail(&mut driver);
    stroke(&mut driver, m + vec2(-80.0, 40.0), m + vec2(80.0, 40.0));
    let light = pixel(&driver, layer, under(&driver, m + vec2(0.0, 40.0)));
    assert!(
        (126..=130).contains(&light[0]),
        "Lighten keeps the lighter gray: {light:?}"
    );
    // Saved as the pen's default, the mode is in the brush file.
    detail(&mut driver, "Ink");
    driver.click_text("Save all settings as default");
    let kept = kept_set(&driver, "Defaults");
    let (_, preset) = kept
        .iter()
        .find(|(name, _)| name == "pen/G-pen")
        .expect("the G-pen's default");
    assert_eq!(preset.blend, sue_model::BlendMode::Lighten);
}

// ----- 00:28:27, 00:28:47: amount and density of paint, stretch, blur --------

/// 00:28:27 and 00:28:47: a mixing brush (Oil paint) shows Amount of
/// paint, Density of paint, Color stretch and Blur intensity in Ink; with
/// no amount of paint it lays almost none of its own black on white.
#[test]
fn fg_002827_fg_002847_mixing_brushes_set_their_paint() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("mixing");
    let layer = new_layer(&mut driver);
    fill_with(&mut driver, layer, |_, _| [255, 255, 255, 255]);
    pick(&mut driver, "brush", "Oil paint");
    detail(&mut driver, "Ink");
    for row in [
        "Amount of paint",
        "Density of paint",
        "Color stretch",
        "Blur intensity",
    ] {
        assert!(shows(&mut driver, row), "{row}");
    }
    close_detail(&mut driver);
    let m = middle(&driver);
    let darkness = |driver: &PaintDriver, y: f32| -> u32 {
        let [x0, py] = under(driver, m + vec2(-50.0, y));
        let [x1, _] = under(driver, m + vec2(50.0, y));
        (x0..x1)
            .map(|x| 255 - u32::from(pixel(driver, layer, [x, py])[0]))
            .sum()
    };
    stroke(&mut driver, m + vec2(-120.0, -60.0), m + vec2(120.0, -60.0));
    let full = darkness(&driver, -60.0);
    detail(&mut driver, "Ink");
    set(&mut driver, "Amount of paint", "0");
    set(&mut driver, "Color stretch", "90");
    set(&mut driver, "Blur intensity", "40");
    close_detail(&mut driver);
    stroke(&mut driver, m + vec2(-120.0, 60.0), m + vec2(120.0, 60.0));
    let none = darkness(&driver, 60.0);
    assert!(full > 1000, "full paint darkens the white: {full}");
    assert!(
        none * 4 < full,
        "no paint of its own lays far less: {none} against {full}"
    );
}

// ----- 00:29:47: brush shape presets -----------------------------------------

/// 00:29:47: a brush shape registered from one sub tool and applied to
/// another gives it the same tip (its thickness here), and is kept in
/// the brush file.
#[test]
fn fg_002947_brush_shape_presets_register_and_apply() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("shape");
    pick(&mut driver, "pen", "G-pen");
    detail(&mut driver, "Brush Tip");
    set(&mut driver, "Thickness", "20");
    detail(&mut driver, "Brush Shape");
    driver.click_text("Register brush shape");
    let shapes = kept_set(&driver, "Brush shapes");
    assert!(
        shapes.iter().any(|(name, _)| name == "G-pen"),
        "registered in the file: {shapes:?}"
    );
    close_detail(&mut driver);
    pick(&mut driver, "pen", "Mapping pen");
    detail(&mut driver, "Brush Shape");
    driver.click_text("Apply brush shape");
    detail(&mut driver, "Brush Tip");
    let field = control(&mut driver, "Thickness");
    assert!(
        driver
            .texts
            .iter()
            .any(|(text, rect)| text.starts_with("20")
                && (rect.center().y - field.center().y).abs() < field.height()),
        "the Mapping pen's thickness is 20 %: {:?}",
        driver.texts
    );
}

// ----- 00:29:53, 00:30:24, FGV-24: tips, thickness, direction, flips ---------

/// 00:29:53, 00:30:24 and FGV-24: a round pen made thin (Thickness 25 %)
/// lays a low line across and a full one down; Vertical turns that
/// round. The tip can be made of several pictures picked from the brush
/// tips, and with more than one the Stroke category offers the Repeat
/// method; the flips are offered as Off, On and Random.
#[test]
fn fg_002953_fg_003024_fgv24_tips_thickness_direction_and_flips() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("tips");
    let layer = new_layer(&mut driver);
    pick(&mut driver, "pen", "G-pen");
    for _ in 0..6 {
        driver.step(vec![key(Key::CloseBracket, Modifiers::NONE)]);
        driver.settle(1);
    }
    detail(&mut driver, "Brush Tip");
    set(&mut driver, "Thickness", "25");
    close_detail(&mut driver);
    let m = middle(&driver);
    let height_of = |driver: &PaintDriver, at: Pos2| -> i64 {
        let [x, y] = under(driver, at);
        (y - 60..y + 60)
            .filter(|yy| pixel(driver, layer, [x, *yy])[3] > 128)
            .count() as i64
    };
    let width_of = |driver: &PaintDriver, at: Pos2| -> i64 {
        let [x, y] = under(driver, at);
        (x - 60..x + 60)
            .filter(|xx| pixel(driver, layer, [*xx, y])[3] > 128)
            .count() as i64
    };
    stroke(
        &mut driver,
        m + vec2(-150.0, -100.0),
        m + vec2(-20.0, -100.0),
    );
    stroke(&mut driver, m + vec2(100.0, -150.0), m + vec2(100.0, -20.0));
    let across = height_of(&driver, m + vec2(-85.0, -100.0));
    let down = width_of(&driver, m + vec2(100.0, -85.0));
    assert!(
        across * 2 < down,
        "a low tip: thin across, full down: {across} {down}"
    );
    detail(&mut driver, "Brush Tip");
    driver.click_text("Vertical");
    close_detail(&mut driver);
    stroke(&mut driver, m + vec2(-150.0, 100.0), m + vec2(-20.0, 100.0));
    let turned = height_of(&driver, m + vec2(-85.0, 100.0));
    assert!(
        turned > across * 2,
        "vertical: full across: {turned} {across}"
    );
    // Several tips, picked from the brush tips.
    detail(&mut driver, "Brush Tip");
    for flip in ["Flip Horizontal", "Flip Vertical"] {
        assert!(shows(&mut driver, flip), "{flip}");
    }
    driver.click_text("Material");
    for name in ["Star", "Dot"] {
        let field = control(&mut driver, "Search tips");
        super::paint_anim::type_into(&mut driver, field, name);
        click_last(&mut driver, name);
        if driver.rect_of("Add brush tip").is_some() && driver.rect_of("Search tips").is_none() {
            driver.click_text("Add brush tip");
        }
    }
    widgets(&mut driver);
    assert!(
        driver
            .texts
            .iter()
            .filter(|(text, _)| text == "Star" || text == "Dot")
            .count()
            >= 2,
        "two tips listed: {:?}",
        driver.texts
    );
    detail(&mut driver, "Stroke");
    assert!(shows(&mut driver, "Repeat method"), "{:?}", driver.texts);
}

// ----- 00:30:34: brush density, adjust by gap --------------------------------

/// 00:30:34: Brush density is apart from opacity: a pen at 40 % density
/// lays its line at about 40 %, its opacity still 100 %. Adjust brush
/// density by gap is offered beside it.
#[test]
fn fg_003034_brush_density_apart_from_opacity() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("density");
    let layer = new_layer(&mut driver);
    pick(&mut driver, "pen", "G-pen");
    detail(&mut driver, "Brush Tip");
    assert!(shows(&mut driver, "Adjust brush density by gap"));
    set(&mut driver, "Brush density", "40");
    close_detail(&mut driver);
    let m = middle(&driver);
    stroke(&mut driver, m + vec2(-100.0, 0.0), m + vec2(100.0, 0.0));
    let [x, y] = under(&driver, m);
    let most = (y - 10..y + 10)
        .map(|yy| pixel(&driver, layer, [x, yy])[3])
        .max()
        .unwrap();
    assert!(
        (90..=115).contains(&most),
        "at most about 40 % laid: {most}"
    );
}

// ----- 00:30:58, FGV-25: the spraying effect ---------------------------------

/// 00:30:58 and FGV-25: the Spray shows its spraying effect in Tool
/// Property (particle size linked to the brush, density, deviation and
/// direction); fewer particles lay less paint.
#[test]
fn fg_003058_fgv25_spray_shows_and_sets_its_spraying_effect() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("spray");
    let layer = new_layer(&mut driver);
    pick(&mut driver, "airbrush", "Spray");
    detail(&mut driver, "Spraying Effect");
    for row in [
        "Spraying effect",
        "Particle size",
        "Link to brush size",
        "Particle density",
        "Spray deviation",
        "Direction",
    ] {
        assert!(shows(&mut driver, row), "{row}: {:?}", driver.texts);
    }
    close_detail(&mut driver);
    let m = middle(&driver);
    stroke(
        &mut driver,
        m + vec2(-200.0, -120.0),
        m + vec2(200.0, -120.0),
    );
    let [x0, y] = under(&driver, m + vec2(-150.0, -120.0));
    let [x1, _] = under(&driver, m + vec2(150.0, -120.0));
    let dense: usize = alphas(&driver, layer, [x0, y - 50], [x1, y + 50])
        .iter()
        .filter(|a| **a > 0)
        .count();
    detail(&mut driver, "Spraying Effect");
    set(&mut driver, "Particle density", "2");
    close_detail(&mut driver);
    stroke(&mut driver, m + vec2(-200.0, 120.0), m + vec2(200.0, 120.0));
    let [_, y] = under(&driver, m + vec2(0.0, 120.0));
    let sparse: usize = alphas(&driver, layer, [x0, y - 50], [x1, y + 50])
        .iter()
        .filter(|a| **a > 0)
        .count();
    assert!(
        sparse * 2 < dense,
        "fewer particles, less paint: {sparse} against {dense}"
    );
}

// ----- 00:31:12, FGV-26: gap presets, continuous spraying, repeat method -----

/// 00:31:12 and FGV-26: Droplet's gap presets: Wide lays fewer dabs than
/// Narrow along the same stroke; Continuous spraying keeps laying while
/// the pen holds still.
#[test]
fn fg_003112_fgv26_gap_presets_and_continuous_spraying() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("gap");
    pick(&mut driver, "airbrush", "Droplet");
    let m = middle(&driver);
    let mut laid = Vec::new();
    for gap in ["Narrow", "Wide"] {
        let layer = new_layer(&mut driver);
        detail(&mut driver, "Stroke");
        click_last(&mut driver, gap);
        close_detail(&mut driver);
        stroke(&mut driver, m + vec2(-200.0, 0.0), m + vec2(200.0, 0.0));
        laid.push(painted(&driver, layer));
    }
    assert!(laid[1] * 2 < laid[0], "wide gaps lay fewer dabs: {laid:?}");
    // Continuous spraying: holding still keeps laying, so a long hold
    // lays more than a short one.
    pick(&mut driver, "airbrush", "Spray");
    detail(&mut driver, "Stroke");
    driver.click_text("Continuous spraying");
    close_detail(&mut driver);
    let mut held = Vec::new();
    for (frames, dy) in [(2, 150.0), (60, -150.0)] {
        let layer = new_layer(&mut driver);
        let at = m + vec2(0.0, dy);
        let button = |pressed| Event::PointerButton {
            pos: at,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        driver.step(vec![Event::PointerMoved(at)]);
        driver.step(vec![button(true)]);
        driver.settle(frames);
        driver.step(vec![button(false)]);
        driver.settle(4);
        let ink: u64 = sue_paint::doc::raster(arena(&driver), layer)
            .and_then(sue_core::RasterTile2D::bytes)
            .map_or(0, |bytes| bytes.chunks(4).map(|p| u64::from(p[3])).sum());
        held.push(ink);
    }
    let (early, late) = (held[0], held[1]);
    assert!(late > early, "it sprays on: {early} then {late}");
}

// ----- 00:32:46: Erase all layers, Sharp angles ------------------------------

/// 00:32:46: the Hard eraser with Erase all layers on takes paint off two
/// layers at once, one undo step; Sharp angles is offered in Correction.
#[test]
fn fg_003246_erase_all_layers_and_sharp_angles() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("erase-all");
    let below = new_layer(&mut driver);
    fill_with(&mut driver, below, |_, _| [0, 0, 255, 255]);
    let above = new_layer(&mut driver);
    fill_with(&mut driver, above, |_, _| [255, 0, 0, 255]);
    pick(&mut driver, "eraser", "Hard");
    detail(&mut driver, "Erase");
    driver.click_text("Erase all layers");
    detail(&mut driver, "Correction");
    assert!(shows(&mut driver, "Sharp angles"));
    close_detail(&mut driver);
    let m = middle(&driver);
    stroke(&mut driver, m + vec2(-80.0, 0.0), m + vec2(80.0, 0.0));
    let p = under(&driver, m);
    assert_eq!(pixel(&driver, above, p)[3], 0, "the top layer erased");
    assert_eq!(pixel(&driver, below, p)[3], 0, "and the one under it");
    driver.step(vec![key(Key::Z, Modifiers::COMMAND)]);
    driver.settle(4);
    assert_eq!(pixel(&driver, above, p)[3], 255);
    assert_eq!(pixel(&driver, below, p)[3], 255, "one undo puts both back");
}

/// 00:32:46: Sharp angles off rounds a corner the pen turns: the tip of
/// a sharp V lays less paint past the corner than with it on.
#[test]
fn fg_003246_sharp_angles_off_rounds_corners() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("sharp");
    pick(&mut driver, "pen", "G-pen");
    for _ in 0..8 {
        driver.step(vec![key(Key::CloseBracket, Modifiers::NONE)]);
        driver.settle(1);
    }
    let m = middle(&driver);
    let v = |dx: f32| {
        [
            m + vec2(dx - 60.0, -80.0),
            m + vec2(dx, 60.0),
            m + vec2(dx + 60.0, -80.0),
        ]
    };
    let sharp = new_layer(&mut driver);
    path(&mut driver, &v(-120.0), 10);
    detail(&mut driver, "Correction");
    driver.click_text("Sharp angles");
    close_detail(&mut driver);
    let round = new_layer(&mut driver);
    path(&mut driver, &v(120.0), 10);
    let lowest = |driver: &PaintDriver, layer: EntityId, dx: f32| -> i64 {
        let [x, y] = under(driver, m + vec2(dx, 60.0));
        (y - 20..y + 40)
            .filter(|yy| (x - 6..=x + 6).any(|xx| pixel(driver, layer, [xx, *yy])[3] > 128))
            .max()
            .unwrap_or(0)
    };
    assert!(
        lowest(&driver, round, 120.0) < lowest(&driver, sharp, -120.0),
        "the rounded corner reaches less far"
    );
}

// ----- 00:33:03, FGV-29: post correction by speed, by scale, Bezier ----------

/// 00:33:03 and FGV-29: with post correction on, by speed, by scale and
/// Bezier curve are offered; Bezier lays a wobbly stroke again smoother
/// than the plain correction does.
#[test]
fn fg_003303_fgv29_post_correction_by_speed_by_scale_and_bezier() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("correction");
    pick(&mut driver, "pen", "G-pen");
    detail(&mut driver, "Correction");
    set(&mut driver, "Correction", "4");
    for row in [
        "Post correction by speed",
        "Post correction by scale",
        "Bezier curve",
    ] {
        assert!(shows(&mut driver, row), "{row}: {:?}", driver.texts);
    }
    close_detail(&mut driver);
    let m = middle(&driver);
    // A slow wave of 1.5 pixels either side, every 60 pixels: post
    // correction at level 4 leaves most of it, Bezier lays it straight.
    let wobbly = |dy: f32| -> Vec<Pos2> {
        (0..=48)
            .map(|i| {
                let x = -180.0 + i as f32 * 7.5;
                let wave = (x / 60.0 * std::f32::consts::TAU).sin() * 1.5;
                m + vec2(x, dy + wave)
            })
            .collect()
    };
    let plain = new_layer(&mut driver);
    path(&mut driver, &wobbly(-80.0), 2);
    detail(&mut driver, "Correction");
    driver.click_text("Bezier curve");
    close_detail(&mut driver);
    let bezier = new_layer(&mut driver);
    path(&mut driver, &wobbly(80.0), 2);
    // How far the line's middle wanders up and down along it.
    let wander = |driver: &PaintDriver, layer: EntityId, dy: f32| -> f32 {
        let centers: Vec<f32> = (-150..150)
            .step_by(5)
            .filter_map(|dx| {
                let [x, y] = under(driver, m + vec2(dx as f32, dy));
                let ys: Vec<i64> = (y - 30..y + 30)
                    .filter(|yy| pixel(driver, layer, [x, *yy])[3] > 128)
                    .collect();
                (!ys.is_empty()).then(|| ys.iter().sum::<i64>() as f32 / ys.len() as f32)
            })
            .collect();
        centers.windows(2).map(|p| (p[1] - p[0]).abs()).sum()
    };
    let (a, b) = (wander(&driver, plain, -80.0), wander(&driver, bezier, 80.0));
    assert!(b < a, "Bezier wanders less: {b} against {a}");
}

// ----- 00:33:16: Enable snapping per sub tool --------------------------------

/// 00:33:16: with the grid and Snap to Grid on, a pen stroke snaps to a
/// grid line; the same pen with Enable snapping off draws where it was
/// dragged.
#[test]
fn fg_003316_enable_snapping_per_sub_tool() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("snapping");
    run(&mut driver, "view.grid");
    run(&mut driver, "view.snap_grid");
    pick(&mut driver, "pen", "G-pen");
    let m = middle(&driver);
    let rows_of = |driver: &PaintDriver, layer: EntityId| -> Vec<i64> {
        let [x, _] = under(driver, m);
        let mut ys: Vec<i64> = (0..size(driver)[1] as i64)
            .filter(|y| pixel(driver, layer, [x, *y])[3] > 128)
            .collect();
        ys.dedup();
        ys
    };
    let snapped = new_layer(&mut driver);
    stroke(&mut driver, m + vec2(-150.0, 13.0), m + vec2(150.0, 13.0));
    let free_layer = new_layer(&mut driver);
    detail(&mut driver, "Correction");
    driver.click_text("Enable snapping");
    close_detail(&mut driver);
    stroke(&mut driver, m + vec2(-150.0, 13.0), m + vec2(150.0, 13.0));
    let off = rows_of(&driver, free_layer);
    let [_, y] = under(&driver, m + vec2(0.0, 13.0));
    assert!(!off.is_empty(), "the free line is there");
    let centre = |ys: &[i64]| ys.iter().sum::<i64>() as f32 / ys.len() as f32;
    assert!(
        (centre(&off) - y as f32).abs() < 2.0,
        "free: where dragged ({y}): {off:?}"
    );
    // Snapped, the line went onto the grid: not where it was dragged.
    let on = rows_of(&driver, snapped);
    assert!(painted(&driver, snapped) > 0, "the snapped line is there");
    assert!(
        on.is_empty() || (centre(&on) - centre(&off)).abs() > 2.0,
        "snapping moved the line: {on:?} {off:?}"
    );
}

// ----- 00:33:22, FGV-31: anti-overflow ---------------------------------------

/// 00:33:22 and FGV-31: a reference layer holds a closed square; a pen
/// with anti-overflow on, its stroke starting inside and dragged far
/// past the square, lays nothing outside it.
#[test]
fn fg_003322_fgv31_anti_overflow_keeps_inside_the_reference_lines() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("overflow");
    let m = middle(&driver);
    let lines = new_layer(&mut driver);
    let [cx, cy] = under(&driver, m);
    fill_with(&mut driver, lines, |x, y| {
        let (dx, dy) = (i64::from(x) - cx, i64::from(y) - cy);
        let edge = (dx.abs() <= 60 && dy.abs() <= 60) && (dx.abs() >= 56 || dy.abs() >= 56);
        if edge { [0, 0, 0, 255] } else { [0, 0, 0, 0] }
    });
    run(&mut driver, "layer.toggle.reference");
    let paint = new_layer(&mut driver);
    pick(&mut driver, "pen", "G-pen");
    detail(&mut driver, "Anti-overflow");
    driver.click_text("Do not cross lines of reference layer");
    close_detail(&mut driver);
    stroke(&mut driver, m + vec2(-20.0, 0.0), m + vec2(200.0, 0.0));
    let inside = pixel(&driver, paint, [cx + 10, cy]);
    let outside = pixel(&driver, paint, [cx + 120, cy]);
    assert!(inside[3] > 128, "inside is painted: {inside:?}");
    assert_eq!(outside[3], 0, "nothing past the line: {outside:?}");
}

// ----- 00:10:49, 00:13:54: tone dot shapes, Noise size and factor ------------

/// The tone layer `layer` drawn by the compositor, alpha only.
fn tone_look(driver: &PaintDriver) -> Vec<u8> {
    let flat = sue_paint::composite::flatten(
        arena(driver),
        picture(driver),
        sue_paint::composite::RenderOptions::default(),
    )
    .expect("the picture");
    flat.bytes().unwrap().chunks(4).map(|p| p[0]).collect()
}

/// 00:10:49 and 00:13:54: Layer Property's tone offers the Ellipse,
/// Heart and Flower dots, each drawn its own way, and Noise with its size
/// and factor, which change how the noise looks.
#[test]
fn fg_001049_fg_001354_tone_dot_shapes_and_noise() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("tone");
    let layer = new_layer(&mut driver);
    fill_with(&mut driver, layer, |_, _| [150, 150, 150, 255]);
    run(&mut driver, "window.layer_property");
    let tone = control(&mut driver, "Tone");
    click(&mut driver, tone.center());
    let mut looks = Vec::new();
    for (index, shape) in [(0, "Circle"), (7, "Ellipse"), (8, "Heart"), (9, "Flower")] {
        choose(&mut driver, "Dot shape", index, shape);
        looks.push(tone_look(&driver));
    }
    for i in 0..looks.len() {
        for j in i + 1..looks.len() {
            assert_ne!(looks[i], looks[j], "shapes {i} and {j} differ");
        }
    }
    choose(&mut driver, "Dot shape", 5, "Noise");
    assert!(shows(&mut driver, "Noise size"), "{:?}", driver.texts);
    assert!(shows(&mut driver, "Noise factor"));
    let fine = tone_look(&driver);
    set(&mut driver, "Noise size", "6");
    let coarse = tone_look(&driver);
    assert_ne!(fine, coarse, "the size changes the grain");
    set(&mut driver, "Noise factor", "0");
    let veil = tone_look(&driver);
    let distinct: std::collections::BTreeSet<u8> = veil.iter().copied().collect();
    assert!(distinct.len() <= 3, "no factor: an even veil: {distinct:?}");
}

// ----- 00:24:07: Selection pen hardness, density, stabilization --------------

/// 00:24:07: the Selection pen shows Hardness, Brush density and
/// Stabilization; at 50 % density it selects half.
#[test]
fn fg_002407_selection_pen_hardness_density_and_stabilization() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("selection-pen");
    pick(&mut driver, "selection", "Selection pen");
    detail(&mut driver, "Correction");
    assert!(shows(&mut driver, "Stabilization"), "{:?}", driver.texts);
    detail(&mut driver, "Ink");
    for row in ["Hardness", "Brush density"] {
        assert!(shows(&mut driver, row), "{row}: {:?}", driver.texts);
    }
    set(&mut driver, "Brush density", "50");
    close_detail(&mut driver);
    let m = middle(&driver);
    stroke(&mut driver, m + vec2(-80.0, 0.0), m + vec2(80.0, 0.0));
    // The selection, cut to a layer: half of a filled layer comes along.
    let layer = new_layer(&mut driver);
    run(&mut driver, "edit.fill");
    let at = under(&driver, m);
    let alpha = pixel(&driver, layer, at)[3];
    assert!(
        (110..=145).contains(&alpha),
        "half selected, half filled: {alpha}"
    );
}

// ----- 00:16:15, FGV-12: Save all settings as default ------------------------

/// 00:16:15 and FGV-12: Save all settings as default keeps the pen as it
/// is in the brush file; changed after, Reset brings back the saved
/// default, not Sue Paint's own.
#[test]
fn fg_001615_fgv12_save_all_settings_as_default() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("default");
    pick(&mut driver, "pen", "G-pen");
    detail(&mut driver, "Correction");
    set(&mut driver, "Stabilization", "42");
    driver.click_text("Save all settings as default");
    let kept = kept_set(&driver, "Defaults");
    let (_, preset) = kept
        .iter()
        .find(|(name, _)| name == "pen/G-pen")
        .expect("saved");
    assert_eq!(preset.input.stabilization, 42);
    set(&mut driver, "Stabilization", "7");
    run(&mut driver, "sub_tool.reset");
    detail(&mut driver, "Correction");
    let field = control(&mut driver, "Stabilization");
    assert!(
        driver.texts.iter().any(|(text, rect)| text == "42"
            && (rect.center().y - field.center().y).abs() < field.height()),
        "Reset brings back 42: {:?}",
        driver.texts
    );
}

// ----- 00:45:18, 00:45:55, FG-11, FGV-48: Register Material and its tip ------

/// 00:45:18, FG-11 and FGV-48: Register Material asks for the save place,
/// tags, brush tip and paper use, scaling ratio, tiling and layer order;
/// 00:45:55: the brush tip it made is found in Sub Tool Detail's tip
/// picker by a tag and becomes the brush's tip, and the Texture row
/// offers it as a paper.
#[test]
fn fg_004518_fg_004555_fg11_fgv48_register_material_and_pick_its_tip_by_tag() {
    let _english = sue_lang::scope(sue_lang::ENGLISH);
    let mut driver = open("register");
    let layer = new_layer(&mut driver);
    fill_with(&mut driver, layer, |x, y| {
        if x < 40 && y < 40 && (x + y) % 7 < 3 {
            [20, 90, 30, 255]
        } else {
            [0, 0, 0, 0]
        }
    });
    run(&mut driver, "edit.material.image");
    widgets(&mut driver);
    for row in [
        "Save location",
        "Tags",
        "Use for brush tip shape",
        "Use for paper texture",
        "Scaling ratio",
        "Tiling",
        "Layer order",
    ] {
        assert!(shows(&mut driver, row), "{row}: {:?}", driver.texts);
    }
    let name = control(&mut driver, "Material name");
    super::paint_anim::type_into(&mut driver, name, "Leafy");
    let tags = control(&mut driver, "Tags");
    super::paint_anim::type_into(&mut driver, tags, "foliage, green");
    driver.click_text("Use for brush tip shape");
    driver.click_text("Use for paper texture");
    driver.click_text("Tiling");
    choose(
        &mut driver,
        "Layer order",
        1,
        "At the top of the Layer palette",
    );
    driver.click_text("OK");
    driver.settle(6);
    let files: Vec<std::path::PathBuf> = walk(&driver.root);
    assert!(
        files.iter().any(|p| p.ends_with("Leafy.suematerial")),
        "its settings kept: {files:?}"
    );
    assert!(
        files
            .iter()
            .any(|p| p.ends_with("tips/Leafy.png") || p.ends_with("tips\\Leafy.png")),
        "kept with the brush tips too: {files:?}"
    );
    // 00:45:55: picked by its tag.
    pick(&mut driver, "brush", "Oil paint");
    detail(&mut driver, "Brush Tip");
    driver.click_text("Material");
    let field = control(&mut driver, "Search tips");
    super::paint_anim::type_into(&mut driver, field, "foliage");
    click_last(&mut driver, "Leafy (foliage, green)");
    widgets(&mut driver);
    assert!(
        driver.texts.iter().any(|(text, _)| text == "Leafy"),
        "the tip is the material: {:?}",
        driver.texts
    );
    // The paper: offered in the Texture row.
    detail(&mut driver, "Texture");
    let texture = control(&mut driver, "Texture");
    click(&mut driver, texture.center());
    driver.settle(3);
    assert!(
        driver.texts.iter().any(|(text, _)| text == "Leafy"),
        "a paper too: {:?}",
        driver.texts
    );
    driver.step(vec![key(Key::Escape, Modifiers::NONE)]);
    close_detail(&mut driver);
}

/// Every file under `dir`.
fn walk(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                out.extend(walk(&path));
            } else {
                out.push(path);
            }
        }
    }
    out
}
