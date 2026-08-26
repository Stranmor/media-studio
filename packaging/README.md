# Packaging

## Arch Linux

`packaging/arch/PKGBUILD` builds the `media-studio` package with Arch's Rust
toolchain. The package installs the binary and documentation; run
`media-studio install` as the user who owns the Dolphin session.

## Flatpak profiles

The repository deliberately ships two Flatpak manifests with different trust
boundaries:

- `io.github.stranmor.MediaStudio.yml` is the strict sandbox profile. It uses
  Wayland/X11, DRI, and the standard XDG media directories only. FFmpeg and
  FFprobe plus the `codecs-extra` runtime extension are supplied by the
  Freedesktop runtime; ImageMagick, Dolphin KIO,
  user-systemd, and host dialogs are outside this profile.
- `io.github.stranmor.MediaStudio.HostIntegration.yml` is experimental host
  integration. It delegates FFmpeg, FFprobe, ImageMagick, user-systemd, KDE
  cache, and dialog commands through `flatpak-spawn --host`, and therefore is
  not a strict sandbox.

Build either profile with:

```bash
flatpak-builder --force-clean build-dir packaging/flatpak/io.github.stranmor.MediaStudio.yml
flatpak-builder --force-clean build-dir packaging/flatpak/io.github.stranmor.MediaStudio.HostIntegration.yml
```

`packaging/flatpak/conversion-smoke.sh` performs a real conversion inside an
installed bundle, verifies the output with FFprobe and full decode, and checks
the Media Studio `RESULT=verified` job marker.

## GPU runners

Use `packaging/gpu/provision-runner.sh` to register one dedicated VAAPI or
NVENC Actions runner and keep it under a bounded user-systemd service. The
workflow emits a receipt and preserves the conversion log as an Actions
artifact.
