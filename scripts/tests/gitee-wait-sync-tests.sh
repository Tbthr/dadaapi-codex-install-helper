#!/usr/bin/env bash

set -euo pipefail

repository_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
source "$repository_root/scripts/release/wait-gitee-sync.sh"

fixture_directory=$(mktemp -d "${TMPDIR:-/tmp}/dada-gitee-wait-sync-tests.XXXXXX")
trap 'rm -rf "$fixture_directory"' EXIT
mkdir -p "$fixture_directory/bin"

RELEASE_TAG=v1.0.1
GITEE_REPOSITORY=lyq_power/dadaapi-codex-install-helper
GITEE_TOKEN=test-secret
export RELEASE_TAG GITEE_REPOSITORY GITEE_TOKEN

# --- gitee_wait_expected_names -------------------------------------------------

expected_names=$(printf '%s\n' \
  "Dada-Assistant_1.0.1_arm64-setup.exe" \
  "Dada-Assistant_1.0.1_universal.dmg" \
  "Dada-Assistant_1.0.1_x64-setup.exe" \
  "checksums.txt" | LC_ALL=C sort)
test "$(gitee_wait_expected_names | LC_ALL=C sort | diff -u - <(printf '%s\n' "$expected_names"))" = ""

# --- gitee_wait_release_complete ----------------------------------------------

release_final="$fixture_directory/release-final.json"
cat > "$release_final" <<'EOF'
{"id": 777, "tag_name": "v1.0.1", "prerelease": false}
EOF
attachments_full="$fixture_directory/attachments-full.json"
cat > "$attachments_full" <<'EOF'
[
  {"id": 1, "name": "checksums.txt", "size": 305},
  {"id": 2, "name": "Dada-Assistant_1.0.1_arm64-setup.exe", "size": 3788434},
  {"id": 3, "name": "Dada-Assistant_1.0.1_universal.dmg", "size": 12914102},
  {"id": 4, "name": "Dada-Assistant_1.0.1_x64-setup.exe", "size": 4244209}
]
EOF
gitee_wait_release_complete "$release_final" "$attachments_full"

cat > "$fixture_directory/release-prerelease.json" <<'EOF'
{"id": 777, "tag_name": "v1.0.1", "prerelease": true}
EOF
if gitee_wait_release_complete "$fixture_directory/release-prerelease.json" "$attachments_full"; then
  echo "A prerelease must not be considered complete." >&2
  exit 1
fi

cat > "$fixture_directory/attachments-missing.json" <<'EOF'
[
  {"id": 1, "name": "checksums.txt", "size": 305},
  {"id": 2, "name": "Dada-Assistant_1.0.1_arm64-setup.exe", "size": 3788434}
]
EOF
if gitee_wait_release_complete "$release_final" "$fixture_directory/attachments-missing.json"; then
  echo "A release missing assets must not be considered complete." >&2
  exit 1
fi

cat > "$fixture_directory/attachments-extra.json" <<'EOF'
[
  {"id": 1, "name": "checksums.txt", "size": 305},
  {"id": 2, "name": "Dada-Assistant_1.0.1_arm64-setup.exe", "size": 3788434},
  {"id": 3, "name": "Dada-Assistant_1.0.1_universal.dmg", "size": 12914102},
  {"id": 4, "name": "Dada-Assistant_1.0.1_x64-setup.exe", "size": 4244209},
  {"id": 5, "name": "extra.bin", "size": 1}
]
EOF
if gitee_wait_release_complete "$release_final" "$fixture_directory/attachments-extra.json"; then
  echo "A release with unexpected extra assets must not be considered complete." >&2
  exit 1
fi

printf 'not json' > "$fixture_directory/not-json.json"
if gitee_wait_release_complete "$fixture_directory/not-json.json" "$attachments_full"; then
  echo "Malformed release JSON must not be considered complete." >&2
  exit 1
fi

# --- gitee_wait_fetch_release / gitee_wait_fetch_attachments (fake curl) -------

cat > "$fixture_directory/bin/curl" <<'EOF'
#!/usr/bin/env bash
args=("$@")
output=""
for ((i = 0; i < ${#args[@]}; i++)); do
  if [ "${args[$i]}" = "-o" ]; then
    output="${args[$((i + 1))]}"
  fi
done
printf '%s\n' "$@" > "$GITEE_WAIT_TEST_ARGUMENTS"
if [ -n "${GITEE_WAIT_TEST_BODY:-}" ] && [ -n "$output" ]; then
  printf '%s' "$GITEE_WAIT_TEST_BODY" > "$output"
fi
printf '%s' "${GITEE_WAIT_TEST_STATUS:-200}"
exit "${GITEE_WAIT_TEST_EXIT:-0}"
EOF
chmod 700 "$fixture_directory/bin/curl"

arguments="$fixture_directory/arguments.txt"
release_response="$fixture_directory/fetch-release.json"
attachments_response="$fixture_directory/fetch-attachments.json"

PATH="$fixture_directory/bin:$PATH"
GITEE_WAIT_TEST_ARGUMENTS="$arguments"
export PATH GITEE_WAIT_TEST_ARGUMENTS

GITEE_WAIT_TEST_BODY='{"id": 777, "prerelease": false}'
export GITEE_WAIT_TEST_BODY
status=$(gitee_wait_fetch_release "$release_response")
test "$status" = 200
grep -F -- "https://gitee.com/api/v5/repos/$GITEE_REPOSITORY/releases/tags/$RELEASE_TAG" "$arguments" >/dev/null
grep -Fx -- "Authorization: token test-secret" "$arguments" >/dev/null
test "$(cat "$release_response")" = '{"id": 777, "prerelease": false}'

: > "$arguments"
status=$(gitee_wait_fetch_attachments 777 "$attachments_response")
test "$status" = 200
grep -F -- "https://gitee.com/api/v5/repos/$GITEE_REPOSITORY/releases/777/attach_files" "$arguments" >/dev/null
grep -Fx -- "per_page=100" "$arguments" >/dev/null
grep -Fx -- "Authorization: token test-secret" "$arguments" >/dev/null

# --- gitee_wait_main input validation ------------------------------------------

if ( RELEASE_TAG=not-a-tag GITEE_REPOSITORY=x GITEE_TOKEN=x gitee_wait_main ) >/dev/null 2>&1; then
  echo "Invalid RELEASE_TAG must be rejected." >&2
  exit 1
fi
if ( unset RELEASE_TAG; RELEASE_TAG= GITEE_REPOSITORY=x GITEE_TOKEN=x gitee_wait_main ) >/dev/null 2>&1; then
  echo "Missing RELEASE_TAG must be rejected." >&2
  exit 1
fi

echo "gitee-wait-sync tests passed."
