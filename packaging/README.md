# Packaging

## Arch Linux

`packaging/arch/PKGBUILD` tracks the public `main` branch and builds `media-studio-git` with Arch's Rust toolchain. The package installs only the binary and documentation; run `media-studio install` as the user who owns the Dolphin session.

## Flatpak

`packaging/flatpak/io.github.stranmor.MediaStudio.yml` is an experimental host-integration manifest. It intentionally keeps access to host media tools, `/dev/dri`, user-systemd and Dolphin data, so it is not a strict sandboxed replacement for the native package.

Build with:

```bash
flatpak-builder --force-clean build-dir packaging/flatpak/io.github.stranmor.MediaStudio.yml
```
