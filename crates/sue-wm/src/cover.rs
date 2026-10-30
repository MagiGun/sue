//! What covers what: menus, popups, dialogs and sheets lie over a page,
//! and what is under them must not draw or take the pointer.
//!
//! egui's own hit test already keeps a widget from reacting under a higher
//! layer, but much of Sue reads the pointer straight from egui's events (a
//! canvas's strokes and pinches, a palette's drags) and never asks it. Those
//! ask [`covered`] instead: a position is covered when a layer above the
//! asker's lies there (egui's areas: popups, dialogs, the Sue menu), or a
//! float painted straight onto a page (a phone's sheets and menus) said so
//! with [`cover`] this frame or the last.

use egui::{Context, Id, LayerId, Pos2, Rect};

/// The floats painted onto pages this frame and the last, as layer and
/// rectangle.
#[derive(Clone, Debug, Default)]
struct Covers {
    now: Vec<(LayerId, Rect)>,
    last: Vec<(LayerId, Rect)>,
}

fn key() -> Id {
    Id::unique("sue-wm-covers")
}

/// Starts a frame: this frame's floats become the last frame's. The shell
/// calls it before drawing anything.
pub fn begin_frame(ctx: &Context) {
    ctx.data_mut(|data| {
        let covers = data.get_temp_mut_or_default::<Covers>(key());
        covers.last = std::mem::take(&mut covers.now);
    });
}

/// A float painted onto `layer` this frame over `rect` (a sheet, a menu):
/// what is under it on that layer is covered.
pub fn cover(ctx: &Context, layer: LayerId, rect: Rect) {
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<Covers>(key())
            .now
            .push((layer, rect));
    });
}

/// `true` when something lies over `pos` above `layer`: an egui layer drawn
/// over it there, or a float painted onto `layer` covering it (this frame
/// or the last, since a float is often painted after what is under it
/// reads its input).
#[must_use]
pub fn covered(ctx: &Context, layer: LayerId, pos: Pos2) -> bool {
    if ctx
        .layer_id_at(pos)
        .is_some_and(|top| top != layer && top.order >= layer.order)
    {
        return true;
    }
    ctx.data(|data| {
        data.get_temp::<Covers>(key()).is_some_and(|covers| {
            covers
                .now
                .iter()
                .chain(&covers.last)
                .any(|(on, rect)| *on == layer && rect.contains(pos))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_painted_float_covers_its_page_for_two_frames() {
        let ctx = Context::default();
        let page = LayerId::background();
        let sheet = Rect::from_min_max(egui::pos2(0.0, 300.0), egui::pos2(400.0, 600.0));
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            begin_frame(ui.ctx());
            cover(ui.ctx(), page, sheet);
            assert!(covered(ui.ctx(), page, egui::pos2(10.0, 400.0)));
            assert!(!covered(ui.ctx(), page, egui::pos2(10.0, 100.0)));
            // Another layer's floats are not this one's.
            assert!(!covered(
                ui.ctx(),
                LayerId::new(egui::Order::Foreground, Id::unique("x")),
                egui::pos2(10.0, 400.0)
            ));
        });
        output.textures_delta.clear();
        // The next frame, before the sheet is painted again: still covered.
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            begin_frame(ui.ctx());
            assert!(covered(ui.ctx(), page, egui::pos2(10.0, 400.0)));
        });
        output.textures_delta.clear();
        // A frame without it: gone.
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            begin_frame(ui.ctx());
            assert!(!covered(ui.ctx(), page, egui::pos2(10.0, 400.0)));
        });
        output.textures_delta.clear();
    }

    #[test]
    fn a_popup_area_covers_the_page_under_it() {
        let ctx = Context::default();
        let page = LayerId::background();
        let at = egui::pos2(60.0, 60.0);
        for _ in 0..3 {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                egui::Area::new(Id::unique("popup"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(egui::pos2(40.0, 40.0))
                    .show(ui.ctx(), |ui| {
                        ui.allocate_exact_size(egui::vec2(100.0, 100.0), egui::Sense::click());
                    });
            });
            output.textures_delta.clear();
        }
        assert!(covered(&ctx, page, at));
        assert!(!covered(&ctx, page, egui::pos2(400.0, 400.0)));
    }
}
