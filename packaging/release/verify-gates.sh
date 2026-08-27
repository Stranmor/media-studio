#!/usr/bin/env bash
set -euo pipefail

assets_dir=${1:?usage: verify-gates.sh RELEASE_ASSETS RELEASE_GATES EXPECTED_COMMIT}
gates_dir=${2:?usage: verify-gates.sh RELEASE_ASSETS RELEASE_GATES EXPECTED_COMMIT}
expected_commit=${3:-${GITHUB_SHA:-}}

[[ -d "$assets_dir" ]] || {
  printf 'release assets directory does not exist: %s\n' "$assets_dir" >&2
  exit 2
}
[[ -d "$gates_dir" ]] || {
  printf 'release gates directory does not exist: %s\n' "$gates_dir" >&2
  exit 2
}
[[ "$expected_commit" =~ ^[0-9a-fA-F]{40}$ ]] || {
  printf 'an exact 40-character release commit is required for provenance checks\n' >&2
  exit 2
}

verify_source_sha() {
  local provenance="$1"
  local label="$2"
  local value extra
  [[ -s "$provenance" ]] || {
    printf '%s provenance receipt is missing: %s\n' "$label" "$provenance" >&2
    exit 1
  }
  read -r value extra < "$provenance"
  [[ "$value" == "$expected_commit" && -z "${extra:-}" ]] || {
    printf '%s is not bound to release commit %s\n' "$label" "$expected_commit" >&2
    exit 1
  }
  [[ "$(wc -l < "$provenance")" -eq 1 ]] || {
    printf '%s provenance receipt must contain exactly one line\n' "$label" >&2
    exit 1
  }
}

[[ -s Cargo.toml ]] || {
  printf 'Cargo.toml is required to validate package versions\n' >&2
  exit 2
}
expected_version="$(awk -F'"' '$1 == "version = " { print $2; exit }' Cargo.toml)"
[[ "$expected_version" =~ ^[0-9]+\.[0-9]+\.[0-9]+([.-][0-9A-Za-z.-]+)?$ ]] || {
  printf 'could not resolve a valid package version from Cargo.toml\n' >&2
  exit 2
}

single_asset() {
  local pattern="$1"
  local label="$2"
  local path
  local -a matches=()
  while IFS= read -r -d '' path; do
    matches+=("$path")
  done < <(find "$assets_dir" -maxdepth 1 -type f -name "$pattern" -print0 | sort -z)
  if ((${#matches[@]} != 1)); then
    printf 'expected exactly one %s matching %s, found %s\n' \
      "$label" "$pattern" "${#matches[@]}" >&2
    exit 1
  fi
  printf '%s\n' "${matches[0]}"
}

verify_deb() {
  local package="$1"
  local expected_arch="$2"
  local package_version
  command -v dpkg-deb >/dev/null || {
    printf 'dpkg-deb is required to validate %s\n' "$package" >&2
    exit 1
  }
  [[ "$(dpkg-deb -f "$package" Package)" == media-studio ]]
  package_version="$(dpkg-deb -f "$package" Version)"
  [[ "$package_version" == "$expected_version" || "$package_version" == "$expected_version-"* ]]
  [[ "$(dpkg-deb -f "$package" Architecture)" == "$expected_arch" ]]
}

verify_rpm() {
  local package="$1"
  local expected_arch="$2"
  local package_version
  command -v rpm >/dev/null || {
    printf 'rpm is required to validate %s\n' "$package" >&2
    exit 1
  }
  [[ "$(rpm -qp --queryformat '%{NAME}' "$package")" == media-studio ]]
  package_version="$(rpm -qp --queryformat '%{VERSION}' "$package")"
  [[ "$package_version" == "$expected_version" ]]
  [[ "$(rpm -qp --queryformat '%{ARCH}' "$package")" == "$expected_arch" ]]
}

verify_checksum_manifest() {
  local manifest="$1"
  local target="$2"
  local expected_name="$3"
  local label="$4"
  local hash name extra actual
  local record_count=0
  [[ -s "$manifest" ]]
  while read -r hash name extra; do
    [[ -n "${hash:-}" ]] || continue
    record_count=$((record_count + 1))
    [[ "$record_count" -eq 1 ]] || {
      printf '%s must contain exactly one checksum record\n' "$label" >&2
      exit 1
    }
    [[ "$hash" =~ ^[0-9a-fA-F]{64}$ ]] || {
      printf '%s contains an invalid SHA-256 digest\n' "$label" >&2
      exit 1
    }
    name="${name#\*}"
    [[ "$name" == "$expected_name" && -z "${extra:-}" ]] || {
      printf '%s is not bound to %s\n' "$label" "$expected_name" >&2
      exit 1
    }
    actual="$(sha256sum -- "$target" | awk '{ print $1 }')"
    [[ "$actual" == "$hash" ]] || {
      printf '%s digest mismatch for %s\n' "$label" "$expected_name" >&2
      exit 1
    }
  done < "$manifest"
  [[ "$record_count" -eq 1 ]] || {
    printf '%s does not contain a checksum record\n' "$label" >&2
    exit 1
  }
}

verify_tarball() {
  local tarball="$1"
  local binary="$2"
  local expected_sha archive_sha
  [[ -s "$tarball" ]]
  tar -tzf "$tarball" | grep -Fx "$binary" >/dev/null
  expected_sha="$(sha256sum -- "$assets_dir/$binary" | awk '{ print $1 }')"
  archive_sha="$(tar -xOf "$tarball" "$binary" | sha256sum | awk '{ print $1 }')"
  [[ "$archive_sha" == "$expected_sha" ]]
}

verify_media_output() {
  local output="$1"
  local label="$2"
  local duration
  command -v ffprobe >/dev/null || {
    printf 'ffprobe is required to validate %s\n' "$label" >&2
    exit 1
  }
  command -v ffmpeg >/dev/null || {
    printf 'ffmpeg is required to validate %s\n' "$label" >&2
    exit 1
  }
  duration="$(ffprobe -v error -show_entries format=duration -of default=noprint_wrappers=1:nokey=1 "$output")"
  awk -v duration="$duration" 'BEGIN { exit !(duration ~ /^[0-9]+([.][0-9]+)?$/ && duration > 0) }' || {
    printf '%s has no positive media duration: %s\n' "$label" "$duration" >&2
    exit 1
  }
  ffmpeg -hide_banner -nostdin -loglevel error -xerror -i "$output" -map 0:v? -map 0:a? -f null -
}

verify_receipt_duration() {
  local receipt="$1"
  local output="$2"
  local label="$3"
  local field_count receipt_duration output_duration
  field_count="$(grep -Ec '^duration_seconds=' "$receipt" || true)"
  [[ "$field_count" -eq 1 ]] || {
    printf '%s must contain exactly one duration_seconds field\n' "$label" >&2
    exit 1
  }
  receipt_duration="$(sed -n 's/^duration_seconds=//p' "$receipt")"
  output_duration="$(ffprobe -v error -show_entries format=duration \
    -of default=noprint_wrappers=1:nokey=1 "$output")"
  awk -v receipt="$receipt_duration" -v output="$output_duration" '
    BEGIN {
      valid = receipt ~ /^[0-9]+([.][0-9]+)?$/ && output ~ /^[0-9]+([.][0-9]+)?$/
      positive = receipt > 0 && output > 0
      within_tolerance = (receipt - output < 0 ? output - receipt : receipt - output) <= 0.01
      exit !(valid && positive && within_tolerance)
    }
  ' || {
    printf '%s duration_seconds does not match FFprobe output: receipt=%s output=%s\n' \
      "$label" "$receipt_duration" "$output_duration" >&2
    exit 1
  }
}

for arch in x86_64 aarch64; do
  binary="media-studio-linux-$arch"
  test -s "$assets_dir/$binary"
  verify_source_sha "$assets_dir/$binary.source-sha" "$binary source"
  test -s "$assets_dir/$binary.sha256"
  verify_checksum_manifest "$assets_dir/$binary.sha256" "$assets_dir/$binary" "$binary" "$binary checksum"
  verify_tarball "$assets_dir/$binary.tar.gz" "$binary"
  case "$arch" in
    x86_64)
      deb_arch=amd64
      rpm_arch=x86_64
      ;;
    aarch64)
      deb_arch=arm64
      rpm_arch=aarch64
      ;;
  esac
  deb_package="$(single_asset "media-studio_*_${deb_arch}.deb" "$arch Debian package")"
  rpm_package="$(single_asset "media-studio-*.${rpm_arch}.rpm" "$arch RPM package")"
  test -s "$deb_package"
  test -s "$rpm_package"
  verify_deb "$deb_package" "$deb_arch"
  verify_rpm "$rpm_package" "$rpm_arch"
done

for bundle in media-studio-sandbox.flatpak media-studio-host-integration.flatpak; do
  test -s "$assets_dir/$bundle"
  verify_source_sha "$assets_dir/$bundle.source-sha" "$bundle source"
done

for backend in vaapi nvenc; do
  receipt="$gates_dir/hardware-$backend/receipt.txt"
  job_log="$gates_dir/hardware-$backend/job.log"
  output="$gates_dir/hardware-$backend/output.mp4"
  checksum="$gates_dir/hardware-$backend/output.sha256"
  verify_source_sha "$gates_dir/hardware-$backend/source-sha.txt" "hardware-$backend source"
  test -s "$receipt"
  test -s "$job_log"
  test -s "$output"
  test -s "$checksum"
  verify_checksum_manifest "$checksum" "$output" output.mp4 "hardware-$backend checksum"
  verify_media_output "$output" "hardware-$backend output"
  verify_receipt_duration "$receipt" "$output" "hardware-$backend receipt"
  grep -Fx 'result=verified' "$receipt"
  grep -Fx "backend=$backend" "$receipt"
  grep -Fx "encoder=h264_$backend" "$receipt"
  grep -Eq '^runner=[^[:space:]]+$' "$receipt"
  grep -Fx 'RESULT=verified' "$job_log"
  backend_label="$(printf '%s' "$backend" | tr '[:lower:]' '[:upper:]')"
  encoder="h264_$backend"
  grep -Fx "HARDWARE_USED=$backend_label ENCODER=$encoder" "$job_log"
  if grep -Fq 'HARDWARE_FALLBACK=' "$job_log"; then
    printf 'hardware fallback marker found for %s\n' "$backend" >&2
    exit 1
  fi
  if [ "$backend" = vaapi ]; then
    vaapi_device="$(sed -n 's/^vaapi_device=//p' "$receipt")"
    [[ "$vaapi_device" =~ ^/dev/dri/renderD[0-9]+$ ]]
    grep -Fx "HARDWARE=VAAPI selected DEVICE=$vaapi_device" "$job_log"
  else
    hardware_detail="$(sed -n 's/^hardware_detail=//p' "$receipt")"
    test -n "$hardware_detail"
    grep -Fx 'HARDWARE=NVENC selected DEVICE=nvidia:nvenc' "$job_log"
  fi
done

for kind in sandbox host-integration; do
  receipt="$gates_dir/flatpak-$kind/receipt.txt"
  job_log="$gates_dir/flatpak-$kind/job.log"
  output="$gates_dir/flatpak-$kind/output.mp4"
  checksum="$gates_dir/flatpak-$kind/output.sha256"
  verify_source_sha "$gates_dir/flatpak-$kind/source-sha.txt" "Flatpak $kind source"
  test -s "$receipt"
  test -s "$job_log"
  test -s "$output"
  test -s "$checksum"
  verify_checksum_manifest "$checksum" "$output" output.mp4 "Flatpak $kind checksum"
  verify_media_output "$output" "Flatpak $kind output"
  verify_receipt_duration "$receipt" "$output" "Flatpak $kind receipt"
  if [ "$kind" = sandbox ]; then
    expected_app_id='io.github.stranmor.MediaStudio'
  else
    expected_app_id='io.github.stranmor.MediaStudio.HostIntegration'
  fi
  grep -Fx "app_id=$expected_app_id" "$receipt"
  grep -Fx 'result=verified' "$receipt"
  grep -Fx 'RESULT=verified' "$job_log"
  if [ "$kind" = sandbox ]; then
    grep -Fx 'tool_mode=sandbox' "$receipt"
  else
    grep -Fx 'tool_mode=runtime-fallback' "$receipt"
  fi
done

printf 'release_gate=verified\nnative_arches=x86_64,aarch64\nflatpak_profiles=sandbox,host-integration\nhardware_backends=vaapi,nvenc\n' \
  > "$gates_dir/release-gates.txt"
