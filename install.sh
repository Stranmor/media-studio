#!/usr/bin/env bash
set -euo pipefail

repo="${MEDIA_STUDIO_REPO:-Stranmor/media-studio}"
case "$(uname -m)" in
  x86_64) asset="media-studio-linux-x86_64" ;;
  aarch64|arm64) asset="media-studio-linux-aarch64" ;;
  *) echo "Unsupported architecture: $(uname -m)" >&2; exit 2 ;;
esac
base_url="https://github.com/${repo}/releases/latest/download"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

echo "Downloading ${repo}/${asset}"
curl --fail --location --silent --show-error "${base_url}/${asset}" --output "${tmp_dir}/${asset}"
curl --fail --location --silent --show-error "${base_url}/${asset}.sha256" --output "${tmp_dir}/${asset}.sha256"
(cd "$tmp_dir" && sha256sum --check "${asset}.sha256")

install -Dm755 "${tmp_dir}/${asset}" "${HOME}/.local/bin/media-studio"
"${HOME}/.local/bin/media-studio" install
echo "Media Studio installed at ${HOME}/.local/bin/media-studio"
