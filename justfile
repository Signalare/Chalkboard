# SPDX-License-Identifier: MIT OR Apache-2.0
#
# Developer tasks. `just check` runs everything CI runs (CONTRIBUTING.md).

set shell := ["bash", "-euo", "pipefail", "-c"]

# Homebrew installs LLVM keg-only; make its clang-format/clang-tidy visible.
export PATH := "/opt/homebrew/opt/llvm/bin:/usr/local/opt/llvm/bin:" + env("PATH")

render_build := "build/render"

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

# Remove build outputs.
clean:
    rm -rf build web/dist
    cargo clean --manifest-path agent/Cargo.toml
