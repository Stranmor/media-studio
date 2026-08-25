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

cargo build --release

proof_dir="$(mktemp -d)"
trap 'rm -rf "$proof_dir"' EXIT
mkdir -p "$proof_dir/input" "$proof_dir/output" "$proof_dir/home" "$proof_dir/state"
export HOME="$proof_dir/home"
export XDG_CONFIG_HOME="$HOME/.config"
export XDG_STATE_HOME="$proof_dir/state"

ffmpeg -hide_banner -loglevel error \
  -f lavfi -i testsrc2=size=640x360:rate=24:duration=2 \
  -f lavfi -i sine=frequency=1000:sample_rate=48000:duration=2 \
  -shortest -c:v libx264 -c:a aac "$proof_dir/input/fixture.mkv"

target/release/media-studio run \
  --job-id "gpu-${backend}" \
  --profile "$profile" \
  --hardware-fallback=false \
  --output-dir "$proof_dir/output" \
  -- "$proof_dir/input/fixture.mkv"

output_file="$(find "$proof_dir/output" -maxdepth 1 -type f -name '*.mp4' -print -quit)"
test -n "$output_file"
test -s "$output_file"
ffprobe -v error -show_entries format=duration -of default=noprint_wrappers=1 "$output_file"
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
