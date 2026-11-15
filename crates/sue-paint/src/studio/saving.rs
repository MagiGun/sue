//! File, Save: where it writes each canvas, and the one question it asks.
//!
//! A canvas belongs to the Sue project and Save writes the project
//! (through the shell), unless it was opened from a file of another
//! format:
//!
//! - From a `.clip`: the first Save asks once whether to export back to
//!   that `.clip` or to keep working in a Sue project beside it, and
//!   never writes a file the person did not choose. Once answered, Save
//!   does that without asking.
//! - From a flat picture Sue writes (PNG, JPEG, ...): Save writes it back
//!   in its format, as Clip Studio Paint does. When the canvas has more
//!   than one layer by then, the first Save says the layers will be
//!   merged into the picture (the canvas keeps them) and asks.
//!
//! The window title names the file Save writes ([`SuePaint::save_title`]).

use std::path::{Path, PathBuf};

use sue_core::{Arena, EntityId, PixelFormat};
use sue_model::picture::LayerKind;
use sue_wm::ui::{Answer, Dialog as UiDialog, Tone, confirm};
use sue_wm::{AppletContext, FilePick, FileType, ShellRequest, Theme};

use super::SuePaint;
use crate::composite::{self, RenderOptions};
use crate::doc;

/// Where Save writes a canvas that is not kept in the project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SaveTarget {
    /// The `.clip` it was opened from; Save has not asked yet.
    ClipAsk(PathBuf),
    /// That `.clip`: the person chose to export back to it.
    Clip(PathBuf),
    /// The flat picture it was opened from, in its format; `merge_ok`
    /// once the person said its layers may be merged into it.
    Picture { path: PathBuf, merge_ok: bool },
}

impl SaveTarget {
    /// The file Save writes, or asks about.
    pub(crate) fn path(&self) -> &Path {
        match self {
            Self::ClipAsk(path) | Self::Clip(path) | Self::Picture { path, .. } => path,
        }
    }
}

/// The question Save asks about one canvas.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SaveAsk {
    /// The canvas.
    pub(crate) picture: EntityId,
    /// What it was opened from.
    pub(crate) target: SaveTarget,
}

/// The picture formats Save writes back over the file a canvas was opened
/// from. GIF and ICO are left out: they hold more than one picture, and
/// Sue reads only the first.
const WRITES_BACK: &[&str] = &[
    "png", "jpg", "jpeg", "jxl", "webp", "tif", "tiff", "bmp", "tga", "qoi", "ppm",
];

/// Whether `bytes` hold more than one picture (an animated PNG or WebP),
/// which writing one back would lose.
fn animated(bytes: &[u8]) -> bool {
    if bytes.starts_with(b"\x89PNG") {
        // Chunks before the first IDAT: an acTL makes it an APNG.
        let mut at = 8;
        while at + 8 <= bytes.len() {
            let len = u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
            let kind = &bytes[at + 4..at + 8];
            if kind == b"acTL" {
                return true;
            }
            if kind == b"IDAT" {
                return false;
            }
            at = at.saturating_add(12).saturating_add(len as usize);
        }
        return false;
    }
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        return bytes.windows(4).any(|window| window == b"ANIM");
    }
    false
}

/// Whether the TIFF `bytes` hold more than one 8-bit picture: more than
/// 8 bits or float samples, a second page, or a layout this reading does
/// not follow (then it is not risked).
fn tiff_holds_more(bytes: &[u8]) -> bool {
    let little = match bytes.get(..4) {
        Some(b"II*\0") => true,
        Some(b"MM\0*") => false,
        // BigTIFF, or not a TIFF at all.
        _ => return true,
    };
    let word = |at: usize| {
        bytes.get(at..at + 2).map(|b| {
            if little {
                u16::from_le_bytes([b[0], b[1]])
            } else {
                u16::from_be_bytes([b[0], b[1]])
            }
        })
    };
    let long = |at: usize| {
        bytes.get(at..at + 4).map(|b| {
            let b = [b[0], b[1], b[2], b[3]];
            if little {
                u32::from_le_bytes(b)
            } else {
                u32::from_be_bytes(b)
            }
        })
    };
    let Some(ifd) = long(4).map(|at| at as usize) else {
        return true;
    };
    let Some(count) = word(ifd).map(usize::from) else {
        return true;
    };
    for entry in 0..count {
        let at = ifd + 2 + entry * 12;
        let (Some(tag), Some(values)) = (word(at), long(at + 4)) else {
            return true;
        };
        let values = values as usize;
        match tag {
            // BitsPerSample: short values, in the entry when they fit.
            258 => {
                let from = if values <= 2 {
                    at + 8
                } else {
                    match long(at + 8) {
                        Some(from) => from as usize,
                        None => return true,
                    }
                };
                for sample in 0..values.min(16) {
                    match word(from + sample * 2) {
                        Some(bits) if bits <= 8 => {}
                        _ => return true,
                    }
                }
            }
            // SampleFormat: 3 is floating point.
            339 if word(at + 8) == Some(3) => return true,
            _ => {}
        }
    }
    // A next page.
    long(ifd + 2 + count * 12).is_none_or(|next| next != 0)
}

/// Whether the PNM `bytes` hold samples of more than 8 bits.
fn pnm_holds_more(bytes: &[u8]) -> bool {
    if !matches!(bytes.get(..2), Some(b"P2" | b"P3" | b"P5" | b"P6")) {
        return false;
    }
    // Width, height and the largest sample, between blanks and comments.
    let mut numbers = Vec::new();
    let mut at = 2;
    while numbers.len() < 3 && at < bytes.len() {
        match bytes[at] {
            b'#' => {
                while at < bytes.len() && bytes[at] != b'\n' {
                    at += 1;
                }
            }
            byte if byte.is_ascii_digit() => {
                let mut number = 0_u64;
                while at < bytes.len() && bytes[at].is_ascii_digit() {
                    number = number
                        .saturating_mul(10)
                        .saturating_add(u64::from(bytes[at] - b'0'));
                    at += 1;
                }
                numbers.push(number);
                continue;
            }
            _ => {}
        }
        at += 1;
    }
    numbers.get(2).is_none_or(|max| *max > 255)
}

/// Whether writing one 8-bit picture over the file `bytes` of type `ext`
/// would lose some of it: frames, pages, or samples deeper than 8 bits
/// that Sue Paint read as 8.
fn holds_more(ext: &str, bytes: &[u8]) -> bool {
    match ext {
        // The bit depth in the header chunk.
        "png" => animated(bytes) || bytes.get(24).is_none_or(|bits| *bits > 8),
        "webp" => animated(bytes),
        "tif" | "tiff" => tiff_holds_more(bytes),
        "ppm" => pnm_holds_more(bytes),
        "jxl" => {
            use sue_io::JxlPipeline;
            sue_io::NativeJxlPipeline
                .decode(bytes, sue_io::ProgressiveTarget::DcPreview)
                .map_or(true, |decoded| {
                    decoded.info.bits_per_sample > 8 || decoded.info.frames > 1
                })
        }
        _ => false,
    }
}

/// The extension of `path`, in lower case.
fn extension(path: &Path) -> String {
    path.extension()
        .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

/// Whether Save may write a canvas made of `format` pixels read from
/// `bytes` back over `path`: only when that loses nothing of the file.
#[must_use]
pub(crate) fn writes_back(path: &Path, bytes: &[u8], format: PixelFormat) -> bool {
    let ext = extension(path);
    format != PixelFormat::RgbaF32
        && WRITES_BACK.contains(&ext.as_str())
        && !holds_more(&ext, bytes)
}

/// How many layers of `picture` would be merged into one flat picture:
/// every layer but paper and folders.
fn drawn_layers(arena: &Arena, picture: EntityId) -> usize {
    doc::layers(arena, picture)
        .iter()
        .filter(|info| info.kind != LayerKind::Paper && !doc::is_folder(arena, info.id))
        .count()
}

impl SuePaint {
    /// File, Save on the canvas in front.
    pub(crate) fn save(&mut self, cx: &mut AppletContext<'_>) {
        let Some(picture) = self.picture() else {
            self.save_project(cx);
            return;
        };
        match self.save_targets.get(&picture).cloned() {
            Some(SaveTarget::Clip(path)) => self.write_clip(cx, picture, &path),
            Some(target @ SaveTarget::ClipAsk(_)) => {
                self.save_ask = Some(SaveAsk { picture, target });
            }
            Some(SaveTarget::Picture { path, merge_ok }) => {
                if merge_ok || drawn_layers(&cx.world.arena, picture) <= 1 {
                    self.write_picture(cx, picture, &path);
                } else {
                    self.save_ask = Some(SaveAsk {
                        picture,
                        target: SaveTarget::Picture { path, merge_ok },
                    });
                }
            }
            None if crate::clip_io::saves_clip(cx) => match self.clip_paths.get(&picture) {
                Some(path) => {
                    let path = path.clone();
                    self.write_clip(cx, picture, &path);
                }
                None => self.save_as(cx),
            },
            None => self.save_project(cx),
        }
    }

    /// Save through the shell: the whole project, to its file (asking
    /// where when it has none).
    fn save_project(&mut self, cx: &mut AppletContext<'_>) {
        cx.request(ShellRequest::Command("file.save".to_owned()));
        self.mark_clean();
        let at = cx.history.cursor();
        for doc in &self.docs {
            self.saved_at.insert(doc.picture, at);
        }
    }

    /// Remembers that `picture` was opened from `target`'s file, so Save
    /// goes there (or asks about it).
    pub(crate) fn opened_from(&mut self, picture: EntityId, target: SaveTarget) {
        if let SaveTarget::ClipAsk(path) | SaveTarget::Clip(path) = &target {
            self.clip_paths.insert(picture, path.clone());
        }
        self.save_targets.insert(picture, target);
    }

    /// Remembers the flat picture file `layer` was read from (if Sue
    /// writes that format and the file holds one 8-bit picture) as where
    /// Save writes `picture`, the canvas made around it.
    pub(crate) fn came_from_picture(
        &mut self,
        cx: &AppletContext<'_>,
        layer: EntityId,
        picture: EntityId,
    ) {
        let Ok(entity) = cx.world.arena.get_by_id(layer) else {
            return;
        };
        let Some(source) = entity.meta(sue_io::meta::SOURCE).map(PathBuf::from) else {
            return;
        };
        let format = doc::raster(&cx.world.arena, layer).map(sue_core::RasterTile2D::format);
        let Ok(bytes) = sue_platform::fs::read(&source) else {
            return;
        };
        if let Some(format) = format
            && writes_back(&source, &bytes, format)
        {
            self.opened_from(
                picture,
                SaveTarget::Picture {
                    path: source,
                    merge_ok: false,
                },
            );
        }
    }

    /// The file name the window title shows for `picture`: the file Save
    /// writes when that is not the project.
    #[must_use]
    pub(crate) fn save_title(&self, picture: EntityId) -> Option<String> {
        let target = self.save_targets.get(&picture)?;
        Some(target.path().file_name()?.to_string_lossy().into_owned())
    }

    /// Marks `picture` saved now: its tab loses the modified dot and
    /// Revert comes back here.
    pub(super) fn saved_now(&mut self, cx: &AppletContext<'_>, picture: EntityId) {
        self.saved_at.insert(picture, cx.history.cursor());
        if let Some(doc) = self.docs.iter().find(|doc| doc.picture == picture) {
            let (_, _, version) = doc.compositor.display();
            self.clean.insert(picture, version);
        }
    }

    /// Writes `picture` flattened over the picture file at `path`, in the
    /// format of its extension.
    fn write_picture(&mut self, cx: &mut AppletContext<'_>, picture: EntityId, path: &Path) {
        let options = RenderOptions {
            paper: true,
            drafts: false,
        };
        let Some(flat) = composite::flatten(&cx.world.arena, picture, options) else {
            cx.fail(tr!("this-picture-has-no-size"));
            return;
        };
        let ext = extension(path);
        let bytes = sue_io::encode_as(
            &flat,
            &ext,
            super::files::jpeg_quality(cx),
            super::files::jxl_effort(cx),
        );
        match bytes {
            Ok(bytes) => {
                if self.write_export(cx, path, &bytes) {
                    self.saved_now(cx, picture);
                    cx.request(ShellRequest::Remember {
                        path: path.to_path_buf(),
                        saved: true,
                    });
                }
            }
            Err(error) => cx.fail(tr!("cannot-save-as", extension = ext, error = error)),
        }
    }

    /// Asks where to keep `picture` as a Sue project, offering a `.sue`
    /// named after `beside` in its folder.
    fn keep_in_project(&mut self, cx: &mut AppletContext<'_>, picture: EntityId, beside: &Path) {
        self.save_targets.remove(&picture);
        let name = beside.file_stem().map_or_else(
            || tr!("picture"),
            |stem| stem.to_string_lossy().into_owned(),
        );
        cx.request(ShellRequest::PickFile(
            FilePick::save(
                None,
                "save-project",
                &tr!("save-project"),
                &format!("{name}.sue"),
            )
            .types(vec![FileType::new(&tr!("sue-project"), &["sue"])])
            .starting_in(beside.parent().map(Path::to_path_buf)),
        ));
    }

    /// What answering the question does.
    pub(crate) fn answer_save(&mut self, cx: &mut AppletContext<'_>, ask: SaveAsk, answer: Answer) {
        let SaveAsk { picture, target } = ask;
        if cx.world.arena.get_by_id(picture).is_err() {
            return;
        }
        match (answer, target) {
            (Answer::Accept, SaveTarget::ClipAsk(path)) => {
                self.save_targets
                    .insert(picture, SaveTarget::Clip(path.clone()));
                self.write_clip(cx, picture, &path);
            }
            (Answer::Accept, SaveTarget::Picture { path, .. }) => {
                self.save_targets.insert(
                    picture,
                    SaveTarget::Picture {
                        path: path.clone(),
                        merge_ok: true,
                    },
                );
                self.write_picture(cx, picture, &path);
            }
            (Answer::Alternate, target) => self.keep_in_project(cx, picture, target.path()),
            _ => {}
        }
    }

    /// Draws Save's question while it is asked.
    pub(crate) fn save_ask_ui(
        &mut self,
        ctx: &egui::Context,
        cx: &mut AppletContext<'_>,
        theme: &Theme,
    ) {
        let Some(ask) = self.save_ask.clone() else {
            return;
        };
        let file = ask
            .target
            .path()
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        let (title, message, accept) = match &ask.target {
            SaveTarget::Picture { path, .. } => (
                tr!("save-merge-title"),
                tr!(
                    "save-merge-message",
                    file = file.as_str(),
                    format = extension(path).to_ascii_uppercase()
                ),
                tr!(
                    "save-merge-accept",
                    format = extension(path).to_ascii_uppercase()
                ),
            ),
            _ => (
                tr!("save-clip-title"),
                tr!("save-clip-message", file = file.as_str()),
                tr!("save-clip-accept"),
            ),
        };
        let dialog = UiDialog::new("sue-paint-save-ask", &title, theme)
            .accept(accept)
            .alternate(tr!("save-keep-in-project"))
            .cancel();
        let tone = if matches!(ask.target, SaveTarget::Picture { .. }) {
            Tone::Warning
        } else {
            Tone::Info
        };
        if let Some(answer) = confirm(ctx, dialog, tone, &message) {
            self.save_ask = None;
            self.answer_save(cx, ask, answer);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn animated_pictures_and_float_files_are_not_written_back() {
        let still =
            sue_io::encode_png(&sue_core::RasterTile2D::new(2, 2, PixelFormat::Rgba8).unwrap())
                .unwrap();
        assert!(writes_back(Path::new("a.png"), &still, PixelFormat::Rgba8));
        assert!(writes_back(Path::new("a.PNG"), &still, PixelFormat::Gray8));
        assert!(!writes_back(Path::new("a.exr"), &still, PixelFormat::Rgba8));
        assert!(!writes_back(
            Path::new("a.png"),
            &still,
            PixelFormat::RgbaF32
        ));
        assert!(!writes_back(Path::new("a.gif"), &still, PixelFormat::Rgba8));
        let frame = sue_core::RasterTile2D::new(2, 2, PixelFormat::Rgba8).unwrap();
        let apng = sue_io::encode_apng(&[frame.clone(), frame.clone()], 100).unwrap();
        assert!(!writes_back(Path::new("a.png"), &apng, PixelFormat::Rgba8));
        let webp = sue_io::encode_webp_animation(&[frame.clone(), frame], 100).unwrap();
        assert!(!writes_back(Path::new("a.webp"), &webp, PixelFormat::Rgba8));
    }

    #[test]
    fn pictures_deeper_than_8_bits_or_of_many_pages_are_not_written_back() {
        // PNG: the bit depth byte of the header chunk.
        let still =
            sue_io::encode_png(&sue_core::RasterTile2D::new(2, 2, PixelFormat::Rgba8).unwrap())
                .unwrap();
        assert_eq!(still[24], 8);
        let mut deep = still.clone();
        deep[24] = 16;
        assert!(!writes_back(Path::new("a.png"), &deep, PixelFormat::Rgba8));
        // TIFF, little and big endian: one 8-bit page is written back;
        // 16-bit samples, float samples or a second page are not.
        let tiff = |little: bool, bits: u16, format: u16, next: u32| {
            let w16 = |v: u16| {
                if little {
                    v.to_le_bytes()
                } else {
                    v.to_be_bytes()
                }
            };
            let w32 = |v: u32| {
                if little {
                    v.to_le_bytes()
                } else {
                    v.to_be_bytes()
                }
            };
            let mut out = if little {
                b"II*\0".to_vec()
            } else {
                b"MM\0*".to_vec()
            };
            out.extend(w32(8));
            out.extend(w16(2));
            for (tag, value) in [(258_u16, bits), (339, format)] {
                out.extend(w16(tag));
                out.extend(w16(3));
                out.extend(w32(1));
                out.extend(w16(value));
                out.extend([0, 0]);
            }
            out.extend(w32(next));
            out
        };
        let at = |bytes: &[u8]| writes_back(Path::new("a.tif"), bytes, PixelFormat::Rgba8);
        for little in [true, false] {
            assert!(at(&tiff(little, 8, 1, 0)), "one 8-bit page");
            assert!(!at(&tiff(little, 16, 1, 0)), "16-bit");
            assert!(!at(&tiff(little, 8, 3, 0)), "float");
            assert!(!at(&tiff(little, 8, 1, 400)), "a second page");
        }
        assert!(!at(b"II+\0"), "BigTIFF");
        let real = sue_io::encode_as(
            &sue_core::RasterTile2D::new(3, 2, PixelFormat::Rgba8).unwrap(),
            "tiff",
            90,
            1,
        )
        .unwrap();
        assert!(at(&real), "a TIFF Sue wrote");
        // PNM: the largest sample in the header.
        let ppm = |max: &str| format!("P6\n# made by hand\n2 1\n{max}\n").into_bytes();
        assert!(writes_back(
            Path::new("a.ppm"),
            &ppm("255"),
            PixelFormat::Rgba8
        ));
        assert!(!writes_back(
            Path::new("a.ppm"),
            &ppm("65535"),
            PixelFormat::Rgba8
        ));
        // JPEG XL: what its header says, read by the decoder.
        let jxl = sue_io::encode_as(
            &sue_core::RasterTile2D::new(2, 2, PixelFormat::Rgba8).unwrap(),
            "jxl",
            90,
            1,
        )
        .unwrap();
        assert!(writes_back(Path::new("a.jxl"), &jxl, PixelFormat::Rgba8));
        assert!(!writes_back(
            Path::new("a.jxl"),
            b"not a picture",
            PixelFormat::Rgba8
        ));
    }
}
