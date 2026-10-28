//! Player: a persistent audio player in the left drawer. Its playlist is
//! every decoded track in the arena; playback goes through the shared
//! transport, so it keeps playing whatever mode the canvas is in. The
//! scrubber is the track's own waveform.

use egui::{RichText, Sense, Stroke, Ui, pos2, vec2};
use sue_core::{AssetHandle, ComponentKind};
use sue_io::meta;
use sue_rice::{RiceValue, SetRiceValue};
use sue_wm::{
    AppletContext, AppletManifest, AppletRole, Clip, ShellRequest, SueApplet, pcm_of, widgets,
};

use crate::audio_node::peaks;
use crate::util::clock;

/// The applet id.
pub const ID: &str = "player";

/// The transport owner name of the Player.
const OWNER: &str = "player";

/// The playlist and the volume being dragged.
#[derive(Debug, Default)]
pub struct Player {
    volume_drag: Option<f64>,
}

fn title(cx: &AppletContext<'_>, handle: AssetHandle) -> String {
    cx.arena().get(handle).map_or_else(
        |_| "Missing track".to_owned(),
        |entity| {
            entity
                .meta(meta::TITLE)
                .map_or_else(|| entity.name().to_owned(), str::to_owned)
        },
    )
}

impl Player {
    fn play(cx: &mut AppletContext<'_>, handle: AssetHandle) {
        let Some(frames) = pcm_of(cx.world, handle).map(sue_core::PCMFloat1D::frames) else {
            return;
        };
        if cx.audio.is_none() {
            cx.request(ShellRequest::Command("audio.start".to_owned()));
        }
        let clip = Clip {
            source: handle,
            start: 0,
            end: frames,
            gain: 1.0,
            owner: OWNER.to_owned(),
        };
        cx.transport.load(clip, cx.audio.as_deref_mut());
        cx.transport.set_playing(true, cx.audio.as_deref_mut());
    }
}

impl SueApplet for Player {
    fn manifest(&self) -> AppletManifest {
        let mut manifest = AppletManifest::new(ID, "Player", AppletRole::LeftDrawer, "\u{25B6}");
        manifest.summary = "Play MP3, FLAC, WAV, Ogg and M4A while you work.".to_owned();
        manifest.order = 20;
        manifest
    }

    fn drawer_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        let theme = cx.theme.clone();
        let tracks: Vec<AssetHandle> = cx
            .arena()
            .iter()
            .filter(|(_, entity)| entity.base_kind() == ComponentKind::PCMFloat1D)
            .map(|(handle, _)| handle)
            .collect();
        let current = cx
            .transport
            .clip()
            .filter(|clip| clip.owner == OWNER)
            .map(|clip| clip.source);
        let playing = cx.transport.is_playing_for(OWNER);

        // Now playing, with the waveform as the scrubber.
        let shown = current.or_else(|| tracks.first().copied());
        if let Some(handle) = shown {
            ui.label(
                RichText::new(title(cx, handle))
                    .size(theme.ui_size)
                    .color(theme.text),
            );
            let (rect, response) =
                ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::click());
            ui.painter().rect_filled(rect, 6.0, theme.void);
            if let Some(pcm) = pcm_of(cx.world, handle) {
                let columns = (rect.width() / 3.0) as usize;
                let progress = if current == Some(handle) {
                    (cx.transport.position() / pcm.frames().max(1) as f64) as f32
                } else {
                    0.0
                };
                for (index, (low, high)) in
                    peaks(pcm, 0, pcm.frames(), columns).into_iter().enumerate()
                {
                    let x = rect.left() + (index as f32 + 0.5) * 3.0;
                    let half = rect.height() * 0.42;
                    let color = if (index as f32 / columns.max(1) as f32) < progress {
                        theme.text
                    } else {
                        theme.muted
                    };
                    ui.painter().line_segment(
                        [
                            pos2(x, rect.center().y - high * half),
                            pos2(x, rect.center().y - low * half + 0.5),
                        ],
                        Stroke::new(2.0, color),
                    );
                }
                if response.clicked()
                    && let Some(at) = response.interact_pointer_pos()
                {
                    let t = f64::from(((at.x - rect.left()) / rect.width()).clamp(0.0, 1.0));
                    if current != Some(handle) {
                        Self::play(cx, handle);
                    }
                    let frames = pcm_of(cx.world, handle).map_or(0, sue_core::PCMFloat1D::frames);
                    cx.transport
                        .seek(t * frames as f64, cx.audio.as_deref_mut());
                }
                let rate = f64::from(
                    pcm_of(cx.world, handle)
                        .map_or(1, sue_core::PCMFloat1D::sample_rate)
                        .max(1),
                );
                let total =
                    pcm_of(cx.world, handle).map_or(0, sue_core::PCMFloat1D::frames) as f64 / rate;
                let at = if current == Some(handle) {
                    cx.transport.position() / rate
                } else {
                    0.0
                };
                ui.label(
                    RichText::new(format!("{} / {}", clock(at), clock(total)))
                        .font(theme.mono(theme.small_size))
                        .color(theme.muted),
                );
                if playing {
                    ui.ctx()
                        .request_repaint_after(std::time::Duration::from_millis(100));
                }
            }
            ui.horizontal(|ui| {
                let label = if playing {
                    "\u{23F8} Pause"
                } else {
                    "\u{25B6} Play"
                };
                if widgets::pill_button(ui, label, playing, &theme).clicked() {
                    if playing {
                        cx.transport.set_playing(false, cx.audio.as_deref_mut());
                    } else if current == Some(handle) {
                        cx.transport.set_playing(true, cx.audio.as_deref_mut());
                    } else {
                        Self::play(cx, handle);
                    }
                }
                if widgets::pill_button(ui, "\u{23F9} Stop", false, &theme).clicked() {
                    cx.transport.stop(cx.audio.as_deref_mut());
                }
            });
        }

        // Volume: live while dragging, one undo step on release.
        ui.add_space(theme.padding);
        widgets::heading(ui, "Volume", &theme);
        let stored = cx.rice().get_f64("audio.volume").unwrap_or(0.8);
        let mut volume = self.volume_drag.unwrap_or(stored);
        let response = ui.add(egui::Slider::new(&mut volume, 0.0..=1.0).show_value(false));
        if response.dragged() || response.changed() {
            self.volume_drag = Some(volume);
            if let Some(sink) = cx.audio.as_deref_mut() {
                sink.set_volume(volume as f32);
            }
        }
        if (response.drag_stopped() || (response.changed() && !response.dragged()))
            && let Some(volume) = self.volume_drag.take()
            && (volume - stored).abs() > f64::EPSILON
            && let Err(error) = cx.edit_rice(SetRiceValue::new(
                "audio.volume",
                RiceValue::Float((volume * 100.0).round() / 100.0),
            ))
        {
            cx.fail(error.to_string());
        }

        // Playlist.
        ui.add_space(theme.padding);
        widgets::heading(ui, "Playlist", &theme);
        if tracks.is_empty() {
            ui.label(
                RichText::new("Open an MP3, FLAC, WAV, Ogg or M4A file from Files, or drop one on the window.")
                    .color(theme.muted),
            );
        }
        for handle in tracks {
            let chosen = current == Some(handle);
            let (rect, response) = ui.allocate_exact_size(
                vec2(ui.available_width(), theme.ui_size * 1.9),
                Sense::click(),
            );
            if chosen || response.hovered() {
                ui.painter().rect_filled(
                    rect,
                    4.0,
                    if chosen { theme.active } else { theme.hover },
                );
            }
            ui.painter().text(
                rect.left_center() + vec2(8.0, 0.0),
                egui::Align2::LEFT_CENTER,
                title(cx, handle),
                theme.font(theme.ui_size),
                theme.text,
            );
            if let Some(pcm) = pcm_of(cx.world, handle) {
                ui.painter().text(
                    rect.right_center() - vec2(8.0, 0.0),
                    egui::Align2::RIGHT_CENTER,
                    clock(pcm.duration_seconds()),
                    theme.mono(theme.small_size),
                    theme.muted,
                );
            }
            if response.clicked() {
                Self::play(cx, handle);
            }
        }
    }
}
