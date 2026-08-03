#!/usr/bin/env bash

set -euo pipefail

repository_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
test_directory=$(mktemp -d "${TMPDIR:-/tmp}/dada-prune-tests.XXXXXX")
trap 'rm -rf "$test_directory"' EXIT
mkdir "$test_directory/bin" "$test_directory/runner"

cat > "$test_directory/bin/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

if [ "$1" != api ]; then
  exit 2
fi
shift
if [ "${1:-}" = --method ] && [ "${2:-}" = DELETE ]; then
  release_id=${3##*/}
  jq --argjson id "$release_id" '[.[] | select(.id != $id)]' "$MOCK_GITHUB_RELEASES" > "$MOCK_GITHUB_RELEASES.next"
  mv "$MOCK_GITHUB_RELEASES.next" "$MOCK_GITHUB_RELEASES"
  exit 0
fi

jq -s '.' "$MOCK_GITHUB_RELEASES"
EOF

cat > "$test_directory/bin/curl" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

output=""
method=GET
url=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o)
      output="$2"
      shift 2
      ;;
    -X)
      method="$2"
      shift 2
      ;;
    --proto|--proto-redir|--max-redirs|--connect-timeout|--max-time|-w|-H)
      shift 2
      ;;
    http*)
      url="$1"
      shift
      ;;
    *)
      shift
      ;;
  esac
done

if [ "$method" = DELETE ]; then
  release_id=${url##*/}
  jq --argjson id "$release_id" '[.[] | select(.id != $id)]' "$MOCK_GITEE_RELEASES" > "$MOCK_GITEE_RELEASES.next"
  mv "$MOCK_GITEE_RELEASES.next" "$MOCK_GITEE_RELEASES"
  : > "$output"
  printf '204'
  exit 0
fi

cp "$MOCK_GITEE_RELEASES" "$output"
printf '200'
EOF

chmod 700 "$test_directory/bin/gh" "$test_directory/bin/curl"
export PATH="$test_directory/bin:$PATH"
export RUNNER_TEMP="$test_directory/runner"
export MOCK_GITHUB_RELEASES="$test_directory/github.json"
export MOCK_GITEE_RELEASES="$test_directory/gitee.json"
export RELEASE_TAG=v1.0.0
export GITHUB_REPOSITORY=example/dada
export GITEE_REPOSITORY=example/dada
export GITEE_TOKEN=test-token

write_fixture() {
  cat > "$MOCK_GITHUB_RELEASES" <<'EOF'
[
  {"id": 10, "tag_name": "v1.0.0", "draft": false, "prerelease": false, "assets": [
    {"name": "Dada-Assistant_1.0.0_x64-setup.exe"},
    {"name": "Dada-Assistant_1.0.0_arm64-setup.exe"},
    {"name": "Dada-Assistant_1.0.0_universal.dmg"},
    {"name": "checksums.txt"}
  ]},
  {"id": 9, "tag_name": "v0.9.9", "draft": true, "prerelease": false, "assets": []}
]
EOF
  cat > "$MOCK_GITEE_RELEASES" <<'EOF'
[
  {"id": 110, "tag_name": "v1.0.0", "prerelease": false, "assets": [
    {"name": "Dada-Assistant_1.0.0_x64-setup.exe"},
    {"name": "Dada-Assistant_1.0.0_arm64-setup.exe"},
    {"name": "Dada-Assistant_1.0.0_universal.dmg"},
    {"name": "checksums.txt"},
    {"name": "v1.0.0.zip"},
    {"name": "v1.0.0.tar.gz"}
  ]},
  {"id": 109, "tag_name": "v0.9.9", "prerelease": true, "assets": []}
]
EOF
}

write_fixture
bash "$repository_root/scripts/release/prune-old-releases.sh" >/dev/null
jq -e 'length == 1 and .[0].tag_name == "v1.0.0"' "$MOCK_GITHUB_RELEASES" >/dev/null
jq -e 'length == 1 and .[0].tag_name == "v1.0.0"' "$MOCK_GITEE_RELEASES" >/dev/null

bash "$repository_root/scripts/release/prune-old-releases.sh" >/dev/null

write_fixture
jq 'map(if .tag_name == "v1.0.0" then .prerelease = true else . end)' \
  "$MOCK_GITEE_RELEASES" > "$MOCK_GITEE_RELEASES.next"
mv "$MOCK_GITEE_RELEASES.next" "$MOCK_GITEE_RELEASES"
if bash "$repository_root/scripts/release/prune-old-releases.sh" >/dev/null 2>&1; then
  echo "Expected pruning to reject a non-final retained release." >&2
  exit 1
fi
jq -e 'length == 2' "$MOCK_GITHUB_RELEASES" >/dev/null
jq -e 'length == 2' "$MOCK_GITEE_RELEASES" >/dev/null

echo "Release pruning tests passed."
