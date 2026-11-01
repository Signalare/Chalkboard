#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""
Unit and integration test suite for Write Tracer and Budget Validator.
Validates TEST-009, STO-010, and STO-013:
- Strict STO-010 persistent storage allowlist verification
- Rogue writer failure detection with process attribution
- 24h idle write budget (<= 1 MB)
- 24h typical write budget (50 toggles, 1 publication <= 50 MB)
"""

import subprocess
import sys
import unittest
from pathlib import Path

# Add tools directory to path
TOOLS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(TOOLS_DIR))

import write_tracer


class TestWriteTracerAndBudget(unittest.TestCase):
    def test_allowlist_permits_authorized_paths(self):
        allowed_paths = [
            "/data/store/chalkboard.db",
            "/data/store/chalkboard.db-wal",
            "/data/blobs/e3/b0/e3b0c442.bin",
            "/data/snapshots/snapshot-a7f01fbf.bin",
            "/data/snapshots/current",
            "/data/crashes/crash_20261101_080000.txt",
            "/data/keys/node-identity.key",
            "/data/backups/local-backup-001.db",
            "/data/time/hlc-reservation.dat",
            # Temporary RAM paths (OS-004, OS-009)
            "/tmp/test.tmp",
            "/run/chalkboard/render.sock",
            "/var/log/syslog",
        ]
        events = [
            write_tracer.WriteEvent(process_name="chalkboard-agent", pid=4200, path=p, bytes_written=4096)
            for p in allowed_paths
        ]
        errors = write_tracer.validate_write_events(events)
        self.assertEqual(errors, [], "All authorized paths must be permitted")

    def test_rogue_writer_detected_and_reported(self):
        # Inject an unauthorized write outside the allowlist
        rogue_event = write_tracer.WriteEvent(
            process_name="rogue-daemon",
            pid=9999,
            path="/data/stray_debug.log",
            bytes_written=8192,
        )
        errors = write_tracer.validate_write_events([rogue_event])
        self.assertEqual(len(errors), 1)
        err = errors[0]
        self.assertIn("ROGUE WRITE DETECTED", err)
        self.assertIn("rogue-daemon", err)
        self.assertIn("9999", err)
        self.assertIn("/data/stray_debug.log", err)

    def test_rogue_writes_to_boot_and_system_partitions_detected(self):
        forbidden_targets = [
            ("/boot/config.txt.bak", "bad-updater", 101),
            ("/etc/bad_config.conf", "rogue-agent", 102),
            ("/data/unauthorized_leak.bin", "sneaky-proc", 103),
        ]
        for path, proc, pid in forbidden_targets:
            ev = write_tracer.WriteEvent(proc, pid, path, 1024)
            errs = write_tracer.validate_write_events([ev])
            self.assertEqual(len(errs), 1, f"Should catch rogue write to {path}")
            self.assertIn(proc, errs[0])
            self.assertIn(path, errs[0])

    def test_idle_and_typical_daily_budgets(self):
        # Realistic idle day: ~150 KB (SQLite WAL checkpoints + HLC reservations)
        idle_bytes = 150 * 1024
        # Realistic typical day: ~3.2 MB (50 toggles + 1 publication binary snapshot + render cache)
        typical_bytes = 3200 * 1024

        errors = write_tracer.check_daily_budgets(idle_bytes, typical_bytes)
        self.assertEqual(errors, [], "Realistic Chalkboard daily writes must easily fit within budgets")

        # Exceed idle budget (1 MB + 10 bytes)
        over_idle = 1048576 + 10
        errors = write_tracer.check_daily_budgets(over_idle, typical_bytes)
        self.assertTrue(any("Idle writes" in e for e in errors))

        # Exceed typical budget (50 MB + 100 bytes)
        over_typical = 50 * 1024 * 1024 + 100
        errors = write_tracer.check_daily_budgets(idle_bytes, over_typical)
        self.assertTrue(any("Typical writes" in e for e in errors))

    def test_cli_invocation_pass_and_fail(self):
        tracer_bin = TOOLS_DIR / "write_tracer.py"

        # Test CLI check-budget passing
        res = subprocess.run([
            sys.executable, str(tracer_bin), "check-budget",
            "--idle-bytes", "204800",
            "--typical-bytes", "4194304"
        ], capture_output=True, text=True)
        self.assertEqual(res.returncode, 0)
        self.assertIn("PASS", res.stdout)

        # Test CLI check-budget exceeding
        res_fail = subprocess.run([
            sys.executable, str(tracer_bin), "check-budget",
            "--idle-bytes", "2097152",  # 2 MB > 1 MB
            "--typical-bytes", "4194304"
        ], capture_output=True, text=True)
        self.assertEqual(res_fail.returncode, 1)
        self.assertIn("BUDGET EXCEEDED", res_fail.stderr)

        # Test CLI check-events detecting rogue write
        res_rogue = subprocess.run([
            sys.executable, str(tracer_bin), "check-events",
            "--process", "malicious-script",
            "--pid", "666",
            "--path", "/boot/unauthorized.sh",
            "--bytes", "512"
        ], capture_output=True, text=True)
        self.assertEqual(res_rogue.returncode, 1)
        self.assertIn("ROGUE WRITE DETECTED", res_rogue.stderr)


if __name__ == "__main__":
    unittest.main()
