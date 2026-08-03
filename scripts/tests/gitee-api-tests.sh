#!/usr/bin/env bash

set -euo pipefail

repository_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
source "$repository_root/scripts/release/gitee-api.sh"

fixture_directory=$(mktemp -d "${TMPDIR:-/tmp}/dada-gitee-api-tests.XXXXXX")
response="$fixture_directory/response.json"
trap 'rm -rf "$fixture_directory"' EXIT

printf 'null\n' > "$response"
gitee_release_is_absent 200 "$response"
gitee_release_is_absent 404 "$response"

printf '{"tag_name":"v1.0.1","prerelease":true}\n' > "$response"
if gitee_release_is_absent 200 "$response"; then
  echo "Existing releases must not be classified as absent." >&2
  exit 1
fi
if gitee_release_is_absent 500 "$response"; then
  echo "Server errors must not be classified as absent." >&2
  exit 1
fi

echo "Gitee API tests passed."
