# Changelog

What changed in each release of Sue, newest first. Each version's section
opens its release notes on GitHub, so write for the people who use Sue.

While you work, add your change under **Unreleased**, in one of these
groups: Added, Changed, Fixed, Removed. Making a release turns Unreleased
into the new version's section (see `CONTRIBUTING.md`). The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions
follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- A table of contents in Sue Writer (Insert, Table of Contents): the
  headings with their pages, kept up as you write; click an entry to go
  there. It prints in PDFs and goes into Word, OpenDocument, web pages,
  e-books, RTF, Markdown and plain text.
- Find and Replace in Sue Writer takes regular expressions, and the
  replacement can put back what a group caught (`$1`).
- Change Case in Sue Writer (Format, Text): Sentence case, lowercase,
  UPPERCASE, Capitalize Every Word and tOGGLE cASE.
- Headers and footers in Sue Writer (Insert, Header and Footer): left,
  middle and right parts with the page number, page count, title, author
  and date, and a different first page. They print in PDFs and go into
  Word, OpenDocument and RTF as live page numbers.
- Tables in Sue Writer: Table, Insert Table (`Ctrl+F12`), then type in the
  cells; `Tab` moves on and adds a row at the end. Insert and delete rows
  and columns, a heading row that repeats on every page, and text to table
  and back. Tables print in PDFs and go into Word, OpenDocument, EPUB, web
  pages, RTF, Markdown and plain text.

- The Hub is Sue's home page: a box for every studio (Sue Paint, Sue
  Writer) and every tool (the Image Viewer, the Converter). The pin at a
  box's bottom right makes Sue open straight into it; one pin at a time,
  and Settings, General, Startup shows it too.
- The Sue menu, under the Sue button at the window's top left: the Hub,
  every studio and tool, New, Open File, projects, Send To, the pin, the
  command palette and Settings.
- Opening and saving work on phones and tablets again, through the
  system's own picker: Android's Files picker (Google Drive too) and the
  Files browser on iPhone and iPad. On Android, saving a file you opened
  writes back to the original, and a folder you choose is read and
  written (the Converter's Add a Folder and its output folder).
- The Hub lists your work under its boxes: **Recent** files and projects,
  **Saved Projects**, and **Unsaved Work**. Click one to open it in its
  studio. The Sue menu's File, Open Recent shows the same recent files.
- Sue keeps work you have not saved by itself (a minute after a change, and
  when you quit), so nothing is lost to a crash or a quick Quit. The Hub's
  Unsaved Work brings it back (Recover) or throws it away (Discard, which
  asks once more). Opening another project keeps the unsaved work it
  replaces there too. Settings, General sets how often, or turns it off.
- Typing in Sue Writer feels smooth: the caret glides to its new place
  instead of jumping, as you type, delete, move with the keys or click, and
  as lines and pages wrap. It blinks only while it rests. Settings, Display
  & Graphics, Motion has Smooth Caret to turn it off and Caret Glide Time
  for its speed.

### Changed

- Sue Paint's File, Open Recent now shows the pictures from Sue's one list
  of recent files, the same one the Hub shows.

- Sue Writer on a phone: the text flows to the screen's width at a size you
  can read, the formatting is a row of big buttons right above the keyboard,
  and the line you are writing stays in sight when the keyboard comes up.
  Tapping a formatting button keeps the keyboard up.
- Sue Writer has fewer options in the way: one short toolbar (what Word and
  LibreOffice Writer keep at hand), the Format menu grouped as LibreOffice
  groups it, Draft and Page beside the zoom, and the inspector hidden until
  View, Inspector. Tablets get larger toolbar buttons.
- The Sue button sits once, in the window's top left corner, and the
  open studio's or tool's menus follow it.
- The Image Viewer and the Converter are tools with pages of their own,
  like the studios, each with its own menus.
- Every file Sue asks for goes through your system's own Open, Save and
  folder windows.
- Opening a text, Markdown or HTML file brings it into Sue Writer; a
  picture opens in the Image Viewer.
- Your settings file moves to format 5 (the old one is kept as
  `default.suerice.v4.bak`); a studio you chose to start in stays pinned.

### Removed

- The infinite canvas and everything on it (cards, the selection and top
  pills, the right-click marking menu, the timeline).
- The side panels: the File Manager, Notes, the Player, the Inspector and
  History. Projects saved with the canvas still open, and keep its layout
  in the file untouched.

### Fixed

- Sue Writer no longer crashes on a page narrower than about 543 points,
  such as a phone's.
- Studios and tools wear their own colors again: with a light Sue theme
  (Paper), Sue Paint in Gray showed white fields and buttons with dark text
  on its gray panels.
- On iPhone and iPad the on-screen keyboard no longer covers the bottom of
  Sue: Sue fits itself above the keyboard, as on Android, so the line you
  are writing in Sue Writer stays in sight.
- On Android, a file you chose in the system's picker is named as you know
  it, such as "Picture.png in Download", in the status bar, the Image
  Viewer and the Converter, instead of a path inside Sue's cache. There
  the Image Viewer and the Converter no longer offer to show files in
  Explorer, which a phone does not have.

## [0.0.3] - 2026-10-29

### Added

- A setup program for Windows (`-setup.exe`) that carries Sue's icon,
  shows the license and installs Sue. The `.msi` stays for managed PCs.
- Install with [Scoop](https://scoop.sh), which skips Windows' download
  warning: `scoop install
  https://github.com/MagiGun/sue/releases/latest/download/sue.json`, then
  `scoop update sue` for new versions.
- Releases can be sent to winget (`winget install MagiGun.Sue`) as soon as
  the repository holds a token for it.
- Windows builds sign every program and installer when a code signing
  certificate is set up.
- RISC-V Linux packages (`.deb`, `.rpm`, tarball).
- An Android launcher icon that fits any launcher's shape, and a themed
  icon on Android 13 and newer.

### Changed

- On Windows, Sue's window appears once its first frame is drawn, instead
  of empty.
- The installers show Sue's icon on their pages.
- Linux desktops match Sue's window to its icon: the packages, the
  AppImage, the Flatpak and the window all use the id
  `io.github.MagiGun.Sue`.
- Release notes start with how to install Sue on each system.

### Fixed

- The Windows taskbar showed a blank or generic icon instead of Sue's.
- The Start menu shortcut from the `.msi` had no icon.
- The installer's license page broke the text at every line.
- The RISC-V build, which could not install its libraries.

## [0.0.2] - 2026-10-29

### Added

- A package for every system, each named after the release's version:
  - Windows: an `.msi` installer and a portable zip, for x64, x86 and ARM.
  - macOS: a universal `.dmg`, and the command line tool zipped.
  - Linux: AppImage (with `.zsync` for updaters), Flatpak, `.deb`, `.rpm`,
    an Arch package and a tarball, for Intel and ARM.
  - FreeBSD, Android (one APK for phones and tablets) and iPhone and iPad
    (to sign with your own Apple account).
- `sue-cli.exe` on Windows, the same program for use in a terminal.

### Changed

- On Windows, `Sue.exe` opens without a console window.
- Sue reports the release's version.
- The README links the website and the Discord.

## [0.0.1] - 2026-10-29

The first release.

### Added

- **Sue Paint**, a painting, comics and animation studio laid out the way
  Clip Studio Paint users expect: a smooth, pressure sensitive brush
  engine; layers, folders, masks, correction layers and layer effects;
  vector layers, rulers and perspective; selections; text and balloons;
  manga frames, tones and effect lines; pages and stories; animation
  timelines with onion skin, audio, camera work and movie export; 3D
  layers; filters and adjustments; and Clip Studio Paint's menus,
  palettes and shortcuts.
- **Sue Writer**, a writing studio that publishes to PDF, Word, EPUB and
  more.
- The canvas, the Hub, the File Manager, the Converter, a music player and
  notes, with themes in gray tones.
- Layouts for computers, tablets and phones, with touch gestures and a
  switch between them.
- `.sue` projects: every studio works on the same project, and every
  change can be undone.
- Opens PNG, JPEG, JPEG XL, Photoshop and many more formats.
- Packages for Windows, macOS, Linux, Android and iOS, under the Sayn
  License.

[Unreleased]: https://github.com/MagiGun/sue/compare/v0.0.3...HEAD
[0.0.3]: https://github.com/MagiGun/sue/compare/v0.0.2...v0.0.3
[0.0.2]: https://github.com/MagiGun/sue/compare/v0.0.1...v0.0.2
[0.0.1]: https://github.com/MagiGun/sue/releases/tag/v0.0.1
