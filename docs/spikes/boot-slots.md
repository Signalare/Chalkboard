# Spike: A/B boot-slot mechanism and boot time

- **Issue:** [#3](https://github.com/Signalare/Chalkboard/issues/3)
- **Requirements:** UPD-004, OS-003, OS-005, OS-011, BOOT-001, BOOT-004, BOOT-007
- **Status:** Pi 4B done (both mechanisms); Zero 2 W and Pi 5 pending — decision provisional

## Questions to answer

1. Which slot-selection mechanism works on all three SoC families (UPD-004)?
   - **A — `os_prefix`:** one FAT boot partition with `slot-a/` and `slot-b/`
     directories (kernel, device trees, overlays, `cmdline.txt`). `config.txt`
     selects the committed slot; `tryboot.txt` selects the other one.
   - **B — `autoboot.txt`:** a tiny first FAT partition holding only
     `autoboot.txt`, which selects boot partition A or B; `tryboot_a_b=1`
     makes tryboot use the other partition's normal `config.txt`.
2. Does `tryboot` behave as a one-shot on each board, so a failed boot falls
   back to the committed slot on the next (watchdog) reboot?
3. Is the slot commit a single small write that survives power loss at any
   moment (OS-005)?
4. How long does each boot stage take — bootloader, firmware, kernel,
   userspace — and is ≤ 5 s to first frame achievable (BOOT-001)?
5. What does the Chalkboard EEPROM configuration (OS-011) save on BCM2711 and
   BCM2712?

## Method

### Test images

Built from `os/configs/spike_rpi4_defconfig` (Buildroot 2026.02.3, Pi 4B):

| Image | Layout |
|---|---|
| `baseline.img` | One boot partition + one root partition, for boot-time baseline |
| `slots-osprefix.img` | Mechanism A: `p1` boot (`slot-a/`, `slot-b/`), `p2` root A, `p3` root B |
| `slots-autoboot.img` | Mechanism B: `p1` autoboot, `p2` boot A, `p3` boot B, `p5` root A, `p6` root B (MBR extended partition) |

Every image runs BusyBox init. The first `sysinit` action loads the display
driver and runs `cb-firstframe`, which sets a mode through DRM/KMS, fills the
screen (blue in slot A, green in slot B) and prints `CB_FIRST_FRAME` with the
kernel's boot clock on the serial console. A root shell on the serial console
provides:

| Command | Purpose |
|---|---|
| `cb-slot` | Print the running slot and whether this is a tryboot |
| `cb-tryboot` | Reboot once into the other slot (`reboot "0 tryboot"`) |
| `cb-commit` | Make the running slot the default (the single small boot-partition write) |
| `cb-commit-loop` | Commit repeatedly, so power cuts land mid-write |

The root filesystem is ext4 mounted read-only. squashfs and dm-verity are
for #5 and don't affect slot selection.

### Lab setup

- Pi 4B with a throwaway microSD card and an HDMI display.
- 3.3 V USB-UART adapter on GPIO 14/15 (pins 8 and 10) and GND (pin 6).
- Power through a Shelly plug controlled over its local RPC API.
- `tools/spike/spike.py` on the Mac switches power, timestamps every serial
  line, detects `CB_FIRST_FRAME` and records results.

Boot time is measured from the moment the plug confirms power-on to the
`CB_FIRST_FRAME` line, so it includes everything except the display's own
input lag. `uart_2ndstage=1` makes the firmware print its own progress, which
splits the time into stages.

### Test plan per mechanism

1. Boot 100 times from cold; record p50/p95 to first frame.
2. `cb-tryboot` into the other slot, confirm the colour and `cb-slot`, power
   cut, confirm the next boot returns to the committed slot.
3. `cb-tryboot`, `cb-commit`, power cut, confirm the next boot stays on the
   new slot.
4. 200 power cuts at random moments during `cb-commit-loop`; every following
   boot must reach `CB_FIRST_FRAME` in a valid slot.

## Results

### Pi 4B (BCM2711)

Board revision `c03111` (Pi 4B 4 GB), SanDisk-class 8 GB microSD, no HDMI
display attached yet, so boot time is measured to `CB_INIT_START` (init
starting) instead of the first frame.

#### Bootloader (OS-011)

- The board shipped with an EEPROM bootloader built **2021-04-29**. With it,
  **24.4 s** passed between power-on and the firmware (`start4.elf`) starting.
- Updating it through the OS-011 path worked first time: `recovery.bin` on the
  boot partition verified `pieeprom.sig`, flashed `pieeprom.upd`
  (`pieeprom-2026-09-23` from the stable channel, release `v2026.09.25-2712`,
  with the spike configuration embedded), verified it, renamed itself to
  `RECOVERY.000` and reset. Later boots saw the same timestamp and skipped
  the update (`SELF-UPDATE … skip`). Flashing took about 5 s.
- With `BOOT_UART=1` the bootloader logs its own steps, which made the
  timing breakdown below possible.

#### Boot time (bootloader 2026-09-23, unoptimised kernel)

10 cold boots, power-on (plug confirmed) → `CB_INIT_START`:
**p50 10.31 s, p95 10.34 s**, spread about ±40 ms. One boot with a retried
plug command was excluded.

| Stage | Duration | Notes |
|---|---|---|
| Bootloader stage 1 + SDRAM init | ≈ 2.1 s | after a cold power-on; fixed hardware init |
| Bootloader USB port power reset | ≈ 1.0 s | `Reset USB port-power 1000 ms`, although booting from SD |
| Bootloader: SD init, partition scan, load `start4.elf` | ≈ 0.5 s | |
| Firmware `start4.elf` | ≈ 3.8 s | 2.2 s of it reading the 25 MB uncompressed kernel `Image` |
| Linux kernel → init | 1.16–1.19 s | `quiet`, stock `bcm2711_defconfig` + built-in vc4 |
| Remainder | ≈ 1.5 s | cold-start power ramp and plug relay latency |

Obvious savings for #5, to be measured:

- `USB_MSD_PWR_OFF_TIME=0` in the bootloader config (≈ 1 s).
- A smaller kernel configuration and/or a compressed kernel image (part of the 2.2 s load).
- `BOOT_UART=0` and fewer firmware log lines in production.
- Fewer overlays and no EDID probing on the second HDMI port where unused.

#### Mechanism A — `os_prefix`

- The firmware loads device tree, overlays, `cmdline.txt` and kernel from the
  `os_prefix` directory: confirmed in the firmware log
  (`Loaded 'slot-a/bcm2711-rpi-4-b.dtb'`, `slot-a/Image`).
- Slot checks (`spike.py slots`): **5/5 passed** — cold boot uses the committed
  slot; `cb-tryboot` boots the other slot once with the tryboot flag set; a
  power cut during the tryboot falls back to the committed slot; after
  `cb-commit` and a power cut the new slot stays.
- **Commit by rename — rejected.** The first power-cut run committed by
  writing `config.txt.new`, flushing and renaming it over `config.txt`.
  All 200 boots recovered, but the FAT did not: interrupted commits left
  stray `CONFIG~1.NEW` files and lost cluster chains, and the kernel then
  reported `fat_free_clusters: deleting FAT entry beyond EOF` and switched the
  partition to read-only, which would make every later commit fail. The
  firmware still read the damaged partition, so this would have gone
  unnoticed at boot.
- **Commit by single-sector raw write — adopted.** `config.txt` keeps a fixed
  layout in which the slots differ in one character (`os_prefix=slot-a/` vs
  `slot-b/`). `cb-commit` mounts the boot partition read-only, maps the file
  offset to the device with `FIBMAP`, rewrites that one 512-byte sector on the
  raw device and flushes it. It refuses to write unless the target byte is a
  valid slot character and reads the value back afterwards. No directory
  entry, timestamp, FAT or cluster allocation is ever written.
- Slot checks with the raw commit: **5/5 in both directions** (A→B and B→A).
- Power cuts at random moments during `cb-commit-loop` with the raw commit:
  **200/200 recovered, and `fsck.fat -n` reported the boot partition clean
  after every cut.** Both slots booted after cuts (A 90, B 66 in the last 156),
  confirming that cuts landed mid-flip and either outcome is valid.
  Boot time over those cuts: p50 10.39 s, p95 10.43 s, max 10.48 s.

#### Mechanism B — `autoboot.txt`

- Layout: `p1` autoboot (16 MiB FAT, only `autoboot.txt`), `p2`/`p3` boot A/B
  (FAT), `p5`/`p6` root A/B in an MBR extended partition. The 2026-09-23
  bootloader follows `autoboot.txt` (`boot_partition=2`, tryboot → 3).
- Commit: the same raw single-sector write, here of the two `boot_partition`
  characters (`[all]` and `[tryboot]`), which lie in one sector.
- Slot checks: **5/5 passed** on the first run.
- Power cuts during `cb-commit-loop`: **200/200 recovered, `fsck.fat -n` clean
  after every cut**; slots after cuts A 95 / B 105.
- Boot time over those cuts: p50 **10.28 s**, p95 10.34 s, max 13.85 s (one
  occurrence of the early-bootloader stall described below).

#### Comparison on the Pi 4B

| | A — `os_prefix` | B — `autoboot.txt` |
|---|---|---|
| Slot checks | 5/5 both directions | 5/5 |
| 200 power cuts during commits | 200/200, FAT clean | 200/200, FAT clean |
| Boot time p50 / p95 | 10.39 / 10.43 s | 10.28 / 10.34 s |
| What the commit writes | one sector of `config.txt` in the boot partition that holds **both** slots' kernels | one sector of `autoboot.txt` in a partition that holds nothing else |
| What an update install writes | new files into the **shared** FAT partition (allocation, directory entries) next to the running slot | a whole raw FAT image onto the **inactive** slot's own partition; the running slot is never touched |
| Partitions | 3 (fits MBR primaries) | 5 + extended partition |
| Bootloader requirement | any firmware with `tryboot` | bootloader with `autoboot.txt` support (Pi 4/5 EEPROM, current releases) |

Both survive power loss during the commit. The difference is the update
install: with A, writing a new slot means FAT metadata writes in the very
partition the running slot boots from, which the rename experiment showed is
exactly what power loss damages. With B, the inactive slot is written as a raw
partition image and the active one is never touched.

**Provisional recommendation: B**, *provided* the BCM2837 boards (Zero 2 W,
Pi 3) support `autoboot.txt`. They have no EEPROM bootloader — their first
stage comes from `bootcode.bin` on the card — and Raspberry Pi's
documentation does not say whether it implements `autoboot.txt`. If it does
not, A is the only universal mechanism and the update install must avoid FAT
metadata writes in the shared partition (for example by pre-allocating both
slots' files at full size and overwriting them in place). This is the first
thing to test when a Zero 2 W is available.

#### Lab setup: reflashing over Wi-Fi

To switch images without moving the card, the spike images can join a lab
Wi-Fi (credentials in a local file outside the repository; the image stores
only the PSK hash) and `cb-reflash` streams an image from the Mac onto the
card from a RAM chroot, verifies its SHA-256 on the card and reboots.
`slots-autoboot.img` (415 MB) was installed this way in 233 s. Lessons from
building it: Buildroot does not rebuild packages after config changes (`kmod`
stayed without xz support until a `kmod-dirclean`), BusyBox `tar` has no `-z`,
and mounting the shared `devtmpfs` into a scratch directory that is later
removed with `rm -rf` deletes the system's device nodes — `cb-reflash` now
creates private device nodes and never deletes through a mount.

#### Other observations

- During the rename-commit run, 10 of 182 boots spent about 3.5 s longer in
  the bootloader before its first message (before the SD card is read), all
  with `power-on-reset 1`. None occurred in the 200 cuts of the raw-commit
  run of mechanism A, and 1 in 200 with mechanism B (13.85 s). Cause unknown (relay bounce, power ramp or boot ROM); the HIL rig (#4)
  should compare a bench power switch with the smart plug.
- The Mac occasionally lost serial output right after power-on (twice in
  about 450 boots): the board booted normally but the marker never arrived.
  The harness now falls back to checking the node's uptime. The HIL rig
  should not rely on the serial console alone to detect a boot.
- BusyBox `grep` has no `-b` (byte offsets); the first raw-commit script
  therefore patched the wrong byte (in a comment) — caught by the slot
  checks. Offsets are now computed with `awk` and every write is guarded.
- BusyBox `sync` ignores file arguments and flushes everything. Harmless for
  the spike; the agent will call `fsync` on the files it writes.
- A Wi-Fi smart plug is not a dependable lab power switch: it needed retries
  regularly and once hung for minutes. The harness now retries for up to
  3 minutes and marks timings after a retry as unreliable.

### Zero 2 W (BCM2837)

*Pending — board not yet available.*

### Pi 5 (BCM2712)

*Pending — board not yet available.*

## Decision

*Pending.* The chosen mechanism and any per-board boot-time exception will be
recorded in SPEC.md (UPD-004, §21) in a `spec` pull request.
