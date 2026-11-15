//! Export Timelapse (CTV-7): the video's long side, length and shape, and
//! above them a preview that plays the drawing as the video will show it,
//! at its speed and in its shape, white where the drawing does not reach.
//!
//! The person sees what they will get before a file is chosen: changing
//! the shape or the length changes the preview at once, so there is no
//! export to undo. The frames for the preview are read in the background
//! (a few dozen, small), so the dialog opens at once however long the
//! recording is. The export itself then runs in the background too, its
//! progress and a Cancel button in a notice
//! ([`crate::studio::exports`]).

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use egui::{ColorImage, Rect, Sense, TextureHandle, TextureOptions, Ui, pos2, vec2};
use sue_core::JobPool;
use sue_wm::Theme;
use sue_wm::icons::Icon;
use sue_wm::ui::{self as wm_ui, Radius, Space, TextRole};

use super::Outcome;
use crate::timelapse::{self, Aspect, Export};

/// The longest side of a frame of the preview, in pixels.
const PREVIEW_SIDE: u32 = 360;

/// The most frames the preview reads, spread over the recording.
const PREVIEW_FRAMES: usize = 90;

/// The tallest the preview gets, in units of the largest spacing token.
const PREVIEW_ROOM: f32 = 7.0;

/// The video's long sides offered.
const SIDES: [u32; 5] = [480, 720, 1080, 1280, 1920];

/// What the reading of the preview hands back: the canvas's shape (the
/// last frame's) and each frame read, by its place among the kept frames.
type Read = Option<([u32; 2], Vec<(usize, ColorImage)>)>;

/// The worker that reads previews: one thread, so a preview never takes
/// the cores an export or the brush is using.
fn readers() -> &'static JobPool {
    static POOL: OnceLock<JobPool> = OnceLock::new();
    POOL.get_or_init(|| JobPool::new(1))
}

/// Export Timelapse, while it is open.
#[derive(Clone)]
pub(crate) struct TimelapseDialog {
    /// How the video is laid out.
    pub export: Export,
    /// The frames kept when the dialog opened.
    paths: Arc<Vec<PathBuf>>,
    /// The canvas's shape, once read.
    shape: Option<[u32; 2]>,
    /// The preview's frames being read.
    reading: Option<Arc<Mutex<Read>>>,
    /// The preview's frames: their place among the kept frames, and the
    /// picture.
    frames: Vec<(usize, TextureHandle)>,
    /// The preview plays.
    playing: bool,
    /// Where it is, in seconds.
    at: f64,
    /// The time of the frame it last moved on in.
    clock: Option<f64>,
}

impl std::fmt::Debug for TimelapseDialog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TimelapseDialog")
            .field("export", &self.export)
            .field("frames", &self.paths.len())
            .field("shape", &self.shape)
            .field("read", &self.frames.len())
            .field("playing", &self.playing)
            .field("at", &self.at)
            .finish_non_exhaustive()
    }
}

/// Which of `count` kept frames the preview reads: at most
/// [`PREVIEW_FRAMES`], spread evenly, the first and the last among them.
fn preview_picks(count: usize) -> Vec<usize> {
    let take = count.min(PREVIEW_FRAMES);
    let mut picks: Vec<usize> = (0..take)
        .map(|index| {
            if take <= 1 {
                count.saturating_sub(1)
            } else {
                index * (count - 1) / (take - 1)
            }
        })
        .collect();
    picks.dedup();
    picks
}

/// Reads the preview frames of `paths`, small.
fn read_preview(paths: &[PathBuf]) -> Read {
    let last = timelapse::read_frame(paths.last()?).ok()?;
    let shape = [last.width(), last.height()];
    let frames = preview_picks(paths.len())
        .into_iter()
        .filter_map(|index| {
            let tile = timelapse::read_frame(&paths[index]).ok()?;
            let from = [tile.width(), tile.height()];
            let to = timelapse::fit(from, PREVIEW_SIDE);
            let pixels = tile.bytes()?;
            let small = if to == from {
                pixels.to_vec()
            } else {
                crate::doc::resize_rgba(pixels, from, to)
            };
            let image =
                ColorImage::from_rgba_unmultiplied([to[0] as usize, to[1] as usize], &small);
            Some((index, image))
        })
        .collect();
    Some((shape, frames))
}

impl TimelapseDialog {
    /// The dialog for the frames `paths`, laid out as `export`; it starts
    /// reading its preview at once.
    pub(crate) fn open(paths: Vec<PathBuf>, export: Export) -> Self {
        let paths = Arc::new(paths);
        let reading = (!paths.is_empty()).then(|| {
            let slot: Arc<Mutex<Read>> = Arc::default();
            let (into, from) = (Arc::clone(&slot), Arc::clone(&paths));
            // The handle may go: the result comes through the slot.
            let _ = readers().spawn("timelapse-preview", move |_| {
                let read = read_preview(&from);
                *into.lock().unwrap_or_else(PoisonError::into_inner) = read;
                Ok(())
            });
            slot
        });
        Self {
            export,
            paths,
            shape: None,
            reading,
            frames: Vec::new(),
            playing: true,
            at: 0.0,
            clock: None,
        }
    }

    /// `true` once the preview's frames are read (or there are none).
    pub(crate) fn ready(&self) -> bool {
        self.reading.is_none()
    }

    /// Takes in the preview's frames once they are read.
    fn take_frames(&mut self, ctx: &egui::Context) {
        let Some(slot) = &self.reading else {
            return;
        };
        let Some((shape, frames)) = slot.lock().unwrap_or_else(PoisonError::into_inner).take()
        else {
            // Still reading: look again soon.
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
            return;
        };
        self.shape = Some(shape);
        self.frames = frames
            .into_iter()
            .map(|(index, image)| {
                let name = format!("sue-timelapse-preview-{index}");
                (index, ctx.load_texture(name, image, TextureOptions::LINEAR))
            })
            .collect();
        self.reading = None;
    }

    /// The video's frames a second, how many kept frames it shows, every
    /// how many it shows one, and how long it plays in seconds.
    fn timing(&self) -> (u32, usize, usize, f64) {
        let count = self.paths.len();
        let (fps, step) = timelapse::pace(count, self.export);
        let shown = timelapse::shown(count, step);
        let length = if shown == 0 { 0 } else { shown + fps as usize };
        (fps, shown, step, length as f64 / f64::from(fps.max(1)))
    }

    /// The kept frame the video shows `at` seconds in.
    fn kept_at(&self, at: f64) -> usize {
        let (fps, shown, step, _) = self.timing();
        let frame = (at * f64::from(fps)).floor().max(0.0) as usize;
        let chosen = frame.min(shown.saturating_sub(1));
        if chosen + 1 >= shown {
            self.paths.len().saturating_sub(1)
        } else {
            chosen * step
        }
    }

    /// The preview frame nearest before the kept frame `kept`.
    fn texture_for(&self, kept: usize) -> Option<&TextureHandle> {
        self.frames
            .iter()
            .take_while(|(index, _)| *index <= kept)
            .last()
            .or_else(|| self.frames.first())
            .map(|(_, texture)| texture)
    }

    /// The dialog's body.
    pub(super) fn ui(&mut self, ui: &mut Ui, theme: &Theme) -> Outcome {
        self.take_frames(ui.ctx());
        let (fps, shown, _, seconds) = self.timing();
        // The clock: it moves on while the preview plays, round and round.
        let now = ui.input(|input| input.time);
        if self.playing && seconds > 0.0 {
            if let Some(last) = self.clock {
                self.at = (self.at + (now - last).max(0.0)) % seconds;
            }
            self.clock = Some(now);
            ui.ctx().request_repaint();
        } else {
            self.clock = None;
        }
        self.preview(ui, theme);
        ui.add_space(theme.space(Space::S3));
        self.controls(ui, seconds, theme);
        let size = self.export.size(self.shape.unwrap_or([16, 9]));
        if shown > 0 {
            help(
                ui,
                &tr!(
                    "timelapse-video-size",
                    width = size[0],
                    height = size[1],
                    count = shown + fps as usize,
                    fps = fps
                ),
                theme,
            );
        }
        ui.add_space(theme.space(Space::S5));
        self.form(ui, theme);
        help(ui, &tr!("timelapse-formats"), theme);
        Outcome::Open
    }

    /// The picture box: the video's shape, the drawing in it.
    fn preview(&self, ui: &mut Ui, theme: &Theme) {
        let width = ui.available_width();
        let room = theme.space(Space::S9) * PREVIEW_ROOM;
        let video = self.export.size(self.shape.unwrap_or([16, 9]));
        let ratio = video[0] as f32 / video[1].max(1) as f32;
        let height = (width / ratio).min(room);
        let (area, response) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
        let name = tr!("timelapse-preview");
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, &name));
        let picture = Rect::from_center_size(area.center(), vec2(height * ratio, height));
        let painter = ui.painter();
        // The ground around the video, so its shape shows.
        painter.rect(
            area,
            theme.radius(Radius::Md),
            theme.surface(1),
            theme.hairline(),
            egui::StrokeKind::Inside,
        );
        if self.paths.is_empty() || (self.frames.is_empty() && !self.ready()) {
            let text = if self.paths.is_empty() {
                tr!("canvas-has-no-timelapse-yet-record")
            } else {
                tr!("timelapse-preview-loading")
            };
            painter.text(
                area.center(),
                egui::Align2::CENTER_CENTER,
                text,
                theme.text_font(TextRole::Body),
                theme.text_muted(),
            );
            return;
        }
        let texture = self.texture_for(self.kept_at(self.at));
        timelapse_picture(painter, picture, texture);
    }

    /// Play or stop, where it is, and how long it lasts.
    fn controls(&mut self, ui: &mut Ui, seconds: f64, theme: &Theme) {
        ui.horizontal(|ui| {
            let (icon, name) = if self.playing {
                (Icon::Pause, tr!("timelapse-pause"))
            } else {
                (Icon::Play, tr!("timelapse-play"))
            };
            let enabled = seconds > 0.0;
            // Framed, so the one control that plays stands out by the
            // track.
            if wm_ui::icon_button("timelapse-play", icon, &name)
                .kind(wm_ui::ButtonKind::Secondary)
                .enabled(enabled)
                .show(ui, theme)
                .clicked()
            {
                self.playing = !self.playing;
                self.clock = None;
            }
            let time = tr!(
                "timelapse-time",
                now = clock_text(self.at),
                total = clock_text(seconds)
            );
            let label_width = ui
                .painter()
                .layout_no_wrap(
                    time.clone(),
                    theme.text_font(TextRole::Caption),
                    theme.text_muted(),
                )
                .size()
                .x;
            let scrub = tr!("timelapse-scrub");
            let mut at = self.at;
            let width = (ui.available_width() - label_width - theme.space(Space::S5)).max(0.0);
            let moved = wm_ui::slider("timelapse-scrub", &mut at, 0.0..=seconds.max(0.01))
                .name(&scrub)
                .field(false)
                .width(width)
                .enabled(enabled)
                .show(ui, theme);
            if moved.changed() {
                // Moving the knob holds the preview there.
                self.at = at;
                self.playing = false;
                self.clock = None;
            }
            ui.label(
                egui::RichText::new(time)
                    .font(theme.text_font(TextRole::Caption))
                    .color(theme.text_muted()),
            );
        });
    }

    /// The long side, the length and the shape.
    fn form(&mut self, ui: &mut Ui, theme: &Theme) {
        let export = &mut self.export;
        let sides: Vec<String> = SIDES.iter().map(|side| tr!("px-2", side = *side)).collect();
        let lengths = [tr!("fit-to-specified-length"), tr!("export-all-frames")];
        let aspects: Vec<String> = Aspect::ALL.iter().map(|aspect| aspect.label()).collect();
        wm_ui::form_grid(ui, "anim-timelapse-form", theme, |grid| {
            grid.row(&tr!("long-side"), |ui| {
                let mut chosen = SIDES
                    .iter()
                    .position(|side| *side == export.side)
                    .unwrap_or(2);
                if wm_ui::dropdown("anim-timelapse-side", &mut chosen, &sides)
                    .show(ui, theme)
                    .changed()
                {
                    export.side = SIDES[chosen];
                }
            });
            grid.row(&tr!("aspect-ratio"), |ui| {
                let mut chosen = Aspect::ALL
                    .iter()
                    .position(|aspect| *aspect == export.aspect)
                    .unwrap_or(0);
                if wm_ui::dropdown("anim-timelapse-aspect", &mut chosen, &aspects)
                    .show(ui, theme)
                    .changed()
                {
                    export.aspect = Aspect::ALL[chosen];
                }
            });
            grid.row(&tr!("length"), |ui| {
                let mut chosen = usize::from(export.seconds.is_none());
                if wm_ui::dropdown("anim-timelapse-length", &mut chosen, &lengths)
                    .show(ui, theme)
                    .changed()
                {
                    export.seconds = (chosen == 0).then_some(30.0);
                }
            });
            if let Some(seconds) = export.seconds.as_mut() {
                grid.row("", |ui| {
                    let mut value = f64::from(*seconds);
                    if wm_ui::number_field("anim-timelapse-seconds", &mut value)
                        .range(1.0..=600.0)
                        .decimals(0)
                        .suffix(" s")
                        .show(ui, theme)
                        .changed()
                    {
                        *seconds = value as f32;
                    }
                });
            }
        });
        // The length changed: the preview starts again if it went past.
        let (_, _, _, seconds) = self.timing();
        if self.at > seconds {
            self.at = 0.0;
        }
    }
}

/// The video's picture in `rect`: white, as the video is where the
/// drawing does not reach, and the drawing fitted in its middle.
fn timelapse_picture(painter: &egui::Painter, rect: Rect, texture: Option<&TextureHandle>) {
    painter.rect_filled(rect, 0.0, egui::Color32::WHITE);
    let Some(texture) = texture else {
        return;
    };
    let size = texture.size_vec2();
    let scale = (rect.width() / size.x.max(1.0)).min(rect.height() / size.y.max(1.0));
    let shown = Rect::from_center_size(rect.center(), size * scale);
    painter.image(
        texture.id(),
        shown,
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );
}

/// `seconds` as minutes and seconds, `1:05`.
fn clock_text(seconds: f64) -> String {
    let whole = seconds.max(0.0).floor() as u64;
    format!("{}:{:02}", whole / 60, whole % 60)
}

/// A line of muted help.
fn help(ui: &mut Ui, text: &str, theme: &Theme) {
    ui.label(
        egui::RichText::new(text)
            .font(theme.text_font(TextRole::Caption))
            .color(theme.text_muted()),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_preview_reads_a_spread_of_frames_with_the_first_and_the_last() {
        assert!(preview_picks(0).is_empty());
        assert_eq!(preview_picks(1), vec![0]);
        assert_eq!(preview_picks(3), vec![0, 1, 2]);
        let many = preview_picks(1000);
        assert_eq!(many.len(), PREVIEW_FRAMES);
        assert_eq!((many[0], *many.last().unwrap()), (0, 999));
        assert_eq!(clock_text(65.4), "1:05");
    }

    #[test]
    fn the_preview_plays_the_frames_the_video_shows_at_its_speed() {
        let paths: Vec<PathBuf> = (0..10)
            .map(|index| PathBuf::from(format!("{index:06}.jpg")))
            .collect();
        let mut dialog = TimelapseDialog::open(Vec::new(), Export::default());
        dialog.paths = Arc::new(paths.clone());
        // Every frame at 30 a second, the last held a second.
        dialog.export.seconds = None;
        let (fps, shown, step, seconds) = dialog.timing();
        assert_eq!((fps, shown, step), (30, 10, 1));
        assert!((seconds - 40.0 / 30.0).abs() < 1e-9);
        let plan = timelapse::Plan::sized(&paths, dialog.export, [100, 100]);
        assert_eq!(plan.len(), 40, "as long as the video");
        assert_eq!(dialog.kept_at(0.0), 0);
        assert_eq!(dialog.kept_at(3.5 / 30.0), 3);
        assert_eq!(dialog.kept_at(1.2), 9, "the last held");
        // Fit to two seconds: five a second, every frame.
        dialog.export.seconds = Some(2.0);
        let (fps, shown, _, _) = dialog.timing();
        assert_eq!((fps, shown), (5, 10));
        assert_eq!(dialog.kept_at(0.5), 2);
    }
}
