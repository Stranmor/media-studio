#!/usr/bin/env bash
set -euo pipefail

app_id=${1:?usage: conversion-smoke.sh APP_ID [PROOF_DIR]}
proof_dir=${2:-}

if [[ ! "$app_id" =~ ^[A-Za-z0-9.-]+$ ]]; then
  printf 'invalid Flatpak application id: %s\n' "$app_id" >&2
  exit 2
fi

host_integration=0
if [[ "$app_id" == *.HostIntegration ]]; then
  host_integration=1
  command -v dbus-run-session >/dev/null 2>&1 || {
    printf 'host integration smoke requires dbus-run-session\n' >&2
    exit 2
  }
fi

created_proof=0
if [[ -z "$proof_dir" ]]; then
  proof_dir="$(mktemp -d "${HOME}/Downloads/media-studio-flatpak.XXXXXX")"
  created_proof=1
else
  case "$proof_dir" in
    /|"$HOME"|"$HOME/"|"$HOME/Downloads"|"$HOME/Downloads/")
      printf 'refusing an unsafe proof directory: %s\n' "$proof_dir" >&2
      exit 2
      ;;
  esac
  if [[ -e "$proof_dir" ]]; then
    printf 'proof directory already exists: %s\n' "$proof_dir" >&2
    exit 2
  fi
  mkdir -p "$proof_dir"
fi

cleanup() {
  if (( created_proof )); then
    rm -rf -- "$proof_dir"
  fi
}
trap cleanup EXIT

input_dir="$proof_dir/input"
output_dir="$proof_dir/output"
mkdir -p "$input_dir" "$output_dir"
input_file="$input_dir/fixture.mkv"
job_id="flatpak-${app_id//./-}-${GITHUB_RUN_ID:-local}"

flatpak_cmd() {
  if (( host_integration )); then
    timeout 180s dbus-run-session -- flatpak run --command="$1" "$app_id" "${@:2}"
  else
    timeout 180s flatpak run --command="$1" "$app_id" "${@:2}"
  fi
}

if (( host_integration )); then
  input_arg="$input_file"
  output_arg="$output_dir"
  probe_root="$output_dir"
else
  sandbox_home="$(flatpak_cmd sh -c 'printf %s "$HOME"')"
  sandbox_root="$sandbox_home/Downloads/$(basename "$proof_dir")"
  flatpak_cmd sh -c 'mkdir -p "$1/input" "$1/output"' sh "$sandbox_root"
  input_arg="$sandbox_root/input/fixture.mkv"
  output_arg="$sandbox_root/output"
  probe_root="$sandbox_root/output"
fi

flatpak_cmd ffmpeg \
  -hide_banner -loglevel error \
  -f lavfi -i testsrc2=size=640x360:rate=24:duration=2 \
  -f lavfi -i sine=frequency=1000:sample_rate=48000:duration=2 \
  -shortest -c:v libx264 -c:a aac "$input_arg"
if (( host_integration )); then
  test -s "$input_arg"
else
  flatpak_cmd sh -c 'test -s "$1"' sh "$input_arg"
fi

flatpak_cmd media-studio run \
  --job-id "$job_id" \
  --profile video_mp4 \
  --output-dir "$output_arg" \
  -- "$input_arg"

if (( host_integration )); then
  output_file="$(find "$output_dir" -maxdepth 1 -type f -name '*.mp4' -print -quit)"
  test -n "$output_file"
  test -s "$output_file"
  probe_output="$output_file"
else
  sandbox_output_file="$(flatpak_cmd sh -c 'find "$1" -maxdepth 1 -type f -name "*.mp4" -print -quit' sh "$probe_root")"
  test -n "$sandbox_output_file"
  output_file="$output_dir/$(basename "$sandbox_output_file")"
  flatpak_cmd sh -c 'cat "$1"' sh "$sandbox_output_file" > "$output_file"
  test -s "$output_file"
  probe_output="$sandbox_output_file"
fi

duration="$(flatpak_cmd ffprobe -v error \
  -show_entries format=duration \
  -of default=noprint_wrappers=1:nokey=1 "$probe_output")"
awk -v duration="$duration" 'BEGIN { exit !(duration > 0) }'

flatpak_cmd ffmpeg -hide_banner -nostdin -loglevel error -xerror \
  -i "$probe_output" -map 0:v? -map 0:a? -f null -

# The command string is intentionally evaluated inside the Flatpak sandbox.
# shellcheck disable=SC2016
log_file="$(flatpak_cmd sh -c \
  'find "${XDG_STATE_HOME:-$HOME/.local/state}/media-studio/jobs" -maxdepth 1 -type f -name "$1.log" -print -quit' \
  sh "$job_id")"
test -n "$log_file"
# shellcheck disable=SC2016
flatpak_cmd sh -c 'cat "$1"' sh "$log_file" > "$proof_dir/job.log"
grep -Fx 'RESULT=verified' "$proof_dir/job.log"

cp "$output_file" "$proof_dir/output.mp4"
sha256sum "$proof_dir/output.mp4" | sed 's#  .*#  output.mp4#' > "$proof_dir/output.sha256"
printf 'schema_version=1\napp_id=%s\njob_id=%s\nresult=verified\nduration_seconds=%s\noutput=%s\n' \
  "$app_id" "$job_id" "$duration" "$output_file" > "$proof_dir/receipt.txt"
printf 'Flatpak conversion verified: %s (%ss)\n' "$app_id" "$duration"
