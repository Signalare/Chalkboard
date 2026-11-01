#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# Post-build adjustments for the Chalkboard universal OS image.
#
# Called by Buildroot with TARGET_DIR set.

set -euo pipefail

: "${TARGET_DIR:?}"

# Set default hostname (OS-002: node identity is not baked in)
echo "chalkboard" > "${TARGET_DIR}/etc/hostname"

# Ensure /data mount point exists
mkdir -p "${TARGET_DIR}/data"

# Lab Wi-Fi: if /secrets/wifi.env exists (SSID=, PSK=, COUNTRY=), derive PSK hash
secrets="/secrets/wifi.env"
conf="${TARGET_DIR}/etc/wpa_supplicant.conf"
rm -f "$conf"

if [[ -r "$secrets" ]]; then
  python3 -I - "$secrets" "$conf" <<'EOF'
import hashlib, os, sys
values = {}
for line in open(sys.argv[1], encoding="utf-8"):
    if "=" in line and not line.lstrip().startswith("#"):
        key, _, value = line.rstrip("\n").partition("=")
        values[key.strip()] = value
ssid, psk, country = values.get("SSID", ""), values.get("PSK", ""), values.get("COUNTRY", "")
if not ssid or not 8 <= len(psk) <= 63 or len(country) != 2:
    sys.exit("post-build: wifi.env needs SSID, PSK (8-63 chars) and a 2-letter COUNTRY")
digest = hashlib.pbkdf2_hmac("sha1", psk.encode(), ssid.encode(), 4096, 32).hex()
fd = os.open(sys.argv[2], os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
with os.fdopen(fd, "w") as f:
    f.write(f"country={country.upper()}\n")
    f.write("network={\n")
    f.write(f"    ssid={ssid.encode().hex()}\n")
    f.write(f"    psk={digest}\n")
    f.write("}\n")
print(f"post-build: lab Wi-Fi configured for SSID {ssid!r}")
EOF
fi
