# Stateless release review

Review only the current Media Studio v0.2.1 candidate and the bounded manifest supplied with it.

This is the pre-publicization gate. The candidate is intentionally reviewed before the authorized `main` push and `v0.2.1` tag; absence of a pushed tag is expected and is not a defect when all local source and consumer checks pass.

Verdict must be one of `positive`, `negative`, `pending`, `failed`, or `unknown`.

Acceptance criteria:

1. VAAPI and NVENC profiles must check the actual FFmpeg encoder capability; VAAPI must resolve a real render node and support an explicit override; software fallback must remain fail-closed when disabled and be observable when enabled.
2. Arch packaging must be buildable from the candidate source. The Flatpak manifest is an explicitly experimental, non-release host-integration artifact: validate its syntax, v0.2.1 source, explicit host-tool wrappers, and limitation statement, but do not require a flatpak-builder runtime claim when that builder is unavailable in the bounded environment.
3. Existing Dolphin MIME menus, atomic output commits, and queue/watch behavior must not regress. Public installer and release-asset readback are a post-push gate, not a pre-publicization requirement.
4. The candidate must pass its Rust tests, formatting, clippy, packaging validation, and representative consumer checks before a positive verdict; unverified Flatpak runtime behavior must remain outside the release claim.

Return material defects first, then bounded risks and a concise verdict.
