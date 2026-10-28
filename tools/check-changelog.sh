#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# Fails unless the changes since <base> touch CHANGELOG.md. Set
# SKIP_CHANGELOG=true for internal-only changes (the `skip-changelog` label).
#
# Usage: tools/check-changelog.sh <base commit>

set -euo pipefail

base="${1:?usage: $0 <base commit>}"

if [[ "${SKIP_CHANGELOG:-false}" == "true" ]]; then
  echo "Changelog check skipped: pull request is labelled skip-changelog"
  exit 0
fi

if git diff --name-only "$base"...HEAD | grep -qx 'CHANGELOG.md'; then
  echo "CHANGELOG.md updated"
  exit 0
fi

cat >&2 <<'EOF'
CHANGELOG.md was not updated.

Add a user-facing entry under "## [Unreleased]" in the matching section
(Added, Changed, Deprecated, Removed, Fixed or Security). For internal-only
changes, ask a maintainer to apply the `skip-changelog` label.

See CONTRIBUTING.md#changelog.
EOF
exit 1
