# SPDX-License-Identifier: MIT OR Apache-2.0
#
# Developer tasks. `just check` runs everything CI runs (CONTRIBUTING.md).

set shell := ["bash", "-euo", "pipefail", "-c"]

# Homebrew installs LLVM keg-only; make its clang-format/clang-tidy visible.
export PATH := "/opt/homebrew/opt/llvm/bin:/usr/local/opt/llvm/bin:" + env("PATH")

render_build := "build/render"

# Lab Wi-Fi credentials for spike images (SSID=, PSK=, COUNTRY=); kept outside
# the repository and mounted read-only into the build container only if present.
lab_wifi := env("HOME") + "/.config/signalare/chalkboard-lab-wifi.env"
os_docker := "docker run --rm -v chalkboard-os:/build -v \"$PWD\":/src:ro -v \"$PWD/build/os\":/out " + \
    "$( [ -r " + lab_wifi + " ] && echo -v " + lab_wifi + ":/secrets/wifi.env:ro ) chalkboard-buildenv"

# List available recipes.
default:
    @just --list

# Run every formatter check, linter and test (mirrors CI).
check: check-spdx check-md check-shell check-agent check-render check-web

# Apply all formatters.
fmt: install-web
    cargo fmt --manifest-path agent/Cargo.toml --all
    git ls-files -z --cached --others --exclude-standard -- 'render/*.c' 'render/*.h' | xargs -0 clang-format -i
    pnpm --dir web run format

# Install the git pre-commit hook that runs `just check`.
install-hooks:
    install -m 0755 tools/pre-commit.sh "$(git rev-parse --git-path hooks)/pre-commit"
    @echo "pre-commit hook installed"

# Every source file carries the SPDX header (LIC-001).
check-spdx:
    tools/check-spdx.sh

# Markdown style.
check-md: install-web
    pnpm run lint:md

# Shell scripts.
check-shell:
    git ls-files -z --cached --others --exclude-standard -- '*.sh' | xargs -0 shellcheck

# Rust agent: formatting, lints as errors, tests.
check-agent:
    cargo fmt --manifest-path agent/Cargo.toml --all --check
    cargo clippy --manifest-path agent/Cargo.toml --workspace --all-targets --locked -- -D warnings
    cargo test --manifest-path agent/Cargo.toml --workspace --locked

# C renderer: formatting, build with -Werror, clang-tidy, tests.
check-render:
    git ls-files -z --cached --others --exclude-standard -- 'render/*.c' 'render/*.h' | xargs -0 clang-format --dry-run --Werror
    cmake -S render -B {{render_build}} -DCMAKE_BUILD_TYPE=Debug > /dev/null
    cmake --build {{render_build}} --parallel
    git ls-files -z --cached --others --exclude-standard -- 'render/*.c' | xargs -0 clang-tidy --quiet -p {{render_build}}
    ctest --test-dir {{render_build}} --output-on-failure

# Svelte web app: formatting, lints, type check, build.
check-web: install-web
    pnpm --dir web run format:check
    pnpm --dir web run lint
    pnpm --dir web run check
    pnpm --dir web run build

# Install JavaScript dependencies exactly as locked.
install-web:
    pnpm install --frozen-lockfile --silent

# Fuzz the template parser (TPL-011; needs nightly Rust and cargo-fuzz).
fuzz-template seconds="60":
    mkdir -p build/fuzz/template
    cp schema/examples/*.json schema/tests/invalid/*.json build/fuzz/template/
    cd agent/chalkboard-template && cargo +nightly fuzz run validate ../../build/fuzz/template -- -max_total_time={{seconds}}

# Build the OS build container (Buildroot needs Linux).
os-buildenv:
    docker build -q -t chalkboard-buildenv tools/buildenv

# Build an OS image from os/configs/<defconfig>; images land in build/os/.
# Pass a Buildroot target to run it first, e.g. `just os-build chalkboard_defconfig`
# (Buildroot does not notice source changes in local packages).
os-build defconfig="chalkboard_defconfig" target="": os-buildenv
    mkdir -p build/os
    {{ if target != "" { os_docker + " /src/tools/buildenv/build.sh " + defconfig + " " + target } else { "true" } }}
    {{os_docker}} /src/tools/buildenv/build.sh {{defconfig}}

# Open a shell in the OS build container (Buildroot tree in /build).
os-shell: os-buildenv
    docker run --rm -it -v chalkboard-os:/build -v "$PWD":/src:ro -v "$PWD/build/os":/out chalkboard-buildenv bash

# Remove build outputs.
clean:
    rm -rf build web/dist
    cargo clean --manifest-path agent/Cargo.toml
