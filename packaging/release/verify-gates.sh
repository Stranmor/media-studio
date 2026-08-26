#!/usr/bin/env bash
set -euo pipefail

assets_dir=${1:?usage: verify-gates.sh RELEASE_ASSETS RELEASE_GATES}
gates_dir=${2:?usage: verify-gates.sh RELEASE_ASSETS RELEASE_GATES}

[[ -d "$assets_dir" ]] || {
  printf 'release assets directory does not exist: %s\n' "$assets_dir" >&2
  exit 2
}
[[ -d "$gates_dir" ]] || {
  printf 'release gates directory does not exist: %s\n' "$gates_dir" >&2
  exit 2
}

for arch in x86_64 aarch64; do
  binary="media-studio-linux-$arch"
  test -s "$assets_dir/$binary"
  test -s "$assets_dir/$binary.sha256"
  (cd "$assets_dir" && sha256sum -c "$binary.sha256")
done

for bundle in media-studio-sandbox.flatpak media-studio-host-integration.flatpak; do
  test -s "$assets_dir/$bundle"
done

for backend in vaapi nvenc; do
  receipt="$gates_dir/hardware-$backend/receipt.txt"
  job_log="$gates_dir/hardware-$backend/job.log"
  output="$gates_dir/hardware-$backend/output.mp4"
  checksum="$gates_dir/hardware-$backend/output.sha256"
  test -s "$receipt"
  test -s "$job_log"
  test -s "$output"
  test -s "$checksum"
  (cd "$gates_dir/hardware-$backend" && sha256sum -c output.sha256)
  grep -Fx 'result=verified' "$receipt"
  grep -Fx "backend=$backend" "$receipt"
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
  test -s "$receipt"
  test -s "$job_log"
  test -s "$output"
  test -s "$checksum"
  (cd "$gates_dir/flatpak-$kind" && sha256sum -c output.sha256)
  grep -Fx 'result=verified' "$receipt"
  grep -Fx 'RESULT=verified' "$job_log"
done

printf 'release_gate=verified\nnative_arches=x86_64,aarch64\nflatpak_profiles=sandbox,host-integration\nhardware_backends=vaapi,nvenc\n' \
  > "$gates_dir/release-gates.txt"
