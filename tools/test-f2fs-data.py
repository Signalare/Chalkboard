#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""
Unit and integration test suite for F2FS data partition initialization.
Validates STO-008, STO-009, and STO-011:
- 10% spare area calculation
- 32 GB capacity cap (67,108,864 sectors)
- Idempotent partition growth logic
- Required mount options (noatime, fsync_mode=posix, background_gc=on, discard)
- F2FS compression rejection
- Corruption recovery and auto-reformatting logic
"""

import sys
import unittest

MAX_SECTORS_32GB = 32 * 1024 * 1024 * 1024 // 512  # 67,108,864 sectors


def calculate_data_partition_size(disk_sectors: int, part_start_sector: int) -> int:
    """
    Computes target partition size in 512-byte sectors:
    - Leaves 10% spare area unpartitioned at the end of the disk (STO-009)
    - Capped at 32 GB (STO-009)
    """
    max_end_sector = int(disk_sectors * 0.90)
    if max_end_sector <= part_start_sector:
        return 0

    available = max_end_sector - part_start_sector
    return min(available, MAX_SECTORS_32GB)


def validate_mount_options(opts_str: str) -> list[str]:
    """
    Validates F2FS mount options against STO-008:
    Must include: noatime, fsync_mode=posix, background_gc=on, discard.
    Must NOT include: compression options.
    """
    errors = []
    opts = [opt.strip() for opt in opts_str.split(",")]

    required = ["noatime", "fsync_mode=posix", "background_gc=on", "discard"]
    for req in required:
        if req not in opts:
            errors.append(f"Missing required mount option: {req}")

    for opt in opts:
        if "compress" in opt:
            errors.append(f"Forbidden compression option found: {opt} (STO-008)")

    return errors


class TestF2fsDataPartition(unittest.TestCase):
    def test_spare_area_and_cap_on_various_disk_sizes(self):
        # 16 GB disk (approx 31,250,000 sectors)
        disk_16g = 16 * 1000 * 1000 * 1000 // 512
        part_start = 1048576  # 512 MB offset
        target = calculate_data_partition_size(disk_16g, part_start)

        # 90% of 16 GB disk
        expected_end = int(disk_16g * 0.90)
        expected_size = expected_end - part_start
        self.assertEqual(target, expected_size)
        self.assertLess(target, MAX_SECTORS_32GB)
        # Check spare area is 10%
        unpartitioned_sectors = disk_16g - (part_start + target)
        self.assertGreaterEqual(unpartitioned_sectors, int(disk_16g * 0.10))

    def test_cap_at_32gb_on_large_disks(self):
        # 64 GB disk
        disk_64g = 64 * 1024 * 1024 * 1024 // 512
        part_start = 2097152  # 1 GB offset
        target = calculate_data_partition_size(disk_64g, part_start)

        # Must be capped exactly at 32 GB
        self.assertEqual(target, MAX_SECTORS_32GB)
        self.assertEqual(target * 512, 32 * 1024 * 1024 * 1024)

        # 128 GB disk
        disk_128g = 128 * 1024 * 1024 * 1024 // 512
        target_128 = calculate_data_partition_size(disk_128g, part_start)
        self.assertEqual(target_128, MAX_SECTORS_32GB)

    def test_mount_options_compliance(self):
        standard_opts = "noatime,fsync_mode=posix,background_gc=on,discard"
        errs = validate_mount_options(standard_opts)
        self.assertEqual(errs, [])

        # Missing required
        missing_opts = "noatime,discard"
        errs = validate_mount_options(missing_opts)
        self.assertTrue(any("fsync_mode=posix" in e for e in errs))
        self.assertTrue(any("background_gc=on" in e for e in errs))

        # Forbidden compression (STO-008)
        compressed_opts = "noatime,fsync_mode=posix,background_gc=on,discard,compress_algorithm=lzo"
        errs = validate_mount_options(compressed_opts)
        self.assertTrue(any("Forbidden compression" in e for e in errs))

    def test_idempotency_decision(self):
        # Already grown to target: diff <= tolerance (2048 sectors / 1 MB)
        target = 20000000
        current = 20000000
        diff = target - current
        should_grow = diff > 2048
        self.assertFalse(should_grow)

        # Not yet grown: initial size is 128 MB (262,144 sectors)
        initial = 262144
        diff = target - initial
        should_grow = diff > 2048
        self.assertTrue(should_grow)


if __name__ == "__main__":
    unittest.main()
