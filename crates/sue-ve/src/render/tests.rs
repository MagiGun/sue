use super::*;
use sue_core::Rate;

fn s(seconds: f64) -> Time {
    Time::from_seconds(seconds)
}

/// V2 over V1: two clips on V1 meeting at 2 s, one on V2 from 3 s.
fn edit() -> (Sequence, [EntityId; 2], [u64; 3]) {
    let (a, b) = (EntityId::new(), EntityId::new());
    let mut sequence = model::empty_sequence(64, 36, Rate::FPS_25, 2, 1);
    let mut clips = [
        Clip::new(a, Time::ZERO, s(2.0)),
        Clip::new(b, s(2.0), s(2.0)),
        Clip::new(a, s(3.0), s(1.0)),
    ];
    for (index, clip) in clips.iter_mut().enumerate() {
        clip.id = index as u64 + 1;
    }
    clips[2].properties.set(key::OPACITY, 0.5);
    let [one, two, three] = clips;
    sequence.tracks[1].insert(one);
    sequence.tracks[1].insert(two);
    sequence.tracks[0].insert(three);
    sequence.next_id = 10;
    (sequence, [a, b], [1, 2, 3])
}

fn movie(_: EntityId) -> SourceKind {
    SourceKind::Movie
}

#[test]
fn layers_stack_bottom_first_with_their_settings() {
    let (sequence, [a, b], _) = edit();
    let at = layers_at(&sequence, s(1.0), &movie);
    assert_eq!(at.len(), 1);
    assert_eq!((at[0].source, at[0].source_time), (a, s(1.0)));
    let at = layers_at(&sequence, s(3.5), &movie);
    assert_eq!(at.len(), 2);
    assert_eq!(at[0].source, b, "V1 first, under V2");
    assert_eq!(at[1].opacity, 0.5);
    // A hidden track shows nothing; a clip turned off neither.
    let mut hidden = sequence.clone();
    hidden.tracks[0].muted = true;
    assert_eq!(layers_at(&hidden, s(3.5), &movie).len(), 1);
    let mut off = sequence;
    off.tracks[1].clip_mut(2).unwrap().enabled = false;
    assert_eq!(layers_at(&off, s(2.5), &movie).len(), 0);
}

#[test]
fn a_dissolve_mixes_and_a_dip_goes_through_its_color() {
    let (mut sequence, [a, b], ids) = edit();
    sequence.tracks[1].transitions.push(Transition::new(
        "dissolve",
        s(1.5),
        s(1.0),
        ids[0],
        ids[1],
    ));
    let half = layers_at(&sequence, s(2.0), &movie);
    assert_eq!(half.len(), 2);
    assert_eq!((half[0].source, half[0].opacity), (a, 1.0));
    assert_eq!(
        half[0].source_time,
        s(2.0),
        "the outgoing clip runs on into its handle"
    );
    assert_eq!((half[1].source, half[1].opacity), (b, 0.5));
    sequence.tracks[1].transitions[0].kind = "dip-black".into();
    let early = layers_at(&sequence, s(1.75), &movie);
    assert_eq!(early.len(), 1);
    assert_eq!(early[0].source, a);
    assert_eq!(early[0].tint, Some(([0, 0, 0], 0.5)));
    let late = layers_at(&sequence, s(2.25), &movie);
    assert_eq!(late[0].source, b);
    // A fade in from nothing at the start.
    sequence.tracks[1].transitions =
        vec![Transition::new("dissolve", Time::ZERO, s(1.0), 0, ids[0])];
    assert!((layers_at(&sequence, s(0.25), &movie)[0].opacity - 0.25).abs() < 1e-6);
}

fn flat(width: u32, height: u32, rgba: [u8; 4]) -> Arc<VideoFrame> {
    Arc::new(VideoFrame::new(
        width,
        height,
        rgba.repeat((width * height) as usize),
        0,
    ))
}

fn layer(opacity: f32) -> Layer {
    Layer {
        track: 0,
        clip: 1,
        source: EntityId::new(),
        kind: SourceKind::Movie,
        source_time: Time::ZERO,
        opacity,
        scale: 1.0,
        offset: [0.0, 0.0],
        rotation: 0.0,
        tint: None,
    }
}

fn pixel(picture: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let at = ((y * width + x) * 4) as usize;
    [
        picture[at],
        picture[at + 1],
        picture[at + 2],
        picture[at + 3],
    ]
}

#[test]
fn the_compositor_fits_blends_moves_and_turns() {
    let frame = (40, 20);
    // A picture of the frame's size fills it.
    let red = composite(
        frame,
        &[Placed {
            layer: layer(1.0),
            image: flat(40, 20, [200, 0, 0, 255]),
        }],
    );
    assert_eq!(pixel(&red, 40, 0, 0), [200, 0, 0, 255]);
    assert_eq!(pixel(&red, 40, 39, 19), [200, 0, 0, 255]);
    // Half opaque blue over it: half way.
    let both = composite(
        frame,
        &[
            Placed {
                layer: layer(1.0),
                image: flat(40, 20, [200, 0, 0, 255]),
            },
            Placed {
                layer: layer(0.5),
                image: flat(40, 20, [0, 0, 200, 255]),
            },
        ],
    );
    assert_eq!(pixel(&both, 40, 10, 10), [100, 0, 100, 255]);
    // A square picture fits the height, black at the sides.
    let square = composite(
        frame,
        &[Placed {
            layer: layer(1.0),
            image: flat(10, 10, [0, 255, 0, 255]),
        }],
    );
    assert_eq!(pixel(&square, 40, 20, 10), [0, 255, 0, 255]);
    assert_eq!(pixel(&square, 40, 2, 10), [0, 0, 0, 255]);
    // Half size, moved right by 10: covers x 15 to 35.
    let mut small = layer(1.0);
    small.scale = 0.5;
    small.offset = [10.0, 0.0];
    let moved = composite(
        frame,
        &[Placed {
            layer: small,
            image: flat(40, 20, [255, 255, 255, 255]),
        }],
    );
    assert_eq!(pixel(&moved, 40, 25, 10)[0], 255);
    assert_eq!(pixel(&moved, 40, 12, 10)[0], 0);
    // Turned a quarter: a wide bar stands up.
    let mut turned = layer(1.0);
    turned.rotation = 90.0;
    turned.scale = 0.4;
    let stood = composite(
        frame,
        &[Placed {
            layer: turned,
            image: flat(40, 4, [255, 255, 255, 255]),
        }],
    );
    assert_eq!(pixel(&stood, 40, 20, 3)[0], 255, "above the middle");
    assert_eq!(pixel(&stood, 40, 10, 10)[0], 0, "not across");
    // A dip half way to white.
    let mut dipped = layer(1.0);
    dipped.tint = Some(([255, 255, 255], 0.5));
    let washed = composite(
        frame,
        &[Placed {
            layer: dipped,
            image: flat(40, 20, [0, 0, 0, 255]),
        }],
    );
    assert_eq!(pixel(&washed, 40, 5, 5), [128, 128, 128, 255]);
}

#[test]
fn corners_follow_the_frame_the_scale_and_the_turn() {
    let quad = corners(&layer(1.0), (1920, 1080), (1920, 1080));
    assert_eq!(quad[0], [0.0, 0.0]);
    assert_eq!(quad[2], [1920.0, 1080.0]);
    let mut turned = layer(1.0);
    turned.rotation = 180.0;
    let quad = corners(&turned, (100, 100), (200, 100));
    assert!((quad[0][0] - 150.0).abs() < 1e-3 && (quad[0][1] - 100.0).abs() < 1e-3);
}
