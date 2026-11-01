#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""
Chalkboard OS Image & Configuration Validator.

Validates the universal Buildroot configuration, board fragments, genimage layout,
dm-verity setup, and (optionally) generated disk images against Chalkboard
specifications (OS-001..OS-004, OS-007, OS-010, BOOT-004, BOOT-007).
"""

import argparse
import os
import struct
import sys
from pathlib import Path


def check_config_files(repo_root: Path) -> list[str]:
    """Validates Buildroot external tree configs and board fragments."""
    errors = []

    defconfig_path = repo_root / "os/configs/chalkboard_defconfig"
    if not defconfig_path.is_file():
        errors.append(f"Missing {defconfig_path}")
    else:
        content = defconfig_path.read_text(encoding="utf-8")
        required_defconfig = [
            "BR2_aarch64=y",
            "BR2_TARGET_ROOTFS_SQUASHFS=y",
            "BR2_TARGET_ROOTFS_SQUASHFS_XZ=y",
            "BR2_ROOTFS_POST_BUILD_SCRIPT=",
            "BR2_ROOTFS_POST_IMAGE_SCRIPT=",
            "board/chalkboard/linux.fragment",
            "board/chalkboard/config.txt",
            "board/chalkboard/bootconf.txt",
        ]
        for req in required_defconfig:
            if req not in content:
                errors.append(f"defconfig missing required setting: {req}")

    fragment_path = repo_root / "os/board/chalkboard/linux.fragment"
    if not fragment_path.is_file():
        errors.append(f"Missing {fragment_path}")
    else:
        content = fragment_path.read_text(encoding="utf-8")
        required_kernel = [
            "CONFIG_DRM_VC4=y",
            "CONFIG_BLK_DEV_DM=y",
            "CONFIG_DM_INIT=y",
            "CONFIG_DM_VERITY=y",
            "CONFIG_SQUASHFS=y",
            "CONFIG_F2FS_FS=y",
            "CONFIG_BCM2835_WDT=y",
            "CONFIG_PSTORE=y",
            "CONFIG_ZRAM=y",
        ]
        for req in required_kernel:
            if req not in content:
                errors.append(f"linux.fragment missing required kernel config: {req}")

    config_txt_path = repo_root / "os/board/chalkboard/config.txt"
    if not config_txt_path.is_file():
        errors.append(f"Missing {config_txt_path}")
    else:
        content = config_txt_path.read_text(encoding="utf-8")
        required_config = [
            "arm_64bit=1",
            "boot_delay=0",
            "disable_splash=1",
            "kernel8.img",
            "kernel_2712.img",
            "dtoverlay=vc4-kms-v3d",
        ]
        for req in required_config:
            if req not in content:
                errors.append(f"config.txt missing required firmware setting: {req}")

    bootconf_path = repo_root / "os/board/chalkboard/bootconf.txt"
    if not bootconf_path.is_file():
        errors.append(f"Missing {bootconf_path}")
    else:
        content = bootconf_path.read_text(encoding="utf-8")
        required_bootconf = [
            "BOOT_UART=0",
            "BOOT_ORDER=0xf41",
            "USB_MSD_PWR_OFF_TIME=0",
            "DISABLE_HDMI=1",
        ]
        for req in required_bootconf:
            if req not in content:
                errors.append(f"bootconf.txt missing required setting: {req}")

    genimage_path = repo_root / "os/board/chalkboard/genimage.cfg"
    if not genimage_path.is_file():
        errors.append(f"Missing {genimage_path}")
    else:
        content = genimage_path.read_text(encoding="utf-8")
        required_genimage = [
            "partition autoboot",
            "partition boot-a",
            "partition boot-b",
            "partition rootfs-a",
            "partition rootfs-b",
            "partition data",
        ]
        for req in required_genimage:
            if req not in content:
                errors.append(f"genimage.cfg missing required partition: {req}")

    return errors


def check_mbr_image(image_path: Path) -> list[str]:
    """Validates raw MBR image structure."""
    errors = []
    if not image_path.is_file():
        return [f"File not found: {image_path}"]

    size = image_path.stat().st_size
    if size < 512:
        return [f"Image file too small ({size} bytes)"]

    with open(image_path, "rb") as f:
        mbr = f.read(512)

    # Check MBR boot signature
    magic = mbr[510:512]
    if magic != b"\x55\xaa":
        errors.append(f"Invalid MBR signature: {magic.hex()} (expected 55aa)")

    # Read 4 primary partition table entries (16 bytes each, starting at 446)
    partitions = []
    for i in range(4):
        offset = 446 + i * 16
        entry = mbr[offset : offset + 16]
        status, _chs_start, p_type, _chs_end, lba_start, num_sectors = struct.unpack(
            "<B3sB3sII", entry
        )
        if p_type != 0:
            partitions.append(
                {
                    "index": i + 1,
                    "status": status,
                    "type": p_type,
                    "lba_start": lba_start,
                    "num_sectors": num_sectors,
                }
            )

    if len(partitions) < 4:
        errors.append(
            f"Expected 4 primary MBR entries (including extended partition), found {len(partitions)}"
        )

    # First partition should be bootable
    if partitions and partitions[0]["status"] != 0x80:
        errors.append(
            f"First partition (autoboot) is not marked active/bootable (status: {hex(partitions[0]['status'])})"
        )

    return errors


def main():
    parser = argparse.ArgumentParser(description="Chalkboard OS image validator")
    parser.add_argument("--image", help="Path to chalkboard.img (optional)")
    parser.add_argument(
        "--repo-root",
        default=Path(__file__).resolve().parent.parent,
        type=Path,
        help="Chalkboard repository root directory",
    )

    args = parser.parse_args()

    print("Checking Chalkboard OS configuration files...")
    cfg_errors = check_config_files(args.repo_root)
    if cfg_errors:
        for err in cfg_errors:
            print(f"FAIL: {err}", file=sys.stderr)
        sys.exit(1)
    print("PASS: All Buildroot external tree configs and board fragments ok.")

    if args.image:
        image_path = Path(args.image)
        print(f"Checking disk image {image_path}...")
        img_errors = check_mbr_image(image_path)
        if img_errors:
            for err in img_errors:
                print(f"FAIL: {err}", file=sys.stderr)
            sys.exit(1)
        print("PASS: Disk image MBR layout verified.")

    sys.exit(0)


if __name__ == "__main__":
    main()
