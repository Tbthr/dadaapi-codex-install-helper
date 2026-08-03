#!/bin/sh

set -eu

repository_root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
readme="$repository_root/README.md"
release_tag="v1.0.0"
release_ps_hash="99ce3a2b09fbbd15523799fc6f9389207cec9632b959ad754b59ce6bc5bd270d"
release_sh_hash="410984f39ad07e00e3e891e1a425cca3a5bbe98d6462a999d0128e838f6c6b2f"

windows_hash=$(sed -n 's/.*\$scriptHash = "\([0-9a-f]\{64\}\)".*/\1/p' "$readme")
macos_hash=$(sed -n "s/.* script_hash='\([0-9a-f]\{64\}\)'.*/\1/p" "$readme")

[ "$windows_hash" = "$release_ps_hash" ] || {
  printf 'README Windows installer SHA-256 is stale.\n' >&2
  exit 1
}
[ "$macos_hash" = "$release_sh_hash" ] || {
  printf 'README macOS installer SHA-256 is stale.\n' >&2
  exit 1
}

grep -Fq "https://gitee.com/lyq_power/dadaapi-codex-install-helper/raw/$release_tag/scripts/install.ps1" "$readme"
grep -Fq "https://gitee.com/lyq_power/dadaapi-codex-install-helper/raw/$release_tag/scripts/install.sh" "$readme"
grep -Fq "https://raw.githubusercontent.com/Tbthr/dadaapi-codex-install-helper/$release_tag/scripts/install.ps1" "$readme"
grep -Fq "https://raw.githubusercontent.com/Tbthr/dadaapi-codex-install-helper/$release_tag/scripts/install.sh" "$readme"
if grep -Fq 'v1.0.1' "$readme"; then
  printf 'README must not mention unreleased v1.0.1.\n' >&2
  exit 1
fi
if grep -Eq 'curl.*\|[[:space:]]*(/bin/)?sh([[:space:]]|$)|irm.*\|[[:space:]]*iex([[:space:]]|$)' "$readme"; then
  printf 'README must not use a pipe-to-interpreter install command.\n' >&2
  exit 1
fi

printf 'Release README contract tests passed.\n'
