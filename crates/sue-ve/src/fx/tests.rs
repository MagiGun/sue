//! Every effect against what it should do, the effect stack's model, its
//! keyframes and presets.

use super::pixels::{Ctx, Image, apply};
use super::*;
use sue_core::EntityId;
use sue_model::{Interpolation, Key};

fn flat(width: u32, height: u32, rgba: [u8; 4]) -> Image {
    Image::new(width, height, rgba.repeat((width * height) as usize))
}

fn defaults(kind: &str) -> Params {
    let mut params = Params::default();
    for param in &super::kind(kind).expect("a registered kind").params {
        params.values.set(param.key, param.default_value());
    }
    params
}

fn run(kind: &str, params: &Params, image: &Image) -> Image {
    let mut out = image.clone();
    apply(kind, params, &mut out, &Ctx::default());
    out
}

fn px(image: &Image, x: u32, y: u32) -> [u8; 4] {
    image.at(i64::from(x), i64::from(y))
}

/// Left half black, right half white, opaque.
fn edge(width: u32, height: u32) -> Image {
    let mut rgba = Vec::new();
    for _ in 0..height {
        for x in 0..width {
            let v = if x < width / 2 { 0 } else { 255 };
            rgba.extend_from_slice(&[v, v, v, 255]);
        }
    }
    Image::new(width, height, rgba)
}

/// A white square in the middle of a clear picture.
fn square(side: u32, inner: u32) -> Image {
    let mut image = Image::clear(side, side);
    let from = (side - inner) / 2;
    for y in from..from + inner {
        for x in from..from + inner {
            let at = ((y * side + x) * 4) as usize;
            image.rgba[at..at + 4].copy_from_slice(&[255, 255, 255, 255]);
        }
    }
    image
}

#[test]
fn every_builtin_kind_has_a_name_and_runs_on_any_picture() {
    let picture = super::pixels::tests_card();
    for kind in kinds() {
        assert!(!kind.label().is_empty(), "{}", kind.id);
        for param in &kind.params {
            assert!(!param.label().is_empty(), "{}.{}", kind.id, param.key);
        }
        let out = run(kind.id, &defaults(kind.id), &picture);
        assert_eq!((out.width, out.height), (picture.width, picture.height));
        // A one pixel picture never panics either.
        let _ = run(kind.id, &defaults(kind.id), &flat(1, 1, [9, 9, 9, 255]));
    }
}

#[test]
fn blurs_soften_an_edge_and_keep_flat_areas() {
    let picture = edge(64, 16);
    let blurred = run(
        "blur.gaussian",
        &defaults("blur.gaussian").with("radius", 4.0),
        &picture,
    );
    assert_eq!(px(&blurred, 2, 8)[0], 0, "far from the edge it stays");
    let middle = px(&blurred, 32, 8)[0];
    assert!((90..170).contains(&middle), "{middle}");
    // Directional at 0 degrees smears across and not down.
    let lines = {
        let mut image = flat(32, 32, [0, 0, 0, 255]);
        for x in 0..32 {
            let at = ((16 * 32 + x) * 4) as usize;
            image.rgba[at..at + 3].copy_from_slice(&[255, 255, 255]);
        }
        image
    };
    let smeared = run(
        "blur.directional",
        &defaults("blur.directional")
            .with("angle", 0.0)
            .with("length", 10.0),
        &lines,
    );
    assert_eq!(px(&smeared, 16, 16)[0], 255, "along the line it stays");
    assert_eq!(px(&smeared, 16, 12)[0], 0, "nothing spreads across it");
    // Sharpening pushes the two sides of the edge apart.
    let soft = run(
        "blur.gaussian",
        &defaults("blur.gaussian").with("radius", 2.0),
        &picture,
    );
    let sharp = run("blur.sharpen", &defaults("blur.sharpen"), &soft);
    let (before, after) = (px(&soft, 30, 8)[0], px(&sharp, 30, 8)[0]);
    assert!(after < before, "{after} < {before}");
}

#[test]
fn color_corrections_match_sues_one_implementation() {
    let picture = super::pixels::tests_card();
    let params = defaults("color.brightness_contrast")
        .with("brightness", 30.0)
        .with("contrast", 20.0);
    let out = run("color.brightness_contrast", &params, &picture);
    let source = px(&picture, 40, 10);
    let rgb = [source[0], source[1], source[2]].map(|c| f32::from(c) / 255.0);
    let want = sue_io::picture_ops::brightness_contrast(rgb, 0.3, 0.2);
    let got = px(&out, 40, 10);
    for channel in 0..3 {
        let want = (want[channel].clamp(0.0, 1.0) * 255.0).round();
        assert!(
            (f32::from(got[channel]) - want).abs() <= 1.0,
            "{got:?} {want}"
        );
    }
    // Hue turned half way round: red becomes cyan.
    let red = flat(4, 4, [255, 0, 0, 255]);
    let turned = run(
        "color.hue_saturation",
        &defaults("color.hue_saturation").with("hue", 180.0),
        &red,
    );
    let cyan = px(&turned, 1, 1);
    assert!(cyan[0] < 5 && cyan[1] > 250 && cyan[2] > 250, "{cyan:?}");
    // Desaturated fully: gray.
    let gray = run(
        "color.hue_saturation",
        &defaults("color.hue_saturation").with("saturation", -100.0),
        &flat(2, 2, [200, 50, 50, 255]),
    );
    let g = px(&gray, 0, 0);
    assert!(g[0] == g[1] && g[1] == g[2], "{g:?}");
}

#[test]
fn levels_curves_and_balance_follow_their_formulas() {
    let ramp = {
        let mut image = Image::clear(256, 1);
        for x in 0..256 {
            image.rgba[x * 4..x * 4 + 4].copy_from_slice(&[x as u8, x as u8, x as u8, 255]);
        }
        image
    };
    let leveled = run(
        "color.levels",
        &defaults("color.levels")
            .with("in_black", 50.0)
            .with("in_white", 200.0),
        &ramp,
    );
    assert_eq!(px(&leveled, 40, 0)[0], 0);
    assert_eq!(px(&leveled, 210, 0)[0], 255);
    let mid = f32::from(px(&leveled, 125, 0)[0]);
    assert!((mid - 127.5).abs() <= 2.0, "{mid}");
    // An inverted curve inverts; the straight one leaves it.
    let mut params = defaults("color.curves");
    params
        .values
        .set("rgb", curve_value(&[[0.0, 1.0], [1.0, 0.0]]));
    let inverted = run("color.curves", &params, &ramp);
    assert!(px(&inverted, 0, 0)[0] >= 250 && px(&inverted, 255, 0)[0] <= 5);
    assert_eq!(run("color.curves", &defaults("color.curves"), &ramp), ramp);
    // Balance: nothing moves at zero; a red shift in the midtones reddens
    // middle gray.
    assert_eq!(
        run("color.balance", &defaults("color.balance"), &ramp),
        ramp
    );
    let warm = run(
        "color.balance",
        &defaults("color.balance").with("mid_red", 60.0),
        &ramp,
    );
    let at = px(&warm, 128, 0);
    assert!(at[0] > at[2] + 10, "{at:?}");
}

#[test]
fn glow_shadow_and_stroke_reach_past_what_is_drawn() {
    // Glow: light round the bright square, nothing on a dark picture.
    let mut lit = flat(64, 64, [0, 0, 0, 255]);
    for y in 24..40 {
        for x in 24..40 {
            let at = ((y * 64 + x) * 4) as usize;
            lit.rgba[at..at + 3].copy_from_slice(&[255, 255, 255]);
        }
    }
    let glowing = run(
        "stylize.glow",
        &defaults("stylize.glow").with("radius", 6.0),
        &lit,
    );
    assert!(px(&glowing, 20, 32)[0] > 20, "{:?}", px(&glowing, 20, 32));
    let dark = flat(16, 16, [20, 20, 20, 255]);
    assert_eq!(run("stylize.glow", &defaults("stylize.glow"), &dark), dark);
    // Drop shadow down and right, under a clear background.
    let shape = square(64, 16);
    let shadowed = run(
        "stylize.drop_shadow",
        &defaults("stylize.drop_shadow")
            .with("angle", 45.0)
            .with("distance", 8.0)
            .with("softness", 0.0)
            .with("opacity", 100.0),
        &shape,
    );
    assert!(
        px(&shadowed, 44, 44)[3] > 200,
        "{:?}",
        px(&shadowed, 44, 44)
    );
    assert_eq!(px(&shadowed, 20, 20)[3], 0);
    // Stroke: a ring of its width round the square.
    let stroked = run(
        "stylize.stroke",
        &defaults("stylize.stroke").with("width", 4.0),
        &shape,
    );
    assert_eq!(px(&stroked, 22, 32)[3], 255, "inside the ring");
    assert_eq!(px(&stroked, 16, 32)[3], 0, "past it");
    assert_eq!(
        px(&stroked, 32, 32),
        [255, 255, 255, 255],
        "the square stays"
    );
}

#[test]
fn vignette_grain_and_mosaic() {
    let gray = flat(64, 48, [128, 128, 128, 255]);
    let vignetted = run(
        "stylize.vignette",
        &defaults("stylize.vignette").with("amount", 100.0),
        &gray,
    );
    assert_eq!(px(&vignetted, 32, 24)[0], 128, "the middle stays");
    assert!(px(&vignetted, 0, 0)[0] < 30, "the corners darken");
    let lighter = run(
        "stylize.vignette",
        &defaults("stylize.vignette").with("amount", -100.0),
        &gray,
    );
    assert!(px(&lighter, 0, 0)[0] > 220);
    // Grain moves each frame when animated, keeps its mean.
    let params = defaults("stylize.grain").with("amount", 40.0);
    let mut one = gray.clone();
    apply(
        "stylize.grain",
        &params,
        &mut one,
        &Ctx {
            scale: 1.0,
            frame: 1,
        },
    );
    let mut two = gray.clone();
    apply(
        "stylize.grain",
        &params,
        &mut two,
        &Ctx {
            scale: 1.0,
            frame: 2,
        },
    );
    assert_ne!(one, two);
    let mean: f64 = one
        .rgba
        .iter()
        .step_by(4)
        .map(|v| f64::from(*v))
        .sum::<f64>()
        / (64.0 * 48.0);
    assert!((mean - 128.0).abs() < 3.0, "{mean}");
    let mut still = params.clone();
    still.values.set("animated", false);
    let mut a = gray.clone();
    apply(
        "stylize.grain",
        &still,
        &mut a,
        &Ctx {
            scale: 1.0,
            frame: 1,
        },
    );
    let mut b = gray.clone();
    apply(
        "stylize.grain",
        &still,
        &mut b,
        &Ctx {
            scale: 1.0,
            frame: 9,
        },
    );
    assert_eq!(a, b);
    // Mosaic: blocks of one color.
    let tiles = run(
        "stylize.mosaic",
        &defaults("stylize.mosaic").with("size", 8.0),
        &super::pixels::tests_card(),
    );
    assert_eq!(px(&tiles, 8, 8), px(&tiles, 15, 15));
}

#[test]
fn keys_take_out_green_and_the_dark() {
    let mut picture = flat(4, 1, [0, 177, 64, 255]);
    picture.rgba[4..8].copy_from_slice(&[200, 30, 40, 255]);
    picture.rgba[8..12].copy_from_slice(&[120, 200, 110, 255]);
    let keyed = run("key.chroma", &defaults("key.chroma"), &picture);
    assert_eq!(px(&keyed, 0, 0)[3], 0, "the screen goes");
    assert_eq!(px(&keyed, 1, 0)[3], 255, "red stays");
    let spill = px(&keyed, 2, 0);
    assert!(spill[1] < 200, "green spill pulled down: {spill:?}");
    let mut luma = flat(2, 1, [10, 10, 10, 255]);
    luma.rgba[4..8].copy_from_slice(&[250, 250, 250, 255]);
    let keyed = run("key.luma", &defaults("key.luma"), &luma);
    assert_eq!((px(&keyed, 0, 0)[3], px(&keyed, 1, 0)[3]), (0, 255));
    let mut bright = defaults("key.luma");
    bright.values.set("side", Value::Int(1));
    let keyed = run("key.luma", &bright, &luma);
    assert_eq!((px(&keyed, 0, 0)[3], px(&keyed, 1, 0)[3]), (255, 0));
}

#[test]
fn crop_and_mirror() {
    let picture = super::pixels::tests_card();
    let cropped = run(
        "transform.crop",
        &defaults("transform.crop").with("left", 25.0),
        &picture,
    );
    let quarter = picture.width / 4;
    assert_eq!(px(&cropped, quarter - 2, 20)[3], 0);
    assert_eq!(px(&cropped, quarter + 2, 20), px(&picture, quarter + 2, 20));
    let mirrored = run("transform.mirror", &defaults("transform.mirror"), &picture);
    let w = picture.width;
    assert_eq!(px(&mirrored, w - 1 - 5, 7), px(&picture, 5, 7));
    let mut flip = defaults("transform.mirror");
    flip.values.set("mode", Value::Int(5));
    let flipped = run("transform.mirror", &flip, &picture);
    assert_eq!(px(&flipped, 3, picture.height - 1), px(&picture, 3, 0));
}

#[test]
fn a_blur_scales_with_the_picture_so_a_preview_looks_as_the_export_does() {
    let big = edge(128, 8);
    let small = edge(64, 8);
    let params = defaults("blur.gaussian").with("radius", 8.0);
    let mut full = big;
    apply(
        "blur.gaussian",
        &params,
        &mut full,
        &Ctx {
            scale: 1.0,
            frame: 0,
        },
    );
    let mut half = small;
    apply(
        "blur.gaussian",
        &params,
        &mut half,
        &Ctx {
            scale: 0.5,
            frame: 0,
        },
    );
    for x in (0..64).step_by(7) {
        let (a, b) = (
            i32::from(px(&full, x * 2, 4)[0]),
            i32::from(px(&half, x, 4)[0]),
        );
        assert!((a - b).abs() <= 24, "at {x}: {a} {b}");
    }
}

fn clip() -> Clip {
    Clip::new(EntityId::new(), Time::ZERO, Time::from_seconds(4.0))
}

#[test]
fn a_stack_adds_reorders_removes_and_round_trips() {
    let mut clip = clip();
    let blur = add(&mut clip, "blur.gaussian");
    let glow = add(&mut clip, "stylize.glow");
    assert_eq!((blur, glow), (1, 2));
    let list = effects(&clip);
    assert_eq!(list[0].kind, "blur.gaussian");
    assert_eq!(
        list[0].params.get("radius").and_then(Value::as_f64),
        Some(10.0)
    );
    assert!(reorder(&mut clip, glow, -1));
    assert_eq!(effects(&clip)[0].id, glow);
    assert!(update(&mut clip, blur, |effect| effect.enabled = false));
    assert!(!effects(&clip)[1].enabled);
    clip.animation
        .channel_mut(&channel(blur, "radius"))
        .insert(Key {
            time: Time::ZERO,
            value: 1.0,
            interpolation: Interpolation::Linear,
        });
    assert!(remove(&mut clip, blur));
    assert!(
        clip.animation.channel(&channel(blur, "radius")).is_none(),
        "its keys go with it"
    );
    assert_eq!(effects(&clip).len(), 1);
    // A kind this build does not know stays, untouched.
    let mut list = effects(&clip);
    list.push(Effect {
        id: 9,
        kind: "future.thing".to_owned(),
        enabled: true,
        params: Properties::new().with("x", 1.0),
    });
    set_effects(&mut clip, &list);
    assert_eq!(effects(&clip)[1].kind, "future.thing");
    assert_eq!(
        Effect::from_value(&list[1].to_value()).as_ref(),
        Some(&list[1])
    );
}

#[test]
fn parameters_follow_their_keyframes_with_eases() {
    let mut clip = clip();
    clip.source_in = Time::from_seconds(10.0);
    let id = add(&mut clip, "blur.gaussian");
    let effect = effects(&clip)[0].clone();
    assert_eq!(effect.number(&clip, "radius", Time::ZERO), 10.0);
    let curve = clip.animation.channel_mut(&channel(id, "radius"));
    curve.insert(Key {
        time: Time::from_seconds(10.0),
        value: 0.0,
        interpolation: Interpolation::Bezier {
            out: [0.66, 0.0],
            incoming: [-0.66, 0.0],
        },
    });
    curve.insert(Key {
        time: Time::from_seconds(12.0),
        value: 100.0,
        interpolation: Interpolation::Linear,
    });
    let at = |seconds: f64| effect.number(&clip, "radius", Time::from_seconds(seconds));
    assert!(
        (at(1.0) - 50.0).abs() < 1e-6,
        "eased both ways, half way is half"
    );
    assert!(at(0.25) < 12.5, "eased in starts slow: {}", at(0.25));
    assert_eq!(at(3.0), 100.0);
    assert_eq!(effect.at(&clip, Time::from_seconds(1.0)).f("radius"), 50.0);
}

#[test]
fn presets_carry_effects_and_keys_to_another_clip() {
    let mut from = clip();
    from.source_in = Time::from_seconds(5.0);
    let id = add(&mut from, "stylize.vignette");
    from.animation
        .channel_mut(&channel(id, "amount"))
        .insert(Key {
            time: Time::from_seconds(6.0),
            value: 80.0,
            interpolation: Interpolation::Linear,
        });
    let made = preset::capture(&from, "Dark edges");
    assert_eq!(preset::key_times(&made), [Time::SECOND]);
    let file = preset::to_file(&made);
    let read = preset::from_file(&file).unwrap();
    assert_eq!(read, made);
    assert!(preset::from_file(b"something else").is_err());
    let mut to = clip();
    add(&mut to, "blur.gaussian");
    assert_eq!(preset::apply(&read, &mut to), 1);
    let list = effects(&to);
    assert_eq!(list[1].kind, "stylize.vignette");
    let key = to
        .animation
        .channel(&channel(list[1].id, "amount"))
        .unwrap()
        .keys()[0];
    assert_eq!((key.time, key.value), (Time::SECOND, 80.0));
}

#[test]
fn search_finds_by_name_and_category() {
    let found: Vec<&str> = search("blur").iter().map(|kind| kind.id).collect();
    assert!(found.contains(&"blur.gaussian") && found.contains(&"blur.directional"));
    assert!(search("key").iter().any(|kind| kind.id == "key.chroma"));
    assert_eq!(search("").len(), kinds().len());
    register(EffectKind {
        id: "test.nothing",
        category: Category::Stylize,
        params: Vec::new(),
    });
    assert!(kind("test.nothing").is_some());
}
