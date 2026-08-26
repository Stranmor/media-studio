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
  test -s "$receipt"
  test -s "$job_log"
  grep -Fx 'result=verified' "$receipt"
  grep -Fx "backend=$backend" "$receipt"
  backend_label="$(printf '%s' "$backend" | tr '[:lower:]' '[:upper:]')"
  encoder="h264_$backend"
  grep -Fx "HARDWARE_USED=$backend_label ENCODER=$encoder" "$job_log"
  if grep -Fq 'HARDWARE_FALLBACK=' "$job_log"; then
    printf 'hardware fallback marker found for %s\n' "$backend" >&2
    exit 1
  fi
done

for kind in sandbox host-integration; do
  receipt="$gates_dir/flatpak-$kind/receipt.txt"
  job_log="$gates_dir/flatpak-$kind/job.log"
  test -s "$receipt"
  test -s "$job_log"
  grep -Fx 'result=verified' "$receipt"
  grep -Fx 'RESULT=verified' "$job_log"
done

printf 'release_gate=verified\nnative_arches=x86_64,aarch64\nflatpak_profiles=sandbox,host-integration\nhardware_backends=vaapi,nvenc\n' \
  > "$gates_dir/release-gates.txt"
