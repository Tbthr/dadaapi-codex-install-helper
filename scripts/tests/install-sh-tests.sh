#!/bin/sh

set -eu

script_directory=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
DADA_ASSISTANT_INSTALL_LIBRARY_ONLY=1
export DADA_ASSISTANT_INSTALL_LIBRARY_ONLY
. "$script_directory/../install.sh"

tests_run=0

assert_success() {
  description="$1"
  shift
  tests_run=$((tests_run + 1))
  if ! "$@"; then
    printf 'FAIL: %s\n' "$description" >&2
    exit 1
  fi
}

assert_failure() {
  description="$1"
  shift
  tests_run=$((tests_run + 1))
  if "$@"; then
    printf 'FAIL: %s\n' "$description" >&2
    exit 1
  fi
}

assert_equal() {
  description="$1"
  expected="$2"
  actual="$3"
  tests_run=$((tests_run + 1))
  if [ "$expected" != "$actual" ]; then
    printf 'FAIL: %s (expected %s, got %s)\n' "$description" "$expected" "$actual" >&2
    exit 1
  fi
}

fixture_directory=$(/usr/bin/mktemp -d "${TMPDIR:-/tmp}/dada-installer-tests.XXXXXX")
trap '/bin/rm -rf "$fixture_directory"' EXIT HUP INT TERM
hash_a="aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
hash_b="bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
hash_c="cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"

valid_checksums="$fixture_directory/checksums.txt"
printf '%s  %s\n%s  %s\n%s  %s\n' \
  "$hash_a" 'Dada-Assistant_1.0.0_x64-setup.exe' \
  "$hash_b" 'Dada-Assistant_1.0.0_arm64-setup.exe' \
  "$hash_c" 'Dada-Assistant_1.0.0_universal.dmg' > "$valid_checksums"

assert_success "latest version is accepted" validate_install_version latest
assert_success "pinned semantic version is accepted" validate_install_version v1.0.0
assert_failure "version without v is rejected" validate_install_version 1.0.0
assert_failure "prerelease version is rejected" validate_install_version v1.0.0-rc.1
assert_success "auto source is accepted" validate_install_source auto
assert_success "Gitee source is accepted" validate_install_source gitee
assert_success "GitHub source is accepted" validate_install_source github
assert_failure "unknown source is rejected" validate_install_source mirror
assert_equal "Apple Silicon architecture is selected" arm64 "$(normalize_macos_architecture arm64)"
assert_equal "Intel architecture is selected" x86_64 "$(normalize_macos_architecture x86_64)"
assert_failure "unsupported architecture is rejected" normalize_macos_architecture i386

assert_equal "transport failures may fall back" retryable "$(classify_http_result 18 000)"
assert_equal "DNS failures may fall back" retryable "$(classify_http_result 6 000)"
assert_equal "timeouts may fall back" retryable "$(classify_http_result 28 000)"
assert_equal "5xx responses may fall back" retryable "$(classify_http_result 0 503)"
assert_equal "oversized 5xx bodies may still fall back" retryable "$(classify_http_result 63 503)"
assert_equal "4xx responses do not fall back" fatal "$(classify_http_result 0 404)"
assert_equal "TLS certificate failures do not fall back" fatal "$(classify_http_result 60 000)"
assert_equal "redirect policy failures do not fall back" fatal "$(classify_http_result 47 302)"
assert_equal "local write failures do not fall back" fatal "$(classify_http_result 23 200)"
assert_equal "successful responses continue" success "$(classify_http_result 0 200)"
assert_success "auto mode falls back after a Gitee transport failure" should_fallback_to_github auto gitee 10
assert_failure "auto mode does not fall back after a Gitee policy failure" should_fallback_to_github auto gitee 11
assert_failure "explicit Gitee mode never falls back" should_fallback_to_github gitee gitee 10
assert_failure "GitHub failures never recurse" should_fallback_to_github auto github 10
assert_equal "Gitee latest uses the release API" \
  'https://gitee.com/api/v5/repos/lyq_power/dadaapi-codex-install-helper/releases/latest' \
  "$(gitee_latest_release_url)"
assert_failure "Gitee latest is never treated as a download tag" checksums_url gitee latest

assert_success "three-asset checksum contract is accepted" validate_checksum_contract "$valid_checksums"
assert_success "matching pinned macOS asset is selected" select_macos_asset "$valid_checksums" v1.0.0
assert_equal "macOS asset name is selected" 'Dada-Assistant_1.0.0_universal.dmg' "$asset_name"
assert_equal "macOS release version is selected" '1.0.0' "$release_version"
assert_failure "a different pinned version is rejected" select_macos_asset "$valid_checksums" v1.0.1

duplicate_checksums="$fixture_directory/duplicate-checksums.txt"
printf '%s  %s\n%s  %s\n%s  %s\n' \
  "$hash_a" 'Dada-Assistant_1.0.0_x64-setup.exe' \
  "$hash_b" 'Dada-Assistant_1.0.0_x64-setup.exe' \
  "$hash_c" 'Dada-Assistant_1.0.0_universal.dmg' > "$duplicate_checksums"
assert_failure "duplicate assets are rejected" validate_checksum_contract "$duplicate_checksums"

malformed_checksums="$fixture_directory/malformed-checksums.txt"
printf '%s  %s\n%s  %s\n%s  %s\n' \
  'not-a-sha256' 'Dada-Assistant_1.0.0_x64-setup.exe' \
  "$hash_b" 'Dada-Assistant_1.0.0_arm64-setup.exe' \
  "$hash_c" 'Dada-Assistant_1.0.0_universal.dmg' > "$malformed_checksums"
assert_failure "malformed hashes are rejected" validate_checksum_contract "$malformed_checksums"

wrong_name_checksums="$fixture_directory/wrong-name-checksums.txt"
printf '%s  %s\n%s  %s\n%s  %s\n' \
  "$hash_a" 'Other_1.0.0_x64-setup.exe' \
  "$hash_b" 'Dada-Assistant_1.0.0_arm64-setup.exe' \
  "$hash_c" 'Dada-Assistant_1.0.0_universal.dmg' > "$wrong_name_checksums"
assert_failure "unexpected asset prefixes are rejected" validate_checksum_contract "$wrong_name_checksums"

final_release="$fixture_directory/gitee-final.json"
printf '%s' '{"tag_name":"v1.0.0","prerelease":false}' > "$final_release"
assert_equal "Gitee latest final version is parsed" v1.0.0 "$(parse_gitee_latest_version "$final_release")"
prerelease="$fixture_directory/gitee-prerelease.json"
printf '%s' '{"tag_name":"v1.0.0","prerelease":true}' > "$prerelease"
assert_failure "Gitee prereleases are rejected as latest" parse_gitee_latest_version "$prerelease"
invalid_latest="$fixture_directory/gitee-invalid-latest.json"
printf '%s' '{"tag_name":"release-1","prerelease":false}' > "$invalid_latest"
assert_failure "invalid Gitee latest tags are rejected" parse_gitee_latest_version "$invalid_latest"

payload="$fixture_directory/payload"
printf '%s' 'verified payload' > "$payload"
payload_hash=$(/usr/bin/shasum -a 256 "$payload" | /usr/bin/awk '{print $1}')
assert_success "matching SHA-256 is accepted" verify_sha256 "$payload" "$payload_hash"
assert_failure "incorrect SHA-256 is rejected" verify_sha256 "$payload" "$hash_a"

old_app="$fixture_directory/Applications/哒哒助手.app"
new_app="$fixture_directory/new/哒哒助手.app"
backup_app="$fixture_directory/previous.app"
/bin/mkdir -p "$old_app" "$new_app"
printf '%s' old > "$old_app/version"
printf '%s' new > "$new_app/version"
assert_success "existing application is replaced" replace_application "$new_app" "$old_app" "$backup_app"
assert_equal "new application becomes active" new "$(/bin/cat "$old_app/version")"
assert_equal "old application remains recoverable" old "$(/bin/cat "$backup_app/version")"

assert_failure "an invalid application bundle is rejected" verify_macos_application "$fixture_directory/invalid.app"
saved_home="$HOME"
HOME="$fixture_directory/user-home"
assert_equal "installation is scoped to the current user" "$HOME/Applications" "$(user_applications_directory)"
HOME="$saved_home"

release_version="1.0.5"
architecture="arm64"
metadata_source="github"
assert_equal "the install summary safely delimits variables before Chinese punctuation" \
  "下载与 SHA-256 校验完成：v1.0.5 / macOS Universal (arm64，来源：github)" \
  "$(print_install_summary)"

cleanup_directory="$fixture_directory/cleanup-target"
/bin/mkdir -p "$cleanup_directory"
temporary_directory="$cleanup_directory"
cleanup
assert_failure "temporary installation directory is cleaned" test -e "$cleanup_directory"
temporary_directory=""

printf 'install.sh tests passed: %s\n' "$tests_run"
