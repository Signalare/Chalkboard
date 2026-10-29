# tools

Developer scripts and test tooling.

| Script | Purpose |
|---|---|
| `check-spdx.sh` | Fails if a source file lacks the SPDX header (LIC-001). |
| `pre-commit.sh` | Git hook installed by `just install-hooks`; runs `just check`. |
| `check-pr-title.sh`, `check-changelog.sh` | Pull request rules, run by CI. |
| `buildenv/` | Docker build environment for the OS image (`just os-build`). |
| `spike/spike.py` | Boot-slot spike harness (#3): Shelly power control, timestamped serial console, boot-time and power-cut runs. |

Planned: the boot-device write tracer (TEST-009) and the hardware-in-the-loop
harness (TEST-001).
