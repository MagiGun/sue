//! The bottom timeline: one lane per registered track kind, blocks that are
//! the very arena entities on the canvas (a non-destructive Timeline
//! projection), a draggable playhead and play or pause.

use egui::{Align2, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

use crate::{NodeId, Theme, widgets};

/// Width of the lane titles.
const LANE_TITLES: f32 = 132.0;

/// `mm:ss.cc`, e.g. `00:03.25`.
#[must_use]
pub fn timecode(seconds: f64) -> String {
    let centis = (seconds.max(0.0) * 100.0).round() as u64;
    format!(
        "{:02}:{:02}.{:02}",
        centis / 6000,
        (centis / 100) % 60,
        centis % 100
    )
}

/// The timeline's own state.
#[derive(Clone, Debug, PartialEq)]
pub struct TimelineState {
    /// Playhead, in seconds.
    pub playhead: f64,
    /// `true` while playing.
    pub playing: bool,
    /// Horizontal zoom.
    pub px_per_second: f32,
    drag: Option<(NodeId, f64, f32)>,
}

impl Default for TimelineState {
    fn default() -> Self {
        Self {
            playhead: 0.0,
            playing: false,
            px_per_second: 80.0,
            drag: None,
        }
    }
}

/// One block: a node projected onto the timeline.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    /// The node.
    pub node: NodeId,
    /// Its lane.
    pub lane: usize,
    /// Start, in seconds.
    pub start: f64,
    /// Length, in seconds.
    pub duration: f64,
    /// Entity name.
    pub label: String,
    /// `true` if selected on the canvas.
    pub selected: bool,
}

/// What the user did on the timeline.
#[derive(Clone, Debug, PartialEq)]
pub enum TimelineEvent {
    /// Move the playhead.
    Seek(f64),
    /// Play or pause.
    TogglePlay,
    /// Select a node.
    Select(NodeId),
    /// A block was dragged to a new start.
    Moved {
        /// The node.
        node: NodeId,
        /// Old start.
        from: f64,
        /// New start.
        to: f64,
    },
}

fn ruler_step(px_per_second: f32) -> f64 {
    [0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0]
        .into_iter()
        .find(|step| *step as f32 * px_per_second >= 56.0)
        .unwrap_or(120.0)
}

/// Draws the timeline into `rect` and reports what happened.
pub fn timeline_ui(
    ui: &mut Ui,
    rect: Rect,
    state: &mut TimelineState,
    lanes: &[String],
    blocks: &[Block],
    theme: &Theme,
) -> Vec<TimelineEvent> {
    let mut events = Vec::new();
    let painter = ui.painter().clone();
    painter.rect_filled(rect, 0.0, theme.panel);
    painter.hline(rect.x_range(), rect.top(), theme.hairline());

    // Header: play button and timecode.
    let header = Rect::from_min_size(rect.min, vec2(rect.width(), 34.0));
    let mut header_ui = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(header.shrink2(vec2(theme.padding, 4.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    let label = if state.playing {
        "\u{23F8} Pause"
    } else {
        "\u{25B6} Play"
    };
    if widgets::pill_button(&mut header_ui, label, state.playing, theme)
        .on_hover_text("Play or pause (Space)")
        .clicked()
    {
        events.push(TimelineEvent::TogglePlay);
    }
    header_ui.add_space(theme.padding);
    header_ui.label(
        egui::RichText::new(timecode(state.playhead))
            .font(theme.mono(theme.ui_size))
            .color(theme.text),
    );
    header_ui.add_space(theme.padding);
    header_ui.label(
        egui::RichText::new("Timeline")
            .size(theme.small_size)
            .color(theme.muted),
    );

    let origin_x = rect.left() + LANE_TITLES;
    let ruler = Rect::from_min_max(
        pos2(origin_x, header.bottom()),
        pos2(rect.right(), header.bottom() + 20.0),
    );
    let lanes_rect = Rect::from_min_max(pos2(rect.left(), ruler.bottom()), rect.max);
    let pps = state.px_per_second.max(1.0);
    let x_of = |seconds: f64| origin_x + (seconds as f32) * pps;
    let seconds_at = |x: f32| f64::from(((x - origin_x) / pps).max(0.0));

    // Ruler ticks.
    painter.hline(rect.x_range(), ruler.bottom(), theme.hairline());
    let step = ruler_step(pps);
    let mut tick = 0.0;
    while x_of(tick) < ruler.right() {
        let x = x_of(tick);
        painter.vline(x, ruler.bottom() - 6.0..=ruler.bottom(), theme.hairline());
        painter.text(
            pos2(x + 3.0, ruler.top() + 2.0),
            Align2::LEFT_TOP,
            timecode(tick),
            theme.mono(theme.small_size * 0.9),
            theme.muted,
        );
        tick += step;
    }
    let ruler_response = ui.interact(
        ruler,
        ui.scope_id().with("sue-ruler"),
        Sense::click_and_drag(),
    );
    if (ruler_response.clicked() || ruler_response.dragged())
        && let Some(at) = ruler_response.interact_pointer_pos()
    {
        events.push(TimelineEvent::Seek(seconds_at(at.x)));
    }

    // Lanes.
    let lane_height = if lanes.is_empty() {
        0.0
    } else {
        (lanes_rect.height() / lanes.len() as f32).clamp(24.0, 64.0)
    };
    for (index, title) in lanes.iter().enumerate() {
        let row = Rect::from_min_size(
            pos2(
                lanes_rect.left(),
                lanes_rect.top() + index as f32 * lane_height,
            ),
            vec2(lanes_rect.width(), lane_height),
        );
        painter.hline(rect.x_range(), row.bottom(), theme.hairline());
        painter.text(
            pos2(row.left() + theme.padding, row.center().y),
            Align2::LEFT_CENTER,
            title,
            theme.font(theme.small_size),
            theme.muted,
        );
    }
    painter.vline(origin_x, lanes_rect.y_range(), theme.hairline());

    // Blocks.
    let clip = Rect::from_min_max(pos2(origin_x, lanes_rect.top()), lanes_rect.max);
    let block_painter = painter.with_clip_rect(clip.intersect(painter.clip_rect()));
    for block in blocks {
        let start = match state.drag {
            Some((node, from, dx)) if node == block.node => (from + f64::from(dx / pps)).max(0.0),
            _ => block.start,
        };
        let row_top = lanes_rect.top() + block.lane as f32 * lane_height;
        let block_rect = Rect::from_min_max(
            pos2(x_of(start), row_top + 4.0),
            pos2(
                x_of(start + block.duration).max(x_of(start) + 6.0),
                row_top + lane_height - 4.0,
            ),
        );
        block_painter.rect_filled(block_rect, 4.0, theme.pill);
        block_painter.rect_stroke(
            block_rect,
            4.0,
            if block.selected {
                theme.hairline_active()
            } else {
                theme.hairline()
            },
            StrokeKind::Inside,
        );
        block_painter
            .with_clip_rect(block_rect.shrink(2.0).intersect(clip))
            .text(
                block_rect.left_center() + vec2(6.0, 0.0),
                Align2::LEFT_CENTER,
                &block.label,
                theme.font(theme.small_size),
                theme.text,
            );
        let response = ui.interact(
            block_rect.intersect(clip),
            ui.scope_id().with(("sue-block", block.node)),
            Sense::click_and_drag(),
        );
        if response.clicked() {
            events.push(TimelineEvent::Select(block.node));
        }
        if response.drag_started() {
            state.drag = Some((block.node, block.start, 0.0));
            events.push(TimelineEvent::Select(block.node));
        }
        if response.dragged()
            && let Some((node, _, dx)) = state.drag.as_mut()
            && *node == block.node
        {
            *dx += response.drag_delta().x;
        }
        if response.drag_stopped()
            && let Some((node, from, dx)) = state.drag.take()
        {
            let to = ((from + f64::from(dx / pps)).max(0.0) * 100.0).round() / 100.0;
            if (to - from).abs() > f64::EPSILON {
                events.push(TimelineEvent::Moved { node, from, to });
            }
        }
    }

    // Playhead.
    let x = x_of(state.playhead);
    if x >= origin_x && x <= rect.right() {
        painter.vline(x, ruler.top()..=rect.bottom(), Stroke::new(1.0, theme.text));
        let handle = [
            Pos2::new(x - 5.0, ruler.top()),
            Pos2::new(x + 5.0, ruler.top()),
            Pos2::new(x, ruler.top() + 7.0),
        ];
        painter.add(egui::Shape::convex_polygon(
            handle.to_vec(),
            theme.text,
            Stroke::NONE,
        ));
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timecodes_read_like_editors() {
        assert_eq!(timecode(0.0), "00:00.00");
        assert_eq!(timecode(3.256), "00:03.26");
        assert_eq!(timecode(125.5), "02:05.50");
    }

    #[test]
    fn ruler_ticks_stay_readable_at_any_zoom() {
        assert_eq!(ruler_step(80.0), 1.0);
        assert_eq!(ruler_step(600.0), 0.1);
        assert!(ruler_step(2.0) >= 30.0);
    }
}
