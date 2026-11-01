#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Boot-slot spike harness (#3), run on the developer machine.

Switches the Pi's power through a Shelly plug (local RPC API), timestamps
every serial-console line and measures power-on -> CB_FIRST_FRAME.

Environment:
  SHELLY_HOST    plug address, e.g. 10.0.30.203 (required)
  SHELLY_SWITCH  switch id on the plug (default 0)
  SERIAL_PORT    serial device (default: the only /dev/cu.usbserial* port)
  SPIKE_HOST_IP  this machine's address as seen by the Pi (default: route to the Pi)

Commands:
  power on|off|status        switch or query the plug
  log                        print the serial console with timestamps
  boots N [--off S]          N cold boots; CSV of time to first frame
  powercut N                 N power cuts during cb-commit-loop
  slots                      tryboot fallback and commit checks
  reflash IMAGE              write IMAGE to the Pi's card over Wi-Fi (lab only)
  run "COMMAND"              boot, run COMMAND on the console, print output

Anything that switches power requires --yes, so a wrong SHELLY_HOST cannot
cut power to some other device by accident.
"""

import argparse
import glob
import json
import os
import random
import re
import select
import statistics
import sys
import termios
import time
import tty
import urllib.request
from pathlib import Path

FIRST_FRAME = re.compile(rb"CB_FIRST_FRAME slot=(\w) mode=(\S+) boottime_ms=(\d+)")
FIRST_FRAME_FAILED = re.compile(rb"CB_FIRST_FRAME_FAILED")
INIT_START = re.compile(rb"CB_INIT_START boottime_ms=(\d+)")
RESULTS = Path("build/spike")
# Boot-time end marker: "frame" (CB_FIRST_FRAME, needs a display) or "init".
MARKER = "frame"


class Shelly:
    def __init__(self, host: str, switch: int):
        self.host, self.switch = host, switch

    # Wi-Fi plugs drop out now and then (one hung for minutes during the
    # first power-cut run), so keep retrying for a while before giving up.
    PATIENCE_S = 180

    def _rpc(self, method: str, **params) -> dict:
        query = "&".join(f"{k}={str(v).lower()}" for k, v in params.items())
        url = f"http://{self.host}/rpc/{method}?{query}"
        deadline = time.monotonic() + self.PATIENCE_S
        attempt = 0
        while True:
            attempt += 1
            self.last_attempts = attempt
            try:
                with urllib.request.urlopen(url, timeout=2) as resp:
                    return json.load(resp)
            except OSError as err:
                if time.monotonic() >= deadline:
                    raise
                print(f"  shelly: {method} attempt {attempt} failed ({err}); retrying", file=sys.stderr)
                time.sleep(min(10.0, 0.5 * attempt))

    def set(self, on: bool) -> float:
        """Switches the outlet and returns the monotonic time it confirmed.

        After a retry (`last_attempts > 1`) the plug may have switched before
        the confirmed time, so timings from such a call are unreliable.
        """
        self._rpc("Switch.Set", id=self.switch, on=on)
        return time.monotonic()

    def status(self) -> dict:
        return self._rpc("Switch.GetStatus", id=self.switch)


class Serial:
    def __init__(self, port: str):
        self.fd = os.open(port, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
        tty.setraw(self.fd)
        attrs = termios.tcgetattr(self.fd)
        attrs[4] = attrs[5] = termios.B115200
        termios.tcsetattr(self.fd, termios.TCSANOW, attrs)
        self.buf = b""

    def flush(self) -> None:
        termios.tcflush(self.fd, termios.TCIOFLUSH)
        self.buf = b""

    def write_line(self, text: str) -> None:
        os.write(self.fd, text.encode() + b"\r")

    def lines(self, deadline: float):
        """Yields (monotonic time, line) until the deadline passes."""
        while time.monotonic() < deadline:
            ready, _, _ = select.select([self.fd], [], [], min(0.05, max(0, deadline - time.monotonic())))
            if not ready:
                continue
            now = time.monotonic()
            self.buf += os.read(self.fd, 4096)
            while b"\n" in self.buf:
                line, self.buf = self.buf.split(b"\n", 1)
                yield now, line.rstrip(b"\r")


def serial_port() -> str:
    if os.environ.get("SERIAL_PORT"):
        return os.environ["SERIAL_PORT"]
    ports = glob.glob("/dev/cu.usbserial*") + glob.glob("/dev/cu.usbmodem*") + glob.glob("/dev/cu.SLAB*")
    if len(ports) != 1:
        sys.exit(f"Set SERIAL_PORT; found {ports or 'no serial ports'}")
    return ports[0]


def shelly() -> Shelly:
    host = os.environ.get("SHELLY_HOST") or sys.exit("Set SHELLY_HOST to the plug's address")
    return Shelly(host, int(os.environ.get("SHELLY_SWITCH", "0")))


def require_yes(args) -> None:
    if not args.yes:
        sys.exit("This switches power on the plug; re-run with --yes to confirm.")


def boot_once(plug: Shelly, ser: Serial, log, off_s: float, timeout: float):
    """Power-cycles once; returns (ms to end marker, slot, kernel boottime ms, timing reliable) or None."""
    plug.set(False)
    time.sleep(off_s)
    ser.flush()
    t0 = plug.set(True)
    reliable = plug.last_attempts == 1
    log.write(f"--- power on t0={t0:.3f}{'' if reliable else ' (after retry: timing unreliable)'}\n")
    for t, line in ser.lines(t0 + timeout):
        log.write(f"{(t - t0) * 1000:9.1f} ms  {line.decode(errors='replace')}\n")
        m = FIRST_FRAME.search(line)
        if m:
            return (t - t0) * 1000, m.group(1).decode(), int(m.group(3)), reliable
        m = INIT_START.search(line)
        if m and MARKER == "init":
            return (t - t0) * 1000, "?", int(m.group(1)), reliable
        if FIRST_FRAME_FAILED.search(line):
            return None
    return None


def wait_for(ser: Serial, pattern: bytes, timeout: float) -> bool:
    for _, line in ser.lines(time.monotonic() + timeout):
        if re.search(pattern, line):
            return True
    return False


SLOT_STATUS = re.compile(rb"slot=(\w) layout=(\w+) tryboot=(\d+)")


def shell(ser: Serial, command: str, timeout: float = 10) -> list[bytes]:
    """Runs a command on the serial shell and returns its output lines."""
    ser.flush()
    ser.write_line(command)
    out = []
    for _, line in ser.lines(time.monotonic() + timeout):
        if line.strip().endswith(b"#") and out:
            break
        out.append(line)
    return out


def slot_status(ser: Serial) -> tuple[str, int]:
    """Returns (running slot, tryboot flag) from cb-slot."""
    for _ in range(3):
        for line in shell(ser, "cb-slot"):
            m = SLOT_STATUS.search(line)
            if m:
                return m.group(1).decode(), int(m.group(3))
    raise RuntimeError("no answer from cb-slot")


def wait_booted(ser: Serial, timeout: float = 60, max_uptime: float | None = None) -> None:
    """Waits for CB_INIT_START; if that line was lost, accepts a node whose
    uptime is at most max_uptime seconds (default: timeout + 15)."""
    if max_uptime is None:
        max_uptime = timeout + 15
    if wait_for(ser, rb"CB_INIT_START", timeout):
        time.sleep(1)
        return
    # The marker line is occasionally lost on the serial line (about 1 in 200
    # boots). A freshly booted node still proves itself through its uptime.
    for line in shell(ser, "cut -d. -f1 /proc/uptime", timeout=5):
        if line.strip().isdigit() and int(line.strip()) <= max_uptime:
            print(f"  warning: CB_INIT_START not seen, but uptime is {int(line.strip())} s — fresh boot",
                  file=sys.stderr)
            return
    raise RuntimeError("node did not reach init")


def power_boot(plug: Shelly, ser: Serial, off_s: float) -> None:
    plug.set(False)
    time.sleep(off_s)
    ser.flush()
    plug.set(True)
    wait_booted(ser)


def cmd_slots(args) -> None:
    """Checks tryboot fallback and commit for the image's slot mechanism."""
    require_yes(args)
    plug, ser = shelly(), Serial(serial_port())
    results = []

    def check(name: str, expected: tuple[str, int]) -> None:
        got = slot_status(ser)
        ok = got == expected
        results.append(ok)
        print(f"{'PASS' if ok else 'FAIL'}  {name}: expected slot {expected[0]} tryboot={expected[1]}, "
              f"got slot {got[0]} tryboot={got[1]}")

    power_boot(plug, ser, args.off)
    default, _ = slot_status(ser)
    other = "b" if default == "a" else "a"
    print(f"default slot is {default}")
    check("cold boot uses the committed slot", (default, 0))

    ser.write_line("cb-tryboot")
    wait_booted(ser)
    check("cb-tryboot boots the other slot once", (other, 1))

    power_boot(plug, ser, args.off)
    check("power cut during tryboot falls back", (default, 0))

    ser.write_line("cb-tryboot")
    wait_booted(ser)
    check("second tryboot reaches the other slot", (other, 1))
    print("  " + " | ".join(line.decode(errors="replace") for line in shell(ser, "cb-commit")))

    power_boot(plug, ser, args.off)
    check("after commit and power cut the new slot stays", (other, 0))

    print(f"\n{sum(results)}/{len(results)} checks passed; default slot is now {other}")
    sys.exit(0 if all(results) else 1)


def local_ip_towards(host: str) -> str:
    """Returns this machine's address on the route to host."""
    import socket

    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as s:
        s.connect((host, 9))
        return s.getsockname()[0]


def cmd_reflash(args) -> None:
    """Streams an image to the Pi over Wi-Fi; the Pi writes it to its own card."""
    import hashlib
    import http.server
    import secrets
    import threading

    require_yes(args)
    image = Path(args.image)
    size = image.stat().st_size
    sha = hashlib.sha256(image.read_bytes()).hexdigest()
    token = secrets.token_hex(16)

    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self):  # noqa: N802 - name required by BaseHTTPRequestHandler
            if self.path != f"/{token}/{image.name}":
                self.send_error(404)
                return
            self.send_response(200)
            self.send_header("Content-Length", str(size))
            self.end_headers()
            with open(image, "rb") as f:
                sent = 0
                while chunk := f.read(1 << 20):
                    self.wfile.write(chunk)
                    sent += len(chunk)
                self.wfile.flush()

        def log_message(self, fmt, *a):
            print(f"  http: {self.client_address[0]} {fmt % a}", flush=True)

    ser = Serial(serial_port())
    spike_ip = None
    for line in shell(ser, "ip -4 -o addr show wlan0", timeout=5):
        m = re.search(rb"inet (\d+\.\d+\.\d+\.\d+)", line)
        if m:
            spike_ip = m.group(1).decode()
    if not spike_ip:
        sys.exit("The Pi has no Wi-Fi address (is lab Wi-Fi configured in its image?)")
    server = http.server.ThreadingHTTPServer(("0.0.0.0", 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    host_ip = os.environ.get("SPIKE_HOST_IP") or local_ip_towards(spike_ip)
    url = f"http://{host_ip}:{server.server_port}/{token}/{image.name}"
    print(f"Pi at {spike_ip}; serving {image.name} ({size / 1e6:.0f} MB, sha256 {sha[:12]}…)")

    ser.flush()
    ser.write_line(f"cb-reflash {url} {sha} {size}")
    start = time.monotonic()
    for _, line in ser.lines(start + 900):
        text = line.decode(errors="replace")
        if "CB_REFLASH" in text:
            print("  " + text)
        if "CB_REFLASH_FAILED" in text:
            sys.exit("cb-reflash failed before writing; the card is untouched")
        if "CB_REFLASH_BAD" in text:
            sys.exit("Image on the card does not match — card needs attention (power left on)")
        if "CB_REFLASH_OK" in text:
            break
    else:
        sys.exit("No CB_REFLASH_OK within 15 minutes")
    print(f"written and verified in {time.monotonic() - start:.0f} s; waiting for the new image to boot")
    wait_booted(ser, timeout=120)
    for line in shell(ser, "cb-slot", timeout=5):
        if SLOT_STATUS.search(line):
            print("  " + line.decode(errors="replace"))
    server.shutdown()


def cmd_power(args) -> None:
    plug = shelly()
    if args.state == "status":
        print(json.dumps(plug.status(), indent=2))
        return
    require_yes(args)
    plug.set(args.state == "on")
    print(f"switch {plug.switch} on {plug.host}: {args.state}")


def cmd_log(_args) -> None:
    ser = Serial(serial_port())
    start = time.monotonic()
    for t, line in ser.lines(float("inf")):
        print(f"{(t - start):9.3f} s  {line.decode(errors='replace')}", flush=True)


def cmd_boots(args) -> None:
    require_yes(args)
    plug, ser = shelly(), Serial(serial_port())
    RESULTS.mkdir(parents=True, exist_ok=True)
    stamp = time.strftime("%Y%m%d-%H%M%S")
    csv_path, log_path = RESULTS / f"boots-{stamp}.csv", RESULTS / f"boots-{stamp}.log"
    times = []
    with open(csv_path, "w") as csv, open(log_path, "w") as log:
        csv.write("boot,ms_to_marker,slot,kernel_boottime_ms,pre_kernel_ms,reliable\n")
        for i in range(1, args.n + 1):
            result = boot_once(plug, ser, log, args.off, args.timeout)
            if result is None:
                csv.write(f"{i},,,,\n")
                print(f"boot {i}: NO FIRST FRAME within {args.timeout} s")
                continue
            ms, slot, kernel_ms, reliable = result
            if reliable:
                times.append(ms)
            csv.write(f"{i},{ms:.1f},{slot},{kernel_ms},{ms - kernel_ms:.1f},{int(reliable)}\n")
            note = "" if reliable else "  [plug retry: excluded from statistics]"
            print(f"boot {i}: {ms:7.1f} ms to {MARKER} (slot {slot}, kernel+userspace {kernel_ms} ms){note}")
    if times:
        times.sort()
        p95 = times[min(len(times) - 1, int(round(0.95 * len(times))) - 1)]
        print(f"\n{len(times)}/{args.n} boots: p50 {statistics.median(times):.0f} ms, p95 {p95:.0f} ms, "
              f"max {times[-1]:.0f} ms")
    print(f"results: {csv_path}\nlog:     {log_path}")


def cmd_powercut(args) -> None:
    require_yes(args)
    plug, ser = shelly(), Serial(serial_port())
    RESULTS.mkdir(parents=True, exist_ok=True)
    stamp = time.strftime("%Y%m%d-%H%M%S")
    log_path = RESULTS / f"powercut-{stamp}.log"
    slots = {}
    with open(log_path, "w") as log:
        for i in range(1, args.n + 1):
            result = boot_once(plug, ser, log, args.off, args.timeout)
            if result is None:
                try:
                    wait_booted(ser, timeout=1, max_uptime=args.timeout + 20)
                    result = (float("nan"), "?", 0, False)
                except RuntimeError:
                    print(f"cut {i}: node did NOT boot — stopping for inspection (power left on)")
                    plug.set(True)
                    sys.exit(1)
            time.sleep(1)
            slot, _ = slot_status(ser)
            slots[slot] = slots.get(slot, 0) + 1
            fsck = b"\n".join(shell(ser, "fsck.fat -n /dev/mmcblk0p1; echo FSCK_RC=$?", timeout=30))
            if b"FSCK_RC=0" not in fsck:
                print(f"cut {i}: boot partition FAT check FAILED — stopping for inspection (power left on)")
                print(fsck.decode(errors="replace"))
                sys.exit(1)
            started = False
            for _ in range(3):
                ser.write_line("")
                time.sleep(0.5)
                ser.write_line("cb-commit-loop")
                if wait_for(ser, rb"CB_COMMIT_LOOP", 10):
                    started = True
                    break
            if not started:
                print(f"cut {i}: cb-commit-loop did not start — stopping for inspection")
                sys.exit(1)
            time.sleep(random.uniform(0.0, args.max_delay))
            plug.set(False)
            print(f"cut {i}: booted slot {slot} in {result[0]:.0f} ms, FAT clean, cut during commit loop")
    print(f"\n{args.n} cuts survived; boots per slot: {slots}\nlog: {log_path}")
    plug.set(True)


def cmd_run(args) -> None:
    require_yes(args)
    plug, ser = shelly(), Serial(serial_port())
    with open(os.devnull, "w") as log:
        result = boot_once(plug, ser, log, args.off, args.timeout)
    if result is None:
        sys.exit("no first frame")
    print(f"booted slot {result[1]} in {result[0]:.0f} ms")
    time.sleep(1)
    ser.write_line(args.command)
    for _, line in ser.lines(time.monotonic() + args.wait):
        print(line.decode(errors="replace"))


def main() -> None:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--yes", action="store_true", help="confirm that power may be switched")
    p.add_argument("--off", type=float, default=3.0, help="seconds without power before each boot")
    p.add_argument("--timeout", type=float, default=30.0, help="seconds to wait for the first frame")
    sub = p.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("power")
    s.add_argument("state", choices=["on", "off", "status"])
    s.set_defaults(func=cmd_power)
    sub.add_parser("log").set_defaults(func=cmd_log)
    s = sub.add_parser("boots")
    s.add_argument("n", type=int)
    s.set_defaults(func=cmd_boots)
    s = sub.add_parser("powercut")
    s.add_argument("n", type=int)
    s.add_argument("--max-delay", type=float, default=2.0, help="max seconds into the commit loop before cutting")
    s.set_defaults(func=cmd_powercut)
    sub.add_parser("slots", help="tryboot fallback and commit checks").set_defaults(func=cmd_slots)
    s = sub.add_parser("reflash", help="write an image to the Pi's card over Wi-Fi (lab only)")
    s.add_argument("image")
    s.set_defaults(func=cmd_reflash)
    s = sub.add_parser("run")
    s.add_argument("command")
    s.add_argument("--wait", type=float, default=5.0)
    s.set_defaults(func=cmd_run)
    p.add_argument("--marker", choices=["frame", "init"], default="frame",
                   help="end of boot: first frame on HDMI (default) or start of init (no display needed)")
    args = p.parse_args()
    global MARKER
    MARKER = args.marker
    args.func(args)


if __name__ == "__main__":
    main()
