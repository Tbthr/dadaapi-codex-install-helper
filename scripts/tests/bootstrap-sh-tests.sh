#!/bin/sh

set -eu

script_directory=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
DADA_ASSISTANT_BOOTSTRAP_LIBRARY_ONLY=1
export DADA_ASSISTANT_BOOTSTRAP_LIBRARY_ONLY
. "$script_directory/../bootstrap.sh"

repository_root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
expected_release_tag=$(sed -n 's/^[[:space:]]*"version":[[:space:]]*"\([0-9][0-9]*\.[0-9][0-9]*\.[0-9][0-9]*\)".*/\1/p' \
  "$repository_root/apps/desktop/src-tauri/tauri.conf.json" | head -n 1)
[ -n "$expected_release_tag" ] || {
  printf 'Unable to read the desktop version.\n' >&2
  exit 1
}
expected_release_tag="v$expected_release_tag"
gitee_script_url="https://gitee.com/lyq_power/dadaapi-codex-install-helper/raw/$expected_release_tag/scripts/install.sh"
github_script_url="https://raw.githubusercontent.com/Tbthr/dadaapi-codex-install-helper/$expected_release_tag/scripts/install.sh"

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

assert_equal "bootstrap installer tag is pinned" "$expected_release_tag" "$installer_script_tag"
assert_success "auto source is accepted" validate_install_source auto
assert_success "Gitee source is accepted" validate_install_source gitee
assert_success "GitHub source is accepted" validate_install_source github
assert_failure "unknown source is rejected" validate_install_source mirror
assert_equal "Gitee script URL is immutable" \
  "$gitee_script_url" \
  "$(installer_script_url gitee)"
assert_equal "GitHub script URL is immutable" \
  "$github_script_url" \
  "$(installer_script_url github)"
assert_failure "non-HTTPS installer URLs are rejected" ensure_https_url 'http://example.test/install.sh'
assert_equal "transport failures may fall back" retryable "$(classify_http_result 18 000)"
assert_equal "5xx responses may fall back" retryable "$(classify_http_result 0 503)"
assert_equal "4xx responses do not fall back" fatal "$(classify_http_result 0 404)"
assert_equal "successful responses continue" success "$(classify_http_result 0 200)"
assert_success "auto mode falls back after a Gitee transport failure" should_fallback_to_github auto gitee 10
assert_failure "auto mode rejects a Gitee policy failure" should_fallback_to_github auto gitee 11
assert_failure "explicit Gitee mode never falls back" should_fallback_to_github gitee gitee 10
assert_equal "installer script SHA-256 matches the checked-in source" \
  "$installer_script_sha256" \
  "$(/usr/bin/shasum -a 256 "$script_directory/../install.sh" | /usr/bin/awk '{ print tolower($1) }')"

printf 'bootstrap.sh tests passed: %s\n' "$tests_run"
