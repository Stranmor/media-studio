#!/usr/bin/env bash
set -euo pipefail

output_dir=${1:-}
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
root_dir="$(cd -- "$script_dir/../.." && pwd)"
manifest="$script_dir/io.github.stranmor.MediaStudio.yml"
sources="$script_dir/cargo-sources.json"

[[ $# -eq 1 ]] || {
  printf 'Usage: export-submission.sh OUTPUT_DIRECTORY\n' >&2
  exit 2
}
[[ -n "$output_dir" && "$output_dir" != / && "$output_dir" != "$root_dir" && "$output_dir" != "$script_dir" ]] || {
  printf 'refusing to export into the source tree or filesystem root\n' >&2
  exit 2
}
[[ -s "$manifest" && -s "$sources" ]] || {
  printf 'canonical Flathub inputs are incomplete\n' >&2
  exit 1
}

mkdir -p "$output_dir"
install -Dm644 "$manifest" "$output_dir/io.github.stranmor.MediaStudio.yml"
install -Dm644 "$sources" "$output_dir/cargo-sources.json"

printf 'FLATHUB_SUBMISSION_EXPORT=ready directory=%s files=2\n' "$output_dir"
