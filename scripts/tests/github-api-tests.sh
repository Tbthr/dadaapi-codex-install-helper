#!/usr/bin/env bash

set -euo pipefail

repository_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
source "$repository_root/scripts/release/github-api.sh"

fixture_directory=$(mktemp -d "${TMPDIR:-/tmp}/dada-github-api-tests.XXXXXX")
response="$fixture_directory/response.json"
trap 'rm -rf "$fixture_directory"' EXIT

mock_response='[[{"tag_name":"v1.0.1","draft":true},{"tag_name":"v1.0.2","draft":true}]]'
gh() {
  printf '%s\n' "$mock_response"
}

github_release_by_tag owner/repository v1.0.2 "$response"
jq -e '.tag_name == "v1.0.2" and .draft == true' "$response" >/dev/null

github_release_by_tag owner/repository v9.9.9 "$response"
jq -e '. == null' "$response" >/dev/null

mock_response='[[{"tag_name":"v1.0.2"}],[{"tag_name":"v1.0.2"}]]'
if github_release_by_tag owner/repository v1.0.2 "$response" 2>/dev/null; then
  echo "Duplicate releases must be rejected." >&2
  exit 1
fi

echo "GitHub API tests passed."
