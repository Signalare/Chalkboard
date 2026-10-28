#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# Git pre-commit hook installed by `just install-hooks`: runs the same checks
# as CI before every commit.

set -euo pipefail

exec just check
