#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
Usage:
  verify-attestations.sh RELEASE_ASSETS REPOSITORY SOURCE_REF SOURCE_COMMIT

Example:
  verify-attestations.sh release-assets Stranmor/media-studio refs/tags/v0.2.5 SHA
EOF
}

assets_dir=${1:-}
repository=${2:-}
source_ref=${3:-}
source_commit=${4:-}

if [[ $# -ne 4 ]]; then
  usage
  exit 2
fi
[[ -d "$assets_dir" ]] || {
  printf 'release assets directory does not exist: %s\n' "$assets_dir" >&2
  exit 2
}
[[ "$repository" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] || {
  printf 'invalid GitHub repository: %s\n' "$repository" >&2
  exit 2
}
[[ "$source_ref" =~ ^refs/tags/v[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z.-]+)?$ ]] || {
  printf 'source ref must be an immutable version tag: %s\n' "$source_ref" >&2
  exit 2
}
[[ "$source_commit" =~ ^[0-9a-fA-F]{40}$ ]] || {
  printf 'source commit must be a 40-character SHA-1\n' >&2
  exit 2
}
command -v gh >/dev/null || {
  printf 'gh is required for online attestation verification\n' >&2
  exit 2
}

manifest="$assets_dir/release-assets.sha256"
[[ -s "$manifest" ]] || {
  printf 'release asset checksum manifest is missing: %s\n' "$manifest" >&2
  exit 1
}

verify_checksum_manifest() {
  local manifest_path="$1"
  local manifest_dir
  manifest_dir="$(dirname -- "$manifest_path")"
  (
    cd "$manifest_dir"
    sha256sum --check "$(basename -- "$manifest_path")"
  )
}

verify_checksum_manifest "$manifest"

attestation_count=0
while read -r digest name extra; do
  [[ -n "${digest:-}" ]] || continue
  [[ "$digest" =~ ^[0-9a-fA-F]{64}$ ]] || {
    printf 'invalid digest in %s: %s\n' "$manifest" "$digest" >&2
    exit 1
  }
  [[ -z "${extra:-}" ]] || {
    printf 'unexpected checksum fields in %s\n' "$manifest" >&2
    exit 1
  }
  name="${name#\*}"
  [[ "$name" != */* && "$name" != .* ]] || {
    printf 'invalid release asset name in %s: %s\n' "$manifest" "$name" >&2
    exit 1
  }
  asset="$assets_dir/$name"
  [[ -s "$asset" ]] || {
    printf 'release asset listed in checksum manifest is missing: %s\n' "$asset" >&2
    exit 1
  }
  printf 'verifying SLSA provenance: %s\n' "$name"
  gh attestation verify "$asset" \
    --repo "$repository" \
    --signer-workflow "$repository/.github/workflows/release.yml" \
    --source-ref "$source_ref" \
    --source-digest "$source_commit" >/dev/null
  attestation_count=$((attestation_count + 1))
done < "$manifest"

((attestation_count > 0)) || {
  printf 'checksum manifest contains no release assets\n' >&2
  exit 1
}

build_manifest="$assets_dir/build-subjects.sha256"
[[ -s "$build_manifest" ]] || {
  printf 'build subject checksum manifest is missing: %s\n' "$build_manifest" >&2
  exit 1
}
verify_checksum_manifest "$build_manifest"

primary_binary="$assets_dir/media-studio-linux-x86_64"
[[ -s "$primary_binary" ]] || {
  printf 'primary x86_64 binary is missing for SBOM verification\n' >&2
  exit 1
}
printf 'verifying SPDX SBOM attestation: media-studio-linux-x86_64\n'
gh attestation verify "$primary_binary" \
  --repo "$repository" \
  --signer-workflow "$repository/.github/workflows/release.yml" \
  --source-ref "$source_ref" \
  --source-digest "$source_commit" \
  --predicate-type 'https://spdx.dev/Document/v2.3' >/dev/null

printf 'ATTESTATIONS=verified subjects=%s source_ref=%s source_commit=%s\n' \
  "$attestation_count" "$source_ref" "$source_commit"
