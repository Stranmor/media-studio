# Packaging

## Arch Linux

`packaging/arch/PKGBUILD` tracks the reviewed `v0.2.1` tag and builds `media-studio-git` with Arch's Rust toolchain. The package installs only the binary and documentation; run `media-studio install` as the user who owns the Dolphin session.

## Flatpak

`packaging/flatpak/io.github.stranmor.MediaStudio.yml` is an experimental host-integration manifest. It installs explicit wrappers for `ffmpeg`, `ffprobe`, ImageMagick, user-systemd, KDE cache and dialog commands; wrappers call the same host tools through `flatpak-spawn --host`. It intentionally keeps access to host media tools, `/dev/dri`, user-systemd and Dolphin data, so it is not a strict sandboxed replacement for the native package.

Build with:

```bash
flatpak-builder --force-clean build-dir packaging/flatpak/io.github.stranmor.MediaStudio.yml
```
