#!/usr/bin/env bash

set -euo pipefail

repository_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
source "$repository_root/scripts/release/gitee-upload.sh"

fixture_directory=$(mktemp -d "${TMPDIR:-/tmp}/dada-gitee-upload-tests.XXXXXX")
trap 'rm -rf "$fixture_directory"' EXIT
mkdir -p "$fixture_directory/bin"

cat > "$fixture_directory/bin/curl" <<'EOF'
#!/usr/bin/env bash
printf '%s\n' "$@" > "$GITEE_UPLOAD_TEST_ARGUMENTS"
printf '%b' "${GITEE_UPLOAD_TEST_OUTPUT:-201\\t0.100000\\t0.200000\\t0.300000\\t4096}"
exit "${GITEE_UPLOAD_TEST_EXIT:-0}"
EOF
chmod 700 "$fixture_directory/bin/curl"

asset="$fixture_directory/Dada-Assistant_1.0.10_x64-setup.exe"
response="$fixture_directory/response.json"
arguments="$fixture_directory/arguments.txt"
log="$fixture_directory/upload.log"
printf '%4096s' '' > "$asset"
expected_hash=$(shasum -a 256 "$asset" | awk '{ print $1 }')
test "$(gitee_file_size "$asset")" = 4096
test "$(gitee_sha256 "$asset")" = "$expected_hash"
printf '%s  %s\n' "$expected_hash" "$(basename "$asset")" > "$fixture_directory/checksums.txt"
(
  cd "$fixture_directory"
  gitee_verify_sha256_manifest checksums.txt
) >/dev/null

PATH="$fixture_directory/bin:$PATH"
GITEE_UPLOAD_TEST_ARGUMENTS="$arguments"
export PATH GITEE_UPLOAD_TEST_ARGUMENTS

status=$(gitee_upload_attachment_request \
  "$asset" "$(basename "$asset")" 'https://gitee.example.test/attach_files' \
  'Authorization: token test-secret' "$response" 900 1 2 2> "$log")
test "$status" = 201
grep -Fx -- '--connect-timeout' "$arguments" >/dev/null
grep -Fx -- '30' "$arguments" >/dev/null
grep -Fx -- '--max-time' "$arguments" >/dev/null
grep -Fx -- '900' "$arguments" >/dev/null
grep -Fx -- "file=@$asset;filename=$(basename "$asset");type=application/octet-stream" "$arguments" >/dev/null
if grep -Fx -- '--http1.1' "$arguments" >/dev/null || grep -F -- 'Expect:' "$arguments" >/dev/null; then
  echo "Gitee upload must use curl's proven default transport negotiation." >&2
  exit 1
fi
grep -F -- 'curl_exit=0 http=201' "$log" >/dev/null
grep -F -- 'first_byte=0.200000s total=0.300000s uploaded_bytes=4096' "$log" >/dev/null
if grep -F -- 'test-secret' "$log" >/dev/null; then
  echo "Gitee upload logs must not expose credentials." >&2
  exit 1
fi

GITEE_UPLOAD_TEST_OUTPUT=$'000\t0.100000\t0.000000\t900.000000\t4096'
GITEE_UPLOAD_TEST_EXIT=28
export GITEE_UPLOAD_TEST_OUTPUT GITEE_UPLOAD_TEST_EXIT
set +e
status=$(gitee_upload_attachment_request \
  "$asset" "$(basename "$asset")" 'https://gitee.example.test/attach_files' \
  'Authorization: token test-secret' "$response" 900 2 2 2> "$log")
exit_code=$?
set -e
test "$exit_code" -eq 28
test "$status" = 000
grep -F -- 'curl_exit=28 http=000' "$log" >/dev/null

echo "Gitee upload tests passed."
