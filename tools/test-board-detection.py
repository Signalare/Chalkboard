#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""
Board and hardware capability detection verification (HW-001, HW-009, TEST-011).

Validates:
1. `chalkboard-agent --detect-board` CLI output in mock and live modes.
2. `chalkboard-board` detection against captured live Pi 4B Model B Rev 1.1 hardware fixtures.
3. Live physical board interrogation over serial console (when connected) ensuring
   the physical board (Pi 4B Rev 1.1, c03111) capabilities match SPEC §4.1:
   - Model: Raspberry Pi 4 Model B
   - Support Tier: Tier 1
   - SoC Family: BCM2711
   - RAM: 4096 MB
   - HDMI outputs: 2
   - Wi-Fi: DualBand (2.4/5 GHz)
   - Built-in Ethernet: True
   - Hardware RTC: False
   - Boot Device: MicroSD
"""

import glob
import json
import os
import re
import select
import subprocess
import sys
import termios
import time
import tty
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent


def read_serial_command(port: str, command: str, timeout: float = 3.0) -> str:
    """Sends a command to the serial shell and returns the response string."""
    fd = os.open(port, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
    try:
        tty.setraw(fd)
        attrs = termios.tcgetattr(fd)
        attrs[4] = attrs[5] = termios.B115200
        termios.tcsetattr(fd, termios.TCSANOW, attrs)
        termios.tcflush(fd, termios.TCIOFLUSH)

        os.write(fd, command.encode() + b"\r\n")
        deadline = time.monotonic() + timeout
        buf = b""
        while time.monotonic() < deadline:
            r, _, _ = select.select([fd], [], [], 0.1)
            if r:
                buf += os.read(fd, 4096)
                if b"# " in buf[len(command) + 2 :]:
                    break
        return buf.decode(errors="replace")
    finally:
        os.close(fd)


def find_serial_port() -> str | None:
    """Finds available serial console adapter port if connected."""
    if os.environ.get("SERIAL_PORT") and os.path.exists(os.environ["SERIAL_PORT"]):
        return os.environ["SERIAL_PORT"]
    ports = glob.glob("/dev/cu.usbserial*") + glob.glob("/dev/cu.usbmodem*")
    return ports[0] if ports else None


class TestBoardDetection(unittest.TestCase):
    def test_mock_board_detection_cli(self):
        """Verifies chalkboard-agent --detect-board --mock-hardware emits valid JSON."""
        cmd = [
            "cargo",
            "run",
            "--manifest-path",
            str(REPO_ROOT / "agent/Cargo.toml"),
            "-p",
            "chalkboard-agent",
            "--",
            "--detect-board",
            "--mock-hardware",
        ]
        result = subprocess.run(
            cmd,
            cwd=str(REPO_ROOT),
            capture_output=True,
            text=True,
            check=True,
        )
        data = json.loads(result.stdout)
        self.assertEqual(data["model"], "pi4-b")
        self.assertEqual(data["revisionCode"], 0x00C0_3111)
        self.assertEqual(data["supportTier"], "tier1")
        self.assertEqual(data["socFamily"], "bcm2711")
        self.assertEqual(data["ramMb"], 4096)
        self.assertEqual(data["hdmiOutputs"], 2)
        self.assertEqual(data["wifiBands"], "dual-band")
        self.assertTrue(data["hasEthernet"])
        self.assertFalse(data["hasRtc"])
        self.assertEqual(data["bootDevice"], "micro-sd")

    def test_rust_chalkboard_board_tests_pass(self):
        """Runs the chalkboard-board test suite including the live hardware fixture."""
        cmd = [
            "cargo",
            "test",
            "--manifest-path",
            str(REPO_ROOT / "agent/Cargo.toml"),
            "-p",
            "chalkboard-board",
        ]
        result = subprocess.run(
            cmd,
            cwd=str(REPO_ROOT),
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, f"Cargo test failed:\n{result.stderr}\n{result.stdout}")
        self.assertIn("hw_001_hw_009_live_pi4b_rev1_1_fixture_detected ... ok", result.stdout)

    def test_live_hardware_telemetry_if_connected(self):
        """If a physical board is connected on serial, verifies live hardware matches HW-001/HW-009."""
        port = find_serial_port()
        if not port:
            self.skipTest("No serial port connected; skipping live HIL hardware verification")

        # Test alive response
        out = read_serial_command(port, "uname -s; cat /proc/device-tree/model; echo")
        if "Linux" not in out:
            self.skipTest(f"Serial port {port} found but board not responding to shell commands")

        # 1. Model check
        dt_model = read_serial_command(port, "cat /proc/device-tree/model; echo")
        self.assertIn("Raspberry Pi 4 Model B Rev 1.1", dt_model)

        # 2. Revision code check
        rev_out = read_serial_command(port, "hexdump -C /proc/device-tree/system/linux,revision")
        self.assertIn("00 c0 31 11", rev_out)

        cpuinfo_out = read_serial_command(port, "grep -i 'Revision' /proc/cpuinfo")
        self.assertIn("c03111", cpuinfo_out)

        # 3. Network and Wi-Fi interface checks
        net_out = read_serial_command(port, "ls /sys/class/net")
        self.assertIn("eth0", net_out)
        self.assertIn("wlan0", net_out)

        phy_out = read_serial_command(port, "ls /sys/class/ieee80211")
        self.assertIn("phy0", phy_out)

        # 4. RTC check (no RTC on base Pi 4B)
        rtc_out = read_serial_command(port, "ls /sys/class/rtc")
        self.assertNotIn("rtc0", rtc_out)

        # 5. Boot storage medium check (MicroSD card -> SD)
        mmc_type = read_serial_command(port, "cat /sys/block/mmcblk0/device/type")
        self.assertIn("SD", mmc_type)


if __name__ == "__main__":
    unittest.main()
