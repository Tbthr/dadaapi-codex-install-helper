#!/usr/bin/env bash

set -euo pipefail

required=(
  RELEASE_TAG
  GITHUB_REPOSITORY
  GITEE_REPOSITORY
  GITEE_TOKEN
)
for name in "${required[@]}"; do
  if [ -z "${!name:-}" ]; then
    printf 'Missing required environment variable: %s\n' "$name" >&2
    exit 1
  fi
done

if ! [[ "$RELEASE_TAG" =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
  echo "Invalid release tag." >&2
  exit 1
fi
if ! [[ "$GITHUB_REPOSITORY" =~ ^[A-Za-z0-9._-]+/[A-Za-z0-9._-]+$ ]]; then
  echo "Invalid GitHub repository." >&2
  exit 1
fi
if ! [[ "$GITEE_REPOSITORY" =~ ^[A-Za-z0-9._-]+/[A-Za-z0-9._-]+$ ]]; then
  echo "Invalid Gitee repository." >&2
  exit 1
fi

temporary_directory=$(mktemp -d "${RUNNER_TEMP:-/tmp}/dada-release-prune.XXXXXX")
trap 'rm -rf "$temporary_directory"' EXIT
gitee_authorization="Authorization: token $GITEE_TOKEN"

fetch_github_releases() {
  destination="$1"
  gh api --paginate --slurp "repos/$GITHUB_REPOSITORY/releases?per_page=100" |
    jq 'add // []' > "$destination"
}

fetch_gitee_releases() {
  destination="$1"
  printf '[]\n' > "$destination"
  page=1
  while [ "$page" -le 100 ]; do
    page_response="$temporary_directory/gitee-page-$page.json"
    status=$(curl -sS --proto '=https' --proto-redir '=https' --max-redirs 5 \
      --connect-timeout 20 --max-time 60 \
      -o "$page_response" \
      -w '%{http_code}' \
      -H "$gitee_authorization" \
      "https://gitee.com/api/v5/repos/$GITEE_REPOSITORY/releases?per_page=100&page=$page")
    if [ "$status" != 200 ] || ! jq -e 'type == "array"' "$page_response" >/dev/null; then
      echo "Unable to list Gitee releases." >&2
      exit 1
    fi

    jq -s '.[0] + .[1]' "$destination" "$page_response" > "$destination.next"
    mv "$destination.next" "$destination"
    count=$(jq 'length' "$page_response")
    [ "$count" -eq 100 ] || return 0
    page=$((page + 1))
  done

  echo "Gitee release pagination exceeded the safety limit." >&2
  exit 1
}

verify_retained_release() {
  source="$1"
  releases="$2"
  if ! jq -e --arg tag "$RELEASE_TAG" '
    [.[] | select(.tag_name == $tag)] as $matches |
    ($matches | length) == 1 and
    ($matches[0].draft // false) == false and
    $matches[0].prerelease == false
  ' "$releases" >/dev/null; then
    printf '%s does not contain exactly one final release for %s.\n' "$source" "$RELEASE_TAG" >&2
    exit 1
  fi
}

verify_asset_contract() {
  source="$1"
  releases="$2"
  version="${RELEASE_TAG#v}"
  expected_names=(
    "Dada-Assistant_${version}_x64-setup.exe"
    "Dada-Assistant_${version}_arm64-setup.exe"
    "Dada-Assistant_${version}_universal.dmg"
    checksums.txt
  )
  for name in "${expected_names[@]}"; do
    count=$(jq --arg tag "$RELEASE_TAG" --arg name "$name" '
      [.[] | select(.tag_name == $tag) | .assets[] | select(.name == $name)] | length
    ' "$releases")
    if [ "$count" -ne 1 ]; then
      printf '%s retained release is missing or duplicates asset %s.\n' "$source" "$name" >&2
      exit 1
    fi
  done
}

github_releases="$temporary_directory/github-releases.json"
gitee_releases="$temporary_directory/gitee-releases.json"
fetch_github_releases "$github_releases"
fetch_gitee_releases "$gitee_releases"
verify_retained_release GitHub "$github_releases"
verify_retained_release Gitee "$gitee_releases"
verify_asset_contract GitHub "$github_releases"
verify_asset_contract Gitee "$gitee_releases"
test "$(jq --arg tag "$RELEASE_TAG" '[.[] | select(.tag_name == $tag) | .assets[]] | length' "$github_releases")" -eq 4

gitee_deleted=0
while IFS= read -r release_id; do
  [ -n "$release_id" ] || continue
  [[ "$release_id" =~ ^[0-9]+$ ]] || {
    echo "Invalid Gitee release ID." >&2
    exit 1
  }
  response="$temporary_directory/gitee-delete-$release_id.json"
  status=$(curl -sS --proto '=https' --proto-redir '=https' --max-redirs 5 \
    --connect-timeout 20 --max-time 60 \
    -o "$response" \
    -w '%{http_code}' \
    -X DELETE \
    -H "$gitee_authorization" \
    "https://gitee.com/api/v5/repos/$GITEE_REPOSITORY/releases/$release_id")
  if [ "$status" != 204 ] && [ "$status" != 200 ]; then
    printf 'Unable to delete Gitee release %s (HTTP %s).\n' "$release_id" "$status" >&2
    exit 1
  fi
  gitee_deleted=$((gitee_deleted + 1))
done < <(jq -r --arg tag "$RELEASE_TAG" '.[] | select(.tag_name != $tag) | .id' "$gitee_releases")

github_deleted=0
while IFS= read -r release_id; do
  [ -n "$release_id" ] || continue
  [[ "$release_id" =~ ^[0-9]+$ ]] || {
    echo "Invalid GitHub release ID." >&2
    exit 1
  }
  gh api --method DELETE "repos/$GITHUB_REPOSITORY/releases/$release_id"
  github_deleted=$((github_deleted + 1))
done < <(jq -r --arg tag "$RELEASE_TAG" '.[] | select(.tag_name != $tag) | .id' "$github_releases")

fetch_github_releases "$github_releases"
fetch_gitee_releases "$gitee_releases"
jq -e --arg tag "$RELEASE_TAG" 'length == 1 and .[0].tag_name == $tag' "$github_releases" >/dev/null
jq -e --arg tag "$RELEASE_TAG" 'length == 1 and .[0].tag_name == $tag' "$gitee_releases" >/dev/null
verify_retained_release GitHub "$github_releases"
verify_retained_release Gitee "$gitee_releases"
verify_asset_contract GitHub "$github_releases"
verify_asset_contract Gitee "$gitee_releases"

printf 'Retained %s and deleted %d GitHub release(s) and %d Gitee release(s).\n' \
  "$RELEASE_TAG" "$github_deleted" "$gitee_deleted"
