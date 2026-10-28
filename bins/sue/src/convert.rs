//! Convert: turns an opened picture into JPEG XL, PNG or JPEG, or a song
//! into WAV. Sources are the decoded assets already in the arena, so nothing
//! is read twice; JPEG sources are recompressed bit for bit when a codec
//! allows.

use std::path::{Path, PathBuf};

use egui::{RichText, Sense, Ui, vec2};
use sue_core::{AssetHandle, ComponentKind};
use sue_io::{JxlError, JxlPipeline, NativeJxlPipeline, encode_jpeg, encode_png, encode_wav, meta};
use sue_platform::write_atomic;
use sue_wm::{AppletContext, AppletManifest, AppletRole, SueApplet, World, widgets};

use crate::util::{file_name, free_path, human_bytes};

/// The applet id.
pub const ID: &str = "convert";

/// What a source can become.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// Lossless JPEG XL.
    Jxl,
    /// PNG.
    Png,
    /// Baseline JPEG at `io.jpeg_quality`.
    Jpg,
    /// 16-bit PCM WAV.
    Wav,
}

impl Target {
    fn extension(self) -> &'static str {
        match self {
            Self::Jxl => "jxl",
            Self::Png => "png",
            Self::Jpg => "jpg",
            Self::Wav => "wav",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Jxl => ".jxl",
            Self::Png => ".png",
            Self::Jpg => ".jpg",
            Self::Wav => ".wav",
        }
    }
}

/// What kind of source an entity is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Image,
    Audio,
}

impl Kind {
    fn targets(self) -> &'static [Target] {
        match self {
            Self::Image => &[Target::Jxl, Target::Png, Target::Jpg],
            Self::Audio => &[Target::Wav],
        }
    }
}

fn kind_of(world: &World, handle: AssetHandle) -> Option<Kind> {
    let entity = world.arena.get(handle).ok()?;
    if entity.component(ComponentKind::PCMFloat1D).is_some() {
        return Some(Kind::Audio);
    }
    // Decoded pictures and paint layers; byte maps carry their entropy.
    (entity.component(ComponentKind::RasterTile2D).is_some()
        && (entity.meta(meta::WIDTH).is_some() || entity.meta(crate::paint::REVISION).is_some())
        && entity.meta(meta::ENTROPY).is_none())
    .then_some(Kind::Image)
}

/// Convertible assets in the arena, newest first.
fn candidates(world: &World) -> Vec<(AssetHandle, Kind)> {
    let mut found: Vec<(AssetHandle, Kind)> = world
        .arena
        .iter()
        .filter_map(|(handle, _)| Some((handle, kind_of(world, handle)?)))
        .collect();
    found.sort_by_key(|(handle, _)| std::cmp::Reverse(handle.index()));
    found
}

/// Converts `source` to `target`, writing next to the source file (or into
/// `out_dir`). Returns the written path and a summary.
///
/// # Errors
/// A message for the status line.
pub fn convert(
    world: &World,
    source: AssetHandle,
    target: Target,
    out_dir: &Path,
) -> Result<(PathBuf, String), String> {
    let entity = world.arena.get(source).map_err(|error| error.to_string())?;
    let stem = Path::new(entity.name()).file_stem().map_or_else(
        || "converted".to_owned(),
        |stem| stem.to_string_lossy().into_owned(),
    );
    let effort = world
        .rice
        .get_i64("io.jxl_effort")
        .and_then(|effort| u8::try_from(effort).ok())
        .unwrap_or(7);
    let mut note = None;
    let bytes = match target {
        Target::Wav => {
            let pcm = entity
                .component(ComponentKind::PCMFloat1D)
                .and_then(|data| data.as_pcm())
                .ok_or("only audio converts to WAV")?;
            encode_wav(pcm)
        }
        Target::Png | Target::Jxl | Target::Jpg => {
            let tile = entity
                .component(ComponentKind::RasterTile2D)
                .and_then(|data| data.as_raster())
                .ok_or("only pictures convert to images")?;
            let jpeg_source = entity.meta(meta::FORMAT) == Some("JPEG")
                && world
                    .rice
                    .get_bool("io.jpeg_lossless_transcode")
                    .unwrap_or(true);
            let quality = world
                .rice
                .get_i64("io.jpeg_quality")
                .and_then(|quality| u8::try_from(quality).ok())
                .unwrap_or(90);
            match target {
                Target::Png => encode_png(tile)?,
                Target::Jpg => encode_jpeg(tile, quality)?,
                _ if jpeg_source => {
                    let original = entity
                        .meta(meta::SOURCE)
                        .and_then(|path| std::fs::read(path).ok());
                    match original.map(|jpeg| NativeJxlPipeline.transcode_jpeg_lossless(&jpeg)) {
                        Some(Ok(jxl)) => jxl,
                        Some(Err(JxlError::CodecUnavailable { .. })) | None => {
                            note = Some(
                                "bit-exact JPEG recompression needs a codec package, so the pixels were encoded losslessly",
                            );
                            NativeJxlPipeline
                                .encode_layers(&[tile], effort)
                                .map_err(|error| error.to_string())?
                        }
                        Some(Err(error)) => return Err(error.to_string()),
                    }
                }
                _ => NativeJxlPipeline
                    .encode_layers(&[tile], effort)
                    .map_err(|error| error.to_string())?,
            }
        }
    };
    let path = free_path(&out_dir.join(format!("{stem}.{}", target.extension())));
    write_atomic(&path, &bytes)
        .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    let original = entity
        .meta(meta::BYTES)
        .and_then(|bytes| bytes.parse::<u64>().ok());
    let mut summary = format!(
        "Saved {} ({}{})",
        file_name(&path),
        human_bytes(bytes.len() as u64),
        original.map_or_else(String::new, |original| format!(
            ", {}% of the original",
            (bytes.len() as u64 * 100)
                .checked_div(original)
                .unwrap_or(0)
        ))
    );
    if let Some(note) = note {
        summary.push_str(&format!(". Note: {note}"));
    }
    Ok((path, summary))
}

/// The converter in the left drawer.
#[derive(Debug, Default)]
pub struct Convert {
    source: Option<AssetHandle>,
    target: Option<Target>,
    status: Option<(String, bool)>,
}

impl Convert {
    fn out_dir(cx: &AppletContext<'_>, source: AssetHandle) -> PathBuf {
        let riced = cx.rice().get_str("io.convert_dir").unwrap_or("").trim();
        if !riced.is_empty() {
            return PathBuf::from(riced);
        }
        cx.arena()
            .get(source)
            .ok()
            .and_then(|entity| entity.meta(meta::SOURCE))
            .and_then(|path| Path::new(path).parent().map(Path::to_path_buf))
            .or_else(|| cx.catalog.folders.get("home").cloned())
            .unwrap_or_default()
    }
}

impl SueApplet for Convert {
    fn manifest(&self) -> AppletManifest {
        let mut manifest = AppletManifest::new(ID, "Convert", AppletRole::LeftDrawer, "\u{1F504}");
        manifest.summary =
            "Turn pictures into JPEG XL, PNG or JPEG, and songs into WAV.".to_owned();
        manifest.order = 30;
        manifest
    }

    fn drawer_ui(&mut self, ui: &mut Ui, cx: &mut AppletContext<'_>) {
        let theme = cx.theme.clone();
        let found = candidates(cx.world);
        if self
            .source
            .is_none_or(|source| found.iter().all(|(handle, _)| *handle != source))
        {
            self.source = found.first().map(|(handle, _)| *handle);
        }
        widgets::heading(ui, "Source", &theme);
        if found.is_empty() {
            ui.label(
                RichText::new("Open a picture or a song first; it shows up here.")
                    .color(theme.muted),
            );
            return;
        }
        for (handle, kind) in &found {
            let name = cx
                .arena()
                .get(*handle)
                .map_or_else(|_| String::new(), |entity| entity.name().to_owned());
            let chosen = self.source == Some(*handle);
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
                &name,
                theme.font(theme.ui_size),
                theme.text,
            );
            ui.painter().text(
                rect.right_center() - vec2(8.0, 0.0),
                egui::Align2::RIGHT_CENTER,
                match kind {
                    Kind::Image => "Picture",
                    Kind::Audio => "Audio",
                },
                theme.font(theme.small_size),
                theme.muted,
            );
            if response.clicked() {
                self.source = Some(*handle);
            }
        }
        let Some(source) = self.source else {
            return;
        };
        let Some(kind) = found
            .iter()
            .find(|(handle, _)| *handle == source)
            .map(|(_, kind)| *kind)
        else {
            return;
        };
        let targets = kind.targets();
        if self.target.is_none_or(|target| !targets.contains(&target)) {
            self.target = targets.first().copied();
        }
        ui.add_space(theme.padding);
        widgets::heading(ui, "Format", &theme);
        let labels: Vec<String> = targets
            .iter()
            .map(|target| target.label().to_owned())
            .collect();
        let chosen = self
            .target
            .and_then(|target| targets.iter().position(|t| *t == target));
        if let Some(index) = widgets::segmented(ui, &labels, chosen, &theme) {
            self.target = targets.get(index).copied();
        }
        ui.add_space(theme.padding);
        if widgets::pill_button(ui, "Convert", false, &theme).clicked()
            && let Some(target) = self.target
        {
            let dir = Self::out_dir(cx, source);
            self.status = Some(match convert(cx.world, source, target, &dir) {
                Ok((_, summary)) => (summary, false),
                Err(error) => (error, true),
            });
        }
        if let Some((text, error)) = &self.status {
            ui.add_space(theme.padding * 0.5);
            let shown = if *error {
                format!("! {text}")
            } else {
                text.clone()
            };
            ui.label(RichText::new(shown).color(theme.muted));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sue_core::{PCMFloat1D, PixelFormat, RasterTile2D};
    use sue_rice::{DEFAULT_RICE, RiceSchema};

    #[test]
    fn pictures_and_songs_convert_to_real_files() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos());
        let dir = std::env::temp_dir().join(format!("sue-convert-{}-{unique}", std::process::id()));
        let mut world = World {
            rice: RiceSchema::parse(DEFAULT_RICE).unwrap(),
            ..World::default()
        };
        let tile = RasterTile2D::from_bytes(4, 4, PixelFormat::Rgba8, vec![120; 64]).unwrap();
        let picture = world.arena.spawn("card.png", tile.clone());
        world
            .arena
            .get_mut(picture)
            .unwrap()
            .set_meta(meta::WIDTH, "4");
        let song = world.arena.spawn(
            "tone.mp3",
            PCMFloat1D::new(8_000, 1, vec![0.25; 800]).unwrap(),
        );
        let map = world.arena.spawn("blob.bin", tile);
        world
            .arena
            .get_mut(map)
            .unwrap()
            .set_meta(meta::ENTROPY, "7.9");
        assert_eq!(
            candidates(&world),
            [(song, Kind::Audio), (picture, Kind::Image)],
            "newest first, byte maps excluded"
        );

        let (jxl, summary) = convert(&world, picture, Target::Jxl, &dir).unwrap();
        assert_eq!(file_name(&jxl), "card.jxl");
        assert!(summary.starts_with("Saved card.jxl"));
        let decoded = NativeJxlPipeline
            .decode(
                &std::fs::read(&jxl).unwrap(),
                sue_io::ProgressiveTarget::Full,
            )
            .unwrap();
        assert_eq!(decoded.tile.width(), 4);
        let (png, _) = convert(&world, picture, Target::Png, &dir).unwrap();
        assert!(std::fs::read(&png).unwrap().starts_with(b"\x89PNG"));
        let (again, _) = convert(&world, picture, Target::Png, &dir).unwrap();
        assert_eq!(file_name(&again), "card (2).png", "nothing is overwritten");
        let (jpg, _) = convert(&world, picture, Target::Jpg, &dir).unwrap();
        assert!(std::fs::read(&jpg).unwrap().starts_with(&[0xFF, 0xD8]));
        let (wav, _) = convert(&world, song, Target::Wav, &dir).unwrap();
        assert!(std::fs::read(&wav).unwrap().starts_with(b"RIFF"));
        assert!(convert(&world, song, Target::Png, &dir).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
