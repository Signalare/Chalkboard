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
  (`/api/v1/cec/power`, `/api/v1/cec/status`), and web UI control panel.

[Unreleased]: https://github.com/Signalare/Chalkboard/commits/main
