# Changelog

All notable changes to this project are documented in this file.
Format: Keep a Changelog (https://keepachangelog.com), versioning: Semantic Versioning (https://semver.org).

## [Unreleased]

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

[unreleased]: https://github.com/iocast/wurzelwerk/compare/v0.1.3...HEAD
[0.1.3]: https://github.com/iocast/wurzelwerk/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/iocast/wurzelwerk/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/iocast/wurzelwerk/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/iocast/wurzelwerk/releases/tag/v0.1.0
