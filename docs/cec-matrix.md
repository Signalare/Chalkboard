# HDMI-CEC Compatibility Matrix (TEST-007)

<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->

This document maintains the Hardware-in-the-Loop (HIL) HDMI-CEC compatibility
matrix across physical displays and TV brands (`TEST-007`), validating
requirements `CEC-001` through `CEC-006`.

## Architecture Overview

Chalkboard communicates with physical displays using the standard Linux kernel
CEC character device API (`/dev/cec0`, `/dev/cec1` per `CEC-001`). No
third-party userspace libraries (`libcec`) are permitted on the critical path.

* **Adapter Allocation (`CEC-001`):** Configured as a *Playback Device*
  (`CEC_LOG_ADDR_TYPE_PLAYBACK`, logical address `0x4`).
* **OSD Naming (`CEC-002`):** Sets a configurable OSD name up to 14 characters
  (default: `Chalkboard`).
* **Power & Input Control (`CEC-003`):** Sends `Image View On` (`0x04`) and
  broadcasts `Active Source` (`0x82 [phys_addr]`) on startup and scheduled
  wake.
* **Standby (`CEC-004`):** Sends `Standby` (`0x36`) to the TV at schedule end.
* **Power Status Polling (`CEC-006`):** Polls display power status via `Give
  Device Power Status` (`0x8f`) and monitors `Report Power Status` (`0x90`).

---

## Compatibility Matrix

| Brand | Model / EDID Descriptor | Marketing Name | SoC / Controller | Physical Addr | Logical Addr | Power On (`0x04`) | Active Source (`0x82`) | Standby (`0x36`) | Status Query (`0x8f`) | Status |
|---|---|---|---|---|---|---|---|---|---|---|
| **LG Electronics** | `LG TV SSCR2` (`GSM` / `0xc0c8`) | SimpLink | BCM2837 (`vc4_hdmi`) | `1.0.0.0` | `0x4` (Playback 1) | **Pass** (52 ms) | **Pass** (101 ms) | **Pass** (53 ms) | **Pass** (142 ms) | **Verified (HIL)** |
| **Samsung** | *Lab Reference Profile* | Anynet+ | BCM2711 / BCM2837 | `1.0.0.0` | `0x4` (Playback 1) | Expected Pass | Expected Pass | Expected Pass | Expected Pass | *Candidate* |
| **Sony** | *Lab Reference Profile* | BRAVIA Sync | BCM2711 / BCM2837 | `1.0.0.0` | `0x4` (Playback 1) | Expected Pass | Expected Pass | Expected Pass | Expected Pass | *Candidate* |

---

## Physical Verification Log: LG Electronics (`LG TV SSCR2`)

Tested on Raspberry Pi 3 Model B+ Rev 1.3 connected directly to an LG TV over a
standard full-size HDMI cable and monitored via UART serial console
(`115200` baud).

### Hardware & EDID Identification

* **Display Manufacturer ID:** `GSM` (GoldStar / LG Electronics)
* **Product Code:** `0xc0c8`
* **EDID ASCII Descriptor:** `LG TV SSCR2`
* **Preferred Resolution:** `1920x1080 @ 60 Hz`
* **Audio:** Disabled via Device Tree overlay (`dtoverlay=vc4-kms-v3d,noaudio`)
  to ensure instant KMS pipeline initialization without ALSA DMA dependencies.
* **Kernel Driver:** `vc4-drm` / `vc4_hdmi` (`Linux 6.12.61-v8`)
* **CEC Device Node:** `/dev/cec0` (major 250, minor 0)
* **Physical Address:** `1.0.0.0` (direct connection to HDMI 1)

### Execution Trace & Latency Measurements

Measurements captured using `tools/hil/cec-test.c`:

1. **Logical Address Allocation & OSD Name Announcement (`CEC-001`, `CEC-002`):**

   ```text
   [CEC-HIL] Opening CEC adapter /dev/cec0...
   [CEC-HIL] Driver: vc4_hdmi, Name: vc4-hdmi, Caps: 0x0000031e
   [CEC-HIL] Physical Address: 1.0.0.0 (raw: 0x1000)
   [CEC-HIL] Claimed Logical Address: 0x4 (mask: 0x0010)
   [CEC-HIL] Setting OSD Name to Chalkboard...
   [CEC-HIL] Set OSD Name sent! tx_status=0x1 (took 292.69 ms)
   ```

2. **Power Status Polling (`CEC-006`):**

   ```text
   [CEC-HIL] Querying TV power status (0x8f Give Device Power Status)...
   [CEC-HIL] Transmit result: tx_status=0x1, rx_status=0x1 (took 142.45 ms)
   [CEC-HIL] TV Power Status: ON (0x00)
   ```

3. **Standby Execution (`CEC-004`):**

   ```text
   [CEC-HIL] Sending Standby (0x36) to TV...
   [CEC-HIL] Standby sent! tx_status=0x1 (took 52.65 ms)
   [CEC-HIL] Querying TV power status (0x8f Give Device Power Status)...
   [CEC-HIL] Transmit result: tx_status=0x1, rx_status=0x1 (took 142.38 ms)
   [CEC-HIL] TV Power Status: STANDBY (0x01)
   ```

4. **Image View On & Active Source Wake (`CEC-003`):**

   ```text
   [CEC-HIL] Sending Image View On (0x04) to TV...
   [CEC-HIL] Image View On sent! tx_status=0x1 (took 52.70 ms)
   [CEC-HIL] Broadcasting Active Source (1.0.0.0)...
   [CEC-HIL] Active Source sent! tx_status=0x1 (took 100.63 ms)
   [CEC-HIL] Querying TV power status (0x8f Give Device Power Status)...
   [CEC-HIL] TV Power Status: ON (0x00)
   ```

### Observations & Brand Specifics

* **LG SimpLink Integration:** The TV responds within ~53 ms to `Image View On`
  and transitions immediately to active display.
* **Standby Confirmation:** Upon receiving opcode `0x36`, the display turns off
  its backlight and transitions to low-power standby mode. Opcode `0x8f` confirms
  status change from `ON (0x00)` to `STANDBY (0x01)` within 142 ms.
* **Auto-Routing:** When `Active Source (1.0.0.0)` is broadcast after power-on,
  the TV automatically switches from broadcast TV / auxiliary inputs directly to
  the Chalkboard HDMI port.

---

## Test Automation Rig (`tools/hil/cec-test.c`)

The test utility exercises `/dev/cec0` directly via standard Linux ioctls
(`CEC_TRANSMIT`, `CEC_RECEIVE`, `CEC_ADAP_G_PHYS_ADDR`, `CEC_ADAP_S_LOG_ADDRS`):

```bash
# Query TV power status and physical connection
/tmp/cec-test probe

# Send Image View On and Active Source
/tmp/cec-test on

# Re-assert Active Source without power toggling
/tmp/cec-test active

# Put TV into low-power Standby
/tmp/cec-test standby
```
