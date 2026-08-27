#!/usr/bin/env bash
set -euo pipefail

backend=${1:?backend is required: vaapi or nvenc}
vaapi_device=${2:-}

case "$backend" in
  vaapi)
    test -n "$vaapi_device"
    test -c "$vaapi_device"
    encoder=h264_vaapi
    profile=video_mp4_vaapi
    export MEDIA_STUDIO_VAAPI_DEVICE="$vaapi_device"
    ;;
  nvenc)
    encoder=h264_nvenc
    profile=video_mp4_nvenc
    command -v nvidia-smi >/dev/null
    nvidia-smi -L
    ;;
  *)
    printf 'unsupported backend: %s\n' "$backend" >&2
    exit 2
    ;;
esac

encoder_listing="$(ffmpeg -hide_banner -encoders 2>/dev/null)"
for required_encoder in libx264 "$encoder"; do
  grep -Eq "^[[:space:]]*[A-Z.]{6}[[:space:]]+${required_encoder}([[:space:]]|$)" <<<"$encoder_listing"
done
command -v systemd-run >/dev/null
command -v sha256sum >/dev/null
command -v timeout >/dev/null

cargo build --release

proof_dir="${MEDIA_STUDIO_PROOF_DIR:-}"
owns_proof=0
if [[ -z "$proof_dir" ]]; then
  proof_dir="$(mktemp -d)"
  owns_proof=1
else
  case "$proof_dir" in
    /|"$HOME"|"$HOME/"|.)
      printf 'refusing an unsafe proof directory: %s\n' "$proof_dir" >&2
      exit 2
      ;;
  esac
  mkdir -p "$proof_dir"
  if find "$proof_dir" -mindepth 1 -maxdepth 1 -print -quit | grep -q .; then
    printf 'proof directory must be empty: %s\n' "$proof_dir" >&2
    exit 2
  fi
fi
cleanup() {
  if (( owns_proof )); then
    rm -rf -- "$proof_dir"
  fi
}
trap cleanup EXIT
mkdir -p "$proof_dir/input" "$proof_dir/output" "$proof_dir/home" "$proof_dir/state"
export HOME="$proof_dir/home"
export XDG_CONFIG_HOME="$HOME/.config"
export XDG_STATE_HOME="$proof_dir/state"
# Hardware smoke is deliberately non-interactive. Suppress desktop helpers so
# a self-hosted runner cannot block on notify-send, Zenity, or KDialog.
export MEDIA_STUDIO_HEADLESS=1
unset DISPLAY WAYLAND_DISPLAY

ffmpeg -hide_banner -loglevel error \
  -f lavfi -i testsrc2=size=640x360:rate=24:duration=2 \
  -f lavfi -i sine=frequency=1000:sample_rate=48000:duration=2 \
  -shortest -c:v libx264 -c:a aac "$proof_dir/input/fixture.mkv"

timeout --signal=TERM --kill-after=15s 300s target/release/media-studio run \
  --job-id "gpu-${backend}" \
  --profile "$profile" \
  --hardware-fallback=false \
  --output-dir "$proof_dir/output" \
  -- "$proof_dir/input/fixture.mkv"

output_file="$(find "$proof_dir/output" -maxdepth 1 -type f -name '*.mp4' -print -quit)"
test -n "$output_file"
test -s "$output_file"
duration="$(ffprobe -v error -show_entries format=duration -of default=noprint_wrappers=1:nokey=1 "$output_file")"
awk -v duration="$duration" 'BEGIN { exit !(duration > 0) }'
ffmpeg -hide_banner -nostdin -loglevel error -xerror -i "$output_file" \
  -map 0:v? -map 0:a? -f null -

log_file="$XDG_STATE_HOME/media-studio/jobs/gpu-${backend}.log"
backend_label="$(printf '%s' "$backend" | tr '[:lower:]' '[:upper:]')"
grep -Fx "HARDWARE_USED=${backend_label} ENCODER=${encoder}" "$log_file"
grep -F "$encoder" "$log_file"
if [ "$backend" = vaapi ]; then
  grep -F -- "$vaapi_device" "$log_file"
fi
if grep -Fq 'HARDWARE_FALLBACK=' "$log_file"; then
  printf 'hardware fallback was used unexpectedly\n' >&2
  exit 1
fi
grep -Fx 'RESULT=verified' "$log_file"

ffmpeg_version="$(ffmpeg -version | sed -n '1p')"
gpu_detail=""
if [[ "$backend" = nvenc ]]; then
  gpu_detail="$(nvidia-smi --query-gpu=name --format=csv,noheader | sed -n '1p')"
fi
cp "$output_file" "$proof_dir/output.mp4"
sha256sum "$proof_dir/output.mp4" | sed 's#  .*#  output.mp4#' > "$proof_dir/output.sha256"
cp "$log_file" "$proof_dir/job.log"
printf 'schema_version=1\nbackend=%s\nencoder=%s\nvaapi_device=%s\nhardware_detail=%s\nrunner=%s\nkernel=%s\nffmpeg=%s\nduration_seconds=%s\nresult=verified\n' \
  "$backend" "$encoder" "$vaapi_device" "$gpu_detail" "$(hostname)" "$(uname -sr)" "$ffmpeg_version" "$duration" > "$proof_dir/receipt.txt"
printf 'Hardware conversion verified: backend=%s encoder=%s duration=%ss\n' "$backend" "$encoder" "$duration"
