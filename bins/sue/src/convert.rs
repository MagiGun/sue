//! Convert: turns an opened picture into JPEG XL or PNG, or a song into WAV.
//! Sources are the decoded assets already in the arena, so nothing is read
//! twice; JPEG sources are recompressed bit for bit when a codec allows.

use std::path::{Path, PathBuf};

use sue_core::{AssetHandle, ComponentKind};
use sue_io::{JxlError, JxlPipeline, NativeJxlPipeline, encode_png, encode_wav, meta};
use sue_platform::{InputEvent, write_atomic};
use sue_wm::{
    AppletCategory, AppletContext, AppletError, AppletId, AppletManifest, InputResponse, Rect,
    ShellRequest, SueApplet, Ui, UiChoice, World, Zone, widget_id,
};

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
    /// 16-bit PCM WAV.
    Wav,
}

impl Target {
    const ALL: [Self; 3] = [Self::Jxl, Self::Png, Self::Wav];

    fn extension(self) -> &'static str {
        match self {
            Self::Jxl => "jxl",
            Self::Png => "png",
            Self::Wav => "wav",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Jxl => ".jxl",
            Self::Png => ".png",
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
            Self::Image => &[Target::Jxl, Target::Png],
            Self::Audio => &[Target::Wav],
        }
    }
}

fn kind_of(world: &World, handle: AssetHandle) -> Option<Kind> {
    let entity = world.arena.get(handle).ok()?;
    if entity.component(ComponentKind::PCMFloat1D).is_some() {
        return Some(Kind::Audio);
    }
    // Decoded pictures carry their size; byte maps carry their entropy.
    (entity.component(ComponentKind::RasterTile2D).is_some()
        && entity.meta(meta::WIDTH).is_some()
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
        Target::Png | Target::Jxl => {
            let tile = entity
                .component(ComponentKind::RasterTile2D)
                .and_then(|data| data.as_raster())
                .ok_or("only pictures convert to images")?;
            let jpeg_source = entity.meta(meta::FORMAT) == Some("JPEG")
                && world
                    .rice
                    .get_bool("io.jpeg_lossless_transcode")
                    .unwrap_or(true);
            match target {
                Target::Png => encode_png(tile)?,
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

/// The converter in the Left Dock.
#[derive(Debug, Default)]
pub struct Convert {
    source: Option<AssetHandle>,
    newest: Option<AssetHandle>,
    target: Option<Target>,
    status: Option<(String, bool)>,
    output: Option<PathBuf>,
}

impl Convert {
    fn out_dir(ctx: &AppletContext<'_>, source: AssetHandle) -> PathBuf {
        let riced = ctx.rice().get_str("io.convert_dir").unwrap_or("").trim();
        if !riced.is_empty() {
            return PathBuf::from(riced);
        }
        ctx.arena()
            .get(source)
            .ok()
            .and_then(|entity| entity.meta(meta::SOURCE))
            .and_then(|path| Path::new(path).parent().map(Path::to_path_buf))
            .or_else(|| ctx.catalog.folders.get("home").cloned())
            .unwrap_or_default()
    }
}

impl SueApplet for Convert {
    fn manifest(&self) -> AppletManifest {
        AppletManifest {
            id: AppletId::new(ID).expect("the Convert id is valid"),
            name: "Convert".to_owned(),
            category: AppletCategory::Utility,
            home_zone: Zone::LeftDock,
            uses_timeline: false,
            summary: "Turn pictures into JPEG XL or PNG and songs into WAV.".to_owned(),
            opens: Vec::new(),
            icon: "\u{1F504}".to_owned(),
            singleton: false,
        }
    }

    fn mount(&mut self, _ctx: &mut AppletContext<'_>) -> Result<(), AppletError> {
        Ok(())
    }

    fn handle_input(
        &mut self,
        _event: &InputEvent,
        _ctx: &mut AppletContext<'_>,
    ) -> Result<InputResponse, AppletError> {
        Ok(InputResponse::Ignored)
    }

    fn draw(&mut self, ui: &mut Ui<'_>, area: Rect, ctx: &mut AppletContext<'_>) {
        let theme = ui.theme;
        let pad = theme.padding;
        let row = ui.row_height();
        let found = candidates(ctx.world);
        // Follow new arrivals: the newest file opened becomes the source.
        let newest = found.first().map(|(handle, _)| *handle);
        if newest != self.newest {
            self.newest = newest;
            self.source = newest;
            self.target = None;
        }
        if self
            .source
            .is_some_and(|source| !found.iter().any(|(handle, _)| *handle == source))
        {
            self.source = newest;
        }
        let mut y = area.y;
        let caption = |ui: &mut Ui<'_>, y: f32, text: &str| {
            ui.label_sized(
                Rect::new(area.x, y, area.width, row),
                text,
                theme.small_size,
                theme.muted,
            );
        };
        caption(ui, y, "SOURCE");
        y += row;
        if found.is_empty() {
            y += ui.wrapped(
                area.x,
                y,
                area.width,
                "Open a picture (JPEG XL, PNG, JPEG) or a song (MP3, FLAC, WAV, Ogg, M4A) to convert it.",
                theme.ui_size,
                theme.muted,
            ) + pad;
        } else {
            let options: Vec<UiChoice> = found
                .iter()
                .map(|(handle, _)| {
                    UiChoice::new(
                        ctx.arena()
                            .get(*handle)
                            .map_or_else(|_| "?".to_owned(), |entity| entity.name().to_owned()),
                    )
                })
                .collect();
            let selected = self
                .source
                .and_then(|source| found.iter().position(|(handle, _)| *handle == source));
            if let Some(picked) = ui.dropdown(
                widget_id("convert", "source"),
                Rect::new(area.x, y, area.width, row),
                &options,
                selected,
            ) {
                self.source = Some(found[picked].0);
                self.target = None;
            }
            y += row + pad / 2.0;
        }
        if ui.button(
            widget_id("convert", "choose"),
            Rect::new(area.x, y, area.width, row),
            "Choose File...",
        ) {
            ctx.request(ShellRequest::Command("file.open".to_owned()));
        }
        y += row + pad;

        let kind = self
            .source
            .and_then(|source| found.iter().find(|(handle, _)| *handle == source))
            .map(|(_, kind)| *kind);
        caption(ui, y, "FORMAT");
        y += row;
        let allowed = kind.map_or(&[][..], Kind::targets);
        if self.target.is_none_or(|target| !allowed.contains(&target)) {
            self.target = allowed.first().copied();
        }
        let options: Vec<UiChoice> = Target::ALL
            .iter()
            .map(|target| UiChoice {
                label: target.label().to_owned(),
                enabled: allowed.contains(target),
            })
            .collect();
        let selected = self.target.and_then(|target| {
            Target::ALL
                .iter()
                .position(|candidate| *candidate == target)
        });
        if let Some(picked) = ui.segmented(
            widget_id("convert", "target"),
            Rect::new(area.x, y, area.width, row),
            &options,
            selected,
        ) {
            self.target = Some(Target::ALL[picked]);
        }
        y += row + pad;

        if let (Some(source), Some(target)) = (self.source, self.target) {
            let out_dir = Self::out_dir(ctx, source);
            let destination = format!("Saves into {}", out_dir.display());
            y += ui.wrapped(
                area.x,
                y,
                area.width,
                &destination,
                theme.small_size,
                theme.muted,
            ) + pad;
            if ui.primary_button(
                widget_id("convert", "go"),
                Rect::new(area.x, y, area.width, row),
                &format!("Convert to {}", target.label()),
                true,
            ) {
                match convert(ctx.world, source, target, &out_dir) {
                    Ok((path, summary)) => {
                        self.status = Some((summary, false));
                        self.output = Some(path);
                    }
                    Err(error) => {
                        self.status = Some((error, true));
                        self.output = None;
                    }
                }
            }
            y += row + pad;
        }
        if let Some((status, error)) = &self.status {
            let color = if *error { theme.alert } else { theme.text };
            y += ui.wrapped(area.x, y, area.width, status, theme.small_size, color) + pad / 2.0;
            if let Some(folder) = self.output.as_ref().and_then(|path| path.parent())
                && ui.button(
                    widget_id("convert", "reveal"),
                    Rect::new(area.x, y, area.width, row),
                    "Show in Folder",
                )
            {
                ctx.request(ShellRequest::Command(format!(
                    "reveal:{}",
                    folder.display()
                )));
            }
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
        let (wav, _) = convert(&world, song, Target::Wav, &dir).unwrap();
        assert!(std::fs::read(&wav).unwrap().starts_with(b"RIFF"));
        assert!(convert(&world, song, Target::Png, &dir).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
