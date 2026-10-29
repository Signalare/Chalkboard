#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# Builds the three spike disk images (#3) from Buildroot's outputs:
#   baseline.img        one boot + one root partition
#   slots-osprefix.img  mechanism A: slot-a/ and slot-b/ in one boot partition
#   slots-autoboot.img  mechanism B: autoboot partition + boot A/B partitions
#
# Called by Buildroot with BINARIES_DIR, BUILD_DIR and HOST_DIR set.

set -euo pipefail

: "${BINARIES_DIR:?}" "${BUILD_DIR:?}"
board_dir="$(dirname "$0")"
work="${BUILD_DIR}/spike-images"
fw="${BINARIES_DIR}/rpi-firmware"
dtb="bcm2711-rpi-4-b.dtb"

rm -rf "$work"
mkdir -p "$work"

cmdline() { # <root partition> <slot> <layout>
  echo "console=ttyAMA0,115200 root=/dev/mmcblk0p$1 rootfstype=ext4 rootwait ro quiet printk.time=1 cb.slot=$2 cb.layout=$3"
}

firmware_to() { # <dir>
  mkdir -p "$1"
  cp "$fw/start4.elf" "$fw/fixup4.dat" "$1/"
}

# EEPROM bootloader update (OS-011): recovery.bin in the first FAT partition
# flashes pieeprom.upd at power-on, then renames itself so it runs only once.
eeprom_to() { # <dir>
  if [[ -d "${BINARIES_DIR}/rpi-eeprom" ]]; then
    cp "${BINARIES_DIR}"/rpi-eeprom/{recovery.bin,pieeprom.upd,pieeprom.sig} "$1/"
  fi
}

kernel_to() { # <dir> <cmdline>
  mkdir -p "$1"
  cp "${BINARIES_DIR}/Image" "${BINARIES_DIR}/${dtb}" "$1/"
  cp -r "$fw/overlays" "$1/"
  echo "$2" > "$1/cmdline.txt"
}

vfat() { # <image> <size in MiB> <label> <source dir>
  rm -f "$1"
  mkdosfs -n "$3" -C "$1" $(($2 * 1024)) > /dev/null
  mcopy -s -i "$1" "$4"/* ::/
}

genimage_run() { # <config>
  local root tmp
  root="$(mktemp -d)"
  tmp="${BUILD_DIR}/genimage.tmp"
  rm -rf "$tmp"
  genimage --rootpath "$root" --tmppath "$tmp" --inputpath "${BINARIES_DIR}" \
    --outputpath "${BINARIES_DIR}" --config "$1"
  rm -rf "$root"
}

# --- baseline -----------------------------------------------------------------
d="$work/baseline"
firmware_to "$d"
cp "$board_dir/config.txt" "$d/config.txt"
kernel_to "$d" "$(cmdline 2 a baseline)"
eeprom_to "$d"
vfat "${BINARIES_DIR}/baseline-boot.vfat" 64 BOOT "$d"

cat > "$work/baseline.cfg" <<'EOF'
image baseline.img {
  hdimage { align = 4M }
  partition boot { partition-type = 0xC bootable = "true" image = "baseline-boot.vfat" }
  partition rootfs { partition-type = 0x83 image = "rootfs.ext4" }
}
EOF
genimage_run "$work/baseline.cfg"

# --- mechanism A: os_prefix -------------------------------------------------------
d="$work/osprefix"
firmware_to "$d"
{ cat "$board_dir/config.txt"; echo "os_prefix=slot-a/"; } > "$d/config.txt"
sed 's#^os_prefix=.*#os_prefix=slot-b/#' "$d/config.txt" > "$d/tryboot.txt"
kernel_to "$d/slot-a" "$(cmdline 2 a osprefix)"
kernel_to "$d/slot-b" "$(cmdline 3 b osprefix)"
eeprom_to "$d"
vfat "${BINARIES_DIR}/osprefix-boot.vfat" 128 BOOT "$d"

cat > "$work/osprefix.cfg" <<'EOF'
image slots-osprefix.img {
  hdimage { align = 4M }
  partition boot { partition-type = 0xC bootable = "true" image = "osprefix-boot.vfat" }
  partition rootfs-a { partition-type = 0x83 image = "rootfs.ext4" }
  partition rootfs-b { partition-type = 0x83 image = "rootfs.ext4" }
}
EOF
genimage_run "$work/osprefix.cfg"

# --- mechanism B: autoboot.txt ----------------------------------------------------
d="$work/autoboot"
mkdir -p "$d/autoboot"
printf '[all]\ntryboot_a_b=1\nboot_partition=2\n[tryboot]\nboot_partition=3\n' > "$d/autoboot/autoboot.txt"
eeprom_to "$d/autoboot"
vfat "${BINARIES_DIR}/autoboot.vfat" 16 AUTOBOOT "$d/autoboot"
for s in a b; do
  root=5
  [[ "$s" == b ]] && root=6
  firmware_to "$d/boot-$s"
  cp "$board_dir/config.txt" "$d/boot-$s/config.txt"
  kernel_to "$d/boot-$s" "$(cmdline "$root" "$s" autoboot)"
  vfat "${BINARIES_DIR}/autoboot-boot-$s.vfat" 64 "BOOT_${s^^}" "$d/boot-$s"
done

cat > "$work/autoboot.cfg" <<'EOF'
image slots-autoboot.img {
  hdimage { align = 4M }
  partition autoboot { partition-type = 0xC bootable = "true" image = "autoboot.vfat" }
  partition boot-a { partition-type = 0xC image = "autoboot-boot-a.vfat" }
  partition boot-b { partition-type = 0xC image = "autoboot-boot-b.vfat" }
  partition rootfs-a { partition-type = 0x83 image = "rootfs.ext4" }
  partition rootfs-b { partition-type = 0x83 image = "rootfs.ext4" }
}
EOF
genimage_run "$work/autoboot.cfg"

ls -lh "${BINARIES_DIR}"/*.img
