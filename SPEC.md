# Chalkboard — Specification

| | |
|---|---|
| **Spec version** | 0.1.0 |
| **Status** | Draft |
| **Date** | 2026-10-28 |

---

## 1. Introduction

### 1.1 Purpose

This document specifies **Chalkboard**, a self-hosted, peer-to-peer digital signage system for menu boards. Screens are driven by 64-bit Raspberry Pi boards (§4), from the Zero 2 W to the Pi 5, rendering content with LVGL. The screens form a leaderless cluster that the user manages as one system through a web app served by every node.

### 1.2 Scope

In scope: device hardware, operating system image, boot, rendering, templates, menu data model, cluster discovery and replication, network fallback, HDMI-CEC control, the web app and its API, security, provisioning, updates, backup, monitoring and verification.

Out of scope for v1: cloud services, multi-site management, video playback, POS integrations, public third-party API.

### 1.3 Conventions

The key words **MUST**, **MUST NOT**, **REQUIRED**, **SHALL**, **SHALL NOT**, **SHOULD**, **SHOULD NOT**, **RECOMMENDED**, **MAY** and **OPTIONAL** are to be interpreted as described in [RFC 2119](https://www.rfc-editor.org/rfc/rfc2119) and [RFC 8174](https://www.rfc-editor.org/rfc/rfc8174) when, and only when, they appear in all capitals.

Every requirement has a stable ID (`AREA-NNN`). IDs are never reused or renumbered; withdrawn requirements are marked *Withdrawn* and kept. Tests, issues, commits and changelog entries SHOULD reference requirement IDs.

### 1.4 Terminology

| Term | Meaning |
|---|---|
| **Node** | One supported Raspberry Pi board (HW-001) with its software, driving one or two displays. |
| **Screen** | The logical display configuration (playlist, rotation, CEC settings) assigned to one HDMI output of a node. |
| **Boot device** | The storage the node boots from and keeps `data` on: microSD, eMMC, USB SSD or NVMe (HW-005). |
| **SoC family** | BCM2837 (Pi 3 generation, Zero 2 W), BCM2711 (Pi 4 generation) or BCM2712 (Pi 5 generation). |
| **Cluster** | All nodes sharing one cluster identity at a site. Managed as a single system. |
| **Record** | The unit of replicated data (an item, category, template, screen, user, …). |
| **HLC** | Hybrid Logical Clock timestamp used to order record versions. |
| **Draft** | Unpublished edits, replicated but not rendered. |
| **Publication** | An immutable, versioned change set that screens render. |
| **Live toggle** | A change that bypasses drafts and takes effect immediately (sold-out, power, announcements). |
| **Snapshot** | The compiled, render-ready form of the current publication stored on each node. |
| **Venue Wi-Fi** | The site's normal wireless network. |
| **Cluster AP** | The fallback hotspot one node raises when the venue Wi-Fi is unavailable. |

---

## 2. Goals and non-goals

### 2.1 Goals

1. **Fast boot** — content on screen ≤ 5 s after power is applied.
2. **Robust** — survives arbitrary power loss, node loss, storage loss and network loss without manual intervention.
3. **Autonomous** — every screen renders correctly with no network at all, indefinitely.
4. **One system** — the user manages all screens as one; any node can be used to administer the cluster.
5. **No single point of failure** — no leader, server or cloud is required for display, editing or recovery.
6. **Template-driven** — screens are generated from templates bound to categories, items and prices.
7. **TV control** — screens control their TV via HDMI-CEC.

### 2.2 Non-goals (v1)

- Video playback or heavy animation.
- More than one site per cluster; cloud management.
- Direct POS integrations or a documented public API.
- Video walls (one canvas spanning several displays).
- Output above 1920×1080, except optional 4K on the BCM2712 family (RND-015).
- GPU-accelerated rendering (RND-017).
- 32-bit boards and the 64-bit-capable Pi 2B v1.2.

---

## 3. System overview

```
             ┌──────────────── Venue LAN (Wi-Fi) or Cluster AP ────────────────┐
             │                                                                  │
   Browser ──┼──► https://chalkboard.local  (any node serves the same web app)  │
             │                                                                  │
             │   ┌──────────── Node ────────────┐   ┌──────── Node ────────┐    │
             │   │  ┌──────────┐  Unix  ┌─────┐ │   │                      │    │
             │   │  │  agent   │◄──────►│rndr │ │◄─►│   (identical)        │◄──►│ …
             │   │  │  (Rust)  │ socket │ (C) │ │   │                      │    │
             │   │  └──┬───┬───┘        └──┬──┘ │   └──────────────────────┘    │
             │   │     │   │ /dev/cecN     │DRM │   mTLS sync + signed beacons  │
             │   │  SQLite │               │KMS │                               │
             │   └─────────┼───────────────┼────┘                               │
             └─────────────┼───────────────┼────────────────────────────────────┘
                           └──── HDMI ─────┴──► TV
```

Each node runs two supervised processes:

- **Renderer** (`chalkboard-render`; C, LVGL, DRM/KMS) — starts first, reads only the local snapshot, never touches the network.
- **Agent** (`chalkboard-agent`; Rust) — discovery, replication, HTTP API and web app, CEC, time, scheduling, provisioning, updates, health.

---

## 4. Hardware

### 4.1 Supported boards

| Tier | Boards | SoC family | Meaning |
|---|---|---|---|
| **1** | Pi Zero 2 W, Pi 4B, Pi 5 | BCM2837, BCM2711, BCM2712 | Tested on hardware before every release; every budget (§21) is guaranteed. The Zero 2 W is the design reference as the slowest board. |
| **2** | Pi 3B, 3B+, 3A+, Pi 400, Pi 500, CM4, CM5 | all three | Same image and code paths; smoke-tested on hardware before every minor release (TEST-011). |
| **3** | CM3, CM3+, CM4S | BCM2837, BCM2711 | Community-supported: fixes are accepted, no release testing or guarantees. |

Not supported: 32-bit boards and the Pi 2B v1.2.

### 4.2 Requirements

| ID | Requirement |
|---|---|
| HW-001 | Each node MUST be one board listed in §4.1. A cluster MAY mix boards of any tier and SoC family. |
| HW-002 | The node MUST connect to each display over HDMI with a cable/adapter that carries the CEC line. |
| HW-003 | The node MUST be powered by a dedicated supply meeting the official rating for its model (Zero 2 W and Pi 3 family: 5 V 2.5 A; Pi 4B and Pi 400: 5.1 V 3 A; Pi 5 and Pi 500: the official 5.1 V 5 A supply; Compute Modules: per carrier board). It MUST NOT be powered from the TV's USB port, because TV standby would cut power and prevent CEC wake. |
| HW-004 | A battery-backed RTC is OPTIONAL per node: the built-in RTC of the BCM2712 family when a backup battery is fitted, otherwise an add-on module (e.g. DS3231 on I²C). The software MUST work with, without, or with a mix of RTC-equipped nodes (see §10). |
| HW-005 | Supported boot devices: industrial / high-endurance microSD (≥ 8 GB, RECOMMENDED) and Compute Module eMMC on all boards; USB SSD on the BCM2711 and BCM2712 families and NVMe on the BCM2712 family (Tier 2). USB boot on the BCM2837 family is not supported. All storage requirements (§6) apply to whichever boot device is used. |
| HW-006 | The enclosure SHOULD keep SoC temperature below 70 °C under sustained load (throttling begins at 80 °C). An active cooler is RECOMMENDED for the BCM2711 and BCM2712 families in enclosures. |
| HW-007 | Radio capabilities differ per board — 2.4 GHz only (Zero 2 W, Pi 3B), dual-band (other boards with Wi-Fi) or no Wi-Fi (some Compute Modules) — and the design MUST handle every combination (§12). |
| HW-008 | Built-in Ethernet MUST be used where present. A USB Ethernet adapter is OPTIONAL on boards without it; the image MUST include drivers for at least RTL8152/8153 and AX88179 chipsets. |
| HW-009 | At boot the agent MUST detect the board model and record it in the Node record together with its capabilities: SoC family, RAM, HDMI outputs, Wi-Fi bands, Ethernet, RTC, and boot device type. |
| HW-010 | On boards with a power button (BCM2712 family), a short press MUST show the identify overlay (RND-009) instead of shutting down; a long press keeps the firmware's forced power-off. |

---

## 5. Operating system and boot

### 5.1 Image

| ID | Requirement |
|---|---|
| OS-001 | The OS image MUST be built with Buildroot from a version-controlled external tree. Builds MUST be reproducible. |
| OS-002 | One universal aarch64 image MUST serve every supported board, so the same boot device works in any of them. It MUST contain the kernels for all SoC families (`kernel8.img` for BCM2837/BCM2711, `kernel_2712.img` for BCM2712), all device trees and the firmware for every generation; the firmware selects the right files at boot. Node identity and cluster membership MUST NOT be baked into the image. |
| OS-003 | The storage layout MUST provide two complete, independent slots (A and B). Each slot MUST contain the kernel, device tree blobs, overlays, kernel command line and a read-only squashfs root filesystem, so kernel and firmware-loaded files are updated and rolled back together with the root filesystem. A separate read-write `data` partition (F2FS, STO-008) MUST hold all persistent state. The layout MUST use MBR, which every supported board can boot from. |
| OS-004 | The root filesystem MUST be mounted read-only. All mutable state MUST live on `data` or in RAM (tmpfs). |
| OS-005 | The FAT partition(s) holding boot firmware files MUST only be written when installing into the inactive slot, at the slot commit (UPD-005), and when consuming a provisioning seed file (PROV-006, PROV-008). The commit MUST be a single small write that leaves the node bootable into a good slot if power is lost at any point during it (verified by TEST-003). |
| OS-006 | The init system SHOULD be BusyBox init with a minimal process supervisor; it MUST restart a crashed renderer or agent within 1 s. |
| OS-007 | The SoC hardware watchdog MUST be enabled on every board. A hung system MUST reboot within 15 s. |
| OS-008 | Logs MUST be kept in size-capped RAM ring buffers and MUST NOT be routinely written to the boot device. Kernel panics, watchdog resets and the last log lines before a crash MUST be captured with pstore/ramoops and saved to `data` once after the next boot (at most 20 crash reports kept). Warnings and errors MUST be mirrored to one peer so recent history survives the loss of a node. |
| OS-009 | Swap MUST NOT be placed on the boot device. A compressed RAM swap (zram, lz4, default 128 MB) MUST be enabled. OOM priorities MUST protect the renderer (`oom_score_adj` −900) and sacrifice the agent first, which the supervisor restarts (OS-006). Every tmpfs mount MUST have an explicit size cap. |
| OS-010 | Each slot's root filesystem MUST be protected by dm-verity, set up from the kernel command line (`dm-mod.create`, no initramfs per BOOT-004), with the root hash signed as part of the update bundle (UPD-001). A verity failure MUST mark the slot bad and reboot into the other slot. The agent MUST verify the slot's boot files (kernel, device tree blobs, overlays, command line) against a signed SHA-256 manifest after boot, and repair mismatches from the other slot (same version) or from a peer. |
| OS-011 | On boards with an EEPROM bootloader (BCM2711 and BCM2712 families), the update bundle MUST carry a tested bootloader image and a Chalkboard bootloader configuration (boot order microSD/eMMC → USB → NVMe, HDMI diagnostics screen disabled, no network-boot attempts, fast-boot settings). The agent MUST apply it only when the node's bootloader is older or configured differently, using Raspberry Pi's official update mechanism, one node at a time like other updates (UPD-006). Interrupted bootloader updates MUST be covered by the power-cut test (TEST-003). |

### 5.2 Boot

| ID | Requirement |
|---|---|
| BOOT-001 | Time from power applied to the last published content visible on HDMI MUST be ≤ 5.0 s (p95, measured on every Tier 1 board, see §22), including boots after an unannounced power loss. If measurements prove that a board's bootloader alone makes this impossible, a per-board target with the measured reason MUST be recorded in §21. The very first boot (partition growth, STO-009) and filesystem repair (STO-011) are exempt. |
| BOOT-002 | The renderer MUST NOT wait for network, sync, CEC, or time synchronisation before showing content. |
| BOOT-003 | The renderer MUST start before the agent and any network service. |
| BOOT-004 | The kernel SHOULD have all required drivers built in (no initramfs, no module loading on the critical path). Firmware boot delays and splash SHOULD be disabled. |
| BOOT-005 | Images and fonts needed by the current publication MUST be available in a pre-decoded render cache so first frame needs no image decoding. |
| BOOT-006 | If no valid snapshot exists, the renderer MUST show the setup screen (first boot) or a neutral "Starting…" screen with the node name, never a blank or console output. |
| BOOT-007 | Kernel and firmware console output MUST NOT be visible on the display. |

---

## 6. Storage and power-loss safety

| ID | Requirement |
|---|---|
| STO-001 | The local store MUST be SQLite in WAL mode with `synchronous=NORMAL`. Writes MUST be group-committed (bursts batched into one transaction, at most 1 s apart), and the WAL MUST be checkpointed every 1,000 pages or 5 min, whichever comes first. |
| STO-002 | Every file the renderer reads (snapshot, render cache) MUST be written atomically: write temp → `fsync` → `rename` → `fsync` directory. |
| STO-003 | The renderer MUST only ever see a complete snapshot. Snapshots MUST be addressed by content hash, and `current` MUST be an atomically replaced pointer. |
| STO-004 | The previous snapshot MUST be retained until the new one has rendered successfully once, and MUST be used as a fallback if the current one fails validation. |
| STO-005 | On database corruption, the agent MUST discard the local database and rebuild it from peers. If no peers are reachable, it MUST restore from the latest local backup (IO-005). The renderer MUST continue showing the last good snapshot throughout. |
| STO-006 | Power loss at any instant MUST NOT leave a node unbootable or rendering partial content. This MUST be verified by the power-cut test (TEST-003). |
| STO-007 | Blob storage (images, fonts) MUST be content-addressed by SHA-256 and verified on read. |
| STO-008 | The `data` partition MUST use F2FS, mounted with `noatime`, `fsync_mode=posix`, `background_gc=on` and `discard`. F2FS compression MUST NOT be used. |
| STO-009 | On first boot, before `data` is first mounted, the `data` partition MUST be grown to fill the boot device minus 10 % left unpartitioned (spare area for the device's wear levelling), capped at 32 GB: the MBR entry is extended, then `resize.f2fs` runs offline. Growth MUST be idempotent — each boot compares target, partition and filesystem sizes — so a power cut during growth simply repeats the step. |
| STO-010 | Only this state MAY be written to the boot device: the database (records, drafts, publications, audit log); blobs; snapshots and render cache (only when a publication is activated); node keys and certificates; the HLC high-water mark (persisted as a reservation at most every 10 s, not per write; SYNC-003); the time state when the clock is stepped; local backups (IO-005); crash reports (OS-008). Everything else — logs, metrics, peer tables, health and CEC state, web sessions (SEC-005) — MUST live in RAM. The allowlist MUST be enforced by TEST-009. |
| STO-011 | `data` MUST be mounted immediately at boot, relying on F2FS checkpoint recovery after power loss; `fsck.f2fs` MUST run only when the filesystem flags that it needs checking, never routinely. If a check is needed or the mount fails, the renderer MUST show the neutral "Starting…" screen (BOOT-006) while fsck runs; if `data` cannot be repaired, the node MUST reformat it and restore from peers (STO-005). |
| STO-012 | A change MUST count as committed only once it is stored on the origin node and acknowledged by at least one peer. A node with no reachable peers MUST commit with `synchronous=FULL` instead. |
| STO-013 | Write budget per node, measured as host bytes written to the boot device (block-layer statistics of the boot device, e.g. `/sys/block/<dev>/stat`), excluding newly uploaded blobs: **idle** (no edits) ≤ 1 MB/day; **typical use** (50 live toggles and 1 publication per day) ≤ 50 MB/day. Bytes written MUST be shown on the dashboard, and an alert MUST fire when the 7-day average exceeds the budget. |
| STO-014 | Each node MUST derive a storage health state — **OK**, **Degraded** or **Failing** — from: bytes written, kernel I/O errors and timeouts, F2FS error flags, dm-verity failures, read-latency trend, and the device's own health data where available (SD/eMMC vendor reports, NVMe SMART). In **Failing**, the node MUST stop all non-essential writes, keep rendering from what it has, alert, and the web app MUST offer *Replace screen* (PROV-005). |
| STO-015 | Every guarantee in this specification MUST hold for an instant, unannounced power loss. The design MUST NOT depend on UPS, supercapacitor or clean-shutdown hardware. |

---

## 7. Renderer

| ID | Requirement |
|---|---|
| RND-001 | The renderer MUST be written in C against LVGL 9.x, outputting via DRM/KMS. |
| RND-002 | Output MUST be 1920×1080 on every board by default. Rotation of 0°, 90°, 180° and 270° MUST be supported per screen (portrait and landscape). |
| RND-003 | Page transitions (cut, fade, slide) MUST run at ≥ 30 fps on every Tier 1 board, including rotated and dual-screen configurations. |
| RND-004 | Still images (PNG, JPEG, WebP) MUST be supported. Video is out of scope for v1. |
| RND-005 | The renderer core MUST be platform-independent, with backends for DRM/KMS (device) and WebAssembly (web preview, WEB-006). Both MUST produce pixel-identical output for the same snapshot and fonts. |
| RND-006 | The renderer MUST communicate with the agent only over a local Unix socket (commands: load snapshot, apply live state, identify, screenshot, status). |
| RND-007 | If the agent is absent or unresponsive, the renderer MUST keep running the current playlist, including daypart and schedule changes based on the local clock. |
| RND-008 | Live toggles (sold-out, announcements) MUST be applied without a full snapshot reload and without a visible flicker. |
| RND-009 | An *identify* overlay MUST display the node name, screen name and IP address(es) on request. |
| RND-010 | The renderer MUST be able to return a screenshot (scaled PNG) for the health dashboard. |
| RND-011 | Renderer resident memory SHOULD stay ≤ 96 MB per screen driven (≤ 192 MB for two 1080p screens). A 4K screen (RND-015) MAY use up to 4× the per-screen budget. |
| RND-012 | Text MUST render with full Unicode support for the configured languages, using FreeType with a glyph cache. Custom fonts (TTF/OTF) MAY be uploaded as blobs, e.g. for CJK or Arabic. |
| RND-013 | The image MUST ship these SIL OFL fonts: **Inter** (default; tabular numerals MUST be used for prices), **Noto Sans** (fallback for Greek, Cyrillic and extended Latin), and the display fonts **Oswald** and **Playfair Display**. Shipped fonts SHOULD total ≤ 4 MB. |
| RND-014 | Glyph fallback MUST be automatic: a glyph missing from the template font MUST be taken from Noto Sans, then from any uploaded fallback font. |
| RND-015 | On the BCM2712 family, a screen MAY be set to 3840×2160 output, at most one 4K screen per node (Tier 2). The 30 fps transition target applies, measured on the Pi 5. Templates render at the output resolution from their logical canvas (TPL-002). The BCM2711 family MUST stay at 1080p. |
| RND-016 | On boards with two HDMI outputs, the renderer MUST drive up to two screens independently, each with its own playlist, rotation and schedule. |
| RND-017 | The renderer MUST use LVGL's software renderer on every board in v1, keeping one code path and pixel-identical output with the preview (RND-005). |

---

## 8. Templates

| ID | Requirement |
|---|---|
| TPL-001 | Templates MUST be declarative JSON documents conforming to a versioned JSON Schema (`schemaVersion`). |
| TPL-002 | A template MUST declare its target orientation and logical canvas size. |
| TPL-003 | Node types MUST include at least: `container` (flex and grid layout), `text`, `price`, `image`, `shape`, `repeater`, `conditional`. |
| TPL-004 | `text` and `price` nodes MUST support data bindings (e.g. `{{item.name}}`, `{{category.title}}`, `{{item.prices.L}}`) resolved against the active language. |
| TPL-005 | A `repeater` MUST iterate over the items/categories produced by the page binding, with a defined slot capacity and sort order. |
| TPL-006 | When bound data exceeds a template's capacity, the page MUST auto-paginate into sub-pages that share the page's duration settings. |
| TPL-007 | `conditional` nodes MUST support at least: sold-out, has-image, has-tag, has-badge, has-variant, language. |
| TPL-008 | The sold-out presentation (hide, grey out, or badge) MUST be configurable per template. |
| TPL-009 | Templates MUST support styles/themes (colours, fonts, spacing) defined once and referenced by name. |
| TPL-010 | Templates MUST be validated by the agent on save and on receipt from a peer; invalid templates MUST be rejected with a readable error and never reach a snapshot. |
| TPL-011 | The template parser MUST be fuzz-tested (TEST-005). |
| TPL-012 | The system MUST ship with a set of built-in starter templates (e.g. list, two-column, grid with images, single-item highlight). |

---

## 9. Data model

All replicated entities are **records** (§11.3). Prices are stored as integers in minor units (e.g. cents/öre) with a single cluster-wide ISO 4217 currency and locale-aware formatting.

| Entity | Key fields |
|---|---|
| **Category** | id, title (per language), description (per language), image, sort order, parent (optional), tags, schedule |
| **Item** | id, name (per language), description (per language), category ids, price variants, image, tags, badges, allergens, schedule, sort order |
| **Price variant** | label (per language, e.g. "S", "33 cl"), amount (minor units), optional daypart overrides |
| **Template** | id, name, schema version, document |
| **Page** | template id, binding (query), duration, schedule, language mode |
| **Screen** | id, name, assigned node id and HDMI output, resolution (RND-015), rotation, playlist (ordered pages), groups, CEC settings, opening hours |
| **Group** | id, name, member screens |
| **Daypart** | id, name, weekly time ranges |
| **Schedule** | opening hours, dayparts, date exceptions (holidays) |
| **Announcement** | text (per language), style, target screens/groups, start/end |
| **User** | id, username, role, Argon2id password hash (Admin/Editor) or Argon2id PIN hash (Staff) |
| **Display currency** | ISO 4217 code, exchange rate (admin-set), rounding rule, enabled flag |
| **Node** | id, public key, name, board model and capabilities (HW-009), rank and optional admin override (WIFI-011) |
| **Blob** | SHA-256, media type, size, dimensions |

| ID | Requirement |
|---|---|
| DATA-001 | Items MUST support multiple price variants. |
| DATA-002 | Items MUST support a sold-out flag as a live toggle (PUB-004). |
| DATA-003 | Items and categories MUST support images. Images MUST be normalised on upload (max dimensions, stripped metadata) and pre-scaled per use. |
| DATA-004 | Items, categories and pages MUST support daypart schedules that control visibility; price variants MAY have daypart-specific amounts. |
| DATA-005 | Items MUST support descriptions, allergens (from a configurable list with icons), tags and badges. |
| DATA-006 | All user-facing text MUST support multiple languages, with a cluster default language. A missing translation MUST fall back to the default. |
| DATA-007 | A page MUST either be fixed to one language or rotate through a list of languages. |
| DATA-008 | Page bindings MUST support selecting by category, tag, explicit item list and combinations thereof, filtered by the active daypart. |
| DATA-009 | The total blob store SHOULD be capped (default 1 GB) with a visible usage indicator in the web app. |
| DATA-010 | The cluster MUST have exactly one pricing currency. All stored amounts are in that currency. |
| DATA-011 | An OPTIONAL secondary display currency MUST be supported: amounts are converted at an admin-set fixed rate with a configurable rounding rule (e.g. nearest 0.50), and are display-only. Templates MUST be able to show it via the `price` node (e.g. `{{item.prices.L | secondary}}`). Changing the rate MUST go through the publish workflow. |

---

## 10. Time and scheduling

| ID | Requirement |
|---|---|
| TIME-001 | The cluster MUST have a single configured IANA time zone. |
| TIME-002 | Each node MUST track a time quality level, in order of preference: (1) NTP-synchronised, (2) local RTC, (3) peer-provided time from a node with better quality, (4) browser-provided time from an authenticated web app session, (5) untrusted. |
| TIME-003 | Nodes with an RTC MUST read it at boot before the renderer evaluates schedules and MUST write NTP or peer time back to it. |
| TIME-004 | Nodes MUST exchange time and quality in discovery beacons. A node MUST adopt time from the best-quality peer if its own quality is lower. |
| TIME-005 | When the web app is opened and the cluster quality is (5) untrusted, the agent MUST offer to set time from the browser. |
| TIME-006 | The web app MUST show a warning while any node's time quality is untrusted. |
| TIME-007 | Schedules MUST be evaluated locally by each node; screens MUST NOT depend on peers to change dayparts or power state. |
| TIME-008 | Large backward clock jumps MUST NOT cause HLC regression (SYNC-003). |

---

## 11. Cluster: discovery, membership and replication

### 11.1 Discovery

| ID | Requirement |
|---|---|
| DISC-001 | Nodes MUST announce themselves with periodic UDP multicast beacons (default every 2 s) containing: cluster id, node id, firmware version, time + quality, data digest, and addresses. |
| DISC-002 | Beacons MUST be authenticated with a MAC keyed from the cluster secret; unauthenticated beacons MUST be ignored. |
| DISC-003 | Nodes MUST also advertise via mDNS/DNS-SD (`_chalkboard._tcp`), each as `<node-name>.local`. |
| DISC-004 | Exactly one reachable node — the online node with the best rank (WIFI-011) — MUST answer for `chalkboard.local`, so the web app has one stable address. |
| DISC-005 | Unadopted nodes MUST beacon a distinct "unadopted" announcement so the web app can list them for adoption. |
| DISC-006 | A node MUST be considered offline after 10 s without beacons; the dashboard MUST reflect this. |

### 11.2 Membership

| ID | Requirement |
|---|---|
| MEM-001 | A cluster MUST be created on the first node, generating the cluster id, cluster secret and cluster CA key. The CA key MUST be distributed to every adopted node (protected per SEC-008), so any node can adopt new nodes and reissue certificates. |
| MEM-002 | Each node MUST hold an Ed25519 identity key generated on first boot and stored on `data`. |
| MEM-003 | Nodes MUST join only by adoption with a join code (PROV-003); on adoption the node receives a certificate signed by the cluster CA. |
| MEM-004 | An admin MUST be able to remove a node; removal MUST revoke its certificate cluster-wide. |
| MEM-005 | The design target is ≤ 10 nodes; the implementation MUST function with up to 16. |

### 11.3 Replication

| ID | Requirement |
|---|---|
| SYNC-001 | Every record MUST carry: collection, id, HLC timestamp, origin node id, tombstone flag, content hash, and an Ed25519 signature by the origin node (MEM-002) over all other fields. |
| SYNC-002 | Conflicts MUST be resolved per record by last-writer-wins on `(HLC, origin node id)`. |
| SYNC-003 | HLC timestamps MUST be 64-bit (48-bit physical milliseconds + 16-bit logical counter) and MUST be monotonic per node even across reboots and clock steps; across reboots this is achieved by resuming above the persisted high-water mark (STO-010). |
| SYNC-004 | Changes MUST be pushed to all reachable peers immediately (best effort), and periodic anti-entropy MUST reconcile using per-collection digests so all nodes converge. |
| SYNC-005 | Node-to-node sync MUST use mutual TLS with cluster-CA-issued certificates. |
| SYNC-006 | Every node MUST hold the complete set of records and all blobs referenced by any retained publication (PUB-005), open draft or local backup (IO-005), so rollback works on any node without network access. Unreferenced blobs MAY be garbage-collected. |
| SYNC-007 | Deletions MUST be tombstones. A tombstone MUST be retained until every known node has acknowledged it or until it is 30 days old, whichever comes first, and MAY be garbage-collected after that. An offline node therefore cannot block garbage collection. |
| SYNC-008 | A node returning after a partition of any length MUST converge without manual intervention. A node whose last successful sync is older than 30 days MUST perform a full resync instead of an incremental one; local changes it made after its last sync MUST be kept as a draft for review rather than silently applied or lost. |
| SYNC-009 | Received records MUST be validated (schema, signature of origin, referential integrity) before being applied. |
| SYNC-010 | The sync protocol MUST be versioned; nodes at firmware version N and N-1 MUST interoperate (required for rolling updates). |
| SYNC-011 | With all nodes online, a live toggle MUST be visible on all affected screens within 2 s (p95). |
| SYNC-012 | With all nodes online, a publication (excluding new blob transfer time) MUST be active on all affected screens within 10 s (p95). |
| SYNC-013 | The protocol MUST be frugal with airtime: beacons SHOULD be ≤ 512 bytes, sync MUST transfer only changed records, and blobs MUST transfer only when referenced by a draft preview request or a publication, each blob at most once per node. Steady-state cluster traffic with no edits SHOULD stay below 2 kB/s per node. |

---

## 12. Network resilience

| ID | Requirement |
|---|---|
| WIFI-001 | Screens MUST render correctly, including schedule changes, with no network connectivity for an unlimited time. |
| WIFI-002 | When a node has lost the venue Wi-Fi for ≥ 2 min (configurable), it MUST look for the cluster AP and join it if present. |
| WIFI-003 | If no cluster AP is visible, the AP-capable node at position *k* (0-based) in the rank order (WIFI-011), counting only nodes with Wi-Fi, MUST start the cluster AP after `2 min + k × 20 s` unless it has seen the cluster AP by then. This yields one AP without coordination. |
| WIFI-004 | The cluster AP SSID and WPA2/WPA3 passphrase MUST be derived from the cluster secret and MUST be viewable by admins in the web app (including as a QR code) for phone access. |
| WIFI-005 | While on the cluster AP, discovery, sync and the web app MUST work exactly as on the venue Wi-Fi. |
| WIFI-006 | Nodes MUST periodically scan for the venue Wi-Fi. When the venue Wi-Fi is usable again, nodes MUST return to it and the AP node MUST stop the AP after the other nodes have left (or after a timeout). |
| WIFI-007 | If two cluster APs exist (split-brain), the one with the worse rank MUST stop within 30 s of detecting the other. |
| WIFI-008 | Multiple venue Wi-Fi networks (priority-ordered) SHOULD be supported. |
| WIFI-009 | A node with a working wired link (HW-008) MUST prefer it for all traffic and keep Wi-Fi associated as a fallback where it has Wi-Fi. A wired node that raises the cluster AP MUST relay cluster traffic (discovery, sync, web app) between AP clients and wired peers, so Wi-Fi-only and wired nodes stay one cluster. |
| WIFI-010 | Each node MUST measure link quality (RSSI, TX retry rate, packet loss to peers) and report it to the dashboard. The web app MUST warn when a node is below a configurable threshold (default RSSI < −72 dBm or retries > 20 %). |
| WIFI-011 | Node rank MUST be computed automatically, ordered by: (1) working wired link, (2) 5 GHz-capable radio, (3) SoC family (BCM2712 > BCM2711 > BCM2837), (4) node id as tie-breaker. An admin MAY override a node's position. Ranks MUST be recomputed and replicated when nodes join, leave or change capabilities. |
| WIFI-012 | Nodes MUST use 5 GHz for the venue Wi-Fi where both board and network support it. The cluster AP MUST use 5 GHz only if every Wi-Fi node in the cluster supports it, otherwise 2.4 GHz. |
| WIFI-013 | Nodes without Wi-Fi MUST be connected by Ethernet, are never AP candidates, and MUST NOT expect a setup AP (PROV-001); they are provisioned over Ethernet or with a seed file (PROV-006). |

---

## 13. HDMI-CEC

| ID | Requirement |
|---|---|
| CEC-001 | CEC MUST be implemented using the Linux kernel CEC API, one adapter per HDMI output (`/dev/cec0`, `/dev/cec1`), each registering as a Playback Device and controlling the screen on that output. |
| CEC-002 | The node MUST set a configurable OSD name (default: screen name, truncated to 14 characters). |
| CEC-003 | On boot and on TV wake, the node MUST send *Image View On* (when the screen is scheduled on) and *Active Source*. |
| CEC-004 | Each screen MUST have a power schedule derived from opening hours, with date exceptions (holidays). At schedule start the node MUST power the TV on; at schedule end it MUST send *Standby*. |
| CEC-005 | Users whose role allows it (SEC-004) MUST be able to power on, standby, and re-assert active source per screen, per group and for all screens from the web app. Group commands MUST be executed by each target node locally. |
| CEC-006 | The node MUST poll TV power status (default every 30 s) and report it to the dashboard. |
| CEC-007 | Watchdog re-assert: during scheduled on-time, if the TV is in standby or another source is active for longer than a grace period (default 60 s), the node MUST restore power and active source. This MUST be configurable per screen and MAY be disabled. |
| CEC-008 | A manual override from the web app MUST suspend the watchdog for that screen until the next schedule boundary or a configurable timeout. |
| CEC-009 | Mute and volume (up/down/set where supported) MUST be available per screen. |
| CEC-010 | The node MUST probe and record which CEC features the TV responds to, and the web app MUST show unsupported features as unavailable rather than failing silently. |
| CEC-011 | CEC actions MUST be logged with source (schedule, watchdog, user). |

---

## 14. Publishing workflow

| ID | Requirement |
|---|---|
| PUB-001 | Changes to templates, pages, playlists, screens, categories, items, prices and display currency settings (DATA-011) MUST be made in a draft. Drafts MUST replicate so editing can continue on any node. |
| PUB-002 | Publishing MUST create an immutable, numbered publication recording author, time and the record versions it contains. |
| PUB-003 | A node MUST switch to a new publication only after all referenced records and blobs are present locally and its snapshot is built and validated; the switch MUST be atomic. |
| PUB-004 | Live toggles — sold-out, announcements, CEC power actions — MUST take effect immediately without a publish. |
| PUB-005 | Users who may publish (SEC-004) MUST be able to roll back to any retained previous publication (default: last 20). Rollback creates a new publication. |
| PUB-006 | The web app MUST show a diff of the draft against the current publication before publishing. |
| PUB-007 | Concurrent drafts by different users MUST merge per record (SYNC-002); the web app MUST indicate records edited by another user since the draft was opened. |

---

## 15. Web app and API

| ID | Requirement |
|---|---|
| WEB-001 | Every node MUST serve the same web app and API. Edits on any node MUST replicate to all nodes. |
| WEB-002 | The web app MUST be a static single-page app bundled in the image; its JS/CSS SHOULD be ≤ 300 kB gzipped, excluding the WASM preview. Svelte is RECOMMENDED. |
| WEB-003 | The web app MUST be usable on a phone (≥ 360 px wide) and on desktop. |
| WEB-004 | The web app MUST provide: menu editor (categories, items, variants, images, translations), template library and editor, screen and playlist management, groups, schedules and dayparts, sold-out panel, announcements, CEC controls, publishing, users, node management, health dashboard, settings, import/export, backup/restore, updates. |
| WEB-005 | A **sold-out panel** MUST let a user toggle availability for any item in ≤ 2 taps from the app's start page. For Staff users it MUST be the start page. |
| WEB-006 | The web app MUST preview pages and screens using the WASM renderer (RND-005), with live data from the current draft. |
| WEB-007 | All web app functions MUST go through a versioned HTTP JSON API (`/api/v1`). The API is internal in v1 and MAY change between minor versions. |
| WEB-008 | Live updates (status, toggles, other users' edits) MUST be pushed to the browser (Server-Sent Events or WebSocket). |
| WEB-009 | The agent's resident memory SHOULD stay ≤ 64 MB. |

---

## 16. Security

| ID | Requirement |
|---|---|
| SEC-001 | By default the web app and API MUST be served over HTTPS using a certificate issued by the cluster CA, valid for every node name, `chalkboard.local` and node IPs, and reissued automatically when a node's addresses change. Plain HTTP MUST only redirect to HTTPS (and serve the CA download, SEC-002). |
| SEC-002 | The web app MUST provide a guided **Trust this cluster** flow with per-platform instructions and installable profiles (iOS/macOS `.mobileconfig`, Android and Windows/Linux certificate files). |
| SEC-003 | Admin and Editor users MUST authenticate with username and password. Passwords MUST be stored as Argon2id hashes. |
| SEC-004 | Roles: **Admin** (everything); **Editor** (menu data, templates, pages, sold-out, announcements, TV power on/standby/active source, publish; not users, nodes, network, CEC settings, updates, backup restore); **Staff** (sold-out toggles, announcements, and TV power on/standby only). Each role's permissions MUST be a superset of the role below it (Admin ⊇ Editor ⊇ Staff). |
| SEC-005 | User accounts MUST replicate so a user can log in on any node. Sessions MUST be stateless tokens signed with a key derived from the cluster secret, valid on every node, so logging in causes no storage write (STO-010). Revocation MUST work by bumping a per-user session epoch stored in the user record. |
| SEC-006 | Login MUST be rate-limited per client and per account. |
| SEC-007 | Every change MUST be attributed to a user (or to a node for automatic actions) and kept in an audit log. |
| SEC-008 | Cluster secrets and private keys MUST be readable only by the agent process. |
| SEC-009 | All input from the network (beacons, sync, API, uploads, imports) MUST be validated, size-limited and fuzz-tested. |
| SEC-010 | SSH MUST be disabled by default and MAY be enabled by an admin with key-only authentication. |
| SEC-011 | An initial admin MUST be created during cluster setup; no default credentials MAY exist. |
| SEC-012 | Optional **domain certificate** mode: an admin MAY configure a domain they control and a DNS provider API token. The cluster MUST then obtain a publicly trusted wildcard certificate via ACME DNS-01 (e.g. `*.chalkboard.example.se`), publish DNS records pointing node names at their LAN IPs, and share the certificate with all nodes via sync. Renewal MUST be attempted from 30 days before expiry by any node with internet access; on failure the cluster MUST keep serving the valid certificate, alert (OPS-003), and fall back to the cluster CA certificate only after expiry. |
| SEC-013 | Staff users MUST log in with a 4–8 digit PIN, stored as an Argon2id hash. PIN login MUST be rate-limited (lockout after 5 failures for 5 min, escalating), and admins MAY restrict Staff login to the venue LAN and cluster AP. Staff sessions MUST expire after a configurable idle time (default 12 h). |
| SEC-014 | The DNS provider token (SEC-012) MUST be treated as a secret (SEC-008) and excluded from backups unless encrypted (IO-004). |
| SEC-015 | With domain certificates enabled, each node MUST choose its certificate per TLS connection by SNI: the public certificate for names under the configured domain, and the cluster CA certificate for `.local` names, IP addresses and connections without SNI. The `.local` address therefore keeps working when the internet or public DNS is unavailable. Setup MUST check that the venue router resolves the public names to LAN IPs and warn if it does not (e.g. DNS rebinding protection). |

---

## 17. Provisioning

| ID | Requirement |
|---|---|
| PROV-001 | On first boot an unconfigured node MUST display a setup screen with a QR code, the node's short id, and instructions, and MUST start a temporary setup AP (`chalkboard-setup-<shortid>`, password shown on screen). Nodes without Wi-Fi MUST instead serve the setup flow over Ethernet, showing their address on screen. |
| PROV-002 | The setup flow (in a phone or computer browser, via the setup AP or over Ethernet per PROV-001) MUST let the user choose and test venue Wi-Fi where the node has Wi-Fi, then either **create a new cluster** or **join an existing cluster**. |
| PROV-003 | Joining MUST require a single-use join code generated by an admin under *Add screen*, valid for ≤ 30 min. |
| PROV-004 | After joining, the node MUST pull the full dataset from peers before rendering its assigned screen. |
| PROV-005 | *Replace screen X* MUST assign an existing screen's configuration to the new node and remove the old node from the cluster. |
| PROV-006 | A seed file on the boot partition (`chalkboard-seed.json`: Wi-Fi credentials, join code, optional screen assignment) MUST allow zero-touch provisioning. The file MUST be deleted after successful use. |
| PROV-007 | An adopted but unassigned node MUST show an identify-style "unassigned" screen. |
| PROV-008 | A factory reset MUST be possible from the web app and by a physical method that does not require the network (e.g. seed file `reset: true`). |

---

## 18. Updates

| ID | Requirement |
|---|---|
| UPD-001 | Firmware updates MUST be delivered as signed bundles verified against a release public key embedded in the image. RAUC is RECOMMENDED. |
| UPD-002 | An admin MUST be able to upload a bundle through the web app. The agent MAY additionally fetch bundles from a configured release URL when internet is available. |
| UPD-003 | Bundles MUST be distributed peer-to-peer; each node downloads from the cluster, not the internet. |
| UPD-004 | Installation MUST target the inactive slot (OS-003) and boot it once using the Raspberry Pi `tryboot` mechanism. The slot-selection mechanism — per-slot `os_prefix` directories selected by `config.txt` / `tryboot.txt`, or separate boot partitions selected by `autoboot.txt` — MUST be one mechanism that works on all three SoC families, chosen by a hardware spike on every Tier 1 board before 0.1 and recorded in this specification. |
| UPD-005 | A node MUST mark the new slot good only after: the renderer has shown content, the agent has rejoined the cluster, and sync is healthy — within 120 s. Otherwise the next (watchdog) reboot MUST return to the previous slot. |
| UPD-006 | Updates MUST roll through the cluster one node at a time, and MUST halt if a node fails to become healthy. |
| UPD-007 | Downgrades MUST be refused unless explicitly forced by an admin. |
| UPD-008 | Data schema migrations MUST be forward-only, automatic, and safe against power loss; the previous firmware MUST tolerate data written by the next (SYNC-010). |

---

## 19. Import, export and backup

| ID | Requirement |
|---|---|
| IO-001 | Categories, items and price variants MUST be importable and exportable as CSV and XLSX. |
| IO-002 | Imports MUST land in a draft and show a diff (added, changed, removed) before being applied. |
| IO-003 | A full backup MUST be downloadable as a single archive containing all records, blobs and templates, with a manifest and a signature. The archive MUST be streamed to the browser and never staged on the boot device. |
| IO-004 | Secrets (cluster secret, keys, password hashes) MUST be excluded from backups unless the admin opts in and provides a passphrase, in which case they MUST be encrypted. |
| IO-005 | Each node MUST keep automatic local backups: a compressed copy of the database without blobs (blobs are kept by reference, SYNC-006), taken at most once a day and only if something changed since the previous backup, retaining the last 7. |
| IO-006 | A backup MUST be restorable into an existing cluster (as a new draft or publication) or onto a fresh node to create a new cluster. |

---

## 20. Monitoring and operations

| ID | Requirement |
|---|---|
| OPS-001 | The health dashboard MUST show per node: board model and support tier, online state, screen assignment per HDMI output, current page, live screenshot, TV power state, active-source state, SoC temperature, throttling flags, active link (Wi-Fi or wired), Wi-Fi network, signal and retry rate (WIFI-010), IP, uptime, firmware version and slot, time quality, sync lag, storage usage, storage health state and bytes written (STO-013, STO-014). |
| OPS-002 | Logs from any node MUST be viewable and downloadable from the web app. |
| OPS-003 | Optional outbound alerts (webhook and ntfy) MUST be supported for: node offline, TV off during opening hours, overheating/throttling, storage low, update failure, poor link quality, certificate renewal failure, storage health Degraded or Failing, write budget exceeded. Alerts MUST be queued while offline and deduplicated. |
| OPS-004 | An OPTIONAL VPN package (Tailscale or WireGuard) MUST be included in the image, disabled by default, configurable by admins. |
| OPS-005 | The system MUST NOT require any cloud service for any function. |

---

## 21. Performance budgets (summary)

Every budget applies to every Tier 1 board (§4.1). Per-board exceptions (BOOT-001): none recorded yet.

| Metric | Budget | Ref |
|---|---|---|
| Power-on → content | ≤ 5.0 s p95 | BOOT-001 |
| Live toggle propagation | ≤ 2 s p95 | SYNC-011 |
| Publication propagation (excl. blobs) | ≤ 10 s p95 | SYNC-012 |
| Transition frame rate | ≥ 30 fps | RND-003 |
| Renderer RSS | ≤ 96 MB per 1080p screen | RND-011 |
| Agent RSS | ≤ 64 MB | WEB-009 |
| Hang → reboot | ≤ 15 s | OS-007 |
| Crash → restart | ≤ 1 s | OS-006 |
| Web bundle (excl. WASM) | ≤ 300 kB gz | WEB-002 |
| Boot-device writes, idle | ≤ 1 MB/day | STO-013 |
| Boot-device writes, typical use | ≤ 50 MB/day | STO-013 |
| Local commit batching | ≤ 1 s | STO-001 |

---

## 22. Verification

| ID | Requirement |
|---|---|
| TEST-001 | Boot time and every other budget in §21 MUST be measured on each Tier 1 board (§4.1) in CI-attached hardware-in-the-loop, using an HDMI capture device or light sensor for boot time, on every release candidate. |
| TEST-002 | Renderer output MUST be covered by golden-image tests (WASM/headless backend) for all built-in templates. |
| TEST-003 | Power-cut torture test: ≥ 1,000 random power cuts (relay-controlled) during idle, sync, publish and update; zero unbootable nodes or partial renders allowed. |
| TEST-004 | Sync convergence MUST be tested with property-based tests and a multi-node simulator including partitions, clock skew, reordering and node loss. |
| TEST-005 | Template parser, sync protocol, beacon parser, API and import parsers MUST be fuzz-tested. |
| TEST-006 | Network fallback (WIFI-*) MUST be tested on a ≥ 3-node rig by disabling the venue AP. |
| TEST-007 | A CEC compatibility matrix MUST be maintained for tested TV brands/models. |
| TEST-008 | Rolling update with induced failure MUST verify rollback (UPD-005, UPD-006). |
| TEST-009 | Boot-device writes MUST be traced per process and path in CI (on an emulated block device) and on hardware-in-the-loop; any write outside the STO-010 allowlist, or exceeding the STO-013 budget over a simulated day, MUST fail the build. |
| TEST-010 | Storage fault injection MUST cover: corrupted squashfs blocks (dm-verity fallback, OS-010), corrupted boot files, an F2FS image flagged for fsck and an unrepairable one (STO-011), corrupted SQLite database (STO-005), and I/O errors driving the health state to Failing (STO-014). |
| TEST-011 | Every Tier 2 board MUST pass a hardware smoke test before every minor release: boot to content, render a built-in template, join a cluster, CEC on/standby, and apply an update. |

---

## 23. Roadmap

| Version | Scope |
|---|---|
| **0.1** | Hardware spike on all Tier 1 boards deciding the boot-slot mechanism (UPD-004) and measuring boot time and transition frame rate with rotation; universal Buildroot image with board detection (HW-009), A/B layout, dm-verity, F2FS `data` with first-boot growth, zram and RAM-only logging; write tracing and budget tests (TEST-009); renderer with JSON templates; local SQLite; single-node web app (menu, templates, screen, playlist); basic CEC (on/standby/active source); BOOT-001 met. |
| **0.2** | Discovery, membership and adoption with join codes (Wi-Fi configured manually), mTLS replication (HLC/LWW); users and roles (Admin/Editor); cluster CA trust flow; health dashboard with link diagnostics and storage health; peer-ack commits (STO-012); built-in and USB Ethernet support. |
| **0.3** | Draft → publish workflow, rollback, WASM preview, sold-out panel and announcements; Staff role with PIN login. |
| **0.4** | Provisioning (setup AP, QR, seed file, replace screen); RTC/NTP/peer time. |
| **0.5** | Signed OTA with P2P distribution and rolling A/B; managed EEPROM bootloader (OS-011); network fallback (cluster AP) with capability-aware ranking and band selection. |
| **0.6** | CEC schedules, watchdog, group commands, volume/mute; dual-HDMI nodes; optional 4K on BCM2712; dayparts; multi-language; allergens/badges; Tier 2 smoke tests (TEST-011). |
| **0.7** | CSV/XLSX import/export; backup/restore; alerts; VPN package; secondary display currency; ACME domain certificates. |
| **1.0** | All MUST requirements met; full verification suite (§22) green; user and operator documentation. |

---

## 24. Naming and licensing

| ID | Requirement |
|---|---|
| NAME-001 | The product and project name is **Chalkboard**. Lower-case `chalkboard` MUST be used for the repository, hostnames (`chalkboard.local`), the mDNS service (`_chalkboard._tcp`), SSIDs (`chalkboard-setup-*`), binaries (`chalkboard-agent`, `chalkboard-render`) and file names. A trademark and namespace check SHOULD be done before the first public release. |
| LIC-001 | Chalkboard's own source code MUST be dual-licensed **MIT OR Apache-2.0**, with `LICENSE-MIT` and `LICENSE-APACHE` in the repository root and an SPDX header (`SPDX-License-Identifier: MIT OR Apache-2.0`) in every source file. |
| LIC-002 | Contributions MUST be accepted under the same dual licence (inbound = outbound). |
| LIC-003 | Every firmware release MUST include a generated licence manifest of all bundled third-party components (Buildroot `legal-info`), shown in the web app under *About*. |
| LIC-004 | For GPL/LGPL components in the image (e.g. Linux, BusyBox), each release MUST publish the corresponding source and build configuration alongside the bundle. |

---

## 25. Versioning and changelog

- This project adheres to [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html). Firmware images, the sync protocol and the template schema each carry a version; the firmware version is the product version.
- Changes are recorded in [CHANGELOG.md](CHANGELOG.md), formatted per [Keep a Changelog 1.0.0](https://keepachangelog.com/en/1.0.0/).
- This specification is versioned independently with SemVer: MAJOR for removed or incompatible requirements, MINOR for added requirements, PATCH for clarifications.
