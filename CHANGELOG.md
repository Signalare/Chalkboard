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

[Unreleased]: https://github.com/Signalare/Chalkboard/commits/main
