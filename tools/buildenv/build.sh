#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# Runs inside the build container: fetches the pinned Buildroot release,
# builds the given defconfig from the Chalkboard external tree and copies the
# resulting images to /out.
#
# Usage: build.sh <defconfig> [make target]

set -euo pipefail

defconfig="${1:?usage: $0 <defconfig> [make target]}"
target="${2:-all}"

BR_VERSION="2026.02.3"
BR_SHA256="5a59e7501b0b4ec52c41f4bfa79412320e0b37eae5f719605a258e8d0c6fc7fb"

br="/build/buildroot-${BR_VERSION}"
out="/build/out/${defconfig%_defconfig}"

if [[ ! -d "$br" ]]; then
  tarball="/build/buildroot-${BR_VERSION}.tar.xz"
  wget -q -O "$tarball" "https://buildroot.org/downloads/buildroot-${BR_VERSION}.tar.xz"
  echo "${BR_SHA256}  ${tarball}" | sha256sum -c -
  tar -xJf "$tarball" -C /build
  rm "$tarball"
fi

export BR2_DL_DIR=/build/dl
make -C "$br" BR2_EXTERNAL=/src/os O="$out" "$defconfig"
make -C "$br" O="$out" "$target"

if [[ "$target" == "all" ]]; then
  cp "$out"/images/*.img /out/
  ls -lh /out/*.img
fi
