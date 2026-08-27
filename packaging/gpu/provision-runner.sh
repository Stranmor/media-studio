#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
Usage: provision-runner.sh --backend vaapi|nvenc [options]

Options:
  --repo OWNER/REPO       GitHub repository (default: Stranmor/media-studio)
  --runner-dir PATH       Dedicated runner directory
  --runner-name NAME      GitHub runner name
  --token-stdin           Read the one-time registration token from stdin (preferred)
  --replace               Replace an existing runner configuration

NixOS:
  Set MEDIA_STUDIO_RUNNER_NIX_LD_LIBRARY_PATH to colon-separated directories
  containing the runner's native dependencies when the host uses stub-ld.
EOF
}

backend=""
repo="Stranmor/media-studio"
runner_dir=""
runner_name=""
read_token=0
replace=0

while (($# > 0)); do
  case "$1" in
    --backend)
      backend=${2:?missing value for --backend}
      shift 2
      ;;
    --repo)
      repo=${2:?missing value for --repo}
      shift 2
      ;;
    --runner-dir)
      runner_dir=${2:?missing value for --runner-dir}
      shift 2
      ;;
    --runner-name)
      runner_name=${2:?missing value for --runner-name}
      shift 2
      ;;
    --token-stdin)
      read_token=1
      shift
      ;;
    --replace)
      replace=1
      shift
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

case "$backend" in
  vaapi|nvenc) ;;
  *)
    printf 'backend must be vaapi or nvenc\n' >&2
    exit 2
    ;;
esac
if [[ ! "$repo" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]]; then
  printf 'invalid GitHub repository: %s\n' "$repo" >&2
  exit 2
fi

runner_dir=${runner_dir:-"$HOME/.local/share/media-studio/actions-runner-$backend"}
runner_name=${runner_name:-"media-studio-$backend-$(hostname -s)"}
runner_user="$(id -un)"
unit_name="media-studio-actions-runner-$backend.service"
if [[ "$runner_user" == root || "$(id -u)" -eq 0 ]]; then
  printf 'run provisioning as the target non-root user so its user service and linger state are owned correctly\n' >&2
  exit 2
fi
[[ "$runner_dir" == /* ]] || {
  printf 'runner directory must be absolute: %s\n' "$runner_dir" >&2
  exit 2
}
[[ "$runner_dir" != *[[:space:]]* ]] || {
  printf 'runner directory must not contain whitespace: %s\n' "$runner_dir" >&2
  exit 2
}
[[ "${runner_dir##*/}" == actions-runner-* ]] || {
  printf 'runner directory must end with actions-runner-<backend>: %s\n' "$runner_dir" >&2
  exit 2
}
[[ "$runner_name" =~ ^[A-Za-z0-9._-]{1,128}$ ]] || {
  printf 'runner name contains unsupported characters: %s\n' "$runner_name" >&2
  exit 2
}
case "$runner_dir" in
  /|"$HOME"|"$HOME/"|.)
    printf 'refusing an unsafe runner directory: %s\n' "$runner_dir" >&2
    exit 2
    ;;
esac

validate_path_components() {
  local path="$1"
  local label="$2"
  local current="/"
  local components component
  IFS='/' read -r -a components <<< "${path#/}"
  for component in "${components[@]}"; do
    [[ -z "$component" ]] && continue
    if [[ "$component" == "." || "$component" == ".." ]]; then
      printf 'refusing traversal in %s: %s\n' "$label" "$path" >&2
      exit 2
    fi
    current="${current%/}/$component"
    if [[ -L "$current" ]]; then
      printf 'refusing a symlinked path component in %s: %s\n' "$label" "$current" >&2
      exit 2
    fi
  done
}

validate_path_components "$runner_dir" "runner directory"
if [[ -L "$runner_dir" ]]; then
  printf 'refusing a symlinked runner directory: %s\n' "$runner_dir" >&2
  exit 2
fi
runner_parent="${runner_dir%/*}"
[[ -n "$runner_parent" ]] || runner_parent=/
provision_lock_path="${runner_dir}.provision.lock"
validate_path_components "$provision_lock_path" "provisioning lock"
if [[ -L "$provision_lock_path" ]]; then
  printf 'refusing a symlinked provisioning lock path: %s\n' "$provision_lock_path" >&2
  exit 2
fi

validate_runner_symlinks() {
  local root="$1"
  local link target
  [[ -d "$root" ]] || return 0
  root="$(readlink -f -- "$root")"
  while IFS= read -r -d '' link; do
    target="$(readlink -f -- "$link" 2>/dev/null || true)"
    if [[ -z "$target" || "$target" != "$root"/* ]]; then
      printf 'refusing a runner symlink that escapes its directory: %s\n' "$link" >&2
      exit 2
    fi
  done < <(find "$root" -type l -print0)
}

validate_unit_field() {
  local value="$1"
  local label="$2"
  # Values below are written directly into systemd unit fields. Restricting
  # them to parser-safe path/PATH characters is fail-closed and avoids turning
  # spaces, quotes, specifiers, or control characters into unit syntax.
  [[ "$value" =~ ^[-A-Za-z0-9._/+=:]+$ ]] || {
    printf 'unsafe systemd unit value for %s: %s\n' "$label" "$value" >&2
    exit 2
  }
}

for tool in bash curl tar sha256sum ffmpeg ffprobe systemd-run systemctl cargo rustc cc flock ps expect; do
  command -v "$tool" >/dev/null || {
    printf 'missing required tool: %s\n' "$tool" >&2
    exit 1
  }
done
command -v loginctl >/dev/null || {
  printf 'missing required tool: loginctl (systemd-logind is required for durable user lingering)\n' >&2
  exit 1
}

linger_state="$(loginctl show-user "$runner_user" -p Linger --value 2>/dev/null || true)"
if [[ "$linger_state" != yes ]]; then
  if ! loginctl enable-linger "$runner_user" >/dev/null 2>&1; then
    if ! command -v sudo >/dev/null 2>&1 || ! sudo -n loginctl enable-linger "$runner_user" >/dev/null 2>&1; then
      printf 'user lingering is disabled for %s; enable it with loginctl before provisioning the durable runner service\n' "$runner_user" >&2
      exit 1
    fi
  fi
  linger_state="$(loginctl show-user "$runner_user" -p Linger --value 2>/dev/null || true)"
fi
if [[ "$linger_state" != yes ]]; then
  printf 'loginctl did not confirm lingering for %s; refusing a non-durable runner service\n' "$runner_user" >&2
  exit 1
fi

encoder_listing="$(ffmpeg -hide_banner -encoders 2>/dev/null)"
grep -Eq '^[[:space:]]*[A-Z.]{6}[[:space:]]+libx264([[:space:]]|$)' <<<"$encoder_listing"
case "$backend" in
  vaapi)
    vaapi_device="${MEDIA_STUDIO_VAAPI_DEVICE:-/dev/dri/renderD128}"
    test -c "$vaapi_device"
    grep -Eq '^[[:space:]]*[A-Z.]{6}[[:space:]]+h264_vaapi([[:space:]]|$)' <<<"$encoder_listing"
    ;;
  nvenc)
    command -v nvidia-smi >/dev/null
    test -e /dev/nvidia0
    nvidia-smi -L
    grep -Eq '^[[:space:]]*[A-Z.]{6}[[:space:]]+h264_nvenc([[:space:]]|$)' <<<"$encoder_listing"
    ;;
esac

if (( read_token )); then
  IFS= read -r registration_token || true
else
  registration_token="${RUNNER_REGISTRATION_TOKEN:-}"
fi
unset RUNNER_REGISTRATION_TOKEN
if [[ -z "${registration_token:-}" ]]; then
  printf 'a one-time registration token is required; prefer --token-stdin (RUNNER_REGISTRATION_TOKEN is a compatibility fallback)\n' >&2
  exit 2
fi

mkdir -p -- "$runner_parent"
validate_path_components "$runner_dir" "runner directory"
validate_path_components "$provision_lock_path" "provisioning lock"
exec {provision_lock_fd}>"$provision_lock_path"
chmod 600 "$provision_lock_path"
if ! flock -n "$provision_lock_fd"; then
  printf 'another provisioning operation already owns %s\n' "$provision_lock_path" >&2
  exit 1
fi

service_state="$(systemctl --user is-active "$unit_name" 2>/dev/null || true)"
case "$service_state" in
  active|activating|deactivating)
    printf 'runner service %s is %s; stop it before using --replace\n' "$unit_name" "$service_state" >&2
    exit 1
    ;;
esac

active_runner_pids="$(ps -eo pid=,args= | awk -v dir="$runner_dir" -v self="$$" '
  $1 != self && index($0, dir "/") &&
    ($0 ~ /\/run\.sh([[:space:]]|$)/ || $0 ~ /\/Runner\.Listener([[:space:]]|$)/) {
      printf "%s ", $1
    }
')"
if [[ -n "$active_runner_pids" ]]; then
  printf 'runner processes are still using %s: %s; refusing to mutate the directory\n' \
    "$runner_dir" "$active_runner_pids" >&2
  exit 1
fi

mkdir -p "$runner_dir"
validate_runner_symlinks "$runner_dir"
if [[ -e "$runner_dir/.runner" && "$replace" -ne 1 ]]; then
  printf 'runner is already configured at %s; pass --replace only for an intentional replacement\n' "$runner_dir" >&2
  exit 1
fi

release_json="$(curl -fsSL https://api.github.com/repos/actions/runner/releases/latest)"
asset_record="$(awk -F'"' '
  /"name": "actions-runner-linux-x64-[0-9].*\.tar\.gz"/ { matched=1 }
  matched && /"digest": "sha256:/ { digest=$4 }
  matched && /"browser_download_url":/ { url=$4 }
  matched && digest != "" && url != "" { print url "\t" digest; exit }
' <<<"$release_json")"
resolved_asset="${asset_record%%$'\t'*}"
expected_digest="${asset_record#*$'\t'}"
if [[ -z "$resolved_asset" || -z "$expected_digest" ]]; then
  printf 'GitHub did not publish a current x86_64 Actions runner asset\n' >&2
  exit 1
fi
archive_name="${resolved_asset##*/}"
if [[ ! "$archive_name" =~ ^actions-runner-linux-x64-[0-9.]+\.tar\.gz$ ]]; then
  printf 'unexpected Actions runner asset name: %s\n' "$archive_name" >&2
  exit 1
fi
archive_path="$runner_dir/$archive_name"
if [[ -L "$archive_path" ]]; then
  printf 'refusing a symlinked runner archive path: %s\n' "$archive_path" >&2
  exit 2
fi
curl -fsSL "$resolved_asset" -o "$archive_path"
test -s "$archive_path"
actual_digest="$(sha256sum "$archive_path" | awk '{print $1}')"
expected_digest="${expected_digest#sha256:}"
if [[ "$actual_digest" != "$expected_digest" ]]; then
  printf 'Actions runner archive digest mismatch\n' >&2
  exit 1
fi
tar -tzf "$archive_path" >/dev/null
tar -xzf "$archive_path" -C "$runner_dir"
validate_runner_symlinks "$runner_dir"

runner_version="${archive_name#actions-runner-linux-x64-}"
runner_version="${runner_version%.tar.gz}"
runner_bash="$(command -v bash)"

# NixOS may not expose /bin/bash even though bash is available in the system
# profile. The upstream launcher executes helper scripts directly, so rewrite
# only its exact bash shebangs in this dedicated runner tree. This leaves the
# host-wide filesystem untouched and survives run.sh's template copy.
if [[ "$runner_bash" != /bin/bash ]]; then
  while IFS= read -r -d '' runner_script; do
    if [[ "$(head -n1 "$runner_script")" == '#!/bin/bash' ]]; then
      sed -i "1c\#!$runner_bash" "$runner_script"
    fi
  done < <(find "$runner_dir" -type f \( -name '*.sh' -o -name 'run-helper.sh.template' \) -print0)
fi

# NixOS intentionally exposes a stub dynamic loader. The upstream runner is a
# generic Linux binary, so patch its ELF entry points to nix-ld when needed.
# This keeps the host-wide loader untouched and scopes compatibility to this
# dedicated runner. The caller supplies the native library directories because
# those are host-specific and must not be guessed from an arbitrary store.
nix_ld_mode=0
nix_ld_bin=""
nix_ld_loader=""
nix_ld_library_path="${MEDIA_STUDIO_RUNNER_NIX_LD_LIBRARY_PATH:-}"
host_loader="$(readlink -f /lib64/ld-linux-x86-64.so.2 2>/dev/null || true)"
if [[ "$host_loader" == *stub-ld* ]]; then
  nix_ld_mode=1
  if [[ -z "$nix_ld_library_path" && -d /run/current-system/sw/share/nix-ld/lib ]]; then
    nix_ld_library_path=/run/current-system/sw/share/nix-ld/lib
  fi
  if [[ -z "$nix_ld_library_path" ]]; then
    printf 'NixOS stub-ld detected; set MEDIA_STUDIO_RUNNER_NIX_LD_LIBRARY_PATH first\n' >&2
    exit 1
  fi
  IFS=: read -r -a nix_ld_dirs <<<"$nix_ld_library_path"
  for nix_ld_dir in "${nix_ld_dirs[@]}"; do
    [[ -n "$nix_ld_dir" && "$nix_ld_dir" != *[[:space:]]* && -d "$nix_ld_dir" ]] || {
      printf 'invalid NixOS runner library directory: %s\n' "$nix_ld_dir" >&2
      exit 2
    }
  done
  if command -v nix-ld >/dev/null 2>&1; then
    nix_ld_bin="$(command -v nix-ld)"
  else
    for candidate in /nix/store/*-nix-ld-*/libexec/nix-ld; do
      if [[ -x "$candidate" ]]; then
        nix_ld_bin="$candidate"
      fi
    done
  fi
  [[ -n "$nix_ld_bin" ]] || {
    printf 'NixOS stub-ld detected but nix-ld is unavailable\n' >&2
    exit 1
  }
  patchelf_bin="$(command -v patchelf 2>/dev/null || true)"
  if [[ -z "$patchelf_bin" ]]; then
    for candidate in /nix/store/*-patchelf-*/bin/patchelf; do
      if [[ -x "$candidate" ]]; then
        patchelf_bin="$candidate"
      fi
    done
  fi
  [[ -x "$patchelf_bin" ]] || {
    printf 'NixOS stub-ld detected but patchelf is unavailable\n' >&2
    exit 1
  }
  nix_ld_loader="$(ldd "$runner_dir/bin/libcoreclr.so" 2>/dev/null \
    | sed -n 's#.*=> \(/nix/store/.*/ld-linux-x86-64.so.2\).*#\1#p; s#^[[:space:]]*\(/nix/store/.*/ld-linux-x86-64.so.2\).*#\1#p' \
    | head -n1)"
  [[ -x "$nix_ld_loader" ]] || {
    printf 'could not resolve the host glibc loader for nix-ld\n' >&2
    exit 1
  }
  runner_rpath="$runner_dir/bin:$nix_ld_library_path"
  while IFS= read -r -d '' runner_file; do
    if "$patchelf_bin" --print-interpreter "$runner_file" >/dev/null 2>&1; then
      "$patchelf_bin" --set-interpreter "$nix_ld_bin" --set-rpath "$runner_rpath" "$runner_file"
    fi
  done < <(find "$runner_dir" -type f -perm -u+x -print0)
  while IFS= read -r -d '' runner_library; do
    "$patchelf_bin" --set-rpath "$runner_rpath" "$runner_library" 2>/dev/null || true
  done < <(find "$runner_dir" -type f \( -name '*.so' -o -name '*.so.*' \) -print0)
fi

config_env=()
if (( nix_ld_mode )); then
  config_env=(
    "NIX_LD=$nix_ld_loader"
    "NIX_LD_LIBRARY_PATH=$nix_ld_library_path"
    "LD_LIBRARY_PATH=$nix_ld_library_path"
    "DOTNET_SYSTEM_GLOBALIZATION_INVARIANT=1"
  )
fi
if (( replace )) && [[ -e "$runner_dir/.runner" ]]; then
  # Remove only the local registration state. The subsequent --replace call
  # lets GitHub replace an offline runner with the same trusted name without
  # requiring a second, separately scoped removal token.
  (cd "$runner_dir" && env "${config_env[@]}" "$runner_bash" ./config.sh remove --local)
fi

# The upstream runner reads registration tokens with Console.ReadKey when the
# token option is omitted, so a redirected pipe is rejected. Feed the token
# through a private inherited file descriptor into an isolated PTY instead of
# placing it in Runner.Listener argv or the environment.
configure_with_token_pty() {
  local runner_root="$1"
  local runner_shell="$2"
  local runner_url="$3"
  local configured_name="$4"
  local configured_labels="$5"
  local handshake_path="$runner_root/runner-provisioning.txt"
  local handshake_fd
  local config_status
  if [[ -L "$handshake_path" ]]; then
    printf 'refusing a symlinked provisioning transcript: %s\n' "$handshake_path" >&2
    exit 2
  fi
  : > "$handshake_path"
  chmod 600 "$handshake_path"
  exec {handshake_fd}>"$handshake_path"
  exec 3<<<"$registration_token"
  unset registration_token
  if (
    cd "$runner_root"
    env "${config_env[@]}" expect - "$runner_shell" "$runner_url" "$configured_name" "$configured_labels" "$handshake_fd" <<'EXPECT_SCRIPT'
set timeout 180
log_user 0
set runner_shell [lindex $argv 0]
set runner_url [lindex $argv 1]
set runner_name [lindex $argv 2]
set runner_labels [lindex $argv 3]
set handshake_fd [lindex $argv 4]
set handshake_channel [open "/proc/self/fd/$handshake_fd" w]
proc mark {channel key value} {
    puts $channel "$key=$value"
    flush $channel
}
mark $handshake_channel pty_spawned yes
set token_channel [open "/proc/self/fd/3" r]
set token [string trim [read $token_channel]]
close $token_channel
spawn -noecho $runner_shell ./config.sh --url $runner_url --name $runner_name --labels $runner_labels --work _work --replace
set prompt_seen 0
set token_sent 0
expect {
    -re {(?i)(what is your runner (register|registration) token\?|enter (the )?runner (register|registration) token|runner (register|registration) token|registration token)} {
        set prompt_seen 1
        mark $handshake_channel token_prompt_seen yes
        send -- "$token\r"
        set token_sent 1
        mark $handshake_channel token_sent yes
        exp_continue
    }
    -re {(?i)(invalid|unauthori[sz]ed|registration failed|configuration failed|error)} {
        mark $handshake_channel child_error yes
        catch { close }
        catch { wait }
        exit 1
    }
    eof { mark $handshake_channel child_eof yes }
    timeout {
        mark $handshake_channel timeout yes
        catch { close }
        catch { wait }
        exit 124
    }
}
set result [wait]
set exit_code [lindex $result 3]
mark $handshake_channel child_exit $exit_code
if {$exit_code eq ""} { exit 1 }
if {$prompt_seen == 0 || $token_sent == 0 || $exit_code != 0} {
    mark $handshake_channel registration_confirmed no
    exit 125
}
mark $handshake_channel registration_confirmed yes
exit $exit_code
EXPECT_SCRIPT
  ); then
    config_status=0
  else
    config_status=$?
  fi
  exec 3<&-
  exec {handshake_fd}>&-
  if (( config_status != 0 )); then
    return "$config_status"
  fi
  grep -Fx 'registration_confirmed=yes' "$handshake_path" >/dev/null
  grep -Fx 'token_prompt_seen=yes' "$handshake_path" >/dev/null
  grep -Fx 'token_sent=yes' "$handshake_path" >/dev/null
  grep -Fx 'child_exit=0' "$handshake_path" >/dev/null
  for registration_state in .runner .credentials; do
    [[ ! -L "$runner_root/$registration_state" && -s "$runner_root/$registration_state" ]] || {
      printf 'runner registration did not produce a regular %s file\n' "$registration_state" >&2
      return 125
    }
  done
  return "$config_status"
}

configure_with_token_pty "$runner_dir" "$runner_bash" "https://github.com/$repo" \
  "$runner_name" "self-hosted,linux,gpu,$backend"
# config.sh snapshots LD_LIBRARY_PATH into .env. Keep that compatibility path
# private to the runner service rather than leaking it into every job process.
if [[ -f "$runner_dir/.env" ]]; then
  sed -i '/^LD_LIBRARY_PATH=/d' "$runner_dir/.env"
fi

# Prefer the real toolchain binaries over workstation wrappers that may enforce
# interactive build admission policies. The service cgroup supplies its own
# CPU and memory ceiling for this trusted hardware job.
cargo_bin="$(command -v cargo)"
rustc_bin="$(command -v rustc)"
cc_bin="$(command -v cc)"
if command -v rustup >/dev/null 2>&1; then
  direct_cargo="$(rustup which cargo 2>/dev/null || true)"
  if [[ -x "$direct_cargo" ]]; then
    cargo_bin="$direct_cargo"
  fi
  direct_rustc="$(rustup which rustc 2>/dev/null || true)"
  if [[ -x "$direct_rustc" ]]; then
    rustc_bin="$direct_rustc"
  fi
fi
runner_path="$(dirname "$cargo_bin"):$(dirname "$rustc_bin"):$PATH"
runner_path="$(dirname "$cc_bin"):$runner_path"
unit_dir="$HOME/.config/systemd/user"
unit_path="$unit_dir/$unit_name"
lock_path="$runner_dir/.runner.lock"
flock_bin="$(command -v flock)"
validate_path_components "$unit_dir" "systemd unit directory"
validate_unit_field "$runner_dir" "runner directory"
validate_unit_field "$runner_bash" "runner shell"
validate_unit_field "$flock_bin" "flock"
validate_unit_field "$lock_path" "runner lock"
validate_unit_field "$runner_path" "PATH"
validate_unit_field "$cc_bin" "C compiler"
validate_unit_field "$unit_dir" "systemd unit directory"
if (( nix_ld_mode )); then
  validate_unit_field "$nix_ld_loader" "NIX_LD loader"
  validate_unit_field "$nix_ld_library_path" "NIX_LD_LIBRARY_PATH"
fi
mkdir -p "$unit_dir"
if [[ -L "$unit_path" ]]; then
  printf 'refusing a symlinked systemd unit path: %s\n' "$unit_path" >&2
  exit 2
fi
if [[ -f "$unit_path" && ! -s "$unit_path" ]]; then
  # Recover from an interrupted write (systemd treats an empty unit as masked).
  rm -f -- "$unit_path"
fi
if [[ -L "$lock_path" ]]; then
  printf 'refusing a symlinked runner lock path: %s\n' "$lock_path" >&2
  exit 2
fi
touch "$lock_path"
chmod 600 "$lock_path"
mkdir -p "$runner_dir/cargo-home"
requires_mount=""
if [[ "$runner_dir" == /mnt/data/* ]]; then
  requires_mount='RequiresMountsFor=/mnt/data'
fi
service_environment=()
if (( nix_ld_mode )); then
  service_environment=(
    "Environment=NIX_LD=$nix_ld_loader"
    "Environment=NIX_LD_LIBRARY_PATH=$nix_ld_library_path"
    'Environment=DOTNET_SYSTEM_GLOBALIZATION_INVARIANT=1'
  )
fi
printf '%s\n' \
  '[Unit]' \
  "Description=Media Studio GitHub Actions $backend runner" \
  'After=network-online.target' \
  "$requires_mount" \
  '' \
  '[Service]' \
  "WorkingDirectory=$runner_dir" \
  "ExecStart=$flock_bin --no-fork --nonblock --exclusive $lock_path $runner_bash $runner_dir/run.sh" \
  "Environment=PATH=$runner_path" \
  "Environment=CARGO_HOME=$runner_dir/cargo-home" \
  "Environment=CC=$cc_bin" \
  "${service_environment[@]}" \
  'Environment=CARGO_BUILD_JOBS=2' \
  'Restart=always' \
  'RestartSec=10' \
  'CPUQuota=200%' \
  'MemoryMax=8G' \
  'TasksMax=4096' \
  'KillMode=process' \
  '' \
  '[Install]' \
  'WantedBy=default.target' > "$unit_path"

systemctl --user daemon-reload
systemctl --user enable --now "$unit_name"
systemctl --user is-active --quiet "$unit_name"

receipt_path="$runner_dir/runner-receipt.txt"
if [[ -L "$receipt_path" ]]; then
  printf 'refusing a symlinked runner receipt path: %s\n' "$receipt_path" >&2
  exit 2
fi
hardware_detail=""
if [[ "$backend" = nvenc ]]; then
  hardware_detail="$(nvidia-smi --query-gpu=name --format=csv,noheader | sed -n '1p')"
fi
handshake_sha256="$(sha256sum -- "$runner_dir/runner-provisioning.txt" | awk '{print $1}')"
printf 'schema_version=1\nrepo=%s\nrunner_name=%s\nbackend=%s\nrunner_version=%s\narchive_sha256=%s\nhost=%s\nkernel=%s\nhardware=%s\nservice=%s\nuser=%s\nlinger=%s\nlabels=self-hosted,linux,gpu,%s\nregistration_handshake=pty\nregistration_confirmed=yes\nprovisioning_transcript_sha256=%s\n' \
  "$repo" "$runner_name" "$backend" "$runner_version" "$actual_digest" "$(hostname)" "$(uname -sr)" "$hardware_detail" "$unit_name" "$runner_user" "$linger_state" "$backend" "$handshake_sha256" > "$receipt_path"
printf 'runner configured and active: %s (%s)\n' "$runner_name" "$backend"
