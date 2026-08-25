# Changelog

## 0.1.0 — 2026-08-25

- initial public release;
- Dolphin service menu for KDE 6;
- target-size video profiles and advanced form;
- VAAPI/NVENC profiles with software fallback;
- user-systemd queue and watch-folders;
- verified output pipeline with atomic rename and decode checks.

## [0.2.0] - 2026-08-25

### Added

- Separate Dolphin service menus for video, audio, and image files.
- Atomic output commits with collision-safe filenames and reserved temporary files.
- Absolute runtime discovery for FFmpeg, FFprobe, ImageMagick, systemd, and KDE helpers.
- ARM64, Debian/Ubuntu, Fedora/RHEL, and Arch release packaging paths.
- KDE/Dolphin, FFmpeg/ImageMagick, and cross-architecture CI coverage.

### Changed

- Legacy duplicate Dolphin menus are hidden during install and restored on uninstall.
- Install script selects the matching x86_64 or ARM64 release asset.
