#!/usr/bin/env bash

set -euo pipefail

repository_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
source "$repository_root/scripts/release/sync-gitee-locally.sh"

fixture_directory=$(mktemp -d "${TMPDIR:-/tmp}/dada-sync-gitee-locally-tests.XXXXXX")
trap 'rm -rf "$fixture_directory"' EXIT

# --- sync_gitee_valid_tag ------------------------------------------------------

sync_gitee_valid_tag v1.0.1
sync_gitee_valid_tag v0.0.0
if sync_gitee_valid_tag 1.0.1; then
  echo "A tag without the v prefix must be rejected." >&2
  exit 1
fi
if sync_gitee_valid_tag v1.0; then
  echo "A two-part tag must be rejected." >&2
  exit 1
fi
if sync_gitee_valid_tag v1.0.1-rc1; then
  echo "A prerelease suffix must be rejected." >&2
  exit 1
fi
if sync_gitee_valid_tag ""; then
  echo "An empty tag must be rejected." >&2
  exit 1
fi

# --- sync_gitee_github_repository ----------------------------------------------

test "$(sync_gitee_github_repository 'https://github.com/Tbthr/dadaapi-codex-install-helper.git')" = \
  "Tbthr/dadaapi-codex-install-helper"
test "$(sync_gitee_github_repository 'https://github.com/Tbthr/dadaapi-codex-install-helper')" = \
  "Tbthr/dadaapi-codex-install-helper"
test "$(sync_gitee_github_repository 'git@github.com:Tbthr/dadaapi-codex-install-helper.git')" = \
  "Tbthr/dadaapi-codex-install-helper"
if sync_gitee_github_repository 'git@gitee.com:other/repo.git' >/dev/null 2>&1; then
  echo "A non-GitHub remote must be rejected." >&2
  exit 1
fi

# --- sync_gitee_load_env --------------------------------------------------------

env_file="$fixture_directory/.env"
cat > "$env_file" <<'EOF'
# 本地同步配置
GITEE_REPOSITORY=lyq_power/dadaapi-codex-install-helper
GITEE_USERNAME=lyq_power
GITEE_TOKEN=local-secret-123
EOF
unset GITEE_REPOSITORY GITEE_USERNAME GITEE_TOKEN || true
SYNC_GITEE_ENV_FILE="$env_file" sync_gitee_load_env
test "${GITEE_REPOSITORY:-}" = "lyq_power/dadaapi-codex-install-helper"
test "${GITEE_USERNAME:-}" = "lyq_power"
test "${GITEE_TOKEN:-}" = "local-secret-123"

unset GITEE_REPOSITORY GITEE_USERNAME GITEE_TOKEN || true
SYNC_GITEE_ENV_FILE="$fixture_directory/missing.env" sync_gitee_load_env
test -z "${GITEE_REPOSITORY:-}"
test -z "${GITEE_TOKEN:-}"

partial_env="$fixture_directory/.env-partial"
cat > "$partial_env" <<'EOF'
GITEE_REPOSITORY="lyq_power/dadaapi-codex-install-helper"
GITEE_TOKEN=
GITEE_USERNAME='lyq_power'
BAD-KEY=should-be-skipped
EOF
export GITEE_TOKEN=external-token
SYNC_GITEE_ENV_FILE="$partial_env" sync_gitee_load_env
test "${GITEE_REPOSITORY:-}" = "lyq_power/dadaapi-codex-install-helper"
test "${GITEE_TOKEN:-}" = "external-token"
test "${GITEE_USERNAME:-}" = "lyq_power"
test -z "${BAD_KEY:-}"
unset GITEE_REPOSITORY GITEE_USERNAME GITEE_TOKEN || true

# --- sync_gitee_download_with_curl (fake curl + fake jq-free JSON) -------------

mkdir -p "$fixture_directory/bin"
cat > "$fixture_directory/bin/curl" <<'EOF'
#!/usr/bin/env bash
args=("$@")
output=""
for ((i = 0; i < ${#args[@]}; i++)); do
  if [ "${args[$i]}" = "-o" ]; then
    output="${args[$((i + 1))]}"
  fi
done
printf '%s\n' "$@" >> "$SYNC_GITEE_TEST_ARGUMENTS"
if [ -n "${SYNC_GITEE_TEST_RELEASE_BODY:-}" ]; then
  if [ -n "$output" ]; then
    printf '%s' "$SYNC_GITEE_TEST_RELEASE_BODY" > "$output"
  else
    printf '%s' "$SYNC_GITEE_TEST_RELEASE_BODY"
  fi
fi
exit "${SYNC_GITEE_TEST_EXIT:-0}"
EOF
chmod 700 "$fixture_directory/bin/curl"

asset_json='{"assets": [
  {"name": "checksums.txt", "browser_download_url": "https://github.com/Tbthr/dadaapi-codex-install-helper/releases/download/v1.0.1/checksums.txt"},
  {"name": "Dada-Assistant_1.0.1_x64-setup.exe", "browser_download_url": "https://github.com/Tbthr/dadaapi-codex-install-helper/releases/download/v1.0.1/Dada-Assistant_1.0.1_x64-setup.exe"},
  {"name": "Dada-Assistant_1.0.1_arm64-setup.exe", "browser_download_url": "https://github.com/Tbthr/dadaapi-codex-install-helper/releases/download/v1.0.1/Dada-Assistant_1.0.1_arm64-setup.exe"},
  {"name": "Dada-Assistant_1.0.1_universal.dmg", "browser_download_url": "https://github.com/Tbthr/dadaapi-codex-install-helper/releases/download/v1.0.1/Dada-Assistant_1.0.1_universal.dmg"}
]}'
assets_directory="$fixture_directory/assets"
mkdir -p "$assets_directory"
release_body="$fixture_directory/release.json"
printf '%s' "$asset_json" > "$release_body"

PATH="$fixture_directory/bin:$PATH"
SYNC_GITEE_TEST_ARGUMENTS="$fixture_directory/arguments.txt"
SYNC_GITEE_TEST_RELEASE_OUTPUT="$release_body"
SYNC_GITEE_TEST_RELEASE_BODY="$asset_json"
export PATH SYNC_GITEE_TEST_ARGUMENTS SYNC_GITEE_TEST_RELEASE_OUTPUT SYNC_GITEE_TEST_RELEASE_BODY

RELEASE_TAG=v1.0.1
export RELEASE_TAG
sync_gitee_download_with_curl \
  "Tbthr/dadaapi-codex-install-helper" "$assets_directory" \
  "Dada-Assistant_1.0.1_x64-setup.exe" \
  "Dada-Assistant_1.0.1_arm64-setup.exe" \
  "Dada-Assistant_1.0.1_universal.dmg"
grep -F -- "https://api.github.com/repos/Tbthr/dadaapi-codex-install-helper/releases/tags/v1.0.1" \
  "$SYNC_GITEE_TEST_ARGUMENTS" >/dev/null

echo "sync-gitee-locally tests passed."
