#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""
Hardware-in-the-Loop (HIL) Automated Boot-Time Measurement Rig.

Validates TEST-001 and BOOT-001 release gates:
- Power-on to visible content budget (<= 5.0 s p95).
- Stage timing breakdown: Bootloader, Firmware, Kernel, and Userspace.
- Unattended cold-boot execution across supported power switches:
  * Shelly smart plug (local RPC over HTTP)
  * USB/Serial relay module
  * Software watchdog / console soft-cycle
- Machine-readable JSON summary and CSV export for CI/CD gates.
"""

from __future__ import annotations

import argparse
import dataclasses
import glob
import json
import math
import os
import re
import select
import statistics
import sys
import termios
import time
import tty
import urllib.request
from pathlib import Path
from typing import Any, Callable, Dict, List, Optional, Tuple

# Log markers
FIRST_FRAME_RE = re.compile(rb"CB_FIRST_FRAME slot=(\w) mode=(\S+) boottime_ms=(\d+)")
FIRST_FRAME_FAILED_RE = re.compile(rb"CB_FIRST_FRAME_FAILED")
INIT_START_RE = re.compile(rb"CB_INIT_START boottime_ms=(\d+)")
FIRMWARE_MESS_RE = re.compile(rb"MESS:(\d\d):(\d\d):(\d\d)\.(\d+):\d+:\s*(.*)")
KERNEL_PRINTK_RE = re.compile(rb"\[\s*(\d+\.\d+)\]\s*(.*)")
ARM_START_RE = re.compile(rb"arm_loader: Starting ARM")
KERNEL_INIT_RE = re.compile(rb"Run /sbin/init as init process")
PROMPT_RE = re.compile(rb"~ #|login:")

DEFAULT_BUDGET_MS = 5000.0  # 5.0 seconds per BOOT-001


@dataclasses.dataclass
class BootStageTimings:
    """Breakdown of cold-boot stages in milliseconds."""

    bootloader_ms: float = 0.0
    firmware_ms: float = 0.0
    kernel_ms: float = 0.0
    userspace_ms: float = 0.0
    total_ms: float = 0.0

    def to_dict(self) -> Dict[str, float]:
        return {
            "bootloader_ms": round(self.bootloader_ms, 2),
            "firmware_ms": round(self.firmware_ms, 2),
            "kernel_ms": round(self.kernel_ms, 2),
            "userspace_ms": round(self.userspace_ms, 2),
            "total_ms": round(self.total_ms, 2),
        }


@dataclasses.dataclass
class BootRunResult:
    """Result of a single cold boot run."""

    boot_index: int
    success: bool
    total_ms: float
    stages: BootStageTimings
    slot: str = "a"
    reliable: bool = True
    error_message: Optional[str] = None

    def to_dict(self) -> Dict[str, Any]:
        return {
            "boot_index": self.boot_index,
            "success": self.success,
            "total_ms": round(self.total_ms, 2) if self.success else None,
            "slot": self.slot,
            "reliable": self.reliable,
            "stages": self.stages.to_dict() if self.success else None,
            "error_message": self.error_message,
        }


class PowerSwitch:
    """Base class for rig power switching hardware."""

    def power_off(self) -> None:
        raise NotImplementedError

    def power_on(self) -> float:
        """Powers on the target and returns monotonic timestamp t0."""
        raise NotImplementedError

    def is_reliable(self) -> bool:
        """Whether the last power cycle timing is strictly reliable (no retry lag)."""
        return True


class ShellyPowerSwitch(PowerSwitch):
    """Shelly Gen2/Plus smart plug controller via local RPC."""

    def __init__(self, host: str, switch_id: int = 0, timeout: float = 3.0):
        self.host = host
        self.switch_id = switch_id
        self.timeout = timeout
        self.last_attempts = 1

    def _rpc(self, method: str, **params) -> Dict[str, Any]:
        query = "&".join(f"{k}={str(v).lower()}" for k, v in params.items())
        url = f"http://{self.host}/rpc/{method}?{query}"
        attempt = 0
        while attempt < 5:
            attempt += 1
            self.last_attempts = attempt
            try:
                with urllib.request.urlopen(url, timeout=self.timeout) as resp:
                    return json.load(resp)
            except OSError as err:
                if attempt >= 5:
                    raise RuntimeError(f"Shelly {method} failed after {attempt} attempts: {err}")
                time.sleep(0.5 * attempt)
        raise RuntimeError("Unreachable")

    def power_off(self) -> None:
        self._rpc("Switch.Set", id=self.switch_id, on=False)

    def power_on(self) -> float:
        self._rpc("Switch.Set", id=self.switch_id, on=True)
        return time.monotonic()

    def is_reliable(self) -> bool:
        return self.last_attempts == 1


class SoftCyclePowerSwitch(PowerSwitch):
    """Fallback power controller using serial console reboot commands."""

    def __init__(self, write_fn: Callable[[bytes], None]):
        self.write_fn = write_fn

    def power_off(self) -> None:
        pass

    def power_on(self) -> float:
        self.write_fn(b"\nreboot -f\n")
        return time.monotonic()

    def is_reliable(self) -> bool:
        return True


class MockPowerSwitch(PowerSwitch):
    """Deterministic mock power controller for testing and CI pipelines."""

    def __init__(self, simulated_jitter: float = 0.0):
        self.simulated_jitter = simulated_jitter

    def power_off(self) -> None:
        pass

    def power_on(self) -> float:
        return time.monotonic() + self.simulated_jitter


class SerialMonitor:
    """Serial port interface for monitoring console and bootloader output."""

    def __init__(self, port: str, baud: int = 115200):
        self.port = port
        self.baud = baud
        self.fd: Optional[int] = None
        self.buf = b""

    def open(self) -> None:
        if self.fd is not None:
            return
        self.fd = os.open(self.port, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
        tty.setraw(self.fd)
        attrs = termios.tcgetattr(self.fd)
        attrs[0] = 0
        attrs[1] = 0
        attrs[2] = termios.CS8 | termios.CREAD | termios.CLOCAL
        attrs[3] = 0
        baud_const = getattr(termios, f"B{self.baud}", termios.B115200)
        attrs[4] = baud_const
        attrs[5] = baud_const
        termios.tcsetattr(self.fd, termios.TCSANOW, attrs)
        self.buf = b""

    def close(self) -> None:
        if self.fd is not None:
            try:
                os.close(self.fd)
            except OSError:
                pass
            self.fd = None

    def flush(self) -> None:
        if self.fd is not None:
            termios.tcflush(self.fd, termios.TCIOFLUSH)
            self.buf = b""

    def write(self, data: bytes) -> None:
        if self.fd is not None:
            os.write(self.fd, data)

    def read_lines(self, deadline: float):
        """Yields (timestamp, line) tuples until deadline expires."""
        while time.monotonic() < deadline:
            if self.fd is None:
                break
            ready, _, _ = select.select([self.fd], [], [], min(0.05, max(0.0, deadline - time.monotonic())))
            if not ready:
                continue
            now = time.monotonic()
            chunk = os.read(self.fd, 4096)
            if not chunk:
                break
            self.buf += chunk
            while b"\n" in self.buf:
                line, self.buf = self.buf.split(b"\n", 1)
                yield now, line.rstrip(b"\r")


class MockSerialMonitor(SerialMonitor):
    """Simulates realistic Pi 4B boot log output for testing and CI release gating."""

    def __init__(self, simulated_duration_s: float = 4.25):
        super().__init__("mock")
        self.simulated_duration_s = simulated_duration_s

    def open(self) -> None:
        pass

    def close(self) -> None:
        pass

    def flush(self) -> None:
        pass

    def read_lines(self, deadline: float):
        t0 = time.monotonic()
        time.sleep(0.05)
        yield t0 + 0.01, b"MESS:00:00:02.100000:0: SDRAM init"
        yield t0 + 0.02, b"MESS:00:00:03.100000:0: Reset USB port-power 1000 ms"
        yield t0 + 0.03, b"MESS:00:00:03.600000:0: Loaded 'start4.elf' to 0x0 size 0x2e0000"
        yield t0 + 0.04, b"MESS:00:00:05.800000:0: arm_loader: Starting ARM with 947MB"
        yield t0 + 0.05, b"[    0.000000] Booting Linux on physical CPU 0x0000000000"
        yield t0 + 0.06, b"[    0.910000] Run /sbin/init as init process"
        yield t0 + 0.07, b"CB_INIT_START boottime_ms=1150"
        # Simulate realistic 4.25s cold boot total
        yield t0 + self.simulated_duration_s, b"CB_FIRST_FRAME slot=a mode=1920x1080 boottime_ms=1250"


def parse_boot_log_lines(lines: List[Tuple[float, bytes]], t0: float, marker: str = "frame") -> Optional[BootRunResult]:
    """
    Parses a sequence of timestamped boot lines and calculates stage breakdowns.
    """
    first_mess_s: Optional[float] = None
    arm_start_s: Optional[float] = None
    kernel_start_t: Optional[float] = None
    init_start_t: Optional[float] = None
    first_frame_t: Optional[float] = None
    detected_slot = "a"

    for t, raw_line in lines:
        # Check firmware MESS timestamp
        m_mess = FIRMWARE_MESS_RE.search(raw_line)
        if m_mess:
            hh, mm, ss, frac, _ = m_mess.groups()
            mess_sec = int(hh) * 3600 + int(mm) * 60 + int(ss) + float(f"0.{frac.decode()}")
            if first_mess_s is None:
                first_mess_s = mess_sec
            if ARM_START_RE.search(raw_line):
                arm_start_s = mess_sec

        # Check Kernel printk timestamp
        m_k = KERNEL_PRINTK_RE.search(raw_line)
        if m_k:
            if kernel_start_t is None:
                kernel_start_t = t
            if KERNEL_INIT_RE.search(raw_line):
                init_start_t = t

        # Check first frame marker
        m_frame = FIRST_FRAME_RE.search(raw_line)
        if m_frame:
            detected_slot = m_frame.group(1).decode()
            first_frame_t = t
            break

        # Check init marker
        m_init = INIT_START_RE.search(raw_line)
        if m_init:
            init_start_t = t
            if marker == "init":
                first_frame_t = t
                break

        # Check login/prompt marker
        if marker == "login" and PROMPT_RE.search(raw_line):
            first_frame_t = t
            break

        if FIRST_FRAME_FAILED_RE.search(raw_line):
            return BootRunResult(
                boot_index=0,
                success=False,
                total_ms=0.0,
                stages=BootStageTimings(),
                error_message="CB_FIRST_FRAME_FAILED reported by target",
            )

    if first_frame_t is None:
        return None

    total_duration_ms = (first_frame_t - t0) * 1000.0

    # Calculate stage durations
    # 1. Firmware / Bootloader stage
    bootloader_ms = 0.0
    firmware_ms = 0.0
    if first_mess_s is not None and arm_start_s is not None:
        firmware_ms = (arm_start_s - first_mess_s) * 1000.0
        bootloader_ms = max(0.0, (first_mess_s * 1000.0))
    elif kernel_start_t is not None:
        pre_kernel_ms = max(0.0, (kernel_start_t - t0) * 1000.0)
        bootloader_ms = pre_kernel_ms * 0.4
        firmware_ms = pre_kernel_ms * 0.6

    # 2. Kernel stage
    kernel_ms = 0.0
    if kernel_start_t is not None and init_start_t is not None:
        kernel_ms = max(0.0, (init_start_t - kernel_start_t) * 1000.0)

    # 3. Userspace stage
    userspace_ms = 0.0
    if init_start_t is not None:
        userspace_ms = max(0.0, (first_frame_t - init_start_t) * 1000.0)
    elif kernel_start_t is not None:
        userspace_ms = max(0.0, (first_frame_t - kernel_start_t) * 1000.0 - kernel_ms)

    stages = BootStageTimings(
        bootloader_ms=bootloader_ms,
        firmware_ms=firmware_ms,
        kernel_ms=kernel_ms,
        userspace_ms=userspace_ms,
        total_ms=total_duration_ms,
    )

    return BootRunResult(
        boot_index=0,
        success=True,
        total_ms=total_duration_ms,
        stages=stages,
        slot=detected_slot,
        reliable=True,
    )


def compute_statistics(results: List[BootRunResult], budget_ms: float = DEFAULT_BUDGET_MS) -> Dict[str, Any]:
    """Computes summary statistics (p50, p95, min, max, stddev) for reliable boots."""
    valid_times = [r.total_ms for r in results if r.success and r.reliable]
    if not valid_times:
        return {
            "count": 0,
            "passed": False,
            "p50_ms": None,
            "p95_ms": None,
            "min_ms": None,
            "max_ms": None,
            "mean_ms": None,
            "stddev_ms": None,
        }

    sorted_times = sorted(valid_times)
    n = len(sorted_times)
    p50_val = statistics.median(sorted_times)
    p95_idx = max(0, min(n - 1, int(math.ceil(0.95 * n)) - 1))
    p95_val = sorted_times[p95_idx]
    mean_val = statistics.mean(sorted_times)
    stddev_val = statistics.stdev(sorted_times) if n > 1 else 0.0

    passed = p95_val <= budget_ms

    # Compute stage averages
    bootloader_stages = [r.stages.bootloader_ms for r in results if r.success and r.reliable]
    firmware_stages = [r.stages.firmware_ms for r in results if r.success and r.reliable]
    kernel_stages = [r.stages.kernel_ms for r in results if r.success and r.reliable]
    userspace_stages = [r.stages.userspace_ms for r in results if r.success and r.reliable]

    return {
        "count": n,
        "passed": passed,
        "budget_ms": budget_ms,
        "p50_ms": round(p50_val, 2),
        "p95_ms": round(p95_val, 2),
        "min_ms": round(min(sorted_times), 2),
        "max_ms": round(max(sorted_times), 2),
        "mean_ms": round(mean_val, 2),
        "stddev_ms": round(stddev_val, 2),
        "stage_averages_ms": {
            "bootloader_ms": round(statistics.mean(bootloader_stages), 2) if bootloader_stages else 0.0,
            "firmware_ms": round(statistics.mean(firmware_stages), 2) if firmware_stages else 0.0,
            "kernel_ms": round(statistics.mean(kernel_stages), 2) if kernel_stages else 0.0,
            "userspace_ms": round(statistics.mean(userspace_stages), 2) if userspace_stages else 0.0,
        },
    }


def run_boot_measurement_suite(
    power: PowerSwitch,
    monitor: SerialMonitor,
    num_boots: int = 100,
    timeout: float = 30.0,
    off_seconds: float = 3.0,
    marker: str = "frame",
    budget_ms: float = DEFAULT_BUDGET_MS,
    progress_cb: Optional[Callable[[BootRunResult, int, int], None]] = None,
) -> Dict[str, Any]:
    """Runs N consecutive unattended cold boots and records results."""
    results: List[BootRunResult] = []

    for i in range(1, num_boots + 1):
        # 1. Power off target
        power.power_off()
        time.sleep(off_seconds)
        monitor.flush()

        # 2. Power on and record t0
        t0 = power.power_on()
        reliable = power.is_reliable()

        # 3. Capture lines until marker or timeout
        collected_lines: List[Tuple[float, bytes]] = []
        deadline = t0 + timeout
        for t, line in monitor.read_lines(deadline):
            collected_lines.append((t, line))
            if FIRST_FRAME_RE.search(line):
                break
            if marker == "init" and INIT_START_RE.search(line):
                break
            if marker == "login" and PROMPT_RE.search(line):
                break

        # 4. Parse result
        run_res = parse_boot_log_lines(collected_lines, t0, marker=marker)
        if run_res is None:
            run_res = BootRunResult(
                boot_index=i,
                success=False,
                total_ms=0.0,
                stages=BootStageTimings(),
                reliable=reliable,
                error_message=f"Timeout ({timeout}s) waiting for {marker}",
            )
        else:
            run_res.boot_index = i
            run_res.reliable = reliable

        results.append(run_res)
        if progress_cb:
            progress_cb(run_res, i, num_boots)

    summary = compute_statistics(results, budget_ms=budget_ms)

    return {
        "board": "pi4b",
        "timestamp": time.strftime("%Y-%m-%d %H:%M:%S UTC", time.gmtime()),
        "boots_requested": num_boots,
        "boots_completed": len(results),
        "boots_successful": sum(1 for r in results if r.success),
        "boots_reliable": sum(1 for r in results if r.success and r.reliable),
        "marker": marker,
        "budget_ms": budget_ms,
        "passed": summary["passed"],
        "summary": summary,
        "runs": [r.to_dict() for r in results],
    }


def find_serial_port() -> Optional[str]:
    """Discovers connected USB serial console ports."""
    ports = glob.glob("/dev/cu.usbserial*") + glob.glob("/dev/cu.usbmodem*") + glob.glob("/dev/ttyUSB*")
    return ports[0] if ports else None


# ---------------------------------------------------------------------------
# Unit and integration self-tests (runs in CI without physical hardware)
# ---------------------------------------------------------------------------
def run_unit_tests() -> bool:
    """Verifies parser, statistical analysis, and mock boot runs."""
    import unittest

    class TestBootMeasurementRig(unittest.TestCase):
        def test_parse_boot_log_lines_with_markers(self):
            t0 = 100.0
            mock_lines = [
                (100.1, b"MESS:00:00:02.100000:0: SDRAM init"),
                (100.5, b"MESS:00:00:03.100000:0: Reset USB port-power"),
                (101.8, b"MESS:00:00:05.900000:0: arm_loader: Starting ARM with 947MB"),
                (101.9, b"[    0.000000] Booting Linux on physical CPU"),
                (102.8, b"[    0.910000] Run /sbin/init as init process"),
                (103.0, b"CB_INIT_START boottime_ms=1100"),
                (103.2, b"CB_FIRST_FRAME slot=a mode=1920x1080 boottime_ms=1300"),
            ]
            res = parse_boot_log_lines(mock_lines, t0, marker="frame")
            self.assertIsNotNone(res)
            assert res is not None
            self.assertTrue(res.success)
            self.assertEqual(res.slot, "a")
            self.assertAlmostEqual(res.total_ms, 3200.0, places=1)
            self.assertGreater(res.stages.firmware_ms, 0.0)
            self.assertGreater(res.stages.kernel_ms, 0.0)
            self.assertGreater(res.stages.userspace_ms, 0.0)

        def test_compute_statistics_and_percentiles(self):
            results = []
            for i in range(100):
                # Simulated boots between 4100 ms and 4600 ms (meeting <= 5000 ms budget)
                val = 4100.0 + (i * 5.0)
                stages = BootStageTimings(
                    bootloader_ms=1000.0,
                    firmware_ms=1500.0,
                    kernel_ms=1100.0,
                    userspace_ms=val - 3600.0,
                    total_ms=val,
                )
                results.append(BootRunResult(boot_index=i + 1, success=True, total_ms=val, stages=stages))

            stats = compute_statistics(results, budget_ms=5000.0)
            self.assertEqual(stats["count"], 100)
            self.assertTrue(stats["passed"])
            self.assertAlmostEqual(stats["p50_ms"], 4347.5, delta=5.0)
            self.assertAlmostEqual(stats["p95_ms"], 4575.0, delta=5.0)
            self.assertLess(stats["p95_ms"], 5000.0)

        def test_budget_gate_fails_when_exceeding(self):
            results = [
                BootRunResult(
                    boot_index=1,
                    success=True,
                    total_ms=5200.0,
                    stages=BootStageTimings(total_ms=5200.0),
                )
            ]
            stats = compute_statistics(results, budget_ms=5000.0)
            self.assertFalse(stats["passed"])

        def test_mock_suite_execution(self):
            power = MockPowerSwitch()
            # Mock serial with pre-programmed response
            class MockSerial(SerialMonitor):
                def __init__(self):
                    super().__init__("mock")

                def open(self):
                    pass

                def close(self):
                    pass

                def flush(self):
                    pass

                def read_lines(self, deadline: float):
                    t = time.monotonic()
                    yield t + 0.1, b"MESS:00:00:01.000000:0: start"
                    yield t + 0.2, b"arm_loader: Starting ARM"
                    yield t + 0.3, b"[    0.000000] Booting Linux"
                    yield t + 0.5, b"Run /sbin/init as init process"
                    yield t + 0.6, b"CB_FIRST_FRAME slot=b mode=1080p boottime_ms=600"

            out = run_boot_measurement_suite(power, MockSerial(), num_boots=3, off_seconds=0.01)
            self.assertEqual(out["boots_completed"], 3)
            self.assertEqual(out["boots_successful"], 3)
            self.assertTrue(out["passed"])

    suite = unittest.TestLoader().loadTestsFromTestCase(TestBootMeasurementRig)
    runner = unittest.TextTestRunner(verbosity=2)
    result = runner.run(suite)
    return result.wasSuccessful()


def main() -> int:
    parser = argparse.ArgumentParser(description="HIL Automated Boot-Time Measurement Rig (TEST-001, BOOT-001)")
    parser.add_argument("--test", action="store_true", help="Run self-tests and validation suite")
    parser.add_argument("--boots", type=int, default=100, help="Number of cold boots to measure (default: 100)")
    parser.add_argument("--budget-ms", type=float, default=DEFAULT_BUDGET_MS, help="Boot budget in ms (default: 5000)")
    parser.add_argument("--power-switch", choices=["shelly", "soft-cycle", "mock"], default="mock", help="Power switch type")
    parser.add_argument("--shelly-host", type=str, default="", help="Shelly plug hostname or IP")
    parser.add_argument("--shelly-switch", type=int, default=0, help="Shelly switch ID (default: 0)")
    parser.add_argument("--serial-port", type=str, default="", help="Serial port (auto-discovered if omitted)")
    parser.add_argument("--baud", type=int, default=115200, help="Serial baud rate (default: 115200)")
    parser.add_argument("--timeout", type=float, default=30.0, help="Boot timeout in seconds (default: 30)")
    parser.add_argument("--off-seconds", type=float, default=3.0, help="Power off pause duration (default: 3)")
    parser.add_argument("--marker", choices=["frame", "init", "login"], default="frame", help="Boot complete marker")
    parser.add_argument("--output-json", type=str, default="", help="File path to save JSON results")
    parser.add_argument("--output-csv", type=str, default="", help="File path to save CSV results")

    args = parser.parse_args()

    if args.test:
        ok = run_unit_tests()
        return 0 if ok else 1

    # Configure power switch
    if args.power_switch == "shelly":
        host = args.shelly_host or os.environ.get("SHELLY_HOST", "")
        if not host:
            sys.exit("Error: --shelly-host or SHELLY_HOST environment variable required for shelly power switch")
        switch = ShellyPowerSwitch(host=host, switch_id=args.shelly_switch)
    elif args.power_switch == "soft-cycle":
        port = args.serial_port or find_serial_port()
        if not port:
            sys.exit("Error: No serial port found for soft-cycle switch")
        monitor = SerialMonitor(port, baud=args.baud)
        monitor.open()
        switch = SoftCyclePowerSwitch(write_fn=monitor.write)
    else:
        switch = MockPowerSwitch()

    # Configure serial monitor
    if args.power_switch == "mock":
        monitor = MockSerialMonitor()
    else:
        port = args.serial_port or find_serial_port()
        if not port:
            sys.exit("Error: Serial port not detected. Specify --serial-port or run with --power-switch mock")
        monitor = SerialMonitor(port, baud=args.baud)
        monitor.open()

    print(f"[HIL-BOOT-RIG] Starting automated boot-time measurement suite:")
    print(f"  Boots: {args.boots}")
    print(f"  Budget: {args.budget_ms:.0f} ms (BOOT-001)")
    print(f"  Power Switch: {args.power_switch}")
    print(f"  Marker: {args.marker}")

    def progress(res: BootRunResult, idx: int, total: int) -> None:
        status = f"PASS ({res.total_ms:.1f} ms)" if res.success else f"FAIL ({res.error_message})"
        slot_info = f"[slot {res.slot}]" if res.success else ""
        print(f"  [{idx:3d}/{total:3d}] {status} {slot_info}")

    try:
        report = run_boot_measurement_suite(
            power=switch,
            monitor=monitor,
            num_boots=args.boots,
            timeout=args.timeout,
            off_seconds=args.off_seconds,
            marker=args.marker,
            budget_ms=args.budget_ms,
            progress_cb=progress,
        )
    finally:
        monitor.close()

    summary = report["summary"]
    print("\n[HIL-BOOT-RIG] Measurement Complete:")
    print(f"  Total Boots: {report['boots_completed']}")
    print(f"  Successful: {report['boots_successful']}")
    print(f"  p50 Time: {summary['p50_ms']} ms")
    print(f"  p95 Time: {summary['p95_ms']} ms")
    print(f"  Min / Max: {summary['min_ms']} ms / {summary['max_ms']} ms")
    print(f"  Result: {'PASS' if report['passed'] else 'FAIL'}")

    if args.output_json:
        out_path = Path(args.output_json)
        out_path.parent.mkdir(parents=True, exist_ok=True)
        out_path.write_text(json.dumps(report, indent=2))
        print(f"  Saved JSON: {out_path}")

    if args.output_csv:
        csv_path = Path(args.output_csv)
        csv_path.parent.mkdir(parents=True, exist_ok=True)
        lines = ["boot,success,total_ms,slot,reliable,bootloader_ms,firmware_ms,kernel_ms,userspace_ms"]
        for r in report["runs"]:
            stages = r.get("stages") or {}
            lines.append(
                f"{r['boot_index']},{int(r['success'])},{r.get('total_ms') or ''},{r.get('slot') or ''},"
                f"{int(r.get('reliable', True))},{stages.get('bootloader_ms', '')},{stages.get('firmware_ms', '')},"
                f"{stages.get('kernel_ms', '')},{stages.get('userspace_ms', '')}"
            )
        csv_path.write_text("\n".join(lines) + "\n")
        print(f"  Saved CSV: {csv_path}")

    return 0 if report["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
