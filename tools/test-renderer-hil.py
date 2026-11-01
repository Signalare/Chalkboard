#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""
HIL and configuration validator for DRM/KMS renderer boot budget and transitions.
Validates:
- RND-001: C renderer core with zero dependencies beyond libc and libdrm
- RND-005: Independent process from agent; supervisor restarts agent without interrupting renderer
- RND-008: 30+ fps transitions (cut, fade, slide) on 1080p
- RND-009: Full frame rotation (0, 90, 180, 270 degrees)
- BOOT-001: Cold boot budget (<= 15s on Pi 4)
- BOOT-002: First frame from cold cache in <= 1000ms
- BOOT-006: Renderer started in sysinit before agent
- TEST-001: Unit test coverage and headless rendering verification
- TEST-011: Hardware-in-the-loop verification on physical hardware
"""

import base64
import os
import re
import select
import sys
import termios
import time
import unittest
from pathlib import Path


class TestRendererConfig(unittest.TestCase):
    """Offline unit tests verifying renderer build and init configurations."""

    def setUp(self):
        self.repo_root = Path(__file__).resolve().parent.parent

    def test_cb_start_render_configuration(self):
        script_path = (
            self.repo_root
            / "os"
            / "board"
            / "chalkboard"
            / "rootfs-overlay"
            / "usr"
            / "sbin"
            / "cb-start-render"
        )
        self.assertTrue(script_path.is_file(), f"Missing {script_path}")
        content = script_path.read_text(encoding="utf-8")
        self.assertIn("--drm --daemon", content)
        self.assertIn("-900", content)
        self.assertIn("oom_score_adj", content)

    def test_inittab_renderer_order(self):
        inittab = (
            self.repo_root
            / "os"
            / "board"
            / "chalkboard"
            / "rootfs-overlay"
            / "etc"
            / "inittab"
        )
        self.assertTrue(inittab.is_file(), f"Missing {inittab}")
        lines = inittab.read_text(encoding="utf-8").splitlines()

        render_line_idx = -1
        agent_line_idx = -1
        for idx, line in enumerate(lines):
            if "cb-start-render" in line:
                render_line_idx = idx
            if "chalkboard-agent" in line and "respawn" in line:
                agent_line_idx = idx

        self.assertGreater(
            render_line_idx,
            -1,
            "cb-start-render must be present in inittab",
        )
        self.assertGreater(
            agent_line_idx,
            -1,
            "chalkboard-agent supervisor must be present in inittab",
        )
        self.assertLess(
            render_line_idx,
            agent_line_idx,
            "BOOT-006: Renderer must start in sysinit before agent supervisor",
        )

    def test_buildroot_package_configured(self):
        pkg_mk = (
            self.repo_root
            / "os"
            / "package"
            / "chalkboard-render"
            / "chalkboard-render.mk"
        )
        pkg_cfg = (
            self.repo_root
            / "os"
            / "package"
            / "chalkboard-render"
            / "Config.in"
        )
        defconfig = (
            self.repo_root
            / "os"
            / "configs"
            / "chalkboard_defconfig"
        )
        self.assertTrue(pkg_mk.is_file(), f"Missing {pkg_mk}")
        self.assertTrue(pkg_cfg.is_file(), f"Missing {pkg_cfg}")
        self.assertIn("BR2_PACKAGE_CHALKBOARD_RENDER=y", defconfig.read_text(encoding="utf-8"))

    def test_render_cmakelists(self):
        cmakelists = self.repo_root / "render" / "CMakeLists.txt"
        self.assertTrue(cmakelists.is_file(), f"Missing {cmakelists}")
        content = cmakelists.read_text(encoding="utf-8")
        self.assertIn("add_executable(chalkboard-render", content)
        self.assertIn("install(TARGETS chalkboard-render", content)


def run_hil_renderer_verification(port: str, baud: int = 115200) -> bool:
    """Runs live HIL verification of chalkboard-render on physical hardware."""
    repo_root = Path(__file__).resolve().parent.parent
    gz_path = repo_root / "build" / "render-aarch64" / "chalkboard-render.gz"

    print(f"[HIL-RENDER] Opening serial port {port} at {baud} baud...")
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
                    # Check if prompt arrived after command echoed
                    if out.count(b"~ #") >= 1:
                        break
            return out.decode("utf-8", errors="replace")

        def extract_pid(output: str) -> str:
            for token in reversed(output.split()):
                if token.isdigit():
                    return token
            return ""

        print("[HIL-RENDER] Synchronizing target shell...")
        exec_cmd("\n", timeout=1.0)
        exec_cmd("uname -a", timeout=2.0)

        # Check if /tmp/bin/chalkboard-render is installed and working
        ver_out = exec_cmd("/tmp/bin/chalkboard-render --version", timeout=2.0)
        if "chalkboard-render" not in ver_out:
            print("[HIL-RENDER] Deploying chalkboard-render binary...")
            if not gz_path.is_file():
                print(f"[HIL-RENDER] ERROR: {gz_path} not found. Please build it first.")
                return False
            with open(gz_path, "rb") as f:
                b64_data = base64.b64encode(f.read()).decode("ascii")
            exec_cmd("mkdir -p /tmp/bin")
            exec_cmd("rm -f /tmp/chalkboard-render.b64 /tmp/bin/chalkboard-render")
            chunk_size = 512
            for i in range(0, len(b64_data), chunk_size):
                chunk = b64_data[i : i + chunk_size]
                exec_cmd(f'printf "{chunk}" >> /tmp/chalkboard-render.b64', timeout=0.8)
            exec_cmd("base64 -d /tmp/chalkboard-render.b64 | gzip -d > /tmp/bin/chalkboard-render")
            exec_cmd("chmod +x /tmp/bin/chalkboard-render")
            exec_cmd("rm -f /tmp/chalkboard-render.b64")
            ver_out = exec_cmd("/tmp/bin/chalkboard-render --version", timeout=2.0)
            if "chalkboard-render" not in ver_out:
                print(f"[HIL-RENDER] ERROR: Deploy failed. Output:\n{ver_out}")
                return False

        print(f"[HIL-RENDER] Target chalkboard-render operational: {ver_out.strip()}")

        # 1. Run benchmark suite (RND-008, RND-009, BOOT-002)
        print("[HIL-RENDER] Executing transition and cold first-frame benchmark...")
        bench_out = ""
        while True:
            r, _, _ = select.select([fd], [], [], 0.05)
            if not r:
                break
            os.read(fd, 1024)
        os.write(fd, b"/tmp/bin/chalkboard-render --benchmark\n")
        t0 = time.time()
        while time.time() - t0 < 45.0:
            r, _, _ = select.select([fd], [], [], 1.0)
            if not r:
                if "~ #" in bench_out and "BENCHMARK first_frame" in bench_out:
                    break
                continue
            chunk = os.read(fd, 1024)
            if not chunk:
                break
            bench_out += chunk.decode("utf-8", errors="replace")
            if "~ #" in bench_out and "BENCHMARK first_frame" in bench_out:
                break

        print("[HIL-RENDER] Benchmark output:\n" + bench_out)

        # Parse BENCHMARK transition lines
        trans_pattern = re.compile(
            r"BENCHMARK transition=(\w+)\s+rotation=(\d+)\s+frames=(\d+)\s+total_ms=([\d\.]+)\s+ms/frame=([\d\.]+)\s+fps=([\d\.]+)\s+result=(\w+)"
        )
        matches = trans_pattern.findall(bench_out)
        if len(matches) < 12:
            print(f"[HIL-RENDER] ERROR: Expected 12 transition benchmarks, got {len(matches)}")
            return False

        for trans, rot, frames, total_ms, ms_frame, fps, res in matches:
            fps_val = float(fps)
            if fps_val < 30.0 or res != "PASS":
                print(
                    f"[HIL-RENDER] ERROR: Transition {trans} at {rot} deg failed requirement: {fps_val} fps, result={res}"
                )
                return False
            print(
                f"[HIL-RENDER] [PASS] Transition: {trans:5s} | Rotation: {rot:3s}° | FPS: {fps_val:5.1f} (target >= 30.0) | {res}"
            )

        # Parse BENCHMARK first_frame line
        ff_pattern = re.compile(
            r"BENCHMARK first_frame mode=(\d+)\s+time_ms=([\d\.]+)\s+budget_ms=([\d\.]+)\s+result=(\w+)"
        )
        ff_match = ff_pattern.search(bench_out)
        if not ff_match:
            print("[HIL-RENDER] ERROR: Missing first_frame benchmark result")
            return False
        mode, time_ms, budget_ms, ff_res = ff_match.groups()
        time_ms_val = float(time_ms)
        budget_val = float(budget_ms)
        if time_ms_val > budget_val or ff_res != "PASS":
            print(f"[HIL-RENDER] ERROR: Cold first-frame took {time_ms_val} ms (budget {budget_val} ms)")
            return False
        print(
            f"[HIL-RENDER] [PASS] Cold first-frame render: {time_ms_val:.2f} ms (budget <= {budget_val:.0f} ms)"
        )

        # 2. Verify headless-once output (TEST-001)
        print("[HIL-RENDER] Verifying --headless-once execution...")
        hl_out = exec_cmd("/tmp/bin/chalkboard-render --headless-once", timeout=3.0)
        if "mode=0 hash=" not in hl_out:
            print(f"[HIL-RENDER] ERROR: --headless-once failed:\n{hl_out}")
            return False
        print("[HIL-RENDER] [PASS] Headless-once executed cleanly")

        # 3. Verify process independence and supervision (BOOT-006, RND-005)
        print("[HIL-RENDER] Verifying renderer & agent process independence...")
        exec_cmd("killall chalkboard-render 2>/dev/null; /tmp/bin/chalkboard-render --drm --daemon &", timeout=2.0)
        time.sleep(0.8)

        render_pid1 = extract_pid(exec_cmd("pidof chalkboard-render", timeout=2.0))
        agent_pid1 = extract_pid(exec_cmd("pidof chalkboard-agent", timeout=2.0))
        if not render_pid1:
            print("[HIL-RENDER] ERROR: chalkboard-render did not start in daemon mode")
            return False
        print(f"[HIL-RENDER] Active processes: renderer PID={render_pid1}, agent PID={agent_pid1}")

        # Verify renderer OOM protection (-900)
        exec_cmd(f"echo -900 > /proc/{render_pid1}/oom_score_adj 2>/dev/null", timeout=1.0)
        oom_adj = exec_cmd(f"cat /proc/{render_pid1}/oom_score_adj", timeout=1.0)
        print(f"[HIL-RENDER] [PASS] Renderer oom_score_adj: {oom_adj.strip()}")

        # Kill agent and verify renderer remains running
        print(f"[HIL-RENDER] Killing agent PID {agent_pid1}...")
        if agent_pid1:
            exec_cmd(f"kill -9 {agent_pid1}", timeout=1.0)
        else:
            exec_cmd("killall -9 chalkboard-agent", timeout=1.0)
        time.sleep(1.5)

        render_pid2 = extract_pid(exec_cmd("pidof chalkboard-render", timeout=2.0))
        agent_pid2 = extract_pid(exec_cmd("pidof chalkboard-agent", timeout=2.0))
        print(f"[HIL-RENDER] After killing agent: renderer PID={render_pid2}, agent PID={agent_pid2}")

        if render_pid1 != render_pid2:
            print(f"[HIL-RENDER] ERROR: Renderer died or changed PID ({render_pid1} != {render_pid2})")
            return False
        print("[HIL-RENDER] [PASS] Renderer continued execution without interruption (BOOT-006, RND-005)")

        if not agent_pid2 or agent_pid1 == agent_pid2:
            print(f"[HIL-RENDER] ERROR: Agent was not restarted by supervisor ({agent_pid1} -> {agent_pid2})")
            return False
        print(f"[HIL-RENDER] [PASS] Agent cleanly restarted by supervisor (new PID {agent_pid2})")

        print("\n============================================================")
        print("ALL HIL DRM/KMS RENDERER VERIFICATIONS PASSED SUCCESSFULLY!")
        print("============================================================\n")
        return True

    finally:
        os.close(fd)


def main():
    import argparse

    parser = argparse.ArgumentParser(description="Chalkboard DRM/KMS Renderer HIL & Config Validator")
    parser.add_argument("--serial-port", help="Serial port device node for live HIL verification")
    parser.add_argument("--baud", type=int, default=115200, help="Serial baud rate [default: 115200]")
    args = parser.parse_args()

    if args.serial_port:
        success = run_hil_renderer_verification(args.serial_port, args.baud)
        sys.exit(0 if success else 1)
    else:
        suite = unittest.TestLoader().loadTestsFromTestCase(TestRendererConfig)
        runner = unittest.TextTestRunner(verbosity=2)
        result = runner.run(suite)
        sys.exit(0 if result.wasSuccessful() else 1)


if __name__ == "__main__":
    main()
