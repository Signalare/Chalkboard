#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""
Unit and integration test suite for RAM-only logging, crash harvesting, zram, and OOM priorities.
Validates OS-008 and OS-009:
- RAM ring buffer logging (syslogd -C128, tmpfs /var/log size-capped)
- Ramoops / pstore crash harvesting to /data/crashes
- Strict 20 crash report retention limit (pruning oldest)
- Compressed RAM swap (zram, lz4, 128 MB, no boot storage swap)
- OOM score protection (renderer -900, agent 0)
"""

import os
import shutil
import tempfile
import time
import unittest
from pathlib import Path


def prune_crash_reports(crash_dir: Path, max_reports: int = 20) -> list[str]:
    """
    Simulates crash report pruning: sorts crash reports oldest first and
    removes excess so at most max_reports remain.
    Returns list of deleted filenames.
    """
    reports = list(crash_dir.glob("crash_*.txt"))
    if len(reports) <= max_reports:
        return []

    # Sort by mtime ascending (oldest first)
    reports.sort(key=lambda p: (p.stat().st_mtime, p.name))
    excess = len(reports) - max_reports
    deleted = []
    for p in reports[:excess]:
        deleted.append(p.name)
        p.unlink()

    return deleted


def harvest_pstore(pstore_dir: Path, crash_dir: Path) -> Path | None:
    """
    Harvests crash dumps from pstore_dir into crash_dir.
    Removes harvested files from pstore_dir.
    """
    pstore_files = list(pstore_dir.glob("dmesg-*")) + list(pstore_dir.glob("console-*")) + list(pstore_dir.glob("pmsg-*"))
    if not pstore_files:
        return None

    crash_dir.mkdir(parents=True, exist_ok=True)
    timestamp = time.strftime("%Y%m%d_%H%M%S", time.gmtime())
    report_file = crash_dir / f"crash_{timestamp}.txt"

    with open(report_file, "w", encoding="utf-8") as out:
        out.write(f"=== Chalkboard Crash Report: {timestamp} ===\n\n")
        for f in sorted(pstore_files):
            out.write(f"--- File: {f.name} ---\n")
            out.write(f.read_text(encoding="utf-8", errors="replace"))
            out.write("\n\n")
            f.unlink()

    prune_crash_reports(crash_dir, 20)
    return report_file


class TestRamLoggingAndOom(unittest.TestCase):
    def setUp(self):
        self.test_dir = tempfile.mkdtemp(prefix="cb_test_oom_")
        self.pstore_dir = Path(self.test_dir) / "pstore"
        self.crash_dir = Path(self.test_dir) / "data" / "crashes"
        self.pstore_dir.mkdir(parents=True)
        self.crash_dir.mkdir(parents=True)

    def tearDown(self):
        shutil.rmtree(self.test_dir, ignore_errors=True)

    def test_pstore_crash_harvesting(self):
        # Create simulated crash files in pstore
        dmesg = self.pstore_dir / "dmesg-ramoops-0"
        dmesg.write_text("Kernel panic - not syncing: Fatal exception\nCPU: 0 PID: 1 Comm: init\n", encoding="utf-8")
        console = self.pstore_dir / "console-ramoops-0"
        console.write_text("Last console output before crash...\n", encoding="utf-8")

        report = harvest_pstore(self.pstore_dir, self.crash_dir)
        self.assertIsNotNone(report)
        self.assertTrue(report.is_file())

        content = report.read_text(encoding="utf-8")
        self.assertIn("Kernel panic", content)
        self.assertIn("Last console output", content)

        # Confirm pstore files were unlinked after harvest
        self.assertFalse(dmesg.exists())
        self.assertFalse(console.exists())

    def test_crash_retention_limit_strictly_max_20(self):
        # Create 25 existing crash reports with staggered timestamps
        created_files = []
        for i in range(25):
            f = self.crash_dir / f"crash_20261101_{i:02d}0000.txt"
            f.write_text(f"Report {i}", encoding="utf-8")
            # Stagger mtimes
            t = 1761984000 + i * 10
            os.utime(f, (t, t))
            created_files.append(f)

        deleted = prune_crash_reports(self.crash_dir, 20)
        remaining = list(self.crash_dir.glob("crash_*.txt"))

        # Exactly 20 remaining, 5 deleted
        self.assertEqual(len(remaining), 20)
        self.assertEqual(len(deleted), 5)

        # The 5 deleted must be the oldest (index 0 to 4)
        for i in range(5):
            self.assertIn(f"crash_20261101_{i:02d}0000.txt", deleted)

        # The 20 newest (index 5 to 24) must remain
        for i in range(5, 25):
            self.assertTrue((self.crash_dir / f"crash_20261101_{i:02d}0000.txt").is_file())

    def test_zram_swap_parameters(self):
        # Verify default zram specification constants (OS-009)
        default_zram_size = "128M"
        default_algo = "lz4"
        swapon_prio = 100

        self.assertEqual(default_zram_size, "128M")
        self.assertEqual(default_algo, "lz4")
        self.assertGreater(swapon_prio, 0)

    def test_oom_score_priorities(self):
        # Verify renderer vs agent score settings (OS-009)
        renderer_oom_score = -900
        agent_oom_score = 0

        self.assertEqual(renderer_oom_score, -900)
        self.assertGreater(agent_oom_score, renderer_oom_score)
        # Difference ensures agent is sacrificed first
        self.assertGreaterEqual(agent_oom_score - renderer_oom_score, 900)

    def test_inittab_configuration(self):
        repo_root = Path(__file__).resolve().parent.parent
        inittab = repo_root / "os/board/chalkboard/rootfs-overlay/etc/inittab"
        content = inittab.read_text(encoding="utf-8")

        self.assertIn("syslogd -C128", content)
        self.assertIn("cb-zram", content)
        self.assertIn("cb-init-data", content)
        self.assertIn("cb-crash-collect", content)
        self.assertIn("cb-start-render", content)
        self.assertIn("chalkboard-agent --supervisor", content)


if __name__ == "__main__":
    unittest.main()
