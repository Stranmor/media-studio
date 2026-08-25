# GPU CI runners

The GPU workflow is manual by design. It runs only on trusted self-hosted Linux runners and never executes untrusted pull-request code on a private machine.

## Runner labels

Each runner must have the following labels:

- `self-hosted`
- `linux`
- `gpu`
- exactly one backend label: `vaapi` or `nvenc`

Both runners must provide Rust/Cargo, `ffmpeg` with the `libx264` encoder, `ffprobe`, `systemd-run`, and a dialog backend (`zenity` or `kdialog`). The smoke preflight rejects a runner missing any required encoder. The VAAPI runner must expose the character device `/dev/dri/renderD128`, grant the runner user access to it, and provide an FFmpeg build with `h264_vaapi`. The NVENC runner must expose a working NVIDIA driver, `nvidia-smi`, the NVIDIA device nodes (`/dev/nvidia0`, `/dev/nvidiactl`, and `/dev/nvidia-uvm` when present), runner-user access to those nodes, and an FFmpeg build with `h264_nvenc`.

## What the workflow proves

`packaging/gpu/hardware-smoke.sh` creates a real video fixture, invokes the selected Media Studio hardware profile with `--hardware-fallback=false`, verifies the committed MP4 with `ffprobe`, and requires a `RESULT=verified` log without `HARDWARE_FALLBACK`.

The matrix is intentionally separate from the normal CI workflow. If no trusted runner is online, the GPU proof is waiting/unverified; normal build and packaging CI remains independent.
