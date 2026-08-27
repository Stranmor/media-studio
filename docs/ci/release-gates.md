# Release gates

The tag release workflow publishes only after all three independent gates pass:

1. native Rust and distro-package builds for x86_64 and aarch64, plus a real
   Arch package build from the exact release tag;
2. strict-sandbox and experimental host-integration Flatpak builds, each with
   an installed-bundle version/profile check and a real media conversion;
3. VAAPI and NVENC matrix conversions on trusted self-hosted runners with
   hardware fallback disabled and durable receipts.

The Flatpak and GPU workflows are reusable so the exact same jobs run on normal
`main` pushes and from a release tag. Successful release builds publish both
Flatpak bundles alongside the native assets; the host-integration bundle stays
clearly labeled as experimental.

The release publish job downloads every gate receipt and runs
`packaging/release/verify-gates.sh` before creating the GitHub release. Missing
native checksums, Flatpak receipts, hardware markers, or verified-result logs
fail the publish job. The verifier also re-runs FFprobe positive-duration and
full-decode checks for every downloaded media output, validates the expected
Flatpak app IDs, and checks exact hardware encoder fields.

Every release checkout uses the event commit SHA rather than a mutable tag
ref, and each published binary, Flatpak bundle, and gate receipt carries a
one-line source-SHA sidecar that the verifier matches to `github.sha`.

The native gate also validates each x86_64 and aarch64 tarball against its
binary checksum and inspects the matching Debian and RPM package metadata
(name, version, and architecture) before publication. The public gate receipt is
copied from this verifier rather than asserting passed values independently.

The hosted host-integration smoke records `tool_mode=runtime-fallback` because
the privileged build container has no desktop Flatpak session helper; the
published bundle remains explicitly experimental and uses host delegation on a
real KDE session.

Arch VCS packaging resolves the matching `v$pkgver` release tag to its commit
and rejects a checkout that is not exactly that tag and commit. The release
workflow independently verifies that the remote tag resolves to `github.sha`;
Debian and RPM metadata declare the FFmpeg and systemd runtime prerequisites
used by the installer.
