#!/usr/bin/env bash

set -euo pipefail

repository_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
failed=false
while IFS= read -r script; do
  if LC_ALL=C perl -ne '
    if (/\$[A-Za-z_][A-Za-z0-9_]*[^\x00-\x7f]/) {
      print "$ARGV:$.:$_";
      $found = 1;
    }
    END { exit($found ? 0 : 1) }
  ' "$script"; then
    failed=true
  fi
done < <(find "$repository_root/scripts" -type f \( -name '*.sh' -o -name '*.ps1' \) -print)

if [ "$failed" = true ]; then
  echo 'Shell variable followed by non-ASCII text must use braces for Bash 3.2 portability.' >&2
  exit 1
fi

echo 'Shell non-ASCII variable boundary tests passed.'
