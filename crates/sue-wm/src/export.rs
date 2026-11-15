//! The export settings dialog: what a picture or a sound becomes on its
//! way to a file. Every studio's export shows it before it writes, and
//! the batch list ([`crate::job_list`]) shows its picture page above the
//! files it converts.
//!
//! - The picture page: the format (every type of
//!   [`sue_io::PICTURE_SAVE_TYPES`]), the output size in six ways, the
//!   resampling, color or grays, the background for transparency, the
//!   JPEG quality and the JPEG XL effort (each shown only for its
//!   format), and a note when the format has a largest side and the
//!   picture is fitted into it (an icon is at most 256 pixels).
//! - The sound page: the format (WAV, FLAC or AIFF), the bits, the
//!   sample rate, the channels, and normalizing the loudest peak.
//!
//! The settings are plain values ([`PictureSettings`], [`SoundSettings`]);
//! the work is done by `sue_io`'s pure functions
//! ([`sue_io::picture_ops`], [`sue_io::transform_sound`]), so it runs on a
//! background thread as well as here. JPEG quality and JPEG XL effort
//! start from the rice keys `io.jpeg_quality` and `io.jxl_effort`, which
//! a person sets in Settings or in their rice file.

use egui::{Context, Ui};
use sue_core::{PCMFloat1D, RasterTile2D};
use sue_io::picture_ops::{
    Filter, PictureOptions, Resize, flattens, resized, transform_picture, write_picture,
};
use sue_io::{PICTURE_SAVE_TYPES, PictureFormat, SampleDepth, SoundOptions};
use sue_rice::RiceSchema;

use crate::Theme;
use crate::ui::{Answer, Dialog, checkbox, dropdown, form_grid, number_field, segmented, slider};

/// The largest width or height an output size may ask for, in pixels.
const MOST_PIXELS: u32 = 32_768;

/// How the output size is chosen, in the order the dialog offers them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SizeMode {
    /// The picture's own size.
    Keep,
    /// A percentage of it.
    Scale,
    /// A width, the height following.
    Width,
    /// A height, the width following.
    Height,
    /// Inside a box, keeping the shape.
    Fit,
    /// Exactly a width and a height.
    Exact,
}

impl SizeMode {
    /// Every way, in the dialog's order.
    pub const ALL: [Self; 6] = [
        Self::Keep,
        Self::Scale,
        Self::Width,
        Self::Height,
        Self::Fit,
        Self::Exact,
    ];

    /// What people read.
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Self::Keep => tr!("export-size-keep"),
            Self::Scale => tr!("export-size-scale"),
            Self::Width => tr!("export-size-width"),
            Self::Height => tr!("export-size-height"),
            Self::Fit => tr!("export-size-fit"),
            Self::Exact => tr!("export-size-exact"),
        }
    }
}

/// Everything the picture page sets, and in the batch list the group
/// "Change every picture" too.
#[derive(Clone, Debug, PartialEq)]
pub struct PictureSettings {
    /// The format, as an index into [`PICTURE_SAVE_TYPES`].
    pub format: usize,
    /// How the output size is chosen.
    pub size: SizeMode,
    /// The percentage of [`SizeMode::Scale`], 1 to 400.
    pub percent: f64,
    /// The width of [`SizeMode::Width`], [`SizeMode::Fit`] and
    /// [`SizeMode::Exact`], in pixels.
    pub width: f64,
    /// The height of [`SizeMode::Height`], [`SizeMode::Fit`] and
    /// [`SizeMode::Exact`], in pixels.
    pub height: f64,
    /// What else changes: resampling, grays, background, quality,
    /// effort, and the batch list's turn, mirror and adjustments. Its
    /// `resize` is made from the fields above by [`Self::options`].
    pub options: PictureOptions,
}

impl Default for PictureSettings {
    fn default() -> Self {
        Self {
            format: 0,
            size: SizeMode::Keep,
            percent: 50.0,
            width: 1920.0,
            height: 1080.0,
            options: PictureOptions::default(),
        }
    }
}

/// A whole number of pixels from a field's value.
fn pixels(value: f64) -> u32 {
    value.round().clamp(1.0, f64::from(MOST_PIXELS)) as u32
}

impl PictureSettings {
    /// The defaults, with the JPEG quality and JPEG XL effort of `rice`.
    #[must_use]
    pub fn from_rice(rice: &RiceSchema) -> Self {
        let mut settings = Self::default();
        let number = |key: &str, low: i64, high: i64| {
            rice.get_i64(key)
                .and_then(|value| u8::try_from(value.clamp(low, high)).ok())
        };
        if let Some(quality) = number("io.jpeg_quality", 1, 100) {
            settings.options.jpeg_quality = quality;
        }
        if let Some(effort) = number("io.jxl_effort", 1, 9) {
            settings.options.jxl_effort = effort;
        }
        settings
    }

    /// The extension files are written with, without its dot.
    #[must_use]
    pub fn extension(&self) -> &'static str {
        PICTURE_SAVE_TYPES
            .get(self.format)
            .unwrap_or(&PICTURE_SAVE_TYPES[0])
            .1[0]
    }

    /// Chooses the format whose extensions hold `extension`; `false`
    /// when Sue writes no such pictures.
    pub fn choose_format(&mut self, extension: &str) -> bool {
        let extension = extension.trim_start_matches('.').to_ascii_lowercase();
        match PICTURE_SAVE_TYPES
            .iter()
            .position(|(_, extensions)| extensions.contains(&extension.as_str()))
        {
            Some(index) => {
                self.format = index;
                true
            }
            None => false,
        }
    }

    /// The resize the size fields ask for.
    #[must_use]
    pub fn resize(&self) -> Resize {
        let (width, height) = (pixels(self.width), pixels(self.height));
        match self.size {
            SizeMode::Keep => Resize::Keep,
            SizeMode::Scale => Resize::Scale(self.percent.clamp(1.0, 400.0) as f32),
            SizeMode::Width => Resize::Width(width),
            SizeMode::Height => Resize::Height(height),
            SizeMode::Fit => Resize::Fit(width, height),
            SizeMode::Exact => Resize::Exact(width, height),
        }
    }

    /// The options `sue_io` works with.
    #[must_use]
    pub fn options(&self) -> PictureOptions {
        PictureOptions {
            resize: self.resize(),
            ..self.options
        }
    }

    /// `true` when the chosen format holds no transparency, so the
    /// background is always used.
    #[must_use]
    pub fn flattens(&self) -> bool {
        flattens(self.extension())
    }

    /// The largest width or height the chosen format holds, if it has one.
    #[must_use]
    pub fn max_side(&self) -> Option<u32> {
        PictureFormat::from_extension(self.extension()).and_then(PictureFormat::max_side)
    }

    /// The size a picture of `width` by `height` is written at, turned
    /// as the options say and fitted into the format's largest side.
    #[must_use]
    pub fn output_size(&self, width: u32, height: u32) -> (u32, u32) {
        let mut size = resized(width, height, self.resize());
        if let Some(max) = self.max_side()
            && size.0.max(size.1) > max
        {
            size = resized(size.0, size.1, Resize::Fit(max, max));
        }
        if self.options.quarter_turns % 2 == 1 {
            (size.1, size.0)
        } else {
            size
        }
    }

    /// The note under the page when the format has a largest side: always
    /// for small ones (an icon), and for large ones when the picture of
    /// `source` size would pass it.
    #[must_use]
    pub fn fit_note(&self, source: Option<(u32, u32)>) -> Option<String> {
        let max = self.max_side()?;
        let passes = source.is_some_and(|(width, height)| {
            let size = resized(width, height, self.resize());
            size.0.max(size.1) > max
        });
        (max <= 256 || passes).then(|| {
            tr!(
                "export-fit-note",
                format =
                    PictureFormat::from_extension(self.extension()).map_or("", PictureFormat::name),
                max = sue_lang::number(f64::from(max), 0)
            )
        })
    }

    /// Changes `tile` as the settings say and writes it in the chosen
    /// format.
    ///
    /// # Errors
    /// What `sue_io` says: float pixels, or the encoder's message.
    pub fn export(&self, tile: &RasterTile2D) -> Result<Vec<u8>, String> {
        let options = self.options();
        let extension = self.extension();
        let changed = transform_picture(tile, &options, extension)?;
        write_picture(&changed, extension, &options)
    }
}

/// The name of the picture format at `index` of [`PICTURE_SAVE_TYPES`],
/// in the language in use.
#[must_use]
pub fn format_name(index: usize) -> String {
    let (name, _) = PICTURE_SAVE_TYPES
        .get(index)
        .unwrap_or(&PICTURE_SAVE_TYPES[0]);
    sue_lang::translate("io", name)
}

/// The sound formats the sound page writes: the name `sue_io` gives
/// each, and its extension.
pub const SOUND_FORMATS: [(&str, &str); 3] = [
    ("WAV sound", "wav"),
    ("FLAC sound", "flac"),
    ("AIFF sound", "aiff"),
];

/// The sample rates offered; `None` keeps the sound's own.
pub const SAMPLE_RATES: [Option<u32>; 9] = [
    None,
    Some(8_000),
    Some(16_000),
    Some(22_050),
    Some(32_000),
    Some(44_100),
    Some(48_000),
    Some(88_200),
    Some(96_000),
];

/// The channel counts offered; `None` keeps the sound's own.
pub const CHANNELS: [Option<u16>; 3] = [None, Some(1), Some(2)];

/// Everything the sound page sets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoundSettings {
    /// The format, as an index into [`SOUND_FORMATS`].
    pub format: usize,
    /// Bits per sample. 32-bit float is for WAV only.
    pub depth: SampleDepth,
    /// The sample rate, as an index into [`SAMPLE_RATES`].
    pub rate: usize,
    /// The channels, as an index into [`CHANNELS`].
    pub channels: usize,
    /// Bring the loudest peak to -1 dB.
    pub normalize: bool,
}

impl Default for SoundSettings {
    fn default() -> Self {
        Self {
            format: 0,
            depth: SampleDepth::Int16,
            rate: 0,
            channels: 0,
            normalize: false,
        }
    }
}

impl SoundSettings {
    /// The extension files are written with, without its dot.
    #[must_use]
    pub fn extension(&self) -> &'static str {
        SOUND_FORMATS
            .get(self.format)
            .unwrap_or(&SOUND_FORMATS[0])
            .1
    }

    /// The depths the chosen format writes.
    #[must_use]
    pub fn depths(&self) -> &'static [SampleDepth] {
        if self.extension() == "wav" {
            &[SampleDepth::Int16, SampleDepth::Int24, SampleDepth::Float32]
        } else {
            &[SampleDepth::Int16, SampleDepth::Int24]
        }
    }

    /// The depth written: the chosen one, or 24-bit where the format has
    /// no float samples.
    #[must_use]
    pub fn written_depth(&self) -> SampleDepth {
        if self.depths().contains(&self.depth) {
            self.depth
        } else {
            SampleDepth::Int24
        }
    }

    /// The options `sue_io` works with. Volume, cuts and fades belong to
    /// a clip's own settings on the timeline, not to an export.
    #[must_use]
    pub fn options(&self) -> SoundOptions {
        SoundOptions {
            sample_rate: SAMPLE_RATES.get(self.rate).copied().flatten(),
            channels: CHANNELS.get(self.channels).copied().flatten(),
            depth: self.written_depth(),
            normalize: self.normalize,
            ..SoundOptions::default()
        }
    }

    /// Changes `pcm` as the settings say and writes it in the chosen
    /// format.
    ///
    /// # Errors
    /// What `sue_io` says, e.g. that no sound is left.
    pub fn export(&self, pcm: &PCMFloat1D) -> Result<Vec<u8>, String> {
        let options = self.options();
        let changed = sue_io::transform_sound(pcm, &options)?;
        sue_io::write_sound(&changed, self.extension(), options.depth)
    }
}

/// A depth as people read it.
fn depth_label(depth: SampleDepth) -> String {
    match depth {
        SampleDepth::Int16 => tr!("export-bits-16"),
        SampleDepth::Int24 => tr!("export-bits-24"),
        SampleDepth::Float32 => tr!("export-bits-float"),
    }
}

/// A sample rate as people read it.
fn rate_label(rate: Option<u32>) -> String {
    match rate {
        None => tr!("export-rate-keep"),
        Some(44_100) => tr!("export-rate-cd", rate = sue_lang::number(44_100.0, 0)),
        Some(rate) => tr!(
            "export-rate-hz",
            rate = sue_lang::number(f64::from(rate), 0)
        ),
    }
}

/// A channel count as people read it.
fn channels_label(channels: Option<u16>) -> String {
    match channels {
        None => tr!("export-channels-keep"),
        Some(1) => tr!("export-channels-mono"),
        Some(2) => tr!("export-channels-stereo"),
        Some(count) => tr!("export-channels-count", count = count),
    }
}

/// Draws the picture page: the format, the output size, resampling,
/// colors, background, quality and effort. `source` is the size of the
/// picture being exported, when there is one picture; it shows the size
/// written and decides the note on a format's largest side.
pub fn picture_page(
    ui: &mut Ui,
    theme: &Theme,
    settings: &mut PictureSettings,
    source: Option<(u32, u32)>,
) {
    let formats: Vec<String> = (0..PICTURE_SAVE_TYPES.len()).map(format_name).collect();
    let sizes: Vec<String> = SizeMode::ALL.iter().map(|mode| mode.label()).collect();
    let filters: Vec<String> = Filter::ALL
        .iter()
        .map(|filter| sue_lang::translate("io", filter.name()))
        .collect();
    let pixels_suffix = tr!("export-px");
    form_grid(ui, "sue-export-picture", theme, |form| {
        form.row(&tr!("export-format"), |ui| {
            dropdown("sue-export-format", &mut settings.format, &formats)
                .label(&tr!("export-format"))
                .show(ui, theme);
        });
        form.row(&tr!("export-output-size"), |ui| {
            let mut mode = SizeMode::ALL
                .iter()
                .position(|mode| *mode == settings.size)
                .unwrap_or(0);
            if dropdown("sue-export-size", &mut mode, &sizes)
                .label(&tr!("export-output-size"))
                .show(ui, theme)
                .changed()
            {
                settings.size = SizeMode::ALL[mode];
            }
        });
        match settings.size {
            SizeMode::Keep => {}
            SizeMode::Scale => {
                form.row(&tr!("export-size-scale"), |ui| {
                    number_field("sue-export-percent", &mut settings.percent)
                        .range(1.0..=400.0)
                        .suffix(" %")
                        .show(ui, theme);
                });
            }
            mode => {
                if mode != SizeMode::Height {
                    form.row(&tr!("export-size-width"), |ui| {
                        number_field("sue-export-width", &mut settings.width)
                            .range(1.0..=f64::from(MOST_PIXELS))
                            .suffix(&pixels_suffix)
                            .show(ui, theme);
                    });
                }
                if mode != SizeMode::Width {
                    form.row(&tr!("export-size-height"), |ui| {
                        number_field("sue-export-height", &mut settings.height)
                            .range(1.0..=f64::from(MOST_PIXELS))
                            .suffix(&pixels_suffix)
                            .show(ui, theme);
                    });
                }
            }
        }
        if settings.size != SizeMode::Keep {
            form.row(&tr!("export-resampling"), |ui| {
                let mut chosen = Filter::ALL
                    .iter()
                    .position(|filter| *filter == settings.options.filter)
                    .unwrap_or(0);
                if dropdown("sue-export-filter", &mut chosen, &filters)
                    .label(&tr!("export-resampling"))
                    .show(ui, theme)
                    .changed()
                {
                    settings.options.filter = Filter::ALL[chosen];
                }
            });
        }
        form.row(&tr!("export-colors"), |ui| {
            let options = [tr!("export-colors-color"), tr!("export-colors-grays")];
            let chosen = usize::from(settings.options.grayscale);
            if let Some(picked) = segmented("sue-export-grays", &options, Some(chosen))
                .show(ui, theme)
                .picked
            {
                settings.options.grayscale = picked == 1;
            }
        });
        let forced = settings.flattens();
        form.row(&tr!("export-background"), |ui| {
            ui.horizontal(|ui| {
                let on = forced || settings.options.flatten;
                if checkbox("sue-export-flatten", &tr!("export-fill-transparency"), on)
                    .enabled(!forced)
                    .show(ui, theme)
                    .clicked()
                {
                    settings.options.flatten = !settings.options.flatten;
                }
                if on {
                    let [r, g, b] = settings.options.background;
                    let mut rgba = [r, g, b, 255];
                    if crate::color_button(ui, "sue-export-background", &mut rgba, false, theme)
                        .changed
                    {
                        settings.options.background = [rgba[0], rgba[1], rgba[2]];
                    }
                }
            });
        });
        if forced {
            form.wide(|ui| note(ui, theme, &tr!("export-no-transparency")));
        }
        match settings.extension() {
            "jpg" => {
                form.row(&tr!("export-quality"), |ui| {
                    let mut quality = f64::from(settings.options.jpeg_quality);
                    if slider("sue-export-quality", &mut quality, 1.0..=100.0)
                        .step(1.0)
                        .name(&tr!("export-quality"))
                        .show(ui, theme)
                        .changed()
                    {
                        settings.options.jpeg_quality = quality.round().clamp(1.0, 100.0) as u8;
                    }
                });
            }
            "jxl" => {
                form.row(&tr!("export-effort"), |ui| {
                    let mut effort = f64::from(settings.options.jxl_effort);
                    if slider("sue-export-effort", &mut effort, 1.0..=9.0)
                        .step(1.0)
                        .name(&tr!("export-effort"))
                        .show(ui, theme)
                        .on_hover_text(tr!("export-effort-hint"))
                        .changed()
                    {
                        settings.options.jxl_effort = effort.round().clamp(1.0, 9.0) as u8;
                    }
                });
            }
            _ => {}
        }
        if let Some((width, height)) = source {
            let (width, height) = settings.output_size(width, height);
            form.wide(|ui| {
                note(
                    ui,
                    theme,
                    &tr!("export-written-at", width = width, height = height),
                );
            });
        }
        if let Some(text) = settings.fit_note(source) {
            form.wide(|ui| note(ui, theme, &text));
        }
    });
}

/// Draws the sound page: the format, the bits, the sample rate, the
/// channels and normalizing.
pub fn sound_page(ui: &mut Ui, theme: &Theme, settings: &mut SoundSettings) {
    let formats: Vec<String> = SOUND_FORMATS
        .iter()
        .map(|(name, _)| sue_lang::translate("io", name))
        .collect();
    let rates: Vec<String> = SAMPLE_RATES.iter().map(|rate| rate_label(*rate)).collect();
    let channels: Vec<String> = CHANNELS
        .iter()
        .map(|count| channels_label(*count))
        .collect();
    form_grid(ui, "sue-export-sound", theme, |form| {
        form.row(&tr!("export-format"), |ui| {
            dropdown("sue-export-sound-format", &mut settings.format, &formats)
                .label(&tr!("export-format"))
                .show(ui, theme);
        });
        form.row(&tr!("export-bits"), |ui| {
            let depths = settings.depths();
            let labels: Vec<String> = depths.iter().map(|depth| depth_label(*depth)).collect();
            let written = settings.written_depth();
            let mut chosen = depths
                .iter()
                .position(|depth| *depth == written)
                .unwrap_or(0);
            if dropdown("sue-export-bits", &mut chosen, &labels)
                .label(&tr!("export-bits"))
                .show(ui, theme)
                .changed()
            {
                settings.depth = depths[chosen];
            }
        });
        form.row(&tr!("export-sample-rate"), |ui| {
            dropdown("sue-export-rate", &mut settings.rate, &rates)
                .label(&tr!("export-sample-rate"))
                .show(ui, theme);
        });
        form.row(&tr!("export-channels"), |ui| {
            dropdown("sue-export-channels", &mut settings.channels, &channels)
                .label(&tr!("export-channels"))
                .show(ui, theme);
        });
        form.wide(|ui| {
            if checkbox(
                "sue-export-normalize",
                &tr!("export-normalize"),
                settings.normalize,
            )
            .show(ui, theme)
            .clicked()
            {
                settings.normalize = !settings.normalize;
            }
        });
    });
}

/// A muted line under a form.
fn note(ui: &mut Ui, theme: &Theme, text: &str) {
    ui.add(
        egui::Label::new(
            egui::RichText::new(text)
                .font(theme.text_font(crate::ui::TextRole::Caption))
                .color(theme.text_muted()),
        )
        .wrap(),
    );
}

/// Which page an [`ExportDialog`] shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ExportPage {
    /// A picture is exported.
    Picture,
    /// A sound is exported.
    Sound,
}

/// The export settings dialog. Keep it while it is open and
/// [`show`](Self::show) it every frame; on [`Answer::Accept`] the caller
/// exports with [`Self::picture`] or [`Self::sound`].
#[derive(Clone, Debug, PartialEq)]
pub struct ExportDialog {
    /// The page shown.
    pub page: ExportPage,
    /// The picture page's settings.
    pub picture: PictureSettings,
    /// The sound page's settings.
    pub sound: SoundSettings,
    /// The size of the picture exported, when there is one.
    pub source: Option<(u32, u32)>,
    /// The dialog's title, e.g. the command that opened it.
    pub title: String,
}

impl ExportDialog {
    /// The dialog for a picture of `source` size, starting from
    /// `settings`.
    #[must_use]
    pub fn for_picture(
        title: impl Into<String>,
        settings: PictureSettings,
        source: Option<(u32, u32)>,
    ) -> Self {
        Self {
            page: ExportPage::Picture,
            picture: settings,
            sound: SoundSettings::default(),
            source,
            title: title.into(),
        }
    }

    /// The dialog for a sound, starting from `settings`.
    #[must_use]
    pub fn for_sound(title: impl Into<String>, settings: SoundSettings) -> Self {
        Self {
            page: ExportPage::Sound,
            picture: PictureSettings::default(),
            sound: settings,
            source: None,
            title: title.into(),
        }
    }

    /// The extension the export writes, without its dot.
    #[must_use]
    pub fn extension(&self) -> &'static str {
        match self.page {
            ExportPage::Picture => self.picture.extension(),
            ExportPage::Sound => self.sound.extension(),
        }
    }

    /// Shows the dialog over `ctx` for one frame. Returns the answer in
    /// the frame it is given: [`Answer::Accept`] for Export,
    /// [`Answer::Cancel`] otherwise.
    pub fn show(&mut self, ctx: &Context, theme: &Theme) -> Option<Answer> {
        let title = self.title.clone();
        let page = self.page;
        Dialog::new(("sue-export", page), &title, theme)
            .accept(tr!("export-accept"))
            .cancel()
            .show(ctx, |ui| match page {
                ExportPage::Picture => picture_page(ui, theme, &mut self.picture, self.source),
                ExportPage::Sound => sound_page(ui, theme, &mut self.sound),
            })
            .answer
    }
}

#[cfg(test)]
mod tests {
    use egui::{Event, Key, Modifiers, Pos2, RawInput, Rect, vec2};
    use sue_core::PixelFormat;

    use super::*;

    fn card(width: u32, height: u32) -> RasterTile2D {
        let pixels = (0..width * height)
            .flat_map(|index| [(index % 251) as u8, 90, 200, 255])
            .collect();
        RasterTile2D::from_bytes(width, height, PixelFormat::Rgba8, pixels).unwrap()
    }

    #[test]
    fn the_size_fields_make_the_resize_and_the_size_written() {
        let mut settings = PictureSettings::default();
        assert_eq!(settings.resize(), Resize::Keep);
        assert_eq!(settings.output_size(400, 200), (400, 200));
        settings.size = SizeMode::Scale;
        settings.percent = 25.0;
        assert_eq!(settings.output_size(400, 200), (100, 50));
        settings.size = SizeMode::Fit;
        settings.width = 100.0;
        settings.height = 100.0;
        assert_eq!(settings.resize(), Resize::Fit(100, 100));
        assert_eq!(settings.output_size(400, 200), (100, 50));
        settings.size = SizeMode::Exact;
        settings.width = 0.0;
        assert_eq!(settings.resize(), Resize::Exact(1, 100), "at least a pixel");
        // A quarter turn swaps the sides.
        settings.size = SizeMode::Keep;
        settings.options.quarter_turns = 1;
        assert_eq!(settings.output_size(400, 200), (200, 400));
        assert_eq!(settings.options().resize, Resize::Keep);
    }

    #[test]
    fn a_format_with_a_largest_side_fits_and_says_so() {
        let mut settings = PictureSettings::default();
        assert_eq!(settings.extension(), "png");
        assert!(settings.fit_note(Some((5000, 5000))).is_none());
        assert!(settings.choose_format(".ICO"));
        assert_eq!(settings.extension(), "ico");
        assert_eq!(settings.output_size(600, 300), (256, 128));
        let note = settings.fit_note(None).expect("icons are small");
        assert!(note.contains("256"), "{note}");
        // WebP's side is large: the note only when the picture passes it.
        assert!(settings.choose_format("webp"));
        assert!(settings.fit_note(Some((800, 600))).is_none());
        assert!(settings.fit_note(Some((20_000, 100))).is_some());
        assert_eq!(settings.output_size(20_000, 100), (16_383, 82));
        assert!(!settings.choose_format("wav"));
        assert_eq!(
            settings.extension(),
            "webp",
            "a refused format changes nothing"
        );
    }

    #[test]
    fn formats_without_transparency_use_the_background() {
        let mut settings = PictureSettings::default();
        assert!(!settings.flattens());
        for extension in ["jpg", "ppm"] {
            assert!(settings.choose_format(extension));
            assert!(settings.flattens(), "{extension}");
        }
        settings.options.background = [10, 20, 30];
        let clear = RasterTile2D::from_bytes(1, 1, PixelFormat::Rgba8, vec![0; 4]).unwrap();
        let bytes = settings.export(&clear).unwrap();
        let back =
            sue_io::picture_ops::read_picture(std::path::Path::new("a.ppm"), &bytes).unwrap();
        assert_eq!(back.bytes().unwrap(), [10, 20, 30, 255]);
    }

    #[test]
    fn every_picture_format_exports_with_its_settings() {
        let mut settings = PictureSettings {
            size: SizeMode::Width,
            width: 12.0,
            ..PictureSettings::default()
        };
        settings.options.grayscale = true;
        for index in 0..PICTURE_SAVE_TYPES.len() {
            settings.format = index;
            let bytes = settings.export(&card(24, 8)).unwrap();
            let path = std::path::Path::new("out").with_extension(settings.extension());
            let back = sue_io::picture_ops::read_picture(&path, &bytes).unwrap();
            assert_eq!((back.width(), back.height()), (12, 4), "{path:?}");
        }
    }

    #[test]
    fn quality_and_effort_start_from_the_rice() {
        let mut rice = RiceSchema::parse(sue_rice::DEFAULT_RICE).unwrap();
        let defaults = PictureSettings::from_rice(&rice);
        assert_eq!(defaults.options.jpeg_quality, 90);
        assert_eq!(defaults.options.jxl_effort, 7);
        rice.set("io.jpeg_quality", sue_rice::RiceValue::Int(55))
            .unwrap();
        rice.set("io.jxl_effort", sue_rice::RiceValue::Int(3))
            .unwrap();
        let riced = PictureSettings::from_rice(&rice);
        assert_eq!(riced.options.jpeg_quality, 55);
        assert_eq!(riced.options.jxl_effort, 3);
    }

    #[test]
    fn the_sound_page_writes_rate_channels_bits_and_loudness() {
        let rate = 8_000;
        let samples: Vec<f32> = (0..rate).flat_map(|_| [0.25_f32, 0.25]).collect();
        let pcm = PCMFloat1D::new(rate, 2, samples).unwrap();
        let settings = SoundSettings {
            format: 1,
            depth: SampleDepth::Int24,
            rate: SAMPLE_RATES
                .iter()
                .position(|r| *r == Some(16_000))
                .unwrap(),
            channels: CHANNELS.iter().position(|c| *c == Some(1)).unwrap(),
            normalize: true,
        };
        assert_eq!(settings.extension(), "flac");
        let bytes = settings.export(&pcm).unwrap();
        let (back, _) = sue_io::decode_audio(bytes, Some("flac")).unwrap();
        assert_eq!((back.pcm.sample_rate(), back.pcm.channels()), (16_000, 1));
        // Normalized: the loudest peak at -1 dB.
        let peak = back
            .pcm
            .samples()
            .iter()
            .fold(0.0_f32, |m, s| m.max(s.abs()));
        assert!((peak - 0.891).abs() < 0.01, "{peak}");
        // Float samples are for WAV only.
        let mut float = SoundSettings {
            depth: SampleDepth::Float32,
            ..SoundSettings::default()
        };
        assert_eq!(float.written_depth(), SampleDepth::Float32);
        float.format = 2;
        assert_eq!(float.written_depth(), SampleDepth::Int24);
        assert_eq!(float.options().depth, SampleDepth::Int24);
        // Nothing chosen keeps the sound as it is.
        let kept = SoundSettings::default().options();
        assert_eq!((kept.sample_rate, kept.channels), (None, None));
        assert!(!kept.normalize);
    }

    #[test]
    fn every_name_shown_is_in_the_language_database() {
        let pictures = PICTURE_SAVE_TYPES.iter().map(|(name, _)| *name);
        let sounds = SOUND_FORMATS.iter().map(|(name, _)| *name);
        for name in pictures.chain(sounds) {
            assert!(sue_lang::has_english("io", name), "{name}");
        }
        for mode in SizeMode::ALL {
            assert!(!mode.label().is_empty());
        }
    }

    /// Runs `frames` frames of the dialog in a window of 1000 by 800
    /// points, the last with `events`, and returns the last answer.
    fn run(dialog: &mut ExportDialog, frames: usize, events: Vec<Event>) -> Option<Answer> {
        let ctx = Context::default();
        let theme = Theme::from_rice(&RiceSchema::default());
        theme.motion.store(&ctx);
        let mut answer = None;
        for frame in 0..frames {
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 800.0))),
                time: Some(frame as f64 * 0.1),
                predicted_dt: 0.1,
                events: if frame + 1 == frames {
                    events.clone()
                } else {
                    Vec::new()
                },
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| answer = dialog.show(ui.ctx(), &theme));
            output.textures_delta.clear();
        }
        answer
    }

    #[test]
    fn the_dialog_shows_each_page_and_answers() {
        let mut settings = PictureSettings::default();
        assert!(settings.choose_format("jpg"));
        settings.size = SizeMode::Fit;
        let mut picture = ExportDialog::for_picture("Export", settings.clone(), Some((800, 600)));
        assert_eq!(run(&mut picture, 6, Vec::new()), None);
        assert_eq!(picture.picture, settings, "showing changes nothing");
        assert_eq!(picture.extension(), "jpg");
        let key = |key| Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        let enter = key(Key::Enter);
        assert_eq!(run(&mut picture, 6, vec![enter]), Some(Answer::Accept));
        let mut sound = ExportDialog::for_sound("Audio", SoundSettings::default());
        assert_eq!(sound.extension(), "wav");
        let escape = key(Key::Escape);
        assert_eq!(run(&mut sound, 6, vec![escape]), Some(Answer::Cancel));
    }
}
