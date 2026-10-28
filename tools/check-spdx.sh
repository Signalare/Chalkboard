#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
#
# Fails if any tracked or staged source file lacks the project SPDX header
# within its first five lines (LIC-001).

set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

header="SPDX-License-Identifier: MIT OR Apache-2.0"
missing=0

while IFS= read -r -d '' file; do
  [[ -f "$file" ]] || continue
  if ! head -n 5 "$file" | grep -qF "$header"; then
    echo "missing SPDX header: $file" >&2
    missing=$((missing + 1))
  fi
done < <(git ls-files -z --cached --others --exclude-standard -- \
  '*.rs' '*.c' '*.h' '*.ts' '*.js' '*.svelte' '*.sh' '*.py' \
  'CMakeLists.txt' '*.cmake' '*.toml' 'justfile' 'index.html' '*.yaml' \
  ':!:pnpm-lock.yaml' ':!:.github/**')

if ((missing > 0)); then
  echo "$missing file(s) without '$header'" >&2
  exit 1
fi
echo "SPDX headers ok"
