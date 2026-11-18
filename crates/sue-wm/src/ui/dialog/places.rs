//! Where each dialog was last put and how big it was made, kept across
//! openings and, through the rice (`display.dialog_places`), across runs.
//!
//! A dialog is known by its id's hash, which stays the same from run to
//! run. A place is the dialog's top left corner and, once someone resized
//! it, its size, in points: on the screen for a dialog in a window of its
//! own, in the window for one drawn inside it. The text the rice keeps
//! reads `id:x,y` or `id:x,y,w,h` for each, separated by semicolons.

use std::collections::BTreeMap;

use egui::{Context, Id, Pos2, Vec2, pos2, vec2};

/// The rice key that keeps the places between runs.
pub const PLACES_KEY: &str = "display.dialog_places";

/// How many dialogs' places are kept: the ones moved last stay.
const KEPT: usize = 64;

/// How long the places rest unchanged before they are written to the
/// rice, in seconds: once a move or a resize is over, not every frame of
/// it.
const SETTLE: f64 = 1.0;

/// Where a dialog was put, and its size once it was resized.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    /// Its top left corner.
    pub at: Pos2,
    /// Its size, when someone made it another than it starts with.
    pub size: Option<Vec2>,
}

/// Every dialog's place, and since when they changed unwritten.
#[derive(Clone, Debug, Default)]
struct Places {
    map: BTreeMap<u64, (u64, Place)>,
    /// Bumped by every change, to order the places by how recent.
    stamp: u64,
    /// The time of the last change not yet written, if any.
    unwritten: Option<f64>,
    /// The rice's text was read.
    loaded: bool,
}

fn store() -> Id {
    Id::NULL.with("sue-dialog-places")
}

fn with<R>(ctx: &Context, f: impl FnOnce(&mut Places) -> R) -> R {
    ctx.data_mut(|data| f(data.get_temp_mut_or_default::<Places>(store())))
}

/// Where the dialog `id` was last put, if it was.
#[must_use]
pub fn remembered(ctx: &Context, id: Id) -> Option<Place> {
    with(ctx, |places| {
        places.map.get(&id.value()).map(|(_, place)| *place)
    })
}

/// Keeps `place` for the dialog `id`. A change of less than a point is no
/// change.
pub fn remember(ctx: &Context, id: Id, place: Place) {
    let now = ctx.input(|input| input.time);
    with(ctx, |places| {
        let same = places.map.get(&id.value()).is_some_and(|(_, kept)| {
            kept.at.distance(place.at) < 1.0
                && match (kept.size, place.size) {
                    (Some(a), Some(b)) => (a - b).length() < 1.0,
                    (None, None) => true,
                    _ => false,
                }
        });
        if same {
            return;
        }
        places.stamp += 1;
        let stamp = places.stamp;
        places.map.insert(id.value(), (stamp, place));
        while places.map.len() > KEPT {
            let oldest = places
                .map
                .iter()
                .min_by_key(|(_, (stamp, _))| *stamp)
                .map(|(key, _)| *key);
            match oldest {
                Some(key) => places.map.remove(&key),
                None => break,
            };
        }
        places.unwritten = Some(now);
    });
}

/// Reads the places from the rice's text, once: later calls do nothing,
/// so what the dialogs keep in the meantime stays.
pub fn load(ctx: &Context, text: &str) {
    with(ctx, |places| {
        if places.loaded {
            return;
        }
        places.loaded = true;
        for (stamp, item) in text.split(';').enumerate() {
            if let Some((key, place)) = parse(item) {
                places.map.insert(key, (stamp as u64, place));
                places.stamp = places.stamp.max(stamp as u64);
            }
        }
    });
}

/// The places as the rice keeps them, when they changed and have rested
/// long enough to be written; `None` otherwise. Once given, they count as
/// written.
#[must_use]
pub fn take_settled(ctx: &Context) -> Option<String> {
    let now = ctx.input(|input| input.time);
    let busy = ctx.input(|input| input.pointer.any_down());
    let (written, wait) = with(ctx, |places| match places.unwritten {
        None => (None, false),
        Some(since) if busy || now - since < SETTLE => (None, true),
        Some(_) => {
            places.unwritten = None;
            (Some(text(places)), false)
        }
    });
    if wait {
        // Asked again once it has rested.
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(SETTLE));
    }
    written
}

fn text(places: &Places) -> String {
    let mut items: Vec<(u64, u64, Place)> = places
        .map
        .iter()
        .map(|(key, (stamp, place))| (*key, *stamp, *place))
        .collect();
    items.sort_by_key(|(_, stamp, _)| *stamp);
    items
        .iter()
        .map(|(key, _, place)| {
            let at = format!("{key:016x}:{},{}", place.at.x.round(), place.at.y.round());
            match place.size {
                Some(size) => format!("{at},{},{}", size.x.round(), size.y.round()),
                None => at,
            }
        })
        .collect::<Vec<_>>()
        .join(";")
}

fn parse(item: &str) -> Option<(u64, Place)> {
    let (key, rest) = item.trim().split_once(':')?;
    let key = u64::from_str_radix(key, 16).ok()?;
    let numbers: Vec<f32> = rest
        .split(',')
        .map(|part| part.trim().parse::<f32>())
        .collect::<Result<_, _>>()
        .ok()?;
    if numbers.iter().any(|number| !number.is_finite()) {
        return None;
    }
    let place = match numbers.as_slice() {
        [x, y] => Place {
            at: pos2(*x, *y),
            size: None,
        },
        [x, y, w, h] if *w > 0.0 && *h > 0.0 => Place {
            at: pos2(*x, *y),
            size: Some(vec2(*w, *h)),
        },
        _ => return None,
    };
    Some((key, place))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(ctx: &Context, time: f64, f: impl FnOnce(&Context)) {
        let input = egui::RawInput {
            time: Some(time),
            ..Default::default()
        };
        let mut f = Some(f);
        ctx.run_ui(input, |ui| {
            if let Some(f) = f.take() {
                f(ui.ctx());
            }
        })
        .textures_delta
        .clear();
    }

    #[test]
    fn places_are_kept_written_once_settled_and_read_back() {
        let ctx = Context::default();
        let new = Id::unique("New");
        let export = Id::unique("Export");
        frame(&ctx, 1.0, |ctx| {
            load(ctx, "");
            remember(
                ctx,
                new,
                Place {
                    at: pos2(120.0, 80.0),
                    size: None,
                },
            );
            remember(
                ctx,
                export,
                Place {
                    at: pos2(-300.0, 40.0),
                    size: Some(vec2(500.0, 420.0)),
                },
            );
            assert_eq!(take_settled(ctx), None, "not before it rests");
        });
        let mut written = None;
        frame(&ctx, 2.5, |ctx| written = take_settled(ctx));
        let written = written.expect("written once it rested");
        assert!(written.contains(":120,80;"), "{written}");
        assert!(written.ends_with(":-300,40,500,420"), "{written}");
        frame(&ctx, 3.0, |ctx| assert_eq!(take_settled(ctx), None, "once"));

        // Another run reads them back.
        let next = Context::default();
        frame(&next, 1.0, |ctx| {
            load(ctx, &format!("{written};junk;1:2"));
            assert_eq!(
                remembered(ctx, export),
                Some(Place {
                    at: pos2(-300.0, 40.0),
                    size: Some(vec2(500.0, 420.0)),
                })
            );
            assert_eq!(
                remembered(ctx, new).map(|place| place.at),
                Some(pos2(120.0, 80.0))
            );
            // Read once: a later load leaves what is kept.
            load(ctx, "");
            assert!(remembered(ctx, new).is_some());
        });
    }
}
