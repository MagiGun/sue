# Changelog

What changed in each release of Sue, newest first. Each version's section
opens its release notes on GitHub, so write for the people who use Sue.

While you work, add your change under **Unreleased**, in one of these
groups: Added, Changed, Fixed, Removed. Making a release turns Unreleased
into the new version's section (see `CONTRIBUTING.md`). The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions
follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

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
