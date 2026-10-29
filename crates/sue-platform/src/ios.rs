//! iOS: UIKit's document picker, shown by `sue-ios` (the audited
//! Objective-C calls), as the system behind the picks. Opened documents
//! are copies, a chosen folder is a path Sue writes in, and a saved file
//! is offered to the export picker once written.

use std::fs;
use std::path::Path;

use crate::picks::{Answer, Kind, System};

/// Files on iOS, through the document picker. Places are paths.
#[derive(Default)]
pub(crate) struct Files;

impl System for Files {
    const SAVE_ASKS_FIRST: bool = false;
    const FOLDERS_ARE_PATHS: bool = true;
    const WRITES_BACK: bool = false;

    fn show(&self, kind: Kind, extensions: &[String], _file_name: &str) -> Result<(), String> {
        match kind {
            Kind::Open => sue_ios::show_open(extensions),
            Kind::Folder => sue_ios::show_folder(),
            Kind::Save => Err("iOS asks where to save once the file is written".to_owned()),
        }
    }

    fn export(&self, file: &Path) -> Result<(), String> {
        sue_ios::show_export(file)
    }

    fn answer(&self) -> Option<Answer> {
        sue_ios::take_picked().map(|picked| match picked {
            sue_ios::Picked::Chosen(path) => Answer::Chosen(path.to_string_lossy().into_owned()),
            sue_ios::Picked::Cancelled => Answer::Cancelled,
            sue_ios::Picked::Failed(why) => Answer::Failed(why),
        })
    }

    fn name(&self, place: &str) -> String {
        Path::new(place).file_name().map_or_else(
            || "file".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        )
    }

    fn folder(&self, place: &str) -> Option<String> {
        crate::picks::files_folder(place)
    }

    fn copy_in(&self, place: &str, file: &Path) -> Result<(), String> {
        fs::copy(place, file).map_err(|error| error.to_string())?;
        // The picker's copy is Sue's to tidy away.
        let _ = fs::remove_file(place);
        Ok(())
    }

    fn copy_out(&self, _file: &Path, _place: &str) -> Result<(), String> {
        Err("an opened file is a copy on iOS".to_owned())
    }
}
