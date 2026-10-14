# Changelog

All notable changes to this project are documented in this file.
Format: Keep a Changelog (https://keepachangelog.com), versioning: Semantic Versioning (https://semver.org).

## [Unreleased]

## [0.2.0] - 2026-10-14

### Added
- Timeline (`T` or *Timeline* in the toolbar): a year range slider under the board shows who was alive between two years; people appear in their birth year, disappear when they died before the range and fade when they died within it, and clan hulls follow their visible members
- Timeline controls: drag either end of the range or the range itself; *Play* (`Space`) moves the end forward year by year. Lines whose name contains a year (e.g. `married 1789`) appear from that year
- *Open example workspace* (welcome screen and *Settings → Workspace*) creates a generated board in `Documents/Wurzelwerk Example`: 11 clans, about 600 people from the 1620s to today, with clan and founder notes and a *Chronicle* book

### Changed
- `--demo RxC` generates families across four centuries (dated weddings, remarriages, twins, godparents, notes) instead of people born between 1880 and 2040
- Language setting: new *System* option, the default for new installations, follows the language of the operating system

## [0.1.3] - 2026-10-11

### Changed
- Updates install themselves: a new version downloads in the background and installs with *Restart now* (Windows installer, macOS, AppImage); `.deb` and self-built copies still link to the release page

## [0.1.2] - 2026-10-11

### Fixed
- Windows: the taskbar, Alt+Tab and Explorer show the Wurzelwerk icon instead of the generic application icon

## [0.1.1] - 2026-10-11

### Fixed
- Release builds now produce the installers: Windows (`.exe`, `.msi`), macOS (`.dmg`) and Linux (`.deb`, `.AppImage`)
- Building from source on Linux and macOS works with a native protoc: set `PROTOC` to its path (the bundled Windows `protoc.exe` was always used before)

## [0.1.0] - 2026-10-11

### Added
- Board for families and clans: person cards with name and years, free pan and zoom, fit to window and search
- Clans drawn as coloured hulls around their members; a person can belong to several clans, and empty clans show as a dashed box
- Named lines between people, styled by their name (`married` plain, `parents of` arrow, others such as `godfather of` dashed)
- Lines route themselves orthogonally around cards; bends, endpoints, line labels and clan labels can be dragged and reset
- Auto-layout: partners side by side, children in the next generation, few line crossings and compactly packed clan groups
- Inspector for people, clans and lines, with Markdown notes and file attachments (drop files on a person to copy them into the workspace)
- Workspaces: a folder with an embedded database and the attachments; created on first start, switchable in *Settings → Workspace*, or opened for one run with `wurzelwerk <path>`
- Appearance setting (System / Light / Dark) and a light/dark toggle in the toolbar
- English and German user interface
- Keyboard shortcuts (`V`, `G`, `P`, `C`, `F`, `Ctrl+F`, `Ctrl+B`, `Ctrl+,`, `Del`, `Esc`) and a shortcuts panel
- Update check against the latest GitHub release at start-up or via *Settings → Updates → Check now*; can be turned off
- Large boards with 10 000 people stay smooth; `--demo RxC` generates such a board for load testing

[unreleased]: https://github.com/iocast/wurzelwerk/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/iocast/wurzelwerk/compare/v0.1.3...v0.2.0
[0.1.3]: https://github.com/iocast/wurzelwerk/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/iocast/wurzelwerk/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/iocast/wurzelwerk/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/iocast/wurzelwerk/releases/tag/v0.1.0
