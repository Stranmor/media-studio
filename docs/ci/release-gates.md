# Release gates

The tag release workflow publishes only after all three independent gates pass:

1. native Rust and distro-package builds for x86_64 and aarch64;
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
fail the publish job.

The hosted host-integration smoke records `tool_mode=runtime-fallback` because
the privileged build container has no desktop Flatpak session helper; the
published bundle remains explicitly experimental and uses host delegation on a
real KDE session.
