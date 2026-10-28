//! Audio: a waveform entity on the canvas. The samples stay in the arena as
//! `PCMFloat1D`; a node shows a clip of them (`clip_start` to `clip_end`, in
//! frames) at a gain. Splitting places a second node over the same samples,
//! and gain and normalize change only node properties: every audio edit is
//! non-destructive and zero-copy (backbone rule 5).

use std::f32::consts::TAU;

use egui::{Rect, Sense, Stroke, Ui, pos2, vec2};
use sue_core::{ComponentKind, Entity, PCMFloat1D};
use sue_wm::{
    AppletContext, AppletManifest, AppletRole, Batch, Clip, MediaClass, ModeSpec, NewEntity,
    NodeId, PlaceNode, SceneNode, SetNodeProp, ShellRequest, SueApplet, World, pcm_of,
    project_to_timeline, widgets,
};

use crate::util::clock;

/// The applet id.
pub const ID: &str = "audio";

/// The clip a node plays: first frame, end frame, gain.
#[must_use]
pub fn clip_of(node: &SceneNode, pcm: &PCMFloat1D) -> (usize, usize, f32) {
    let frames = pcm.frames() as f64;
    let start = node.prop("clip_start", 0.0).clamp(0.0, frames);
    let end = node.prop("clip_end", frames).clamp(start, frames);
    (start as usize, end as usize, node.prop("gain", 1.0) as f32)
}

/// A mono A 440 tone of `seconds`, faded in and out so it never clicks.
#[must_use]
pub fn tone(sample_rate: u32, seconds: f32) -> PCMFloat1D {
    let frames = (sample_rate as f32 * seconds) as usize;
    let samples = (0..frames)
        .map(|frame| {
            let t = frame as f32 / sample_rate as f32;
            let fade = (t * 20.0).min(1.0).min((seconds - t) * 20.0).max(0.0);
            (TAU * 440.0 * t).sin() * 0.3 * fade
        })
        .collect();
    PCMFloat1D::new(sample_rate, 1, samples).expect("a mono buffer is valid")
}

/// Minimum and maximum of each of `columns` slices of the clip.
#[must_use]
pub fn peaks(pcm: &PCMFloat1D, start: usize, end: usize, columns: usize) -> Vec<(f32, f32)> {
    let channels = usize::from(pcm.channels().max(1));
    let samples = pcm.samples();
    let span = end.saturating_sub(start).max(1);
    (0..columns.max(1))
        .map(|column| {
            let from = start + span * column / columns.max(1);
            let to = (start + span * (column + 1) / columns.max(1))
                .max(from + 1)
                .min(end.max(from + 1));
            let step = ((to - from) / 64).max(1);
            let mut low = 0.0_f32;
            let mut high = 0.0_f32;
            let mut frame = from;
            while frame < to && frame < pcm.frames() {
                let value = samples[frame * channels];
                low = low.min(value);
                high = high.max(value);
                frame += step;
            }
            (low, high)
        })
        .collect()
}

/// Audio entities on the canvas.
#[derive(Debug, Default)]
pub struct AudioNode;

fn owner(node: NodeId) -> String {
    format!("{ID}:{}", node.0)
}

impl AudioNode {
    fn toggle(cx: &mut AppletContext<'_>, node: NodeId) {
        let owner = owner(node);
        if cx.transport.is_playing_for(&owner) {
            let sink = cx.audio.as_deref_mut();
            cx.transport.set_playing(false, sink);
            return;
        }
        let Some(scene_node) = cx.node(node).cloned() else {
            return;
        };
        let Some(pcm) = pcm_of(cx.world, scene_node.entity) else {
            return;
        };
        let (start, end, gain) = clip_of(&scene_node, pcm);
        if cx.audio.is_none() {
            cx.request(ShellRequest::Command("audio.start".to_owned()));
        }
        let resume = cx
            .transport
            .clip()
            .filter(|clip| clip.owner == owner)
            .map(|_| cx.transport.position())
            .filter(|position| *position < end as f64 - 1.0);
        let sink = cx.audio.as_deref_mut();
        cx.transport.load(
            Clip {
                source: scene_node.entity,
                start,
                end,
                gain,
                owner,
            },
            sink,
        );
        if let Some(position) = resume {
            cx.transport.seek(position, cx.audio.as_deref_mut());
        }
        cx.transport.set_playing(true, cx.audio.as_deref_mut());
    }

    fn set_gain(cx: &mut AppletContext<'_>, node: NodeId, gain: f64, label: &str) {
        let gain = gain.clamp(0.01, 16.0);
        match cx.apply(SetNodeProp::new(node, "gain", Some(gain), label)) {
            Ok(_) => {
                if cx.transport.is_playing_for(&owner(node)) {
                    cx.transport.set_gain(gain as f32);
                }
            }
            Err(error) => cx.fail(error.to_string()),
        }
    }

    /// Places a second node over the same samples from `at` on, and ends
    /// this one there.
    fn split(cx: &mut AppletContext<'_>, node: NodeId) {
        let Some(scene_node) = cx.node(node).cloned() else {
            return;
        };
        let Some(pcm) = pcm_of(cx.world, scene_node.entity) else {
            return;
        };
        let (start, end, _) = clip_of(&scene_node, pcm);
        let playing = cx
            .transport
            .clip()
            .filter(|clip| clip.owner == owner(node))
            .map(|_| cx.transport.position() as usize);
        let at = playing
            .filter(|at| *at > start && *at < end)
            .unwrap_or((start + end) / 2);
        if at <= start || at >= end {
            cx.fail("The clip is too short to split");
            return;
        }
        let mut tail = scene_node.clone();
        tail.props.insert("clip_start".to_owned(), at as f64);
        tail.props.insert("clip_end".to_owned(), end as f64);
        tail.props.remove("t0");
        tail.rect = scene_node
            .rect
            .translate(vec2(scene_node.rect.width() + 32.0, 0.0));
        let split = Batch::new("Split audio")
            .then(SetNodeProp::new(node, "clip_end", Some(at as f64), "Trim"))
            .then(PlaceNode::new(tail));
        match cx.apply(split) {
            Ok(_) => cx.notify(format!(
                "Split at {} (both halves share the same samples)",
                clock(at as f64 / f64::from(pcm_rate(cx, node)))
            )),
            Err(error) => cx.fail(error.to_string()),
        }
    }

    fn normalize(cx: &mut AppletContext<'_>, node: NodeId) {
        let Some(scene_node) = cx.node(node).cloned() else {
            return;
        };
        let Some(pcm) = pcm_of(cx.world, scene_node.entity) else {
            return;
        };
        let (start, end, _) = clip_of(&scene_node, pcm);
        let channels = usize::from(pcm.channels().max(1));
        let peak = pcm.samples()[start * channels..end * channels]
            .iter()
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
        if peak <= f32::EPSILON {
            cx.fail("The clip is silent");
            return;
        }
        Self::set_gain(cx, node, f64::from(0.98 / peak), "Normalize");
    }
}

fn pcm_rate(cx: &AppletContext<'_>, node: NodeId) -> u32 {
    cx.node(node)
        .and_then(|node| pcm_of(cx.world, node.entity))
        .map_or(48_000, PCMFloat1D::sample_rate)
}

fn decibels(gain: f64) -> String {
    format!("{:+.1} dB", 20.0 * gain.max(1e-6).log10())
}

impl SueApplet for AudioNode {
    fn manifest(&self) -> AppletManifest {
        let mut manifest = AppletManifest::new(ID, "Audio", AppletRole::Node, "\u{1F3B5}");
        manifest.summary =
            "Hear, trim, split and level audio without touching the samples.".to_owned();
        manifest.order = 30;
        manifest.opens = vec![MediaClass::Audio];
        manifest.mode = Some(ModeSpec {
            label: "Audio".to_owned(),
            in_pill: false,
        });
        manifest.lane = Some("Audio Track".to_owned());
        manifest
    }

    fn accepts(&self, entity: &Entity) -> bool {
        entity.base_kind() == Some(ComponentKind::PCMFloat1D)
    }

    fn node_size(&self, _entity: &Entity) -> egui::Vec2 {
        vec2(560.0, 200.0)
    }

    fn new_entity(&mut self, cx: &mut AppletContext<'_>) -> Option<NewEntity> {
        self.create("tone", cx)
    }

    fn templates(&self) -> Vec<sue_wm::FileTemplate> {
        [("tone", "Tone (A 440)"), ("silence", "Silence (10 s)")]
            .into_iter()
            .map(|(key, label)| sue_wm::FileTemplate::new("Audio", label, key))
            .collect()
    }

    fn create(&mut self, key: &str, _cx: &mut AppletContext<'_>) -> Option<NewEntity> {
        let rate = 48_000_u32;
        let (name, pcm) = match key {
            "tone" => ("Tone", tone(rate, 2.0)),
            "silence" => (
                "Silence",
                PCMFloat1D::silence(rate, 2, rate as usize * 10).ok()?,
            ),
            _ => return None,
        };
        Some(NewEntity {
            name: name.to_owned(),
            base: pcm.into(),
            meta: Vec::new(),
            size: vec2(560.0, 200.0),
            props: Vec::new(),
        })
    }

    fn node_ui(&mut self, ui: &mut Ui, node: NodeId, cx: &mut AppletContext<'_>) {
        let Some(scene_node) = cx.node(node).cloned() else {
            return;
        };
        let area = ui.max_rect().shrink2(vec2(12.0, 10.0));
        let theme = cx.theme.clone();
        let painter = ui.painter().clone();
        let Some(pcm) = pcm_of(cx.world, scene_node.entity) else {
            painter.text(
                area.center(),
                egui::Align2::CENTER_CENTER,
                "No audio",
                theme.font(theme.ui_size),
                theme.muted,
            );
            return;
        };
        let (start, end, gain) = clip_of(&scene_node, pcm);
        let rate = f64::from(pcm.sample_rate().max(1));
        let wave = Rect::from_min_max(area.min, pos2(area.right(), area.bottom() - 18.0));
        painter.rect_filled(wave, 6.0, theme.void);
        painter.hline(
            wave.x_range(),
            wave.center().y,
            Stroke::new(1.0 / cx.view.zoom.max(0.1), theme.border),
        );
        let columns = (wave.width() * cx.view.zoom.clamp(0.25, 2.0)).clamp(16.0, 1600.0) as usize;
        let column = wave.width() / columns as f32;
        let half = wave.height() * 0.45;
        for (index, (low, high)) in peaks(pcm, start, end, columns).into_iter().enumerate() {
            let x = wave.left() + (index as f32 + 0.5) * column;
            let top = wave.center().y - (high * gain).clamp(-1.0, 1.0) * half;
            let bottom = wave.center().y - (low * gain).clamp(-1.0, 1.0) * half;
            painter.line_segment(
                [pos2(x, top), pos2(x, bottom.max(top + 0.5))],
                Stroke::new(column.max(0.5) * 0.8, theme.text.gamma_multiply(0.85)),
            );
        }
        let playing = cx
            .transport
            .clip()
            .is_some_and(|clip| clip.owner == owner(node));
        if playing {
            let progress = ((cx.transport.position() - start as f64) / (end - start).max(1) as f64)
                .clamp(0.0, 1.0);
            let x = wave.left() + wave.width() * progress as f32;
            painter.vline(
                x,
                wave.y_range(),
                Stroke::new(1.5 / cx.view.zoom.max(0.1), theme.border_active),
            );
            if cx.transport.is_playing() {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_millis(40));
            }
        }
        let label = format!(
            "{} of {}   {}   {} Hz   {} ch",
            clock((end - start) as f64 / rate),
            clock(pcm.frames() as f64 / rate),
            decibels(f64::from(gain)),
            pcm.sample_rate(),
            pcm.channels()
        );
        painter.text(
            pos2(area.left(), area.bottom()),
            egui::Align2::LEFT_BOTTOM,
            label,
            theme.font(theme.small_size),
            theme.muted,
        );
        let response = ui.interact(
            wave,
            egui::Id::unique(("sue-audio-wave", node.0)),
            Sense::click(),
        );
        if response.double_clicked() {
            Self::toggle(cx, node);
        } else if response.clicked()
            && playing
            && let Some(at) = response.interact_pointer_pos()
        {
            let t = ((at.x - wave.left()) / wave.width()).clamp(0.0, 1.0);
            let frame = start as f64 + f64::from(t) * (end - start) as f64;
            cx.transport.seek(frame, cx.audio.as_deref_mut());
        }
    }

    fn pill_ui(&mut self, ui: &mut Ui, node: NodeId, cx: &mut AppletContext<'_>) {
        let theme = cx.theme.clone();
        let playing = cx.transport.is_playing_for(&owner(node));
        let label = if playing {
            "\u{23F8} Pause"
        } else {
            "\u{25B6} Play"
        };
        if widgets::pill_button(ui, label, playing, &theme).clicked() {
            Self::toggle(cx, node);
        }
        widgets::pill_divider(ui, &theme);
        let gain = cx.node(node).map_or(1.0, |n| n.prop("gain", 1.0));
        if widgets::pill_button(ui, "Gain -", false, &theme).clicked() {
            Self::set_gain(cx, node, gain / 10_f64.powf(1.5 / 20.0), "Gain down");
        }
        ui.label(
            egui::RichText::new(decibels(gain))
                .size(theme.small_size)
                .color(theme.muted),
        );
        if widgets::pill_button(ui, "Gain +", false, &theme).clicked() {
            Self::set_gain(cx, node, gain * 10_f64.powf(1.5 / 20.0), "Gain up");
        }
        widgets::pill_divider(ui, &theme);
        if widgets::pill_button(ui, "Split", false, &theme)
            .on_hover_text("Split at the playhead, or in the middle")
            .clicked()
        {
            Self::split(cx, node);
        }
        if widgets::pill_button(ui, "Normalize", false, &theme).clicked() {
            Self::normalize(cx, node);
        }
        widgets::pill_divider(ui, &theme);
        if widgets::pill_button(ui, "Open in Timeline", false, &theme).clicked() {
            let seconds = cx
                .node(node)
                .and_then(|n| pcm_of(cx.world, n.entity).map(|pcm| self.duration_of(n, pcm)))
                .unwrap_or(4.0);
            project_to_timeline(cx, node, seconds);
        }
    }

    fn duration(&self, node: &SceneNode, world: &World) -> f64 {
        pcm_of(world, node.entity).map_or(4.0, |pcm| self.duration_of(node, pcm))
    }
}

impl AudioNode {
    fn duration_of(&self, node: &SceneNode, pcm: &PCMFloat1D) -> f64 {
        let (start, end, _) = clip_of(node, pcm);
        (end - start) as f64 / f64::from(pcm.sample_rate().max(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Pos2;
    use sue_core::UndoStack;
    use sue_wm::AppletId;

    #[test]
    fn splitting_shares_the_samples_and_undoes_in_one_step() {
        let mut world = World::default();
        let handle = world.arena.spawn("tone", tone(8_000, 1.0));
        let node = SceneNode::new(
            handle,
            AppletId::new(ID).unwrap(),
            Rect::from_min_size(Pos2::ZERO, vec2(400.0, 200.0)),
        );
        let id = world.scene.allocate();
        world.scene.insert(id, node.clone());
        let mut tail = node.clone();
        tail.props.insert("clip_start".to_owned(), 4_000.0);
        let split = Batch::new("Split audio")
            .then(SetNodeProp::new(id, "clip_end", Some(4_000.0), "Trim"))
            .then(PlaceNode::new(tail));
        let mut history = UndoStack::new();
        history.apply(&mut world, split).unwrap();
        assert_eq!(world.scene.len(), 2);
        assert_eq!(world.arena.len(), 1, "no samples were copied");
        let pcm = pcm_of(&world, handle).unwrap();
        let halves: Vec<(usize, usize, f32)> = world
            .scene
            .iter()
            .map(|(_, node)| clip_of(node, pcm))
            .collect();
        assert!(halves.contains(&(0, 4_000, 1.0)));
        assert!(halves.contains(&(4_000, 8_000, 1.0)));
        history.undo(&mut world).unwrap();
        assert_eq!(world.scene.len(), 1);
    }

    #[test]
    fn peaks_follow_the_waveform() {
        let pcm = tone(8_000, 1.0);
        let columns = peaks(&pcm, 0, pcm.frames(), 50);
        assert_eq!(columns.len(), 50);
        assert!(columns.iter().any(|(low, high)| *high > 0.1 && *low < -0.1));
    }
}
