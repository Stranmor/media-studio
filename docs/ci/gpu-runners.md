# GPU CI runners

The GPU workflow is restricted to trusted self-hosted Linux runners. It can be
started manually on `main` and is also called by the release workflow, so a
missing or failing backend blocks publication instead of being silently
ignored.

## Runner labels

Each runner must have these labels:

- `self-hosted`
- `linux`
- `gpu`
- exactly one backend label: `vaapi` or `nvenc`

Both runners provide current Rust/Cargo (`cargo` and `rustc`), a C linker (`cc`), FFmpeg with
`libx264`, FFprobe, and `systemd-run`. The VAAPI runner exposes `/dev/dri/renderD128` and
`h264_vaapi`. The NVENC runner exposes a working NVIDIA driver, `nvidia-smi`,
`/dev/nvidia0`, and `h264_nvenc`. A dialog backend is not required for the
non-interactive hardware smoke.

## Provisioning

`packaging/gpu/provision-runner.sh` downloads the current Actions runner,
registers it with a one-time repository token read from stdin, validates the
selected hardware, and installs a bounded user-systemd service. Generate the
token with an authorized GitHub client and never place it in a file or command
line history:

```bash
gh api -X POST repos/Stranmor/media-studio/actions/runners/registration-token --jq .token \
  | packaging/gpu/provision-runner.sh --backend vaapi --token-stdin
```

Run the same command with `--backend nvenc` on the NVIDIA host. If the system
volume is small, put the runner work tree on a mounted data volume, for example
`--runner-dir /mnt/data/media-studio/actions-runner-nvenc`; the generated user
unit adds `RequiresMountsFor=/mnt/data` for that layout. The script
keeps the runner directory, service name, labels, hardware identity, and
runner version in `runner-receipt.txt`; the registration token is not stored.

## What the workflow proves

`packaging/gpu/hardware-smoke.sh` creates a real video fixture, invokes the
selected Media Studio hardware profile with `--hardware-fallback=false`,
verifies the committed MP4 with FFprobe and full decode, and requires an exact
`HARDWARE_USED=<BACKEND> ENCODER=<ENCODER>` marker, no
`HARDWARE_FALLBACK`, and `RESULT=verified`. Each successful matrix leg uploads
`receipt.txt`, `job.log`, the output checksum, and the proof directory as an
Actions artifact.

The matrix is intentionally separate from normal CI. If no trusted runner is
online, the GPU gate is waiting/unverified and the release cannot publish.
