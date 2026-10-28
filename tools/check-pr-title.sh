#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# Checks that a pull request title is a Conventional Commit with an allowed
# scope, because it becomes the squash commit message (CONTRIBUTING.md).
#
# Usage: tools/check-pr-title.sh "feat(cec): re-assert active source"

set -euo pipefail

title="${1:?usage: $0 <pull request title>}"

types="feat|fix|perf|refactor|test|docs|build|ci|chore|revert"
scopes="render|tpl|agent|sync|net|cec|time|web|sec|prov|upd|os|spec|docs|ci"
pattern="^(${types})\((${scopes})\)!?: [a-z0-9\`].*[^.]$"

if [[ "$title" =~ $pattern ]]; then
  echo "PR title ok: $title"
  exit 0
fi

cat >&2 <<EOF
PR title is not a valid Conventional Commit:

  $title

Expected: <type>(<scope>): <summary in imperative mood, lower case, no period>
  types:  ${types//|/, }
  scopes: ${scopes//|/, }
  breaking changes: add '!' after the scope, e.g. feat(sync)!: …

See CONTRIBUTING.md#commit-messages.
EOF
exit 1
