# Flathub submission bundle

This directory contains the strict standalone Flatpak manifest and the
generated Cargo source closure for a human-reviewed Flathub submission.

## Scope

`io.github.stranmor.MediaStudio.yml` packages the standalone command-line media
workflow. It has no host filesystem, session bus, systemd, KIO, or network
permission. Dolphin service menus, watch-folders, host dialogs, and host tool
delegation remain in the native package and the explicitly experimental
`HostIntegration` manifest.

The manifest is pinned to release tag `v0.2.5` and commit
`d314bd4aa4da74b6047c67a283606950c3421542`. The commit is deliberately the
upstream release commit, while this submission metadata can evolve separately.

## Reproducible offline build

`Cargo.lock` and `cargo-sources.json` are committed because Flathub builders do
not allow network access during the build phase. Regenerate the generated file
from the matching upstream lockfile whenever Rust dependencies change, then
run:

```bash
packaging/flathub/verify-submission.sh
```

The preflight checks the app ID, immutable source identity, strict permissions,
metadata, icon dimensions, license, and generated source closure. The actual
Flatpak build is exercised by `.github/workflows/flathub-preflight.yml`.

## Submission boundary

This repository does not automate a Flathub pull request. A human maintainer
must review the policy fit and create the submission through Flathub's normal
review flow. The project is primarily a Dolphin/CLI integration tool, so the
strict bundle is intentionally presented as a policy-review candidate rather
than an assumed approval.
