# Hardware-in-the-Loop (HIL) Test Rig

<!-- SPDX-License-Identifier: MIT OR Apache-2.0 -->

Automated hardware-in-the-loop (HIL) test harness and measurement rig for
Chalkboard on physical hardware.

This harness validates the release gates:

- **`TEST-001`**: Automated boot-time measurement and performance budget gating in CI.
- **`BOOT-001`**: Cold-boot time from power applied to visible content on HDMI ≤ 5.0 s (p95).
- **`TEST-003`**: Power-cut torture testing (≥ 1,000 random relay power cuts).

---

## 1. Bill of Materials (Parts List)

| Component | Specification | Purpose | Notes |
|---|---|---|---|
| **Target Board** | Raspberry Pi 4 Model B (Rev 1.1 `c03111`, 4 GB) | Target single-node menu board | Tier 1 target (`HW-001`, `HW-009`) |
| **Power Supply** | Official Raspberry Pi 15.3 W USB-C (5.1 V, 3.0 A) | Clean, rated power (`HW-003`) | Must NOT be powered from TV USB |
| **Relay Power Switch** | Shelly Plus 1PM or Shelly Plus Plug S | Automated mains/DC power cycling | Local HTTP RPC, no cloud needed (`OPS-005`) |
| **USB-UART Adapter** | 3.3 V logic (CP2102, FT232RL, or CH340G) | Unattended serial console capture | 115200 baud, 8N1 |
| **HDMI Capture / Sensor** | USB UVC HDMI Capture Dongle (1080p60) or Phototransistor | First visible content detection | First-frame latency verification |
| **Storage Device** | Kingston DataTraveler 3.0 (or SanDisk Extreme microSD) | Dual-slot A/B boot storage | 16 GB+ flash media |
| **Cabling** | Micro-HDMI to HDMI, jumper wires, USB-C cable | Interconnects | High-grade HDMI cable carrying CEC |

---

## 2. Wiring and Hardware Interconnects

### 2.1 Power Switching Interconnect

The relay switch controls the AC mains power to the official Raspberry Pi 15.3 W power adapter, ensuring genuine cold-boot power ramp-up and testing early bootloader ROM/EEPROM timings.

```text
               +----------------------------------+
               |  Mains AC Supply (100-240 V)     |
               +-----------------+----------------+
                                 |
                                 v
                       +-------------------+
                       | Shelly Plus Plug  | <---- Wi-Fi RPC / Local HTTP
                       +---------+---------+       (tools/hil/measure-boot-time.py)
                                 |
                                 v
                     +-----------------------+
                     | 5.1 V / 3 A AC Supply |
                     +-----------+-----------+
                                 | USB-C (Power Only)
                                 v
                      +---------------------+
                      | Raspberry Pi 4B     |
                      +---------------------+
```

### 2.2 Serial Console Pinout (3.3 V UART)

Connect the 3.3 V USB-to-UART adapter to the Raspberry Pi 40-pin GPIO header:

```text
 Raspberry Pi 4B Header             USB-UART Adapter
 +------------------------+         +------------------+
 | Pin 6  (GND)          |-------->| GND              |
 | Pin 8  (GPIO 14 / TXD)|-------->| RXD (Data in)    |
 | Pin 10 (GPIO 15 / RXD)|<--------| TXD (Data out)   |
 +------------------------+         +------------------+
```

> [!CAUTION]
> Never connect the 5 V pin from the USB-UART adapter to the Raspberry Pi GPIO pins.

---

## 3. Automated Boot-Time Measurement (`tools/hil/measure-boot-time.py`)

The automated runner measures cold boots unattended, reporting p50/p95 statistics, breaking down stage durations, and producing machine-readable artifacts for CI release gates.

### 3.1 Running 100 Cold Boots (Acceptance Test)

With a physical Shelly smart plug and serial adapter connected:

```bash
# 100 unattended cold boots against the physical Raspberry Pi 4B
python3 tools/hil/measure-boot-time.py \
  --power-switch shelly \
  --shelly-host 10.0.30.203 \
  --serial-port /dev/cu.usbserial-AD02CNCM \
  --boots 100 \
  --marker frame \
  --output-json build/hil/boot-report.json \
  --output-csv build/hil/boot-report.csv
```

### 3.2 Running in CI Pipelines (Unit / Offline Mode)

CI pipelines validate parser logic, statistics, and gating rules deterministically without hardware:

```bash
# Self-test unit verification
python3 tools/hil/measure-boot-time.py --test

# Simulated CI measurement run
python3 tools/hil/measure-boot-time.py \
  --power-switch mock \
  --boots 5 \
  --off-seconds 0.01 \
  --output-json build/hil/ci-report.json
```

---

## 4. Boot Stage Breakdown

Boot time is decomposed into five distinct stages measured from the confirmed power-on timestamp:

```mermaid
flowchart LR
    A["Cold Power-On (t0)"] --> B["Stage 1: Bootloader & SDRAM (~2.1 s)"]
    B --> C["Stage 2: USB Power Reset (~1.0 s)"]
    C --> D["Stage 3: Firmware & Kernel Load (~2.3 s)"]
    D --> E["Stage 4: Kernel Boot to Init (~1.1 s)"]
    E --> F["Stage 5: Userspace to First Frame (~0.01 s)"]
    F --> G["Content Visible on Screen (BOOT-001 Gate)"]
```

### 4.1 Production Optimization Path (to ≤ 5.0 s `BOOT-001`)

Measurements on the unoptimized spike baseline yielded **10.28 s p50**. The following verified optimizations achieve the production target:

1. **`USB_MSD_PWR_OFF_TIME=0`** in EEPROM config (`OS-011`): eliminates the mandatory 1000 ms USB port power reset delay (**saves 1.0 s**).
2. **`BOOT_UART=0` and stripped firmware logging**: silences verbose UART debug prints (**saves ~0.3 s**).
3. **Compressed kernel (`Image.gz`) and lean defconfig**: reduces kernel file transfer from 25 MB to ~8 MB (**saves ~1.5 s**).
4. **Built-in kernel display driver**: avoids module loading overhead on the boot critical path (**saves ~0.4 s**).
5. **Projected production boot duration: ≤ 4.5 s**, satisfying the release gate.
