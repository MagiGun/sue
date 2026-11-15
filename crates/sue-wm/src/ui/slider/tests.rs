//! Tests of the sliders, driven through real egui frames as the inputs'
//! tests are.

use super::*;
use crate::ui::input::tests::{Harness, typed, widget_events};

/// The place the sliders of these tests are drawn at.
fn place() -> Rect {
    Rect::from_min_size(
        pos2(100.0, 100.0),
        vec2(400.0, Density::Regular.control_height()),
    )
}

/// The track of a slider at [`place`] with no label and no field.
fn bare_track() -> Rect {
    rail_in(place(), Density::Regular)
}

/// The point `share` of the way along `track`, on its middle line.
fn along(track: Rect, share: f32) -> Pos2 {
    pos2(track.left() + share * track.width(), track.center().y)
}

fn bare(ui: &mut Ui, theme: &Theme, value: &mut f64) -> Response {
    slider("bare", value, 0.0..=100.0)
        .name("Bare")
        .field(false)
        .show_at(ui, place(), theme)
}

// ----- Values on a track --------------------------------------------------------

#[test]
fn an_even_scale_maps_values_onto_the_track_and_back() {
    let scale = Scale::new(&(-50.0..=150.0), false);
    assert_eq!(scale.to_unit(-50.0), 0.0);
    assert_eq!(scale.to_unit(50.0), 0.5);
    assert_eq!(scale.to_unit(400.0), 1.0, "held to the track");
    assert_eq!(scale.from_unit(0.25), 0.0);
    // A range given high end first is the same range.
    assert_eq!(Scale::new(&(150.0..=-50.0), false), scale);
    // A track of no length, or a value that is no number, sits at its start.
    assert_eq!(Scale::new(&(3.0..=3.0), false).to_unit(3.0), 0.0);
    assert_eq!(scale.to_unit(f64::NAN), 0.0);
    assert_eq!(Scale::new(&(f64::NAN..=1.0), false).to_unit(0.5), 0.0);
}

#[test]
fn a_logarithmic_scale_gives_the_small_end_room() {
    let scale = Scale::new(&(1.0..=1000.0), true);
    assert!((scale.to_unit(10.0) - 1.0 / 3.0).abs() < 1e-9);
    assert!((scale.from_unit(2.0 / 3.0) - 100.0).abs() < 1e-9);
    for value in [1.0, 2.5, 37.0, 999.0, 1000.0] {
        assert!((scale.from_unit(scale.to_unit(value)) - value).abs() < 1e-9);
    }
    // From 0, the very end is 0 and the rest is still logarithmic.
    let from_zero = Scale::new(&(0.0..=100.0), true);
    assert_eq!(from_zero.from_unit(0.0), 0.0);
    assert_eq!(from_zero.to_unit(0.0), 0.0);
    assert!(from_zero.from_unit(0.5) < 1.0);
    assert_eq!(from_zero.from_unit(1.0), 100.0);
    // A range that never reaches above 0 cannot be logarithmic.
    assert_eq!(
        Scale::new(&(-10.0..=0.0), true),
        Scale::new(&(-10.0..=0.0), false)
    );
}

#[test]
fn a_key_steps_by_the_step_or_a_hundredth_of_the_track() {
    let scale = Scale::new(&(0.0..=200.0), false);
    assert_eq!(stepped(50.0, 3, scale, Some(5.0), 0), 65.0);
    assert_eq!(stepped(50.0, -1, scale, None, 0), 48.0);
    assert_eq!(stepped(199.0, 10, scale, Some(1.0), 0), 200.0);
    assert_eq!(stepped(1.0, -10, scale, None, 0), 0.0);
    assert_eq!(rounded(-0.0004, 2), 0.0);
    assert!(rounded(-0.0004, 2).is_sign_positive());
    assert_eq!(rounded(2.345_67, 2), 2.35);
}

#[test]
fn a_key_moves_a_value_that_rounding_would_hold_still() {
    // A hundredth of a short track is less than the last digit kept.
    let short = Scale::new(&(0.0..=10.0), false);
    assert_eq!(stepped(4.0, 1, short, None, 0), 5.0);
    assert_eq!(stepped(4.0, -1, short, None, 0), 3.0);
    // The small end of a logarithmic size track moves a whole pixel.
    let sizes = Scale::new(&(1.0..=1000.0), true);
    assert_eq!(stepped(1.0, 1, sizes, None, 0), 2.0);
    // At an end it stays there.
    assert_eq!(stepped(10.0, 1, short, None, 0), 10.0);
    assert_eq!(stepped(0.0, -1, short, None, 0), 0.0);
}

#[test]
fn a_rounded_value_stays_inside_the_range() {
    let scale = Scale::new(&(0.3..=10.0), false);
    assert_eq!(value_at(scale, 0.0, 0), 0.3, "0 would lie below the range");
    assert_eq!(value_at(scale, 1.0, 0), 10.0);
    assert_eq!(value_at(scale, 0.25, 0), 3.0);
}

#[test]
fn the_keys_move_a_short_slider_that_keeps_no_decimals() {
    let mut harness = Harness::new();
    let mut value = 4.0;
    let draw = |ui: &mut Ui, theme: &Theme, value: &mut f64| {
        slider("short", value, 0.0..=10.0)
            .field(false)
            .show_at(ui, place(), theme)
    };
    harness.frame(vec![], |ui, theme| draw(ui, theme, &mut value));
    harness.key(Key::Tab, |ui, theme| draw(ui, theme, &mut value));
    harness.frame(vec![], |ui, theme| draw(ui, theme, &mut value));
    harness.key(Key::ArrowRight, |ui, theme| draw(ui, theme, &mut value));
    assert_eq!(value, 5.0);
}

// ----- The slider ----------------------------------------------------------------

#[test]
fn a_color_slider_paints_its_stops_along_the_track_and_still_moves() {
    let stops = [Color32::RED, Color32::GREEN, Color32::BLUE];
    let draw = |ui: &mut Ui, theme: &Theme, value: &mut f64| {
        slider("colors", value, 0.0..=100.0)
            .name("Colors")
            .field(false)
            .gradient(&stops)
            .show_at(ui, place(), theme)
    };
    let mut harness = Harness::new();
    let mut value = 0.0;
    let (_, output) = harness.frame_output(vec![], |ui, theme| draw(ui, theme, &mut value));
    let mut colors = Vec::new();
    for clipped in &output.shapes {
        if let Shape::Mesh(mesh) = &clipped.shape {
            colors.extend(mesh.vertices.iter().map(|vertex| vertex.color));
        }
    }
    for stop in stops {
        assert!(colors.contains(&stop), "{stop:?} is painted: {colors:?}");
    }
    let track = bare_track();
    harness.click(along(track, 0.5), |ui, theme| draw(ui, theme, &mut value));
    assert_eq!(value, 50.0, "the track still takes the pointer");
}

#[test]
fn a_press_on_the_track_moves_the_value_there_and_a_drag_follows() {
    let mut harness = Harness::new();
    let mut value = 0.0;
    let track = bare_track();
    let response = harness.click(along(track, 0.25), |ui, theme| bare(ui, theme, &mut value));
    assert_eq!(value, 25.0);
    assert!(
        !response.changed(),
        "the change was in the frame of the press"
    );
    let response = harness.drag(
        along(track, 0.25),
        along(track, 0.75),
        Modifiers::NONE,
        |ui, theme| bare(ui, theme, &mut value),
    );
    assert_eq!(value, 75.0);
    assert!(response.drag_stopped());
    // Past the ends it stays at them.
    harness.drag(
        along(track, 0.5),
        along(track, 1.0) + vec2(300.0, 0.0),
        Modifiers::NONE,
        |ui, theme| bare(ui, theme, &mut value),
    );
    assert_eq!(value, 100.0);
}

#[test]
fn shift_slows_a_slider_drag_to_a_tenth() {
    let mut harness = Harness::new();
    let mut value = 50.0;
    let track = bare_track();
    let from = along(track, 0.5);
    harness.drag(
        from,
        from + vec2(track.width() * 0.4, 0.0),
        Modifiers::SHIFT,
        |ui, theme| bare(ui, theme, &mut value),
    );
    // The press itself lands where it is; the drag then moves a tenth.
    assert!((value - 54.0).abs() <= 1.0, "{value}");
}

#[test]
fn the_keys_move_a_slider_that_holds_focus() {
    let mut harness = Harness::new();
    let mut value = 50.0;
    let draw = |ui: &mut Ui, theme: &Theme, value: &mut f64| {
        slider("keys", value, 0.0..=100.0)
            .field(false)
            .step(2.0)
            .show_at(ui, place(), theme)
    };
    let response = harness.key(Key::Tab, |ui, theme| draw(ui, theme, &mut value));
    let response = if response.has_focus() {
        response
    } else {
        harness.frame(vec![], |ui, theme| draw(ui, theme, &mut value))
    };
    assert!(response.has_focus(), "Tab reaches the slider");
    for (key, expected) in [
        (Key::ArrowRight, 52.0),
        (Key::ArrowUp, 54.0),
        (Key::ArrowLeft, 52.0),
        (Key::ArrowDown, 50.0),
        (Key::PageUp, 70.0),
        (Key::PageDown, 50.0),
        (Key::End, 100.0),
        (Key::ArrowRight, 100.0),
        (Key::Home, 0.0),
    ] {
        let response = harness.key(key, |ui, theme| draw(ui, theme, &mut value));
        assert_eq!(value, expected, "{key:?}");
        assert_eq!(
            response.changed(),
            key != Key::ArrowRight || expected != 100.0
        );
        assert!(response.has_focus(), "the arrows do not move focus away");
    }
}

#[test]
fn the_field_of_a_slider_types_its_value() {
    let _english = sue_lang::scope("en");
    let mut harness = Harness::new();
    let mut value = 10.0;
    let draw = |ui: &mut Ui, theme: &Theme, value: &mut f64| {
        slider("typed", value, 0.0..=100.0)
            .label("Opacity")
            .suffix("%")
            .show_at(ui, place(), theme)
    };
    let field = place().right_center() - vec2(10.0, 0.0);
    harness.click(field, |ui, theme| draw(ui, theme, &mut value));
    harness.frame(vec![], |ui, theme| draw(ui, theme, &mut value));
    harness.frame(vec![typed("64")], |ui, theme| draw(ui, theme, &mut value));
    assert_eq!(value, 10.0);
    let response = harness.key(Key::Enter, |ui, theme| draw(ui, theme, &mut value));
    assert_eq!(value, 64.0);
    assert!(response.changed());
}

#[test]
fn a_logarithmic_slider_puts_the_geometric_middle_in_the_middle() {
    let mut harness = Harness::new();
    let mut value = 1.0;
    let draw = |ui: &mut Ui, theme: &Theme, value: &mut f64| {
        slider("size", value, 1.0..=100.0)
            .logarithmic()
            .decimals(1)
            .field(false)
            .show_at(ui, place(), theme)
    };
    harness.click(along(bare_track(), 0.5), |ui, theme| {
        draw(ui, theme, &mut value)
    });
    assert_eq!(value, 10.0);
}

#[test]
fn a_disabled_slider_takes_neither_the_pointer_nor_the_keys() {
    let mut harness = Harness::new();
    let mut value = 30.0;
    let draw = |ui: &mut Ui, theme: &Theme, value: &mut f64| {
        slider("off", value, 0.0..=100.0)
            .enabled(false)
            .show_at(ui, place(), theme)
    };
    let track = bare_track();
    harness.drag(
        along(track, 0.1),
        along(track, 0.9),
        Modifiers::NONE,
        |ui, theme| draw(ui, theme, &mut value),
    );
    harness.key(Key::Tab, |ui, theme| draw(ui, theme, &mut value));
    harness.key(Key::ArrowRight, |ui, theme| draw(ui, theme, &mut value));
    assert_eq!(value, 30.0);
}

#[test]
fn a_slider_tells_assistive_technology_its_name_and_value() {
    let mut harness = Harness::new();
    let mut value = 30.0;
    let (_, output) = harness.frame_output(
        vec![super::super::input::tests::press(Key::Tab, Modifiers::NONE)],
        |ui, theme| bare(ui, theme, &mut value),
    );
    let (_, later) = harness.frame_output(vec![], |ui, theme| bare(ui, theme, &mut value));
    let infos: Vec<&WidgetInfo> = widget_events(&output)
        .into_iter()
        .chain(widget_events(&later))
        .collect();
    let info = infos
        .iter()
        .find(|info| info.role == Role::Slider)
        .expect("focus reached the slider");
    assert_eq!(info.label.as_deref(), Some("Bare"));
    assert_eq!(info.value, Some(30.0));
}

// ----- The tall slider -----------------------------------------------------------

#[test]
fn a_tall_slider_is_most_at_the_top() {
    let mut harness = Harness::new();
    let mut value = 50.0;
    let lane = Rect::from_min_size(pos2(100.0, 100.0), vec2(32.0, 300.0));
    let draw = |ui: &mut Ui, theme: &Theme, value: &mut f64| {
        tall_slider("tall", value, 0.0..=100.0).show_at(ui, lane, theme)
    };
    let track = harness.frame(vec![], |ui, _| {
        TallGeometry::new(ui.painter(), lane, Density::Regular, false).track
    });
    let at = |share: f32| pos2(track.center().x, track.bottom() - share * track.height());
    harness.click(at(0.8), |ui, theme| draw(ui, theme, &mut value));
    assert!((value - 80.0).abs() <= 0.5, "{value}");
    harness.drag(at(0.8), at(0.2), Modifiers::NONE, |ui, theme| {
        draw(ui, theme, &mut value)
    });
    assert!((value - 20.0).abs() <= 0.5, "{value}");
    // Up is more for the keys too.
    let response = harness.key(Key::Tab, |ui, theme| draw(ui, theme, &mut value));
    assert!(response.has_focus());
    harness.key(Key::ArrowUp, |ui, theme| draw(ui, theme, &mut value));
    assert!(value > 20.0);
}

#[test]
fn the_opacity_track_of_a_tall_slider_sits_on_whole_pixels() {
    let mut harness = Harness::new();
    for scale in [1.0, 1.25, 1.5, 2.0] {
        harness.ctx.set_pixels_per_point(scale);
        let lane = Rect::from_min_size(pos2(10.3, 20.7), vec2(32.0, 211.1));
        let geometry = harness.frame(vec![], |ui, _| {
            assert_eq!(ui.painter().pixels_per_point(), scale);
            TallGeometry::new(ui.painter(), lane, Density::Regular, true)
        });
        for edge in [
            geometry.track.left(),
            geometry.track.right(),
            geometry.track.top(),
            geometry.track.bottom(),
        ] {
            let pixels = edge * scale;
            assert!((pixels - pixels.round()).abs() < 1e-3, "{scale}: {edge}");
        }
    }
}

// ----- The range -----------------------------------------------------------------

fn pressure(ui: &mut Ui, theme: &Theme, low: &mut f64, high: &mut f64) -> Response {
    range("pressure", low, high, 0.0..=100.0)
        .label("Pressure")
        .gap(10.0)
        .fields(false)
        .show_at(ui, place(), theme)
}

#[test]
fn a_range_moves_the_handle_nearer_the_press() {
    let mut harness = Harness::new();
    let (mut low, mut high) = (20.0, 80.0);
    let track = bare_track();
    harness.click(along(track, 0.7), |ui, theme| {
        pressure(ui, theme, &mut low, &mut high)
    });
    assert_eq!((low, high), (20.0, 70.0));
    harness.click(along(track, 0.3), |ui, theme| {
        pressure(ui, theme, &mut low, &mut high)
    });
    assert_eq!((low, high), (30.0, 70.0));
}

#[test]
fn the_handles_of_a_range_never_pass_each_other() {
    let mut harness = Harness::new();
    let (mut low, mut high) = (20.0, 80.0);
    let track = bare_track();
    let response = harness.drag(
        along(track, 0.2),
        along(track, 0.95),
        Modifiers::NONE,
        |ui, theme| pressure(ui, theme, &mut low, &mut high),
    );
    assert_eq!((low, high), (70.0, 80.0), "held the gap below the high end");
    assert!(response.changed() || low == 70.0);
    harness.drag(
        along(track, 0.8),
        along(track, 0.0),
        Modifiers::NONE,
        |ui, theme| pressure(ui, theme, &mut low, &mut high),
    );
    assert_eq!(
        (low, high),
        (70.0, 80.0),
        "the high handle stops at the gap"
    );
}

#[test]
fn two_handles_that_lie_together_part_the_way_the_pointer_goes() {
    let mut harness = Harness::new();
    let track = bare_track();
    let draw = |ui: &mut Ui, theme: &Theme, low: &mut f64, high: &mut f64| {
        range("together", low, high, 0.0..=100.0)
            .fields(false)
            .show_at(ui, place(), theme)
    };
    let (mut low, mut high) = (50.0, 50.0);
    harness.drag(
        along(track, 0.5),
        along(track, 0.8),
        Modifiers::NONE,
        |ui, theme| draw(ui, theme, &mut low, &mut high),
    );
    assert_eq!((low, high), (50.0, 80.0), "to the right: the high handle");
    let (mut low, mut high) = (50.0, 50.0);
    harness.drag(
        along(track, 0.5),
        along(track, 0.2),
        Modifiers::NONE,
        |ui, theme| draw(ui, theme, &mut low, &mut high),
    );
    assert_eq!((low, high), (20.0, 50.0), "to the left: the low handle");
    // Both at the low end: dragging right still moves one of them.
    let (mut low, mut high) = (0.0, 0.0);
    harness.drag(
        along(track, 0.0),
        along(track, 0.4),
        Modifiers::NONE,
        |ui, theme| draw(ui, theme, &mut low, &mut high),
    );
    assert_eq!((low, high), (0.0, 40.0));
}

#[test]
fn setting_one_end_of_a_range_keeps_the_gap_and_the_bounds() {
    let (mut low, mut high) = (20.0, 80.0);
    let mut range = range("set", &mut low, &mut high, 0.0..=100.0).gap(5.0);
    let scale = Scale::new(&(0.0..=100.0), false);
    range.set(false, 90.0, scale);
    assert_eq!((*range.low, *range.high), (75.0, 80.0));
    range.set(true, 200.0, scale);
    assert_eq!((*range.low, *range.high), (75.0, 100.0));
    range.set(true, 0.0, scale);
    assert_eq!((*range.low, *range.high), (75.0, 80.0));
    range.set(false, -5.0, scale);
    assert_eq!((*range.low, *range.high), (0.0, 80.0));
}

#[test]
fn each_handle_of_a_range_takes_the_keys_when_it_holds_focus() {
    let mut harness = Harness::new();
    let (mut low, mut high) = (20.0, 80.0);
    let draw = |ui: &mut Ui, theme: &Theme, low: &mut f64, high: &mut f64| {
        range("keys", low, high, 0.0..=100.0)
            .step(1.0)
            .fields(false)
            .show_at(ui, place(), theme)
    };
    // Tab reaches the low handle, then the high one.
    for _ in 0..2 {
        harness.frame(vec![], |ui, theme| draw(ui, theme, &mut low, &mut high));
    }
    harness.key(Key::Tab, |ui, theme| draw(ui, theme, &mut low, &mut high));
    harness.frame(vec![], |ui, theme| draw(ui, theme, &mut low, &mut high));
    harness.key(Key::ArrowRight, |ui, theme| {
        draw(ui, theme, &mut low, &mut high)
    });
    assert_eq!((low, high), (21.0, 80.0));
    harness.key(Key::Tab, |ui, theme| draw(ui, theme, &mut low, &mut high));
    harness.frame(vec![], |ui, theme| draw(ui, theme, &mut low, &mut high));
    harness.key(Key::End, |ui, theme| draw(ui, theme, &mut low, &mut high));
    assert_eq!((low, high), (21.0, 100.0));
    harness.key(Key::Home, |ui, theme| draw(ui, theme, &mut low, &mut high));
    assert_eq!((low, high), (21.0, 21.0), "it stops at the low handle");
}

// ----- The wedge -----------------------------------------------------------------

#[test]
fn a_wedge_is_set_where_it_is_pressed_and_follows_a_drag() {
    let mut harness = Harness::new();
    let mut value = 50.0;
    let rect = Rect::from_min_size(pos2(100.0, 100.0), vec2(200.0, 22.0));
    let draw = |ui: &mut Ui, theme: &Theme, value: &mut f64| {
        wedge("size", value, 0.0..=100.0).show_at(ui, rect, theme)
    };
    let shape = wedge_rect(rect);
    let at = |share: f32| pos2(shape.left() + share * shape.width(), shape.center().y);
    harness.click(at(0.1), |ui, theme| draw(ui, theme, &mut value));
    assert_eq!(value, 10.0);
    harness.drag(
        at(0.1),
        at(1.0) + vec2(50.0, 0.0),
        Modifiers::NONE,
        |ui, theme| draw(ui, theme, &mut value),
    );
    assert_eq!(value, 100.0);
}

// ----- Every component, laid out --------------------------------------------------

#[test]
fn every_slider_fits_its_density_and_the_review_sheets_draw() {
    let mut harness = Harness::new();
    for density in Density::ALL {
        let heights = harness.frame(vec![], |ui, theme| {
            density.scope(ui, |ui| {
                let (mut one, mut low, mut high) = (3.0, 1.0, 2.0);
                [
                    slider("slider", &mut one, 0.0..=10.0)
                        .label("Size")
                        .show(ui, theme)
                        .rect
                        .height(),
                    range("range", &mut low, &mut high, 0.0..=10.0)
                        .show(ui, theme)
                        .rect
                        .height(),
                ]
            })
        });
        for height in heights {
            assert_eq!(height, density.control_height(), "{density:?}");
        }
    }
    for page in Sheet::ALL {
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0));
        harness.frame(vec![], |ui, theme| sheet(ui.painter(), rect, theme, page));
        assert!(!page.name().is_empty());
    }
}
