#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
Usage:
  verify-source.sh --resolve --repo URL_OR_PATH --tag TAG
  verify-source.sh --repo URL_OR_PATH --tag TAG --expected-commit SHA
EOF
}

mode="verify"
repo=""
tag=""
expected_commit=""
while (($# > 0)); do
  case "$1" in
    --resolve)
      mode="resolve"
      shift
      ;;
    --repo)
      repo=${2:?missing value for --repo}
      shift 2
      ;;
    --tag)
      tag=${2:?missing value for --tag}
      shift 2
      ;;
    --expected-commit)
      expected_commit=${2:?missing value for --expected-commit}
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage
      exit 2
      ;;
  esac
done

[[ "$repo" != *$'\n'* && "$repo" != *$'\r'* ]] || {
  printf 'repository contains a control character\n' >&2
  exit 2
}
[[ "$tag" =~ ^[A-Za-z0-9._/-]+$ ]] || {
  printf 'invalid tag: %s\n' "$tag" >&2
  exit 2
}

resolve_commit() {
  local peeled="" plain="" sha ref
  if [[ "$repo" == "." || "$repo" == /* || "$repo" == ./* ]]; then
    git -C "$repo" rev-parse --verify "$tag^{commit}"
    return
  fi
  while IFS=$'\t' read -r sha ref; do
    [[ "$sha" =~ ^[0-9a-fA-F]{40}$ ]] || continue
    case "$ref" in
      "refs/tags/$tag^{}") peeled="$sha" ;;
      "refs/tags/$tag") plain="$sha" ;;
    esac
  done < <(git ls-remote "$repo" "refs/tags/$tag^{}" "refs/tags/$tag")
  printf '%s\n' "${peeled:-$plain}"
}

resolved_commit="$(resolve_commit)"
[[ "$resolved_commit" =~ ^[0-9a-fA-F]{40}$ ]] || {
  printf 'tag does not resolve to a commit: %s\n' "$tag" >&2
  exit 1
}

if [[ "$mode" == resolve ]]; then
  printf '%s\n' "$resolved_commit"
  exit 0
fi

[[ "$expected_commit" =~ ^[0-9a-fA-F]{40}$ ]] || {
  printf 'expected commit must be a 40-character SHA-1\n' >&2
  exit 2
}
if [[ "${resolved_commit,,}" != "${expected_commit,,}" ]]; then
  printf 'source identity mismatch: tag=%s resolved=%s expected=%s\n' \
    "$tag" "$resolved_commit" "$expected_commit" >&2
  exit 1
fi
printf 'arch_source=verified tag=%s commit=%s\n' "$tag" "$expected_commit"
