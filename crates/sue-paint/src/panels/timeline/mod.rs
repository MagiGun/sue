//! The Timeline palette, and Sue Paint's side of animation (Clip Studio
//! Paint's Animation menu and Timeline palette): making timelines,
//! folders and cels, stepping and playing frames, onion skin, and
//! exporting an animated GIF or a sequence of pictures.

mod export;
mod exposure;
mod playback;
mod tracks;
mod view;

pub(crate) use view::TimelineView;

use egui::{Rect, Ui, pos2, vec2};
use sue_wm::{AppletContext, Theme};

use crate::icons::Icon;
use crate::panels::header;
use crate::studio::SuePaint;

pub(crate) type Edit = sue_core::txn::Batch<sue_core::Arena>;

impl SuePaint {
    pub(crate) fn timeline_panel(
        &mut self,
        ui: &mut Ui,
        rect: Rect,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        ui.painter().rect_filled(rect, 0.0, theme.panel);
        ui.painter()
            .hline(rect.x_range(), rect.top(), theme.hairline());
        let (body, _) = header(
            ui,
            rect,
            &[(tr!("timeline"), Icon::Timeline, true)],
            0,
            theme,
        );
        let bar = Rect::from_min_size(body.min, vec2(body.width(), 28.0));
        ui.painter()
            .hline(body.x_range(), bar.bottom(), theme.hairline());
        let tracks = Rect::from_min_max(pos2(body.left(), bar.bottom()), body.max);
        // Clip Studio Paint's timeline: its tools and playback, then the
        // folders' cels frame by frame.
        self.timeline_ui(ui, bar, tracks, cx, theme);
    }
}
