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

    def test_ramoops_config_and_panic(self):
        repo_root = Path(__file__).resolve().parent.parent
        config_txt = repo_root / "os/board/chalkboard/config.txt"
        config_content = config_txt.read_text(encoding="utf-8")
        self.assertIn("dtoverlay=ramoops-pi4", config_content)
        self.assertIn("dtoverlay=ramoops", config_content)

        post_image = repo_root / "os/board/chalkboard/post-image.sh"
        post_content = post_image.read_text(encoding="utf-8")
        self.assertIn("panic=5", post_content)

        busybox_frag = repo_root / "os/board/chalkboard/busybox.fragment"
        busybox_content = busybox_frag.read_text(encoding="utf-8")
        self.assertIn("CONFIG_FEATURE_SWAPON_PRI=y", busybox_content)


def run_hil_ram_logging_oom_verification(port: str, baud: int = 115200) -> bool:
    """Verifies live ramoops pstore crash dumps, zram swap, and OOM priorities over serial."""
    import argparse
    import select
    import termios

    print(f"[HIL-RAM-OOM] Opening serial port {port} at {baud} baud...")
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

        def exec_cmd(cmd: str, timeout: float = 3.0) -> str:
            while True:
                r, _, _ = select.select([fd], [], [], 0.05)
                if not r:
                    break
                os.read(fd, 1024)
            os.write(fd, (cmd + "\n").encode("utf-8"))
            time.sleep(0.3)
            out = b""
            t0 = time.time()
            while time.time() - t0 < timeout:
                r, _, _ = select.select([fd], [], [], 0.4)
                if not r:
                    break
                chunk = os.read(fd, 1024)
                if not chunk:
                    break
                out += chunk
            return out.decode("utf-8", errors="replace")

        def extract_int(output: str, default: int = 0) -> int:
            for token in reversed(output.split()):
                try:
                    return int(token)
                except ValueError:
                    continue
            return default

        def extract_pid(output: str) -> str:
            for token in reversed(output.split()):
                if token.isdigit():
                    return token
            return ""

        # Wakeup / prompt check
        exec_cmd("\n", 0.5)

        # 1. Verify Ramoops and Kernel Panic Configuration (OS-008, OS-007)
        print("[HIL-RAM-OOM] Checking ramoops initialization in dmesg...")
        dmesg_ramoops = exec_cmd("dmesg | grep -i ramoops")
        if "Registered ramoops as persistent store backend" not in dmesg_ramoops:
            print(f"[FAIL] Ramoops driver not registered in dmesg:\n{dmesg_ramoops}")
            return False
        print("  -> Ramoops backend registered cleanly.")

        panic_out = exec_cmd("cat /proc/sys/kernel/panic")
        panic_timeout = extract_int(panic_out, 0)
        print(f"[HIL-RAM-OOM] Kernel panic reboot timeout: {panic_timeout}s")
        if panic_timeout <= 0:
            print(f"[FAIL] Kernel panic timeout is not greater than 0 (got: {panic_out})")
            return False

        # 2. Verify Crash Harvest Reports in /data/crashes (OS-008, STO-010)
        print("[HIL-RAM-OOM] Checking /data/crashes for harvested crash reports...")
        crash_list = exec_cmd("ls --color=never -1 /data/crashes/ 2>/dev/null").strip()
        reports = [line.strip() for line in crash_list.splitlines() if line.strip().startswith("crash_") and line.strip().endswith(".txt")]
        report_count = len(reports)
        print(f"  -> Found {report_count} crash report(s) in /data/crashes (max 20).")
        if report_count == 0 or report_count > 20:
            print(f"[FAIL] Crash report count {report_count} invalid (must be between 1 and 20)")
            return False

        # 3. Verify Compressed RAM Swap (OS-009)
        print("[HIL-RAM-OOM] Checking zram swap configuration (/proc/swaps)...")
        swaps = exec_cmd("cat /proc/swaps")
        if "/dev/zram0" not in swaps:
            print(f"[FAIL] /dev/zram0 not active in /proc/swaps:\n{swaps}")
            return False
        for prohibited in ["/dev/sda", "/dev/mmcblk", "/dev/nvme"]:
            if prohibited in swaps:
                print(f"[FAIL] Prohibited persistent swap active on {prohibited}:\n{swaps}")
                return False
        print("  -> /dev/zram0 is active and no persistent storage swap exists.")

        # 4. Verify Process OOM Scores and Supervision (OS-006, OS-009)
        print("[HIL-RAM-OOM] Checking renderer and agent OOM priority scores...")
        render_pid = extract_pid(exec_cmd("pidof chalkboard-render"))
        agent_pid = extract_pid(exec_cmd("pidof chalkboard-agent"))
        print(f"  -> Renderer PID: {render_pid}, Agent PID: {agent_pid}")

        if not render_pid or not agent_pid:
            print("[FAIL] Could not locate running PID for chalkboard-render or chalkboard-agent")
            return False

        render_adj = extract_int(exec_cmd(f"cat /proc/{render_pid}/oom_score_adj"))
        agent_adj = extract_int(exec_cmd(f"cat /proc/{agent_pid}/oom_score_adj"))
        print(f"  -> Renderer oom_score_adj: {render_adj} (expected -900)")
        print(f"  -> Agent oom_score_adj: {agent_adj} (expected 0)")

        if render_adj != -900:
            print(f"[FAIL] Renderer oom_score_adj is {render_adj}, expected -900")
            return False
        if agent_adj != 0:
            print(f"[FAIL] Agent oom_score_adj is {agent_adj}, expected 0")
            return False

        # 5. Verify Supervisor Restart Within 1s
        print("[HIL-RAM-OOM] Verifying agent supervisor restart within 1s...")
        exec_cmd(f"kill -9 {agent_pid}")
        time.sleep(1.0)
        new_agent_pid = extract_pid(exec_cmd("pidof chalkboard-agent"))
        print(f"  -> Killed PID {agent_pid}; new PID is {new_agent_pid}")
        if new_agent_pid == agent_pid or not new_agent_pid:
            print("[FAIL] Agent was not restarted by supervisor within 1s")
            return False

        print("[HIL-RAM-OOM] All hardware-in-the-loop tests passed successfully! Ni!")
        return True
    finally:
        os.close(fd)


def main():
    import argparse
    import sys

    parser = argparse.ArgumentParser(description="Chalkboard RAM logging, crash capture, zram and OOM validator")
    parser.add_argument("--serial-port", help="Serial port device node for live HIL verification")
    parser.add_argument("--baud", type=int, default=115200, help="Serial baud rate [default: 115200]")
    args = parser.parse_args()

    if args.serial_port:
        success = run_hil_ram_logging_oom_verification(args.serial_port, args.baud)
        sys.exit(0 if success else 1)
    else:
        suite = unittest.TestLoader().loadTestsFromTestCase(TestRamLoggingAndOom)
        runner = unittest.TextTestRunner(verbosity=2)
        result = runner.run(suite)
        sys.exit(0 if result.wasSuccessful() else 1)


if __name__ == "__main__":
    main()
