## Sue itself (bins/sue): the window, opening and saving files and
## projects, and the messages about them.

## runtime.rs

could-not-save-the-rice = Could not save the rice: { $error }
quit-sue = Quit Sue?
your-settings-are-saved-first = Your settings are saved first.
quit = Quit
cancel = Cancel
settings-error = Settings: { $error }
unknown-command = Unknown command { $other }
folder = Folder: { $dir }
cannot-open-the-folder = Cannot open the folder: { $error }
link = Link: { $link }
cannot-open-the-link = Cannot open the link: { $error }
file = File: { $path }
cannot-show-the-file = Cannot show the file: { $error }
open-file = Open File
cannot-save = Cannot save { $value }: { $error }
export-failed = Export failed: { $error }
nothing-asked-file = Nothing asked for a file to { $other }
choosing-files-needs-desktop-window = Choosing files needs the desktop window
cannot-choose-a-file = Cannot choose a file: { $why }
save-project = Save Project
untitled-sue = Untitled.sue
sue-project = Sue project
opened = Opened { $value }
it-is-a-folder = it is a folder
over-mib-limit-io-max-ingest = it is { $value }, over the { $limit_mib } MiB limit (io.max_ingest_mib)
cannot-open = Cannot open { $name }: { $reason }
no-studio-tool-opens-yet = No studio or tool opens { $name } yet
there-no-audio-device-mode = there is no audio device in this mode
audio = Audio: { $error }
cannot-read = Cannot read { $path }: { $error }
rice-file-has-error = The rice file has an error: { $error }
rice-file-matches-what-sue-shows = The rice file matches what Sue shows
reload-default-suerice = Reload default.suerice
reloaded-values-from-disk = Reloaded { $count } values from disk
reload-failed = Reload failed: { $error }
export-rice = Export Rice

## runtime.rs

could-not-keep-unsaved-work = Could not keep the unsaved work: { $error }
that-unsaved-work-is-gone = That unsaved work is gone
cannot-recover = Cannot recover { $title }: { $error }
recovered-save-keep = Recovered { $title }; save it to keep it{ $value }
discarded = Discarded { $title }
cannot-discard = Cannot discard { $title }: { $error }
cannot-remember-the-recent-files = Cannot remember the recent files: { $error }
opened-2 = Opened { $value }{ $value2 }

## update.rs: Sue's own updates

checking-for-updates = Checking for updates
update-current-version = This is Sue { $version }.
sue-is-up-to-date = Sue is up to date
update-browser-is-newest = The browser always runs the newest Sue.
update-available = Sue { $version } is out
update-run-command = Sue updates through its package manager:
update-install-and-restart = Install and Restart
update-download-and-install = Download and Install
update-download = Download
update-copy-command = Copy Command
update-command-copied = Copied the command
update-release-page = Release Page
update-later = Later
update-downloading = Downloading Sue { $version }
update-progress = { $received } of { $total }
update-hide = Hide
update-close = Close
update-quit-sue = Quit Sue
update-failed = Sue could not update
update-dmg-opened = The disk image is open: quit Sue, then drag the new Sue into Applications.
update-installer-opened = The system's installer takes it from here.
update-package-downloaded = Downloaded to { $path }. Install it with:
