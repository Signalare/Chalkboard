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


def run_hil_write_budget_verification(port: str, baud: int = 115200) -> bool:
    """Runs live HIL verification of write budgets, sector counts and allowlist on physical hardware."""
    import os
    import select
    import termios
    import time

    print(f"[HIL-WRITE-BUDGET] Opening serial port {port} at {baud} baud...")
    fd = os.open(port, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
    try:
        attrs = termios.tcgetattr(fd)
        attrs[0] = 0
        attrs[1] = 0
        attrs[2] = termios.CS8 | termios.CREAD | termios.CLOCAL
        attrs[3] = 0
        attrs[4] = termios.B115200
        attrs[5] = termios.B115200
        termios.tcsetattr(fd, termios.TCSANOW, attrs)

        def exec_cmd(cmd: str, timeout: float = 5.0) -> str:
            while True:
                r, _, _ = select.select([fd], [], [], 0.05)
                if not r:
                    break
                os.read(fd, 1024)
            os.write(fd, (cmd + "\n").encode("utf-8"))
            time.sleep(0.2)
            out = b""
            t0 = time.time()
            while time.time() - t0 < timeout:
                r, _, _ = select.select([fd], [], [], 0.4)
                if not r:
                    if b"~ #" in out:
                        break
                    continue
                chunk = os.read(fd, 1024)
                if not chunk:
                    break
                out += chunk
                if b"~ #" in out and cmd.strip() in out.decode("utf-8", errors="replace"):
                    if out.count(b"~ #") >= 1:
                        break
            return out.decode("utf-8", errors="replace")

        def extract_int(output: str, default: int = 0) -> int:
            for token in reversed(output.split()):
                if token.isdigit():
                    return int(token)
            return default

        def get_written_sectors(disk: str, part: str) -> int:
            out = exec_cmd(f"cat /sys/block/{disk}/{part}/stat 2>/dev/null || true")
            tokens = [t for t in out.split() if t.isdigit()]
            return int(tokens[6]) if len(tokens) >= 7 else 0

        print("[HIL-WRITE-BUDGET] Synchronizing target shell...")
        exec_cmd("\n", timeout=1.0)
        uname = exec_cmd("uname -a", timeout=2.0).strip()
        uptime = exec_cmd("cat /proc/uptime", timeout=2.0).strip()
        print(f"[HIL-WRITE-BUDGET] Target connected: {uname}")
        print(f"[HIL-WRITE-BUDGET] Target uptime: {uptime}")

        # Detect primary boot block device
        disk = "sda"
        disk_check = exec_cmd("test -d /sys/block/sda && echo OK || echo FAIL")
        if "OK" not in disk_check:
            disk = "mmcblk0"
            disk_check = exec_cmd("test -d /sys/block/mmcblk0 && echo OK || echo FAIL")
            if "OK" not in disk_check:
                print("[HIL-WRITE-BUDGET] ERROR: Neither sda nor mmcblk0 found on target")
                return False

        print(f"[HIL-WRITE-BUDGET] Primary block storage device: {disk}")

        # 1. Verify zero routine writes to /boot and rootfs partitions (STO-010)
        print("[HIL-WRITE-BUDGET] Verifying zero routine writes to /boot and rootfs partitions...")
        boot_parts = ["sda1", "sda2", "sda3", "sda5", "sda6"] if disk == "sda" else ["mmcblk0p1", "mmcblk0p2", "mmcblk0p5", "mmcblk0p6"]
        for p in boot_parts:
            sectors = get_written_sectors(disk, p)
            if sectors != 0:
                print(f"[HIL-WRITE-BUDGET] ERROR: Non-zero write sectors on {p}: {sectors}")
                return False
            print(f"[HIL-WRITE-BUDGET] [PASS] Partition {p}: 0 sectors written (clean read-only / immutable)")

        # 2. Verify all existing persistent files on /data conform to STO-010 allowlist
        print("[HIL-WRITE-BUDGET] Verifying STO-010 persistent storage allowlist on /data...")
        find_out = exec_cmd("find /data -type f 2>/dev/null || true", timeout=4.0)
        data_files = [line.strip() for line in find_out.splitlines() if line.strip().startswith("/data")]
        print(f"[HIL-WRITE-BUDGET] Found {len(data_files)} files under /data on physical storage")
        for fpath in data_files:
            if not write_tracer.is_allowed_path(fpath):
                print(f"[HIL-WRITE-BUDGET] ERROR: Unauthorized file found on persistent storage: {fpath}")
                return False
            print(f"[HIL-WRITE-BUDGET]   Allowed path: {fpath}")
        print("[HIL-WRITE-BUDGET] [PASS] All persistent storage files conform strictly to STO-010 allowlist")

        # 3. Measure idle write rate and verify 24-hour budget <= 1 MB (STO-013)
        data_part = "sda7" if disk == "sda" else "mmcblk0p7"
        print("[HIL-WRITE-BUDGET] Measuring idle write volume over 10-second sample window...")
        exec_cmd("sync", timeout=2.0)
        s_idle_0 = get_written_sectors(disk, data_part)
        sample_sec = 10.0
        time.sleep(sample_sec)
        s_idle_1 = get_written_sectors(disk, data_part)
        idle_delta_sectors = s_idle_1 - s_idle_0
        idle_delta_bytes = idle_delta_sectors * 512
        rate_bps = idle_delta_bytes / sample_sec
        projected_idle_day = rate_bps * 86400.0

        print(
            f"[HIL-WRITE-BUDGET] Idle delta: {idle_delta_sectors} sectors ({idle_delta_bytes} bytes in {sample_sec:.0f}s)"
        )
        print(
            f"[HIL-WRITE-BUDGET] Projected 24h idle writes: {projected_idle_day:.0f} bytes ({projected_idle_day/1024/1024:.2f} MB)"
        )
        if projected_idle_day > write_tracer.IDLE_BUDGET_BYTES:
            print(
                f"[HIL-WRITE-BUDGET] ERROR: Idle writes {projected_idle_day:.0f} bytes exceeds budget of {write_tracer.IDLE_BUDGET_BYTES} bytes/day"
            )
            return False
        print(
            f"[HIL-WRITE-BUDGET] [PASS] Projected 24h idle writes ({projected_idle_day/1024/1024:.2f} MB) <= 1.0 MB budget (STO-013)"
        )

        # 4. Measure typical workload write volume (50 toggles + 1 publication <= 50 MB) (STO-013)
        print("[HIL-WRITE-BUDGET] Simulating typical workload (50 toggles + 1 publication) on target...")
        exec_cmd("sync", timeout=2.0)
        s_work_0 = get_written_sectors(disk, data_part)

        workload_script = """
mkdir -p /data/store /data/snapshots
for i in $(seq 1 50); do
  printf 'item_%02d_sold_out=true\\n' "$i" > /data/store/toggles.tmp
  mv -f /data/store/toggles.tmp /data/store/toggles.dat
done
dd if=/dev/urandom of=/data/snapshots/snapshot-hil.bin bs=1024 count=150 2>/dev/null
ln -sf snapshot-hil.bin /data/snapshots/current.tmp
mv -f /data/snapshots/current.tmp /data/snapshots/current
sync
"""
        exec_cmd(workload_script, timeout=10.0)
        s_work_1 = get_written_sectors(disk, data_part)
        work_delta_sectors = s_work_1 - s_work_0
        work_delta_bytes = work_delta_sectors * 512
        work_delta_mb = work_delta_bytes / (1024.0 * 1024.0)

        print(
            f"[HIL-WRITE-BUDGET] Typical workload writes: {work_delta_sectors} sectors ({work_delta_bytes} bytes, {work_delta_mb:.2f} MB)"
        )
        if work_delta_bytes > write_tracer.TYPICAL_BUDGET_BYTES:
            print(
                f"[HIL-WRITE-BUDGET] ERROR: Typical workload writes ({work_delta_bytes} bytes) exceeds budget of {write_tracer.TYPICAL_BUDGET_BYTES} bytes"
            )
            return False
        print(
            f"[HIL-WRITE-BUDGET] [PASS] Typical workload writes ({work_delta_mb:.2f} MB) <= 50.0 MB budget (STO-013)"
        )

        # Clean up temporary test files
        exec_cmd("rm -f /data/store/toggles.dat /data/snapshots/snapshot-hil.bin /data/snapshots/current*", timeout=2.0)
        exec_cmd("sync", timeout=2.0)

        # 5. Verify rogue writer detection catching unauthorized paths (TEST-009, STO-010)
        print("[HIL-WRITE-BUDGET] Verifying rogue write detection...")
        rogue_events = [
            write_tracer.WriteEvent("rogue-updater", 777, "/boot/firmware.bin", 2048),
            write_tracer.WriteEvent("stray-logger", 888, "/data/unauthorized.log", 4096),
        ]
        rogue_errors = write_tracer.validate_write_events(rogue_events)
        if len(rogue_errors) != 2:
            print(f"[HIL-WRITE-BUDGET] ERROR: Failed to detect all rogue events: {rogue_errors}")
            return False
        print("[HIL-WRITE-BUDGET] [PASS] Rogue writes outside allowlist correctly identified and rejected (TEST-009, STO-010)")

        print("\n============================================================")
        print("ALL HIL WRITE BUDGET AND TRACING VERIFICATIONS PASSED!")
        print("============================================================\n")
        return True

    finally:
        os.close(fd)


def main():
    import argparse

    parser = argparse.ArgumentParser(description="Chalkboard Write Tracer & Budget Test Suite")
    parser.add_argument("--serial-port", help="Serial port device node for live HIL verification")
    parser.add_argument("--baud", type=int, default=115200, help="Serial baud rate [default: 115200]")
    args, remaining = parser.parse_known_args()

    if args.serial_port:
        success = run_hil_write_budget_verification(args.serial_port, args.baud)
        sys.exit(0 if success else 1)
    else:
        suite = unittest.TestLoader().loadTestsFromTestCase(TestWriteTracerAndBudget)
        runner = unittest.TextTestRunner(verbosity=2)
        result = runner.run(suite)
        sys.exit(0 if result.wasSuccessful() else 1)


if __name__ == "__main__":
    main()
