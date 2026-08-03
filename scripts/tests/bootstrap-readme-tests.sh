#!/bin/sh

set -eu

repository_root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
readme="$repository_root/README.md"
workspace_version=$(sed -n 's/^version = "\([0-9][0-9.]*\)"$/\1/p' "$repository_root/Cargo.toml")
root_package_version=$(sed -n 's/^  "version": "\([0-9][0-9.]*\)",$/\1/p' "$repository_root/package.json")
desktop_package_version=$(sed -n 's/^  "version": "\([0-9][0-9.]*\)",$/\1/p' "$repository_root/apps/desktop/package.json")
tauri_version=$(sed -n 's/^  "version": "\([0-9][0-9.]*\)",$/\1/p' "$repository_root/apps/desktop/src-tauri/tauri.conf.json")
bootstrap_tag=$(sed -n 's/^installer_script_tag="\(v[0-9][0-9.]*\)"$/\1/p' "$repository_root/scripts/bootstrap.sh")
bootstrap_ps_tag=$(sed -n 's/^\$InstallerScriptTag = "\(v[0-9][0-9.]*\)"$/\1/p' "$repository_root/scripts/bootstrap.ps1")
bootstrap_installer_hash=$(sed -n 's/^installer_script_sha256="\([0-9a-f]\{64\}\)"$/\1/p' "$repository_root/scripts/bootstrap.sh")
bootstrap_ps_installer_hash=$(sed -n 's/^\$InstallerScriptSha256 = "\([0-9a-f]\{64\}\)"$/\1/p' "$repository_root/scripts/bootstrap.ps1")

[ -n "$workspace_version" ]
[ "$workspace_version" = "$root_package_version" ]
[ "$workspace_version" = "$desktop_package_version" ]
[ "$workspace_version" = "$tauri_version" ]
[ "v$workspace_version" = "$bootstrap_tag" ]
[ "$bootstrap_tag" = "$bootstrap_ps_tag" ]
[ "$bootstrap_installer_hash" = "$(/usr/bin/shasum -a 256 "$repository_root/scripts/install.sh" | /usr/bin/awk '{ print tolower($1) }')" ]
[ "$bootstrap_ps_installer_hash" = "$(/usr/bin/shasum -a 256 "$repository_root/scripts/install.ps1" | /usr/bin/awk '{ print tolower($1) }')" ]

windows_hash=$(sed -n 's/.*\$h="\([0-9a-f]\{64\}\)".*/\1/p' "$readme")
macos_hash=$(sed -n "s/.* h='\([0-9a-f]\{64\}\)'.*/\1/p" "$readme")
actual_windows_hash=$(/usr/bin/shasum -a 256 "$repository_root/scripts/bootstrap.ps1" | /usr/bin/awk '{ print tolower($1) }')
actual_macos_hash=$(/usr/bin/shasum -a 256 "$repository_root/scripts/bootstrap.sh" | /usr/bin/awk '{ print tolower($1) }')

[ "$windows_hash" = "$actual_windows_hash" ] || {
  printf 'README Windows Bootstrap SHA-256 is stale.\n' >&2
  exit 1
}
[ "$macos_hash" = "$actual_macos_hash" ] || {
  printf 'README macOS Bootstrap SHA-256 is stale.\n' >&2
  exit 1
}

grep -Fq "https://gitee.com/lyq_power/dadaapi-codex-install-helper/raw/$bootstrap_tag/scripts/bootstrap.ps1" "$readme"
grep -Fq "https://gitee.com/lyq_power/dadaapi-codex-install-helper/raw/$bootstrap_tag/scripts/bootstrap.sh" "$readme"
grep -Fq "https://raw.githubusercontent.com/Tbthr/dadaapi-codex-install-helper/$bootstrap_tag/scripts/bootstrap.ps1" "$readme"
grep -Fq "https://raw.githubusercontent.com/Tbthr/dadaapi-codex-install-helper/$bootstrap_tag/scripts/bootstrap.sh" "$readme"
if grep -Eq 'curl.*\|[[:space:]]*(/bin/)?sh([[:space:]]|$)|irm.*\|[[:space:]]*iex([[:space:]]|$)' "$readme"; then
  printf 'README must not use a pipe-to-interpreter Bootstrap command.\n' >&2
  exit 1
fi

printf 'Bootstrap README contract tests passed.\n'
