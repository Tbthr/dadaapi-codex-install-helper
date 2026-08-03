#!/usr/bin/env bash

set -euo pipefail

repository_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
fixture_directory=$(mktemp -d "${TMPDIR:-/tmp}/dada-msix-metadata-tests.XXXXXX")
trap 'rm -rf "$fixture_directory"' EXIT
mkdir -p "$fixture_directory/bin" "$fixture_directory/runner"

cat > "$fixture_directory/bin/curl" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

output=
method=GET
body=
url=
write_status=false
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o) output="$2"; shift 2 ;;
    -X) method="$2"; shift 2 ;;
    --form-string)
      case "$2" in body=*) body=${2#body=} ;; esac
      shift 2
      ;;
    -w) write_status=true; shift 2 ;;
    -H|--url-query|--connect-timeout|--max-time) shift 2 ;;
    -sS|-fsSL) shift ;;
    http*) url="$1"; shift ;;
    *) shift ;;
  esac
done

case "$url:$method" in
  */releases/tags/v1.0.0:GET)
    if [ -s "$MOCK_RELEASE_BODY" ]; then
      jq -n --rawfile body "$MOCK_RELEASE_BODY" \
        '{id:42,tag_name:"v1.0.0",prerelease:false,body:$body}' > "$output"
    else
      printf '%s\n' '{"id":42,"tag_name":"v1.0.0","prerelease":false,"body":"initial"}' > "$output"
    fi
    ;;
  */releases/42/attach_files:GET)
    cp "$MOCK_ATTACHMENTS" "$output"
    ;;
  */releases/42:PATCH)
    printf '%s' "$body" > "$MOCK_RELEASE_BODY"
    jq -n --arg body "$body" \
      '{id:42,tag_name:"v1.0.0",prerelease:false,body:$body}' > "$output"
    ;;
  *)
    printf 'Unexpected mock curl request: %s %s\n' "$method" "$url" >&2
    exit 1
    ;;
esac
[ "$write_status" = false ] || printf '200'
EOF
chmod 700 "$fixture_directory/bin/curl"

metadata="$fixture_directory/msix-links.json"
cat > "$metadata" <<'EOF'
{
  "generatedAt": "2026-08-03T00:00:00Z",
  "packages": {
    "arm64": {
      "url": "https://dl.delivery.mp.microsoft.com/arm64",
      "expiresAt": "2026-08-04T00:00:00Z"
    },
    "x64": {
      "url": "https://dl.delivery.mp.microsoft.com/x64",
      "expiresAt": "2026-08-04T00:00:00Z"
    }
  }
}
EOF

attachments="$fixture_directory/attachments.json"
cat > "$attachments" <<'EOF'
[
  {"name":"Dada-Assistant_1.0.0_x64-setup.exe"},
  {"name":"Dada-Assistant_1.0.0_arm64-setup.exe"},
  {"name":"Dada-Assistant_1.0.0_universal.dmg"},
  {"name":"checksums.txt"}
]
EOF

export PATH="$fixture_directory/bin:$PATH"
export RUNNER_TEMP="$fixture_directory/runner"
export RELEASE_TAG=v1.0.0
export GITEE_REPOSITORY=owner/repository
export GITEE_TOKEN=test-token
export MSIX_METADATA_PATH="$metadata"
export MOCK_ATTACHMENTS="$attachments"
export MOCK_RELEASE_BODY="$fixture_directory/release-body.txt"

export RELEASE_TAG=v1.0.0-rc.1
if bash "$repository_root/scripts/release/publish-gitee-msix-metadata.sh" >/dev/null 2>&1; then
  echo "Prerelease tag was accepted for MSIX metadata." >&2
  exit 1
fi
export RELEASE_TAG=v1.0.0

bash "$repository_root/scripts/release/publish-gitee-msix-metadata.sh"
grep -Fq '<!-- DADAAPI_MSIX_LINKS_V1' "$MOCK_RELEASE_BODY"
grep -Fq 'DADAAPI_MSIX_LINKS_END -->' "$MOCK_RELEASE_BODY"

jq '. += [{"name":"msix-links.json"}]' "$attachments" > "$fixture_directory/attachments-invalid.json"
export MOCK_ATTACHMENTS="$fixture_directory/attachments-invalid.json"
if bash "$repository_root/scripts/release/publish-gitee-msix-metadata.sh" >/dev/null 2>&1; then
  echo "Unexpected Release attachment was accepted." >&2
  exit 1
fi

echo "Gitee MSIX Release-body metadata tests passed."
