# Flathub submission bundle

This directory contains the strict standalone Flatpak manifest and the
generated Cargo source closure for a human-reviewed Flathub submission.

## Scope

`io.github.stranmor.MediaStudio.yml` packages the standalone command-line media
workflow. It has no host filesystem, session bus, systemd, KIO, or network
permission. Dolphin service menus, watch-folders, host dialogs, and host tool
delegation remain in the native package and the explicitly experimental
`HostIntegration` manifest.

The manifest is pinned to release tag `v0.2.7` and commit
`948fd5f585aa2b6ccf8cd62eb360c88fa24feebb`. The commit is deliberately the
upstream release commit, while this submission metadata can evolve separately.

## Reproducible offline build

`Cargo.lock` and `cargo-sources.json` are committed because Flathub builders do
not allow network access during the build phase. Regenerate the generated file
from the matching upstream lockfile whenever Rust dependencies change, then
run:

```bash
packaging/flathub/verify-submission.sh
```

The Flathub repository requires its manifest at the submission root. Export the
canonical two-file submission tree without duplicating the source of truth:

```bash
packaging/flathub/export-submission.sh /tmp/media-studio-flathub-submission
```

The preflight checks the app ID, immutable source identity, strict permissions,
metadata, icon dimensions, license, and generated source closure. It reports a
technical pass separately from the unresolved Flathub policy gate. The actual
Flatpak build is exercised by `.github/workflows/flathub-preflight.yml`.

## Submission boundary

This repository does not automate a Flathub pull request. Current Flathub rules
prohibit AI-generated submission content and normally reject console or
host-dependent utilities. A human maintainer must independently resolve that
policy boundary, including any exception request, before using the technical
bundle in Flathub's normal review flow.
