#!/usr/bin/env bash
set -euo pipefail

manifest="${1:-$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)/io.github.stranmor.MediaStudio.yml}"
manifest_dir="$(cd -- "$(dirname -- "$manifest")" && pwd)"
root_dir="$(cd -- "$manifest_dir/../.." && pwd)"
app_id="io.github.stranmor.MediaStudio"

[[ -f "$manifest" ]] || {
  printf 'Flathub manifest is missing: %s\n' "$manifest" >&2
  exit 2
}
[[ -d "$root_dir/.git" ]] || {
  printf 'repository root is not a Git checkout: %s\n' "$root_dir" >&2
  exit 2
}
command -v ruby >/dev/null || { printf 'ruby is required\n' >&2; exit 2; }
command -v appstreamcli >/dev/null || { printf 'appstreamcli is required\n' >&2; exit 2; }
command -v desktop-file-validate >/dev/null || { printf 'desktop-file-validate is required\n' >&2; exit 2; }
command -v identify >/dev/null || { printf 'ImageMagick identify is required\n' >&2; exit 2; }

read_manifest_value() {
  local expression="$1"
  ruby -ryaml -e "data = YAML.load_file(ARGV.fetch(0)); value = ${expression}; abort unless value; puts value" "$manifest"
}

manifest_app_id="$(read_manifest_value 'data.fetch("app-id")')"
runtime_version="$(read_manifest_value 'data.fetch("runtime-version")')"
source_url="$(read_manifest_value 'data.fetch("modules").fetch(0).fetch("sources").fetch(0).fetch("url")')"
source_tag="$(read_manifest_value 'data.fetch("modules").fetch(0).fetch("sources").fetch(0).fetch("tag")')"
source_commit="$(read_manifest_value 'data.fetch("modules").fetch(0).fetch("sources").fetch(0).fetch("commit")')"

[[ "$manifest_app_id" == "$app_id" ]] || {
  printf 'unexpected app id: %s\n' "$manifest_app_id" >&2
  exit 1
}
[[ "$runtime_version" == 25.08 ]] || {
  printf 'unexpected Freedesktop runtime version: %s\n' "$runtime_version" >&2
  exit 1
}
[[ "$source_url" == https://github.com/Stranmor/media-studio.git ]] || {
  printf 'source URL is not the canonical upstream repository\n' >&2
  exit 1
}
[[ "$source_tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z.-]+)?$ ]] || {
  printf 'source tag is not an immutable release tag: %s\n' "$source_tag" >&2
  exit 1
}
[[ "$source_commit" =~ ^[0-9a-fA-F]{40}$ ]] || {
  printf 'source commit is not a full SHA-1: %s\n' "$source_commit" >&2
  exit 1
}

if grep -nE -- '--share=network|--filesystem=(host|home)|--socket=session-bus|--talk-name=org\.freedesktop\.(systemd1|Flatpak)|flatpak-spawn' "$manifest"; then
  printf 'strict Flathub manifest contains a host or network permission\n' >&2
  exit 1
fi
grep -Eq '^[[:space:]]+- cargo-sources\.json$' "$manifest" || {
  printf 'generated cargo-sources.json is not included in the manifest\n' >&2
  exit 1
}
grep -Fq -- 'cargo --offline fetch --manifest-path Cargo.toml --locked' "$manifest" || {
  printf 'offline locked Cargo fetch is missing\n' >&2
  exit 1
}
grep -Fq -- 'cargo --offline build --release --locked' "$manifest" || {
  printf 'offline locked Cargo build is missing\n' >&2
  exit 1
}
[[ -s "$root_dir/Cargo.lock" ]] || { printf 'Cargo.lock is missing\n' >&2; exit 1; }
[[ -s "$root_dir/packaging/flathub/cargo-sources.json" ]] || {
  printf 'cargo-sources.json is missing\n' >&2
  exit 1
}

ruby -rjson -e 'items = JSON.parse(File.read(ARGV.fetch(0))); abort unless items.is_a?(Array) && items.any? { |item| item["type"] == "archive" }; puts "cargo_sources=verified entries=#{items.length}"' "$root_dir/packaging/flathub/cargo-sources.json"

desktop="$root_dir/packaging/flatpak/io.github.stranmor.MediaStudio.desktop"
metainfo="$root_dir/packaging/flatpak/io.github.stranmor.MediaStudio.metainfo.xml"
icon_128="$root_dir/packaging/flatpak/icons/hicolor/128x128/apps/$app_id.png"
icon_256="$root_dir/packaging/flatpak/icons/hicolor/256x256/apps/$app_id.png"
license_file="$root_dir/LICENSE"
appstreamcli validate "$metainfo"
desktop-file-validate "$desktop"
grep -Fqx "  <id>$app_id</id>" "$metainfo"
grep -Fqx "Icon=$app_id" "$desktop"
grep -Fqx "  <launchable type=\"desktop-id\">$app_id.desktop</launchable>" "$metainfo"
for file in "$icon_128" "$icon_256" "$license_file"; do
  [[ -s "$file" ]] || {
    printf 'required packaging file is missing: %s\n' "$file" >&2
    exit 1
  }
done
identify -format '%m %wx%h\n' "$icon_128" "$icon_256" | awk '
  $1 != "PNG" || $2 !~ /^[0-9]+x[0-9]+$/ { exit 1 }
  { split($2, size, "x"); if (size[1] < 128 || size[2] < 128) exit 1 }
'

if [[ "${MEDIA_STUDIO_SKIP_REMOTE_SOURCE_CHECK:-0}" != 1 ]]; then
  remote_commit="$($root_dir/packaging/arch/verify-source.sh --resolve --repo "$source_url" --tag "$source_tag")"
  [[ "${remote_commit,,}" == "${source_commit,,}" ]] || {
    printf 'manifest source commit differs from remote tag: manifest=%s remote=%s\n' "$source_commit" "$remote_commit" >&2
    exit 1
  }
fi

printf 'FLATHUB_PREFLIGHT=passed app_id=%s runtime=%s source_tag=%s source_commit=%s\n' \
  "$manifest_app_id" "$runtime_version" "$source_tag" "$source_commit"
