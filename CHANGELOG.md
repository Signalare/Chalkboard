# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Draft specification (`SPEC.md` v0.1.0) for Chalkboard: offline-first,
  peer-to-peer menu board signage for 64-bit Raspberry Pi.
- README, contribution guidelines, security policy, Code of Conduct, and
  GitHub issue and pull request templates.
- Dual licensing under MIT OR Apache-2.0.
- Template format v1 (`schema/template.v1.json`) with four starter templates,
  and the `cb-template validate` tool, which reports every problem with its
  exact location and suggests corrections for typos.
- Automatic Raspberry Pi board and hardware capability detection at startup
  (model, support tier, SoC family, RAM, HDMI outputs, Wi-Fi bands, Ethernet,
  RTC, and boot device type), with clear rejection of unsupported boards.
- `chalkboard-render` C11 renderer core and DRM/KMS backend with atomic
  snapshot loading (`current`/`previous` fallback), pre-rendered image cache,
  1080p60/720p60 surface pipeline, `0`/`90`/`180`/`270` degree rotation,
  repeater auto-pagination, playlist and daypart schedule cycling, `cut`/`fade`/
  `slide` transitions, live sold-out and announcement updates, screen identify
  overlay, first-boot setup and neutral starting screens, and shipped OFL font
  stack with tabular numerals and automatic glyph fallback.
- `chalkboard-agent` daemon skeleton with SQLite in WAL mode
  (`synchronous=NORMAL`, group commit ≤ 1 s, periodic checkpointing), record
  store for 0.1 entities (categories, items, variants, templates, pages,
  screens), database corruption auto-quarantine and backup recovery,
  content-addressed atomic binary snapshot builder with render cache image asset
  support, Unix domain socket renderer IPC protocol (`load_snapshot`,
  `apply_live_state`, `identify`, `screenshot`, `status`), and process
  supervision restarting crashed child services within 1 s.
- Single-node web application (`web/`) built with Svelte 5 and TypeScript
  under a lightweight uncompressed bundle budget (≤ 300 kB), providing
  a fast sold-out toggle start page with accessible switch controls, a
  complete menu editor for categories and items, a template library browser
  covering `classic-list`, `two-column`, `grid-images`, and `highlight`,
  screen configuration (rotation: 0/90/180/270, dimensions, sold-out display
  mode) and playlist sequence manager, and an embedded agent HTTP/1.1 API
  (`/api/v1`) with instant snapshot publish capability.
- In-memory mock device test harness for HDMI-CEC (`CEC-001`–`CEC-006`) with
  abstract adapter trait (`CecAdapter`), pure software simulation
  (`MockCecAdapter`) tracking simulated TV power state, active source and OSD
  name with error/NACK injection, developer mode (`--mock-hardware`), Linux
  kernel character device driver (`/dev/cec0`, `/dev/cec1`), REST API endpoints
  (`/api/v1/cec/*`), and minimalist status badge with control dropdown in screen
  cards.
- Universal aarch64 Buildroot external tree (`os/`) and defconfig
  (`chalkboard_defconfig`) producing dual-slot A/B MBR images (`chalkboard.img`)
  with `autoboot.txt` tryboot switching, read-only squashfs root filesystems,
  kernel cmdline `dm-mod.create` dm-verity Merkle tree integrity verification,
  `tools/mkverity.py`, and `tools/check-os-image.py` validator (`OS-001`–`OS-004`,
  `OS-010`, `BOOT-004`, `BOOT-007`).
- F2FS `/data` partition first-boot growth script (`cb-init-data`), resizing
  partition to device capacity minus 10% spare area (capped at 32 GB) followed
  by offline `resize.f2fs`, idempotent growth checking, fast immediate mounting
  with required flags (`noatime`, `fsync_mode=posix`, `background_gc=on`,
  `discard`), conditional `fsck.f2fs` on error flags, automatic reformatting
  fallback on unrepairable corruption, and automated test suite
  (`tools/test-f2fs-data.py`) (`STO-008`, `STO-009`, `STO-011`, `BOOT-001`).
- RAM-only logging (`syslogd -C128` circular buffer, tmpfs `/var/log`),
  pstore/ramoops panic crash harvester (`cb-crash-collect`) saving to
  `/data/crashes/` with strict 20-report retention, compressed RAM swap
  (`cb-zram`, lz4, 128 MB on `/dev/zram0`, prohibiting swap on boot media),
  and OOM priority protection (`cb-start-render` protecting renderer with
  `oom_score_adj -900` while agent is sacrificed first) (`OS-008`, `OS-009`,
  `STO-010`).
- Write tracer tool (`tools/write-tracer`, `tools/write_tracer.py`) attributing
  boot-device writes to process and path, enforcing the STO-010 persistent
  storage allowlist, reporting rogue writers with process name and target path,
  and testing daily write budgets against idle (≤ 1 MB/day) and typical use
  (50 toggles, 1 publication ≤ 50 MB/day) (`TEST-009`, `STO-010`, `STO-013`).
  (`/api/v1/cec/power`, `/api/v1/cec/status`), and web UI control panel.
- Physical hardware boot verification and automated HIL test suite
  (`tools/test-universal-image-boot.py`) for the universal Buildroot image on
  Raspberry Pi 4 Model B (rev `c03111`), validating cold boot across SD and USB
  storage (`PARTUUID`), read-only rootfs mount under `dm-verity` (`/dev/dm-0`),
  hardware watchdog (`bcm2835_wdt`), DRM/KMS `vc4-drm` display initialization,
  one-shot `tryboot` slot switching (Slot A/B), and runtime dm-verity corruption
  detection triggering immediate fail-safe reboot and fallback to the alternate
  slot (`OS-001`–`OS-004`, `OS-007`, `OS-010`, `BOOT-004`, `BOOT-007`,
  `TEST-011`).
- Physical hardware verification of F2FS `/data` partition expansion, 32 GB
  capacity capping, 10% wear-levelling reserve, and unrepairable corruption
  recovery on target hardware (Raspberry Pi 4 Model B with 124 GB USB storage,
  `/dev/sda7`), adding native C partition growth tool (`cb-growpart`), POSIX
  clean shell scripts, and HIL serial test automation (`STO-008`, `STO-009`,
  `STO-011`, `BOOT-001`, `TEST-003`).
- Physical hardware verification of ramoops/pstore crash capture on panic,
  automatic reboot, retention pruning, compressed zram swap, and process OOM
  prioritization on Raspberry Pi 4 Model B (`OS-006`, `OS-008`, `OS-009`,
  `STO-010`, `TEST-011`). Configured `dtoverlay=ramoops-pi4` in `config.txt` and
  `panic=5` in kernel cmdline, enabled `CONFIG_FEATURE_SWAPON_PRI=y`, verified
  pstore panic harvesting and strict 20-report retention pruning in
  `cb-crash-collect`, validated 128 MB zram swap on `/dev/zram0`, verified
  renderer protection (`oom_score_adj -900`) and agent supervision (`oom_score_adj 0`
  with < 1s restart), and added live `--serial-port` HIL verification to
  `tools/test-ram-logging-oom.py`.
- Verified board and hardware capability detection on physical hardware (Raspberry
  Pi 4 Model B Rev 1.1, `c03111`), validating live kernel device-tree nodes and sysfs
  paths against SPEC §4 requirements (`HW-001`, `HW-009`, `TEST-011`). Captured live
  hardware sysfs/procfs test fixture into `chalkboard-board` test suite, added
  `--detect-board` JSON capability CLI to `chalkboard-agent`, and implemented
  `tools/test-board-detection.py` covering both offline fixtures and live serial HIL
  interrogation.

- Physical hardware verification of DRM/KMS renderer boot budget, 30+ fps
  transitions across rotations, and process independence on Raspberry Pi 4 Model B
  (`RND-001`, `RND-005`, `RND-008`, `RND-009`, `BOOT-001`, `BOOT-002`, `BOOT-006`,
  `TEST-001`, `TEST-011`). Added multi-card scanning (`/dev/dri/card*`) and
  `--benchmark` mode to `chalkboard-render`, verified all 12 transition combinations
  (`cut` at 330+ fps, `fade` at ~60 fps, `slide` at 200+ fps across 0°, 90°, 180°,
  and 270° rotations) exceed the 30 fps budget, verified cold first-frame render
  in 12.3 ms (budget ≤ 1000 ms), validated that killing `chalkboard-agent` leaves
  `chalkboard-render` running uninterrupted while supervisor restarts the agent,
  added Buildroot packaging (`os/package/chalkboard-render`), and added automated
  offline and live `--serial-port` HIL verification to `tools/test-renderer-hil.py`.
- Physical hardware verification of 24-hour storage write budgets, partition
  immutability, and STO-010 allowlist enforcement on Raspberry Pi 4 Model B with
  124 GB USB block storage (`TEST-009`, `STO-010`, `STO-013`, `TEST-011`). Verified
  zero routine sectors written to `/boot` (`sda1`, `sda2`) and rootfs (`sda5`, `sda6`)
  over extended physical runs; verified 100% of persistent files on `/data` conform
  strictly to the STO-010 allowlist; measured idle writes at 0 bytes/sec (projected
  0.00 MB/day <= 1.0 MB budget); measured typical workload writes (50 atomic item
  toggles + 1 publication) at 0.21 MB (well under the 50.0 MB/day budget); and
  extended `tools/test-write-budget.py` with live `--serial-port` HIL verification.
- Decision and specification updates for the A/B boot-slot mechanism based on
  physical hardware spike results on Raspberry Pi 4 Model B (`UPD-004`, `OS-003`,
  `OS-005`, `OS-011`, `BOOT-001`). Adopted Mechanism B (`autoboot.txt` selecting
  independent FAT32 boot partitions `boot-a` and `boot-b` with `tryboot_a_b=1`),
  providing complete filesystem isolation during OTA updates without mutating
  the running slot's boot partition, single-sector raw atomic commit verified
  across 200 random power-cut torture cycles, cold boot stage breakdown
  (10.28 s unoptimized baseline), and documented optimization roadmap to meet
  the ≤ 5.0 s BOOT-001 budget in production.
- Automated hardware-in-the-loop (HIL) boot-time measurement rig
  (`tools/hil/measure-boot-time.py`) and rig documentation (`tools/hil/README.md`)
  for automated unattended cold-boot execution and performance budget gating in
  CI (`TEST-001`, `BOOT-001`, `TEST-003`). Supports relay power switching (Shelly
  HTTP RPC, USB relay, soft-cycle), microsecond log stage timestamp parsing
  (bootloader, firmware, kernel, and userspace), unattended N-cycle statistical
  analysis (`p50`, `p95`, min, max, stddev), machine-readable JSON and CSV
  reporting, and full offline/unit test suite for deterministic CI execution.
- Physical hardware verification of HDMI-CEC display control across physical TV
  hardware (`CEC-001`–`CEC-006`, `TEST-007`). Added `CONFIG_I2C=y` and
  `CONFIG_I2C_BCM2835=y` to `os/board/chalkboard/linux.fragment` for built-in DDC
  EDID negotiation, configured `dtoverlay=vc4-kms-v3d,noaudio` and dedicated
  `[pi3]` firmware profiles in `config.txt`, implemented native ioctl execution
  (`CEC_TRANSMIT`, `CEC_RECEIVE`, `CEC_ADAP_G_PHYS_ADDR`) in `chalkboard-agent`
  `LinuxCecAdapter`, implemented HIL test tool (`tools/hil/cec-test.c`), verified
  OSD announcement (`Chalkboard`), power toggling (`Image View On`, `Standby`),
  input switching (`Active Source`), and power status polling against physical LG
  TV (`LG TV SSCR2`), and published the initial CEC compatibility matrix
  (`docs/cec-matrix.md`).

[Unreleased]: https://github.com/Signalare/Chalkboard/commits/main
