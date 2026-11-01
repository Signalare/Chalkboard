#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# Generates the universal Chalkboard OS disk image (OS-001..OS-004, OS-010).
# Produces dual-slot A/B MBR layout with dm-verity and persistent data partition.
#
# Called by Buildroot with BINARIES_DIR, BUILD_DIR and HOST_DIR set.

set -euo pipefail

: "${BINARIES_DIR:?}" "${BUILD_DIR:?}"

board_dir="$(cd "$(dirname "$0")" && pwd)"
top_dir="$(cd "${board_dir}/../../.." && pwd)"
mkverity="${top_dir}/tools/mkverity.py"
work="${BUILD_DIR}/chalkboard-image"
fw="${BINARIES_DIR}/rpi-firmware"

rm -rf "$work"
mkdir -p "$work"

echo "=== Chalkboard Post-Image Generator ==="

# 1. Prepare Root Filesystems with dm-verity (OS-010)
squashfs_src="${BINARIES_DIR}/rootfs.squashfs"
if [[ ! -f "$squashfs_src" ]]; then
  echo "Error: ${squashfs_src} not found. Ensure BR2_TARGET_ROOTFS_SQUASHFS=y" >&2
  exit 1
fi

echo "Computing dm-verity Merkle tree for squashfs rootfs..."
verity_out="${work}/verity_params.txt"
python3 "$mkverity" "$squashfs_src" --output "${BINARIES_DIR}/rootfs-a.squashfs" > "$verity_out"

# Both slots start with the identical initial release image
cp "${BINARIES_DIR}/rootfs-a.squashfs" "${BINARIES_DIR}/rootfs-b.squashfs"

sectors=$(grep '^DATA_SECTORS=' "$verity_out" | cut -d= -f2)
data_blocks=$(grep '^DATA_BLOCKS=' "$verity_out" | cut -d= -f2)
hash_start=$(grep '^HASH_START_BLOCK=' "$verity_out" | cut -d= -f2)
root_hash=$(grep '^ROOT_HASH=' "$verity_out" | cut -d= -f2)
salt=$(grep '^SALT=' "$verity_out" | cut -d= -f2)

echo "dm-verity parameters: sectors=${sectors}, blocks=${data_blocks}, root_hash=${root_hash}"

cmdline_for_slot() { # <slot_letter> <part_num>
  local slot="$1"
  local part="$2"
  local dev="PARTUUID=5349474e-0${part}"
  local dm_table="0 ${sectors} verity 1 ${dev} ${dev} 4096 4096 ${data_blocks} ${hash_start} sha256 ${root_hash} ${salt} 1 restart_on_corruption"
  echo "console=ttyAMA0,115200 root=/dev/dm-0 rootfstype=squashfs rootwait dm-mod.waitfor=${dev} ro quiet printk.time=1 dm-mod.create=\"rootfs,,ro,${dm_table}\" cb.slot=${slot} cb.layout=autoboot"
}

vfat() { # <image> <size in MiB> <label> <source dir>
  rm -f "$1"
  mkdosfs -n "$3" -C "$1" $(($2 * 1024)) > /dev/null
  mcopy -s -i "$1" "$4"/* ::/
}

firmware_to() { # <dir>
  mkdir -p "$1"
  if [[ -d "$fw" ]]; then
    for f in bootcode.bin start.elf fixup.dat start4.elf fixup4.dat; do
      if [[ -f "$fw/$f" ]]; then
        cp "$fw/$f" "$1/"
      fi
    done
  fi
}

eeprom_to() { # <dir>
  if [[ -d "${BINARIES_DIR}/rpi-eeprom" ]]; then
    for f in recovery.bin pieeprom.upd pieeprom.sig; do
      if [[ -f "${BINARIES_DIR}/rpi-eeprom/$f" ]]; then
        cp "${BINARIES_DIR}/rpi-eeprom/$f" "$1/"
      fi
    done
  fi
}

kernel_and_dtbs_to() { # <dir> <cmdline_content>
  mkdir -p "$1"
  # Support universal kernel names (OS-002)
  if [[ -f "${BINARIES_DIR}/Image" ]]; then
    cp "${BINARIES_DIR}/Image" "$1/Image"
    cp "${BINARIES_DIR}/Image" "$1/kernel8.img"
    cp "${BINARIES_DIR}/Image" "$1/kernel_2712.img"
  fi
  # Copy all device tree blobs
  cp "${BINARIES_DIR}"/*.dtb "$1/" 2>/dev/null || true
  # Copy overlays
  if [[ -d "$fw/overlays" ]]; then
    cp -r "$fw/overlays" "$1/"
  fi
  echo "$2" > "$1/cmdline.txt"
}

# 2. Build Partition 1: AUTOBOOT (FAT16, 16 MiB)
d="$work/autoboot"
mkdir -p "$d"
printf '[all]\ntryboot_a_b=1\nboot_partition=2\n[tryboot]\nboot_partition=3\n' > "$d/autoboot.txt"
if [[ -f "$fw/bootcode.bin" ]]; then
  cp "$fw/bootcode.bin" "$d/"
fi
vfat "${BINARIES_DIR}/autoboot.vfat" 16 AUTOBOOT "$d"

# 3. Build Partition 2: BOOT_A (FAT32, 128 MiB)
d="$work/boot-a"
firmware_to "$d"
cp "$board_dir/config.txt" "$d/config.txt"
kernel_and_dtbs_to "$d" "$(cmdline_for_slot a 5)"
vfat "${BINARIES_DIR}/boot-a.vfat" 128 BOOT_A "$d"

# 4. Build Partition 3: BOOT_B (FAT32, 128 MiB)
d="$work/boot-b"
firmware_to "$d"
cp "$board_dir/config.txt" "$d/config.txt"
kernel_and_dtbs_to "$d" "$(cmdline_for_slot b 6)"
vfat "${BINARIES_DIR}/boot-b.vfat" 128 BOOT_B "$d"

# 5. Build Initial Data Partition: DATA (128 MiB)
data_img="${BINARIES_DIR}/data.f2fs"
rm -f "$data_img"
# Create 128 MiB sparse image
truncate -s 128M "$data_img"
# Format as F2FS if mkfs.f2fs is available, otherwise zero-fill sparse
if command -v mkfs.f2fs >/dev/null 2>&1; then
  mkfs.f2fs -l DATA -q "$data_img" >/dev/null 2>&1 || true
fi

# 6. Run genimage to produce final chalkboard.img
echo "Running genimage to build chalkboard.img..."
genimage_tmp="${BUILD_DIR}/genimage.tmp"
rm -rf "$genimage_tmp"
root_tmp="$(mktemp -d)"

genimage \
  --rootpath "$root_tmp" \
  --tmppath "$genimage_tmp" \
  --inputpath "${BINARIES_DIR}" \
  --outputpath "${BINARIES_DIR}" \
  --config "${board_dir}/genimage.cfg"

rm -rf "$root_tmp" "$genimage_tmp"

echo "Chalkboard OS image built successfully:"
ls -lh "${BINARIES_DIR}/chalkboard.img"
