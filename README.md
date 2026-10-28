# Chalkboard

Fast-booting, offline-first menu board signage for 64-bit Raspberry Pi.
Template-driven LVGL screens that sync peer-to-peer and control the TV via
HDMI-CEC.

> **Status:** design phase. The [specification](SPEC.md) is a draft (v0.1.0);
> no code has been written yet.

## Why Chalkboard

- **Boots in ≤ 5 s** — content is on screen before Wi-Fi is even up.
- **Offline-first** — every screen keeps showing the right menu, including
  dayparts and schedules, with no network at all.
- **No server, no cloud** — screens form a leaderless cluster; every screen
  holds the full configuration and serves the web app.
- **Self-healing** — a failed or replaced screen restores its configuration
  from its peers. Power can be cut at any moment.
- **Templates, not slides** — pages are generated from categories, items and
  prices, and paginate automatically when the menu grows.
- **TV control** — power schedules, input switching and a watchdog that undoes
  "someone pressed the remote", all over HDMI-CEC.

## How it works

Each TV is driven by a Raspberry Pi — anything from a Zero 2 W to a Pi 5, and
boards with two HDMI ports can drive two TVs. Every Pi runs the same read-only
Buildroot image with two processes:

| Process | Language | Role |
|---|---|---|
| `chalkboard-render` | C + LVGL | Draws the current menu from a local snapshot. Starts first and never touches the network. |
| `chalkboard-agent` | Rust | Discovery, peer-to-peer sync, web app and API, HDMI-CEC, time, provisioning and updates. |

Screens find each other on the LAN and replicate every change. If the venue
Wi-Fi goes down, they keep rendering on their own, and one screen raises a
fallback hotspot so the cluster can still sync and be edited from a phone.

Manage everything at `https://chalkboard.local` from any phone or computer on
the network.

## Hardware

Any 64-bit Raspberry Pi, mixed freely in one cluster:

| Support | Boards |
|---|---|
| Tier 1 — tested every release | Pi Zero 2 W, Pi 4B, Pi 5 |
| Tier 2 — smoke-tested | Pi 3B, 3B+, 3A+, Pi 400, Pi 500, CM4, CM5 |
| Tier 3 — community | CM3, CM3+, CM4S |

Per screen you also need:

- An HDMI cable or adapter that carries CEC
- The official power supply for your board (not the TV's USB port)
- A high-endurance microSD card ≥ 8 GB, or eMMC / USB SSD / NVMe where the
  board supports it
- Optional: an RTC battery (Pi 5) or DS3231 module, Ethernet, an active
  cooler for Pi 4/5

## Documentation

- [SPEC.md](SPEC.md) — full system specification with requirement IDs and roadmap
- [CHANGELOG.md](CHANGELOG.md) — notable changes, per [Keep a Changelog](https://keepachangelog.com/en/1.0.0/)
- [CONTRIBUTING.md](CONTRIBUTING.md) — how to propose changes
- [SECURITY.md](SECURITY.md) — how to report vulnerabilities privately
- [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) — community standards

This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
