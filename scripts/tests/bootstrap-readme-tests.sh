#!/bin/sh

set -eu

repository_root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
readme="$repository_root/README.md"
tauri_conf="$repository_root/apps/desktop/src-tauri/tauri.conf.json"

desktop_version=$(sed -n 's/^[[:space:]]*"version":[[:space:]]*"\([0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*\)".*/\1/p' "$tauri_conf" | head -n 1)
[ -n "$desktop_version" ] || {
  printf 'Unable to read the desktop version from %s.\n' "$tauri_conf" >&2
  exit 1
}
release_tag="v$desktop_version"
windows_hash=$(sed -n 's/.*\$scriptHash = "\([0-9a-f]\{64\}\)".*/\1/p' "$readme")
macos_hash=$(sed -n "s/.* script_hash='\([0-9a-f]\{64\}\)'.*/\1/p" "$readme")
ps_sha256=$(shasum -a 256 "$repository_root/scripts/install.ps1" | awk '{ print tolower($1) }')
sh_sha256=$(shasum -a 256 "$repository_root/scripts/install.sh" | awk '{ print tolower($1) }')

[ "$windows_hash" = "$ps_sha256" ] || {
  printf 'README Windows installer SHA-256 is stale.\n' >&2
  exit 1
}
[ "$macos_hash" = "$sh_sha256" ] || {
  printf 'README macOS installer SHA-256 is stale.\n' >&2
  exit 1
}

grep -Fq "https://gitee.com/lyq_power/dadaapi-codex-install-helper/raw/$release_tag/scripts/install.ps1" "$readme"
grep -Fq "https://gitee.com/lyq_power/dadaapi-codex-install-helper/raw/$release_tag/scripts/install.sh" "$readme"
grep -Fq "https://raw.githubusercontent.com/Tbthr/dadaapi-codex-install-helper/$release_tag/scripts/install.ps1" "$readme"
grep -Fq "https://raw.githubusercontent.com/Tbthr/dadaapi-codex-install-helper/$release_tag/scripts/install.sh" "$readme"

readme_tags=$(grep -Eo 'v[0-9]+\.[0-9]+\.[0-9]+' "$readme" | LC_ALL=C sort -u)
[ "$readme_tags" = "$release_tag" ] || {
  printf 'README must reference only the current release tag %s (found: %s).\n' \
    "$release_tag" "$readme_tags" >&2
  exit 1
}
if grep -Eq 'curl.*\|[[:space:]]*(/bin/)?sh([[:space:]]|$)|irm.*\|[[:space:]]*iex([[:space:]]|$)' "$readme"; then
  printf 'README must not use a pipe-to-interpreter install command.\n' >&2
  exit 1
fi

printf 'Release README contract tests passed (tag %s).\n' "$release_tag"
