#!/usr/bin/env bash
# This file is part of Cygnus. Licensed under the GPL v3 or later.
#
# Fails if a Rust source file does not start with the GPL v3 license header
# defined in AGENTS.md. Only the year and the copyright holder may vary.
# Files ignored by git (target/, graft/, ...) are skipped.

set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

expected=(
  '// This file is part of Cygnus.'
  '__COPYRIGHT__'
  '//'
  '// Cygnus is free software: you can redistribute it and/or modify it under'
  '// the terms of the GNU General Public License as published by the Free'
  '// Software Foundation, either version 3 of the License, or (at your option)'
  '// any later version.'
  '//'
  '// Cygnus is distributed in the hope that it will be useful, but WITHOUT ANY'
  '// WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS'
  '// FOR A PARTICULAR PURPOSE. See the GNU General Public License for more'
  '// details.'
  '//'
  '// You should have received a copy of the GNU General Public License along'
  '// with Cygnus. If not, see <https://www.gnu.org/licenses/>.'
)
copyright_re='^// Copyright \(C\) [0-9]{4}(-[0-9]{4})? .+$'

failed=0
checked=0

while IFS= read -r -d '' file; do
  [ -f "$file" ] || continue
  checked=$((checked + 1))
  mapfile -t lines < <(head -n "${#expected[@]}" "$file" | tr -d '\r')
  for i in "${!expected[@]}"; do
    line="${lines[$i]:-}"
    if [ "${expected[$i]}" = '__COPYRIGHT__' ]; then
      [[ "$line" =~ $copyright_re ]] && continue
    elif [ "$line" = "${expected[$i]}" ]; then
      continue
    fi
    echo "missing or invalid license header: $file (line $((i + 1)))" >&2
    failed=1
    break
  done
done < <(git ls-files -z --cached --others --exclude-standard -- '*.rs')

if [ "$failed" -ne 0 ]; then
  exit 1
fi
echo "license headers OK ($checked files)"
