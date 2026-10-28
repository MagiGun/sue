//! Player: plays decoded audio (`PCMFloat1D` entities) through the platform
//! audio sink. The track stays in the arena; on each tick the player feeds
//! the sink's ring with the next slice, resampled to the device rate (the
//! ring is the device boundary, like a texture upload for images).

use std::time::{Duration, Instant};

use sue_core::{AssetHandle, ComponentKind, PCMFloat1D};
use sue_io::meta;
use sue_platform::{AudioSink, CursorShape, InputEvent, Key, NamedKey};
use sue_rice::{RiceValue, SetRiceValue};
use sue_wm::{
    AppletCategory, AppletContext, AppletError, AppletId, AppletManifest, BoxStyle, InputResponse,
    MediaClass, Rect, ShellRequest, SliderEvent, SueApplet, Tick, Ui, World, Zone, widget_id,
};

use crate::util::clock;

/// The applet id.
pub const ID: &str = "player";

/// How often the player tops up the device queue while playing.
const FEED_INTERVAL: Duration = Duration::from_millis(40);
/// Most frames queued in one tick.
const FEED_CHUNK: usize = 16_384;

/// A playlist and its transport.
#[derive(Debug, Default)]
pub struct Player {
    playlist: Vec<AssetHandle>,
    current: Option<usize>,
    playing: bool,
    /// Track frame the device clock was anchored at.
    anchor_frame: f64,
    /// Device frames played at the anchor.
    anchor_played: u64,
    /// Track frame of the next frame to queue.
    feed: f64,
    /// Track frame being heard now.
    position: f64,
    /// Device frames played when last seen; a smaller value means a new device.
    last_played: u64,
    shown_second: Option<u64>,
    asked_for_audio: bool,
    volume_drag: Option<f64>,
}

fn pcm_of(world: &World, handle: AssetHandle) -> Option<&PCMFloat1D> {
    world
        .arena
        .get(handle)
        .ok()?
        .component(ComponentKind::PCMFloat1D)?
        .as_pcm()
}

/// Queues up to `free` device frames from `pcm` starting at track frame
/// `start`, resampling linearly and mapping channels. Returns the samples.
fn resample(
    pcm: &PCMFloat1D,
    start: f64,
    ratio: f64,
    free: usize,
    channels_out: usize,
) -> Vec<f32> {
    let channels_in = usize::from(pcm.channels());
    let samples = pcm.samples();
    let last = pcm.frames().saturating_sub(1);
    let mut out = Vec::with_capacity(free * channels_out);
    for frame in 0..free {
        let t = start + frame as f64 * ratio;
        if t >= last as f64 {
            break;
        }
        let i0 = t.floor() as usize;
        let mix = (t - i0 as f64) as f32;
        let sample = |channel: usize| {
            let a = samples[i0 * channels_in + channel];
            let b = samples[(i0 + 1) * channels_in + channel];
            a + (b - a) * mix
        };
        for channel in 0..channels_out {
            out.push(match (channels_in, channels_out) {
                (1, _) if channel < 2 => sample(0),
                (_, 1) => (sample(0) + sample(1)) / 2.0,
                (inputs, _) if channel < inputs => sample(channel),
                _ => 0.0,
            });
        }
    }
    out
}

impl Player {
    fn track_frames(&self, world: &World) -> usize {
        self.current
            .and_then(|index| self.playlist.get(index))
            .and_then(|handle| pcm_of(world, *handle))
            .map_or(0, PCMFloat1D::frames)
    }

    fn rate(&self, world: &World) -> u32 {
        self.current
            .and_then(|index| self.playlist.get(index))
            .and_then(|handle| pcm_of(world, *handle))
            .map_or(1, PCMFloat1D::sample_rate)
    }

    /// Jumps to track frame `frame`, dropping what is queued.
    fn seek<'s>(&mut self, frame: f64, sink: Option<&mut (dyn AudioSink + 's)>) {
        self.anchor_frame = frame;
        self.feed = frame;
        self.position = frame;
        self.shown_second = None;
        if let Some(sink) = sink {
            sink.clear();
            self.anchor_played = sink.played_frames();
            self.last_played = self.anchor_played;
        } else {
            self.anchor_played = 0;
        }
    }

    fn select<'s>(&mut self, index: usize, sink: Option<&mut (dyn AudioSink + 's)>) {
        self.current = Some(index);
        self.seek(0.0, sink);
    }

    fn set_playing<'s>(&mut self, playing: bool, sink: Option<&mut (dyn AudioSink + 's)>) {
        self.playing = playing;
        if let Some(sink) = sink {
            sink.set_playing(playing);
        }
    }

    /// Feeds the sink and advances the position. Returns `true` when the
    /// shown second changed.
    fn pump(&mut self, world: &World, sink: &mut dyn AudioSink, volume: f32) -> bool {
        let Some(handle) = self
            .current
            .and_then(|index| self.playlist.get(index))
            .copied()
        else {
            return false;
        };
        let Some(pcm) = pcm_of(world, handle) else {
            return false;
        };
        let played = sink.played_frames();
        if played < self.last_played {
            // A new device: what was queued is gone, carry on from here.
            self.anchor_frame = self.position;
            self.anchor_played = played;
            self.feed = self.position;
        }
        self.last_played = played;
        sink.set_volume(volume);
        sink.set_playing(true);
        let ratio = f64::from(pcm.sample_rate()) / f64::from(sink.info().sample_rate.max(1));
        let free = sink.free_frames().min(FEED_CHUNK);
        if free > 0 && self.feed < pcm.frames() as f64 {
            let channels = usize::from(sink.info().channels.max(1));
            let samples = resample(pcm, self.feed, ratio, free, channels);
            let pushed = sink.push(&samples);
            self.feed += pushed as f64 * ratio;
        }
        self.position = (self.anchor_frame
            + (played.saturating_sub(self.anchor_played)) as f64 * ratio)
            .min(pcm.frames() as f64);
        let second = (self.position / f64::from(pcm.sample_rate())) as u64;
        let changed = self.shown_second != Some(second);
        self.shown_second = Some(second);
        changed
    }

    fn title(world: &World, handle: AssetHandle) -> String {
        world.arena.get(handle).map_or_else(
            |_| "Missing track".to_owned(),
            |entity| {
                entity
                    .meta(meta::TITLE)
                    .map_or_else(|| entity.name().to_owned(), str::to_owned)
            },
        )
    }
}

impl SueApplet for Player {
    fn manifest(&self) -> AppletManifest {
        AppletManifest {
            id: AppletId::new(ID).expect("the Player id is valid"),
            name: "Player".to_owned(),
            category: AppletCategory::Utility,
            home_zone: Zone::LeftDock,
            uses_timeline: false,
            summary: "Play MP3, FLAC, WAV, Ogg and M4A while you work.".to_owned(),
            opens: vec![MediaClass::Audio],
            icon: "\u{1F3B5}".to_owned(),
            singleton: false,
        }
    }

    fn mount(&mut self, _ctx: &mut AppletContext<'_>) -> Result<(), AppletError> {
        Ok(())
    }

    fn open(&mut self, asset: AssetHandle, ctx: &mut AppletContext<'_>) -> Result<(), AppletError> {
        pcm_of(ctx.world, asset).ok_or_else(|| AppletError::new("the asset is not audio"))?;
        let index = match self.playlist.iter().position(|track| *track == asset) {
            Some(index) => index,
            None => {
                self.playlist.push(asset);
                self.playlist.len() - 1
            }
        };
        self.select(index, ctx.audio.as_deref_mut());
        self.set_playing(true, ctx.audio.as_deref_mut());
        Ok(())
    }

    fn handle_input(
        &mut self,
        event: &InputEvent,
        ctx: &mut AppletContext<'_>,
    ) -> Result<InputResponse, AppletError> {
        let InputEvent::Keyboard(key) = event else {
            return Ok(InputResponse::Ignored);
        };
        if !key.pressed || key.modifiers.is_command() || self.current.is_none() {
            return Ok(InputResponse::Ignored);
        }
        match key.key {
            Key::Named(NamedKey::Space) => {
                let playing = !self.playing;
                self.set_playing(playing, ctx.audio.as_deref_mut());
            }
            Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowRight) => {
                let rate = f64::from(self.rate(ctx.world));
                let step = if key.key == Key::Named(NamedKey::ArrowLeft) {
                    -5.0
                } else {
                    5.0
                };
                let frames = self.track_frames(ctx.world) as f64;
                let target = (self.position + step * rate).clamp(0.0, frames);
                self.seek(target, ctx.audio.as_deref_mut());
            }
            _ => return Ok(InputResponse::Ignored),
        }
        Ok(InputResponse::Consumed { redraw: true })
    }

    fn tick(&mut self, now: Instant, ctx: &mut AppletContext<'_>) -> Tick {
        if !self.playing || self.current.is_none() {
            return Tick::default();
        }
        let volume = ctx.world.rice.get_f64("audio.volume").unwrap_or(0.8) as f32;
        let Some(sink) = ctx.audio.as_deref_mut() else {
            if !self.asked_for_audio {
                self.asked_for_audio = true;
                ctx.requests
                    .push(ShellRequest::Command("audio.start".to_owned()));
            }
            return Tick {
                redraw: false,
                wake: Some(now + FEED_INTERVAL),
            };
        };
        self.asked_for_audio = false;
        let mut redraw = self.pump(ctx.world, sink, volume);
        let frames = self.track_frames(ctx.world);
        if self.position >= frames as f64 && sink.queued_frames() == 0 {
            // The track ended: the next one, or stop.
            let next = self.current.map_or(0, |index| index + 1);
            if next < self.playlist.len() {
                self.select(next, Some(sink));
            } else {
                self.set_playing(false, Some(sink));
                self.seek(0.0, Some(sink));
            }
            redraw = true;
        }
        Tick {
            redraw,
            wake: self.playing.then_some(now + FEED_INTERVAL),
        }
    }

    fn draw(&mut self, ui: &mut Ui<'_>, area: Rect, ctx: &mut AppletContext<'_>) {
        let theme = ui.theme;
        let pad = theme.padding;
        let row = ui.row_height();
        let Some(index) = self.current else {
            ui.wrapped(
                area.x,
                area.y,
                area.width,
                "Open an MP3, FLAC, WAV, Ogg or M4A file from Files, or drop one on the window.",
                theme.ui_size,
                theme.muted,
            );
            return;
        };
        let handle = self.playlist[index];
        let mut y = area.y;
        let title = Self::title(ctx.world, handle);
        ui.label(Rect::new(area.x, y, area.width, row), &title, theme.text);
        y += row * 0.9;
        let (details, rate, frames) = match ctx.world.arena.get(handle) {
            Ok(entity) => {
                let artist = [entity.meta(meta::ARTIST), entity.meta(meta::ALBUM)]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(", ");
                let pcm = pcm_of(ctx.world, handle);
                let rate = pcm.map_or(1, PCMFloat1D::sample_rate);
                let format = format!(
                    "{}, {:.1} kHz, {}",
                    entity.meta(meta::CODEC).unwrap_or("Audio"),
                    f64::from(rate) / 1000.0,
                    match pcm.map_or(0, PCMFloat1D::channels) {
                        1 => "mono".to_owned(),
                        2 => "stereo".to_owned(),
                        count => format!("{count} channels"),
                    }
                );
                (
                    if artist.is_empty() { format } else { artist },
                    rate,
                    pcm.map_or(0, PCMFloat1D::frames),
                )
            }
            Err(_) => (String::new(), 1, 0),
        };
        ui.label_sized(
            Rect::new(area.x, y, area.width, row * 0.8),
            &details,
            theme.small_size,
            theme.muted,
        );
        y += row;
        if self.playing && ctx.audio.is_none() {
            // Say why nothing is heard instead of pretending to play.
            let status = ctx
                .catalog
                .facts
                .get("audio.stream")
                .map_or("Starting the audio device", |(_, value)| value.as_str());
            y += ui.wrapped(area.x, y, area.width, status, theme.small_size, theme.alert)
                + pad / 2.0;
        }

        // Scrubber with times on both ends.
        let duration = frames as f64 / f64::from(rate);
        let position = self.position / f64::from(rate);
        let scrub = Rect::new(area.x, y, area.width, row * 0.8);
        let shown = match ui.slider(
            widget_id("player", "scrub"),
            scrub,
            position,
            0.0,
            duration.max(0.001),
        ) {
            SliderEvent::Preview(seconds) => seconds,
            SliderEvent::Commit(seconds) => {
                self.seek(seconds * f64::from(rate), ctx.audio.as_deref_mut());
                seconds
            }
            SliderEvent::Idle => position,
        };
        y += row * 0.8;
        ui.label_sized(
            Rect::new(area.x, y, area.width / 2.0, row * 0.8),
            &clock(shown),
            theme.small_size,
            theme.muted,
        );
        ui.label_right(
            Rect::new(area.x, y, area.width, row * 0.8),
            &clock(duration),
            theme.small_size,
            theme.muted,
        );
        y += row;

        // Transport.
        let size = row * 1.3;
        let center = area.x + area.width / 2.0;
        let previous = Rect::new(center - size * 1.5 - pad, y, size, size);
        let play = Rect::new(center - size / 2.0, y, size, size);
        let next = Rect::new(center + size / 2.0 + pad, y, size, size);
        ui.canvas
            .draw_box(play, BoxStyle::filled(theme.accent, size / 2.0));
        let can_previous = index > 0;
        let can_next = index + 1 < self.playlist.len();
        if ui.icon_button(
            widget_id("player", "previous"),
            previous,
            "\u{23EE}",
            can_previous,
            "Previous",
        ) && can_previous
        {
            self.select(index - 1, ctx.audio.as_deref_mut());
        }
        let toggle = if self.playing { "\u{23F8}" } else { "\u{25B6}" };
        let response = ui.interact(widget_id("player", "play"), play);
        ui.label_centered_sized(play, toggle, theme.ui_size * 1.2, theme.on_accent);
        if response.hovered {
            ui.set_cursor(CursorShape::Pointer);
        }
        ui.tooltip(response, play, if self.playing { "Pause" } else { "Play" });
        if response.clicked {
            let playing = !self.playing;
            self.set_playing(playing, ctx.audio.as_deref_mut());
        }
        if ui.icon_button(
            widget_id("player", "next"),
            next,
            "\u{23ED}",
            can_next,
            "Next",
        ) && can_next
        {
            self.select(index + 1, ctx.audio.as_deref_mut());
        }
        y += size + pad;

        // Volume, a rice value: a drag previews, the release is one undo step.
        let volume = ctx.world.rice.get_f64("audio.volume").unwrap_or(0.8);
        ui.label_centered_sized(
            Rect::new(area.x, y, row, row),
            "\u{1F50A}",
            theme.ui_size,
            theme.muted,
        );
        let track = Rect::new(
            area.x + row + pad / 2.0,
            y,
            area.width - row - pad / 2.0,
            row,
        );
        match ui.slider(
            widget_id("player", "volume"),
            track,
            self.volume_drag.unwrap_or(volume),
            0.0,
            1.0,
        ) {
            SliderEvent::Preview(value) => {
                self.volume_drag = Some(value);
                if let Some(sink) = ctx.audio.as_deref_mut() {
                    sink.set_volume(value as f32);
                }
            }
            SliderEvent::Commit(value) => {
                self.volume_drag = None;
                let value = (value * 100.0).round() / 100.0;
                let _ = ctx.edit_rice(SetRiceValue::new("audio.volume", RiceValue::Float(value)));
            }
            SliderEvent::Idle => {}
        }
        y += row + pad;

        // Playlist.
        ui.hline(area.x, y, area.width);
        y += pad / 2.0;
        for (slot, track) in self.playlist.clone().into_iter().enumerate() {
            if y + row > area.bottom() {
                break;
            }
            let rect = Rect::new(area.x, y, area.width, row);
            let response = ui.interact(widget_id("player/track", &slot.to_string()), rect);
            if response.hovered {
                ui.canvas
                    .draw_box(rect, BoxStyle::filled(theme.hover, theme.radius));
                ui.set_cursor(CursorShape::Pointer);
            }
            let color = if slot == index {
                theme.accent
            } else {
                theme.text
            };
            ui.label(
                Rect::new(rect.x + pad / 2.0, rect.y, rect.width - pad, row),
                &Self::title(ctx.world, track),
                color,
            );
            if response.clicked {
                self.select(slot, ctx.audio.as_deref_mut());
                self.set_playing(true, ctx.audio.as_deref_mut());
            }
            y += row;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_core::UndoStack;
    use sue_platform::{FormFactor, NullAudio};
    use sue_rice::{DEFAULT_RICE, HardwareProfile, RiceSchema};
    use sue_wm::Catalog;

    fn world_with_tone(rate: u32, seconds: usize) -> (World, AssetHandle) {
        let mut world = World {
            rice: RiceSchema::parse(DEFAULT_RICE).unwrap(),
            ..World::default()
        };
        let pcm = PCMFloat1D::new(rate, 1, vec![0.5; rate as usize * seconds]).unwrap();
        let handle = world.arena.spawn("tone", pcm);
        (world, handle)
    }

    /// Runs one player call with `sink` as the device.
    fn with_device<R>(
        world: &mut World,
        sink: &mut NullAudio,
        call: impl FnOnce(&mut AppletContext<'_>) -> R,
    ) -> R {
        let (mut history, mut requests, catalog) =
            (UndoStack::new(), Vec::new(), Catalog::default());
        let mut ctx = AppletContext {
            world,
            history: &mut history,
            form_factor: FormFactor::Desktop,
            profile: HardwareProfile::Balanced,
            requests: &mut requests,
            focused: false,
            catalog: &catalog,
            audio: Some(sink),
            inspection: Vec::new(),
        };
        call(&mut ctx)
    }

    #[test]
    fn playing_feeds_the_sink_and_the_clock_follows_the_device() {
        let (mut world, tone) = world_with_tone(24_000, 3);
        let mut sink = NullAudio::new(48_000, 2, 8_000);
        let mut player = Player::default();
        let now = Instant::now();
        let tick = with_device(&mut world, &mut sink, |ctx| {
            player.open(tone, ctx).unwrap();
            player.tick(now, ctx)
        });
        assert!(tick.wake.is_some());
        assert!(sink.is_playing());
        assert_eq!(sink.queued_frames(), 8_000, "the queue is topped up");
        assert!(
            (sink.volume() - 0.8).abs() < 1e-6,
            "the riced volume applies"
        );
        // One second of device time, fed and played in small slices.
        for _ in 0..6 {
            sink.advance(8_000);
            with_device(&mut world, &mut sink, |ctx| player.tick(now, ctx));
        }
        // 48 000 device frames at 48 kHz are one second of a 24 kHz track.
        assert!(
            (player.position - 24_000.0).abs() < 1.0,
            "{}",
            player.position
        );
        with_device(&mut world, &mut sink, |ctx| {
            player.seek(48_000.0, ctx.audio.as_deref_mut());
        });
        assert_eq!(sink.queued_frames(), 0, "seeking drops the queue");
    }

    #[test]
    fn without_a_device_the_player_asks_the_engine_for_one() {
        let (mut world, tone) = world_with_tone(8_000, 1);
        let mut history = UndoStack::new();
        let mut requests = Vec::new();
        let catalog = Catalog::default();
        let mut player = Player::default();
        let mut ctx = AppletContext {
            world: &mut world,
            history: &mut history,
            form_factor: FormFactor::Desktop,
            profile: HardwareProfile::Balanced,
            requests: &mut requests,
            focused: false,
            catalog: &catalog,
            audio: None,
            inspection: Vec::new(),
        };
        player.open(tone, &mut ctx).unwrap();
        player.tick(Instant::now(), &mut ctx);
        player.tick(Instant::now(), &mut ctx);
        assert_eq!(
            requests,
            [ShellRequest::Command("audio.start".to_owned())],
            "asked once"
        );
    }

    #[test]
    fn resampling_maps_channels() {
        let mono = PCMFloat1D::new(10, 1, vec![0.0, 1.0, 2.0, 3.0]).unwrap();
        assert_eq!(
            resample(&mono, 0.0, 0.5, 3, 2),
            [0.0, 0.0, 0.5, 0.5, 1.0, 1.0]
        );
        let stereo = PCMFloat1D::new(10, 2, vec![1.0, 3.0, 1.0, 3.0, 1.0, 3.0]).unwrap();
        assert_eq!(resample(&stereo, 0.0, 1.0, 1, 1), [2.0]);
        assert_eq!(resample(&stereo, 0.0, 1.0, 1, 4), [1.0, 3.0, 0.0, 0.0]);
        assert!(
            resample(&mono, 3.0, 1.0, 4, 2).is_empty(),
            "nothing past the end"
        );
    }
}
