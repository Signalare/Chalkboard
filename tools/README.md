# tools

Developer scripts and test tooling.

| Script | Purpose |
|---|---|
| `check-spdx.sh` | Fails if a source file lacks the SPDX header (LIC-001). |
| `pre-commit.sh` | Git hook installed by `just install-hooks`; runs `just check`. |

Planned: the boot-device write tracer (TEST-009) and the hardware-in-the-loop
harness (TEST-001).
