# Independent CI and hardware review

Review the current Media Studio changes for real Flatpak-builder/runtime smoke coverage and VAAPI/NVENC GPU matrix coverage. Be defect-first and inspect the exact supplied files.

Acceptance criteria:

1. The normal CI workflow must invoke a real Flatpak builder, install the resulting bundle, and run the packaged binary inside Flatpak; static YAML checks alone are insufficient.
2. The Flatpak job must remain bounded and must not claim host integration or production sandbox readiness from a `--version`/profile smoke alone. The broad host permissions are intentional for this explicitly experimental host-integration artifact; treat them as a claim-boundary risk, not as a defect unless the workflow claims production sandbox readiness.
3. The GPU workflow must be isolated from untrusted pull requests, use a real self-hosted runner matrix, verify exact encoders, run the selected hardware profile with software fallback disabled, and require a verified output without fallback markers.
4. Runner labels and device requirements must be explicit enough for an operator to provision the two matrix entries without guessing.
5. Shell, YAML, Rust, and repository-consumer behavior must have no material correctness or safety defect introduced by these changes.

Project policy intentionally uses live-at-head dependency and action references, including the Flatpak runtime branch `master` and mutable upstream action branches. Do not require version pinning as a correction; check instead that protected-ref conditions and claim boundaries prevent those choices from being misrepresented as reproducibility guarantees. The privileged Flatpak builder container is an upstream builder requirement and is allowed only on push events, never on pull requests.

The packaging job compares the release manifest with `io.github.stranmor.MediaStudio.ci.yml` after replacing only the git source with the current checkout directory, and explicitly checks `--filesystem=xdg-data/kio`; treat that parity contract as satisfied when the supplied files contain those checks.

Return material defects first with severity, exact file/line or contract, impact, and the smallest systemic correction. Separate local proof, external CI proof, and claims that remain unverified. Return a typed verdict of `READY`, `REVISE`, `UNSUPPORTED`, or `UNCERTAIN`.
