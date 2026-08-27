# Packaging

## Arch Linux

`packaging/arch/PKGBUILD` builds the `media-studio` package with Arch's Rust
toolchain. The package installs the binary and documentation; run
`media-studio install` as the user who owns the Dolphin session. The PKGBUILD
resolves the release tag to a commit before `prepare()` and fails closed on a
source mismatch; `packaging/arch/verify-source.sh` is the same identity check
used by release CI.

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

The canonical manifests build the checked-out candidate (`type: dir`) so a
local build cannot silently drift to a later branch commit; the CI manifests
are parity-checked copies of the same contract.

Hosted CI runs the experimental host-integration bundle with an explicit
`runtime-fallback` smoke mode because a GitHub container has no desktop
Flatpak session helper; normal desktop launches keep the default host-tool
delegation through `flatpak-spawn --host`.

`media-studio install` requires FFmpeg/FFprobe and user-systemd. KDE cache
builders and `zenity`/`kdialog` are optional helpers: Media Studio probes the
actual command (including Flatpak host wrappers), keeps installation successful
when a helper is absent or fails, and reports the cache/dialog capability as
unavailable instead of treating a wrapper path as proof of host availability.

`provision-runner.sh --replace` acquires a per-runner provisioning lock and
refuses to mutate a runner directory while its user service or Runner.Listener
process is active. Stop the service first, then rerun the explicit replacement.

## GPU runners

Use `packaging/gpu/provision-runner.sh` to register one dedicated VAAPI or
NVENC Actions runner and keep it under a bounded user-systemd service. The
workflow emits a receipt and preserves the conversion log as an Actions
artifact. The generated service owns an exclusive runner lock, so a second
runner process for the same directory fails closed instead of corrupting job
receipts; run the provisioner again only after any manually started runner has
exited.

Pass the one-time registration token through `--token-stdin` (the preferred
route). Provisioning uses the `expect` PTY helper to feed it through a private
file descriptor, never through `Runner.Listener` argv or a persistent
environment variable; `RUNNER_REGISTRATION_TOKEN` remains only as a compatibility
fallback and is cleared immediately after read. The PTY handshake records a
redacted marker transcript, accepts the supported registration-token prompt
variants, requires a zero child exit, and verifies regular `.runner` and
`.credentials` state files before enabling the service.
