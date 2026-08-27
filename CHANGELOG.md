# Changelog

## [0.2.6] - 2026-08-27

### Fixed

- Release checksum subject manifests now preserve NUL-delimited filenames in GitHub Actions.
- Flathub preflight installs the declared Freedesktop SDK and runtime before the strict build.

### Added

- Signed release provenance and SPDX SBOM attestations are published with the immutable release assets.

## [0.2.5] - 2026-08-27

### Added

- Flathub submission bundle with AppStream metadata, icon assets, and an offline Cargo source closure.
- Release artifact attestations, SPDX SBOM publication, and local verification helpers for signed provenance.

### Changed

- Strict Flatpak metadata now declares the standalone sandbox boundary and installs the canonical application metadata.
- Native Dolphin/host integration remains explicitly outside the strict Flathub sandbox profile.

## [0.2.2] - 2026-08-26

### Added

- Trusted VAAPI and NVENC runner provisioning with durable user-systemd services and hardware receipts.
- Strict sandbox and explicitly experimental host-integration Flatpak profiles.
- Installed-bundle conversion smoke with FFprobe, full decode, checksum, and verified job receipts.
- Release gates that validate native, Flatpak, and hardware evidence before publication.

### Changed

- Flatpak and GPU workflows are restricted to trusted release paths and exact source refs.
- Host-integration smoke records the hosted CI runtime fallback explicitly instead of implying desktop host delegation.

## [0.2.1] - 2026-08-25

### Added

- Capability-aware VAAPI/NVENC selection with automatic VAAPI render-node discovery.
- `MEDIA_STUDIO_VAAPI_DEVICE` and `vaapi_device` configuration override.
- Arch `media-studio-git` PKGBUILD and experimental host-integration Flatpak manifest.

### Changed

- `media-studio doctor` reports effective VAAPI/NVENC availability and selected device.
- Hardware profiles fail over only after encoder/device capability checks and log the exact reason.

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
