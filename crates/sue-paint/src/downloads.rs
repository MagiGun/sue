//! Downloads into Sue Paint: a link fetched as a transfer (it shows in Data
//! Transfers, with its progress, and can be cancelled), then brought in by
//! what it is: brushes into the tool, pictures into the materials, and a
//! zip opened for the brushes and pictures inside. Nothing goes through a
//! browser, and nothing needs another program.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use sue_wm::{AppletContext, TransferKind, TransferState};

/// The brush files Sue Paint reads.
const BRUSHES: [&str; 4] = ["sut", "abr", "gbr", "gih"];
/// The pictures that become materials.
const PICTURES: [&str; 1] = ["png"];

/// What a downloaded file becomes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Arrival {
    /// Brushes, added to the current tool.
    Brushes(PathBuf),
    /// A picture, added to the materials.
    Picture(PathBuf),
    /// Anything else, kept where it was saved.
    Kept(PathBuf),
}

/// The files a download gave, once it is done.
type Found = Arc<Mutex<Option<Vec<Arrival>>>>;

/// The downloads still running.
#[derive(Debug, Default)]
pub(crate) struct Downloads {
    running: Vec<(u64, Found)>,
    /// The link typed in Data Transfers.
    pub(crate) link: String,
}

fn has_extension(path: &Path, list: &[&str]) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| list.iter().any(|known| ext.eq_ignore_ascii_case(known)))
}

/// What `file` becomes; a zip is opened into a folder beside it and each
/// brush and picture inside is brought in.
pub(crate) fn unpack(file: &Path) -> Result<Vec<Arrival>, String> {
    if has_extension(file, &BRUSHES) {
        return Ok(vec![Arrival::Brushes(file.to_path_buf())]);
    }
    if has_extension(file, &PICTURES) {
        return Ok(vec![Arrival::Picture(file.to_path_buf())]);
    }
    if !has_extension(file, &["zip"]) {
        return Ok(vec![Arrival::Kept(file.to_path_buf())]);
    }
    let bytes = sue_platform::fs::read(file).map_err(|error| error.to_string())?;
    let folder = file.with_extension("");
    let mut found = Vec::new();
    for (name, data) in sue_io::zip::read_zip(&bytes)? {
        // Only the file's own name: nothing in an archive writes outside
        // its folder.
        let Some(leaf) = Path::new(&name.replace('\\', "/"))
            .file_name()
            .map(|leaf| leaf.to_string_lossy().into_owned())
        else {
            continue;
        };
        let to = folder.join(&leaf);
        let arrival = if has_extension(&to, &BRUSHES) {
            Arrival::Brushes(to.clone())
        } else if has_extension(&to, &PICTURES) {
            Arrival::Picture(to.clone())
        } else {
            continue;
        };
        sue_platform::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
        sue_platform::write_atomic(&to, &data).map_err(|error| error.to_string())?;
        found.push(arrival);
    }
    if found.is_empty() {
        found.push(Arrival::Kept(file.to_path_buf()));
    }
    Ok(found)
}

impl crate::studio::SuePaint {
    /// Downloads `link` into the materials' downloads folder as a
    /// transfer, to bring in when it is done.
    pub(crate) fn start_download(&mut self, cx: &mut AppletContext<'_>, link: &str) {
        let link = link.trim().to_owned();
        if !(link.starts_with("https://") || link.starts_with("http://")) {
            cx.fail(tr!("download-needs-a-link"));
            return;
        }
        let Some(folder) = crate::materials::location(cx) else {
            cx.fail(tr!("download-needs-a-folder"));
            return;
        };
        let dir = folder.join("downloads");
        let name = sue_platform::update::file_name_of(&link);
        let found: Found = Arc::default();
        let into = Arc::clone(&found);
        let id = cx.transfers.spawn(
            TransferKind::Download,
            &name,
            Some(dir.clone()),
            move |job| {
                let file = sue_platform::update::download_link(
                    &link,
                    &dir,
                    concat!("Sue/", env!("CARGO_PKG_VERSION")),
                    &|fraction| job.set_progress(fraction * 0.95),
                    &|| job.is_cancelled(),
                )?;
                let arrivals = unpack(&file)?;
                if let Ok(mut slot) = into.lock() {
                    *slot = Some(arrivals);
                }
                Ok(())
            },
        );
        self.downloads.running.push((id, found));
    }

    /// Brings in the downloads that finished.
    pub(crate) fn poll_downloads(&mut self, cx: &mut AppletContext<'_>) {
        if self.downloads.running.is_empty() {
            return;
        }
        let mut done = Vec::new();
        self.downloads.running.retain(|(id, found)| {
            let running = cx
                .transfers
                .get(*id)
                .is_some_and(|transfer| matches!(transfer.state, TransferState::Running(_)));
            if !running && let Some(arrivals) = found.lock().ok().and_then(|mut slot| slot.take()) {
                done.extend(arrivals);
            }
            running
        });
        let mut pictures = 0;
        for arrival in done {
            match arrival {
                Arrival::Brushes(path) => self.import_brushes(cx, &path),
                Arrival::Picture(path) => {
                    let Some(folder) = crate::materials::location(cx) else {
                        continue;
                    };
                    match crate::materials::add_file(&folder, crate::materials::IMAGE, &path) {
                        Ok(_) => pictures += 1,
                        Err(error) => cx.fail(error),
                    }
                }
                Arrival::Kept(path) => {
                    cx.notify(tr!("download-kept", path = path.display().to_string()))
                }
            }
        }
        if pictures > 0 {
            self.start.materials_changed();
            self.material_thumbs.clear();
            cx.notify(tr!("download-pictures-added", count = pictures));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downloads_bring_in_brushes_pictures_and_what_a_zip_holds() {
        let dir = std::env::temp_dir().join(format!("sue-paint-downloads-{}", std::process::id()));
        let _ = sue_platform::fs::remove_dir_all(&dir);
        sue_platform::fs::create_dir_all(&dir).unwrap();
        let brush = dir.join("ink.abr");
        assert_eq!(unpack(&brush).unwrap(), vec![Arrival::Brushes(brush)]);
        let picture = dir.join("paper.PNG");
        assert_eq!(unpack(&picture).unwrap(), vec![Arrival::Picture(picture)]);
        let other = dir.join("readme.pdf");
        assert_eq!(unpack(&other).unwrap(), vec![Arrival::Kept(other)]);

        let mut zip = sue_io::zip::ZipWriter::new();
        zip.add("set/pens.sut", b"brushes", false);
        zip.add("set/grain.png", b"picture", false);
        zip.add("set/notes.txt", b"words", false);
        zip.add("../../escape.png", b"outside", false);
        let file = dir.join("set.zip");
        sue_platform::write_atomic(&file, &zip.finish()).unwrap();
        let inside = dir.join("set");
        assert_eq!(
            unpack(&file).unwrap(),
            vec![
                Arrival::Brushes(inside.join("pens.sut")),
                Arrival::Picture(inside.join("grain.png")),
                Arrival::Picture(inside.join("escape.png")),
            ]
        );
        assert_eq!(
            sue_platform::fs::read(inside.join("grain.png")).unwrap(),
            b"picture"
        );
        assert!(!sue_platform::fs::exists(inside.join("notes.txt")));

        let empty = dir.join("empty.zip");
        sue_platform::write_atomic(&empty, &sue_io::zip::ZipWriter::new().finish()).unwrap();
        assert_eq!(unpack(&empty).unwrap(), vec![Arrival::Kept(empty)]);
        let _ = sue_platform::fs::remove_dir_all(&dir);
    }
}
