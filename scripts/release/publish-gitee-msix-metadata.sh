#!/usr/bin/env bash

set -euo pipefail

required=(RELEASE_TAG GITEE_REPOSITORY GITEE_TOKEN MSIX_METADATA_PATH)
for name in "${required[@]}"; do
  if [ -z "${!name:-}" ]; then
    printf 'Missing required environment variable: %s\n' "$name" >&2
    exit 1
  fi
done

if ! [[ "$RELEASE_TAG" =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
  echo "RELEASE_TAG must be a final semantic version tag." >&2
  exit 1
fi
if ! [[ "$GITEE_REPOSITORY" =~ ^[A-Za-z0-9._-]+/[A-Za-z0-9._-]+$ ]]; then
  echo "Invalid Gitee repository." >&2
  exit 1
fi
if [ ! -s "$MSIX_METADATA_PATH" ]; then
  echo "MSIX metadata does not exist or is empty." >&2
  exit 1
fi

jq -e '
  (.generatedAt | type == "string" and length > 0) and
  ([.packages.arm64, .packages.x64] | all(
    (.url | type == "string" and test("^https?://([A-Za-z0-9-]+\\.)*dl\\.delivery\\.mp\\.microsoft\\.com/")) and
    (.expiresAt | type == "string" and length > 0)
  ))
' "$MSIX_METADATA_PATH" >/dev/null

metadata=$(jq -c . "$MSIX_METADATA_PATH")
release_body=$(printf '%s\n\n<!-- DADAAPI_MSIX_LINKS_V1\n%s\nDADAAPI_MSIX_LINKS_END -->' \
  "哒哒助手 $RELEASE_TAG 正式版。请使用仓库首页的版本化安装命令。" \
  "$metadata")
export RELEASE_BODY="$release_body"

authorization="Authorization: token $GITEE_TOKEN"
release_api="https://gitee.com/api/v5/repos/$GITEE_REPOSITORY/releases"
release_response="${RUNNER_TEMP:-/tmp}/gitee-msix-release.json"
release_status=$(curl -sS --connect-timeout 20 --max-time 60 \
  -o "$release_response" -w '%{http_code}' -H "$authorization" \
  "$release_api/tags/$RELEASE_TAG")
if [ "$release_status" != 200 ] || ! jq -e '
  .tag_name == env.RELEASE_TAG and .prerelease == false and (.id | type == "number")
' "$release_response" >/dev/null; then
  echo "The target Gitee release must exist and be final." >&2
  exit 1
fi

release_id=$(jq -r '.id' "$release_response")
attachments="${RUNNER_TEMP:-/tmp}/gitee-msix-attachments.json"
attachments_status=$(curl -sS --connect-timeout 20 --max-time 60 \
  -o "$attachments" -w '%{http_code}' -H "$authorization" \
  --url-query 'per_page=100' "$release_api/$release_id/attach_files")
version=${RELEASE_TAG#v}
if [ "$attachments_status" != 200 ] || ! jq -e --arg version "$version" '
  ([.[].name] | sort) == ([
    "Dada-Assistant_\($version)_arm64-setup.exe",
    "Dada-Assistant_\($version)_universal.dmg",
    "Dada-Assistant_\($version)_x64-setup.exe",
    "checksums.txt"
  ] | sort)
' "$attachments" >/dev/null; then
  echo "Gitee Final Release must contain exactly the four formal attachments." >&2
  exit 1
fi

updated_response="${RUNNER_TEMP:-/tmp}/gitee-msix-updated.json"
update_status=$(curl -sS --connect-timeout 20 --max-time 60 \
  -o "$updated_response" -w '%{http_code}' -X PATCH -H "$authorization" \
  --form-string "tag_name=$RELEASE_TAG" \
  --form-string "name=哒哒助手 $RELEASE_TAG" \
  --form-string "body=$release_body" \
  --form-string 'prerelease=false' \
  "$release_api/$release_id")
if [ "$update_status" != 200 ] || ! jq -e '
  .tag_name == env.RELEASE_TAG and .prerelease == false and .body == env.RELEASE_BODY
' "$updated_response" >/dev/null; then
  echo "Unable to publish Gitee MSIX metadata in the Release body." >&2
  exit 1
fi

public_response="${RUNNER_TEMP:-/tmp}/gitee-msix-public.json"
for attempt in 1 2 3 4 5 6 7; do
  if curl -fsSL --connect-timeout 20 --max-time 60 \
    "$release_api/tags/$RELEASE_TAG" -o "$public_response" \
    && jq -e '
      .tag_name == env.RELEASE_TAG and .prerelease == false and .body == env.RELEASE_BODY
    ' "$public_response" >/dev/null; then
    echo "Gitee MSIX metadata is public and current without adding a Release asset."
    exit 0
  fi
  [ "$attempt" -eq 7 ] || sleep 10
done

echo "Public Gitee MSIX metadata did not match the published Release body." >&2
exit 1
