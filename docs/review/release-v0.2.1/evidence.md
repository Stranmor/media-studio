# Candidate evidence

Evidence is bounded to candidate commit `4b44fce7c1001760b2c57b386476f6532981b6df`; package sources resolve implementation commit `63dc708153b76ab7f7527579f103ce24ac106872`.

- Rust formatting passed with isolated rustfmt configuration.
- `cargo test --all-targets`: 14 passed, 0 failed.
- `cargo clippy --all-targets -- -D warnings`: passed.
- `cargo build --release`: passed for `media-studio 0.2.1`.
- VAAPI consumer run: `HARDWARE=VAAPI selected DEVICE=/dev/dri/renderD128`, output committed, `RESULT=verified`.
- NVENC consumer run: capability selected, runtime returned exit status 255, `HARDWARE_FALLBACK=software AFTER_STATUS=...`, output committed, `RESULT=verified`.
- Invalid VAAPI override `/dev/null`: doctor reports `vaapi: MISSING` and does not accept the non-render node.
- Disabled fallback: `hardware_fallback=false` exits nonzero after NVENC failure and emits no software fallback marker.
- Target-size hardware fallback: NVENC runtime failure falls back to libx264 while preserving `-b:v`, `-maxrate`, `-bufsize`, and `-b:a` rate flags; output completed and status is `completed`.
- Queue propagation: fake `systemd-run` captured `--setenv=MEDIA_STUDIO_VAAPI_DEVICE=/dev/dri/renderD128`.
- Watch propagation: generated user unit contains `Environment=MEDIA_STUDIO_VAAPI_DEVICE="/dev/dri/renderD128"` before `ExecStart`.
- Legacy menu migration: isolated install moved a pre-existing `media-studio.desktop` to `servicemenus/disabled/`, and isolated uninstall restored it byte-for-byte.
- Arch packaging: project `makepkg --packagelist` parses the stable `media-studio` PKGBUILD; archive-based build of the same package recipe produced `media-studio-0.2.1-1-x86_64.pkg.tar.zst`; `pacman -Qip` confirms version 0.2.1-1 and dependencies `ffmpeg kio systemd`.
- Flatpak: YAML/app-id/source-commit validation, wrapper shell validation, and explicit `flatpak-spawn --host` wrapper contract passed. `flatpak-builder` is unavailable locally, so Flatpak runtime/build readiness remains explicitly unclaimed.
- Public `v0.2.0` remains the last published consumer; v0.2.1 is not pushed until the fresh review is positive.
