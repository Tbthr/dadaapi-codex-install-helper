#!/usr/bin/env bash

set -euo pipefail

script_directory=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$script_directory/gitee-api.sh"

required=(
  RELEASE_TAG
  RELEASE_COMMIT
  GITEE_REPOSITORY
  GITEE_TOKEN
  RELEASE_ASSETS_DIRECTORY
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
if ! [[ "$RELEASE_COMMIT" =~ ^[0-9a-f]{40}$ ]]; then
  echo "Invalid release commit." >&2
  exit 1
fi
if ! [[ "$GITEE_REPOSITORY" =~ ^[A-Za-z0-9._-]+/[A-Za-z0-9._-]+$ ]]; then
  echo "Invalid Gitee repository." >&2
  exit 1
fi
if [ -n "${GITEE_USERNAME:-}" ] && ! [[ "$GITEE_USERNAME" =~ ^[A-Za-z0-9._-]+$ ]]; then
  echo "Invalid Gitee username." >&2
  exit 1
fi
if [ ! -d "$RELEASE_ASSETS_DIRECTORY" ]; then
  echo "Release asset directory does not exist." >&2
  exit 1
fi
gitee_authorization="Authorization: token $GITEE_TOKEN"

cd "$RELEASE_ASSETS_DIRECTORY"
version="${RELEASE_TAG#v}"
x64_installer="Dada-Assistant_${version}_x64-setup.exe"
arm64_installer="Dada-Assistant_${version}_arm64-setup.exe"
universal_dmg="Dada-Assistant_${version}_universal.dmg"
test -f "$x64_installer"
test -f "$arm64_installer"
test -f "$universal_dmg"
test -f checksums.txt
expected_names=("$x64_installer" "$arm64_installer" "$universal_dmg" checksums.txt)
shopt -s nullglob
actual_names=(* )
test "${#actual_names[@]}" -eq 4
diff -u \
  <(printf '%s\n' "${expected_names[@]}" | LC_ALL=C sort) \
  <(printf '%s\n' "${actual_names[@]}" | LC_ALL=C sort)
test "$(wc -l < checksums.txt | tr -d ' ')" -eq 3
sha256sum --check checksums.txt
diff -u \
  <(printf '%s\n' "$x64_installer" "$arm64_installer" "$universal_dmg" | LC_ALL=C sort) \
  <(awk 'NF == 2 { print $2 }' checksums.txt | LC_ALL=C sort)

manifest="${RUNNER_TEMP:-/tmp}/dada-gitee-expected-assets.tsv"
: > "$manifest"
for name in "${expected_names[@]}"; do
  case "$name" in
    */*|*\\*|*$'\n'*|[-.]*)
      echo "Unsafe release asset name." >&2
      exit 1
      ;;
  esac
  checksum=$(sha256sum -- "$name" | awk '{ print $1 }')
  size=$(stat -c '%s' -- "$name")
  printf '%s\t%s\t%s\n' "$checksum" "$size" "$name" >> "$manifest"
done
LC_ALL=C sort -o "$manifest" "$manifest"

cd "$GITHUB_WORKSPACE"
repository_response="${RUNNER_TEMP:-/tmp}/gitee-repository.json"
repository_status=$(curl -sS --connect-timeout 20 --max-time 60 \
  -o "$repository_response" \
  -w '%{http_code}' \
  -H "$gitee_authorization" \
  "https://gitee.com/api/v5/repos/$GITEE_REPOSITORY")
if [ "$repository_status" != 200 ] || ! jq -e '.private == false' "$repository_response" >/dev/null; then
  echo "Gitee release destination must be an accessible public repository." >&2
  exit 1
fi

release_tag_object=$(git rev-parse --verify "refs/tags/$RELEASE_TAG")
release_tag_commit=$(git rev-parse --verify "refs/tags/$RELEASE_TAG^{commit}")
test "$release_tag_commit" = "$RELEASE_COMMIT"
export GITEE_USERNAME="${GITEE_USERNAME:-${GITEE_REPOSITORY%%/*}}"
gitee_url="https://gitee.com/${GITEE_REPOSITORY}.git"
askpass="${RUNNER_TEMP:-/tmp}/dada-gitee-askpass.sh"
umask 077
cat > "$askpass" <<'EOF'
#!/usr/bin/env sh
case "$1" in
  *Username*) printf '%s\n' "$GITEE_USERNAME" ;;
  *) printf '%s\n' "$GITEE_TOKEN" ;;
esac
EOF
chmod 700 "$askpass"
trap 'rm -f "$askpass"' EXIT
remote_tag_object=$(GIT_ASKPASS="$askpass" GIT_ASKPASS_REQUIRE=force GIT_TERMINAL_PROMPT=0 \
  git ls-remote "$gitee_url" "refs/tags/$RELEASE_TAG" | awk 'NR == 1 { print $1 }')
if [ -n "$remote_tag_object" ] && [ "$remote_tag_object" != "$release_tag_object" ]; then
  echo "Refusing to overwrite a different Gitee tag." >&2
  exit 1
fi
if [ -z "$remote_tag_object" ]; then
  GIT_ASKPASS="$askpass" GIT_ASKPASS_REQUIRE=force GIT_TERMINAL_PROMPT=0 \
    git push "$gitee_url" "refs/tags/$RELEASE_TAG:refs/tags/$RELEASE_TAG"
fi
synced_tag_object=$(GIT_ASKPASS="$askpass" GIT_ASKPASS_REQUIRE=force GIT_TERMINAL_PROMPT=0 \
  git ls-remote "$gitee_url" "refs/tags/$RELEASE_TAG" | awk 'NR == 1 { print $1 }')
test "$synced_tag_object" = "$release_tag_object"

release_api="https://gitee.com/api/v5/repos/$GITEE_REPOSITORY/releases"
release_response="${RUNNER_TEMP:-/tmp}/gitee-release.json"
release_status=$(curl -sS --connect-timeout 20 --max-time 60 \
  -o "$release_response" \
  -w '%{http_code}' \
  -H "$gitee_authorization" \
  "$release_api/tags/$RELEASE_TAG")
if gitee_release_is_absent "$release_status" "$release_response"; then
  created_response="${RUNNER_TEMP:-/tmp}/gitee-release-created.json"
  create_status=$(curl -sS --connect-timeout 20 --max-time 60 \
    -o "$created_response" \
    -w '%{http_code}' \
    -X POST \
    -H "$gitee_authorization" \
    -F "tag_name=$RELEASE_TAG" \
    -F "name=哒哒助手 $RELEASE_TAG（验证中）" \
    -F "body=四个正式资产正在完成公开下载校验；验证完成前请勿使用。" \
    -F "target_commitish=$RELEASE_COMMIT" \
    -F "prerelease=true" \
    "$release_api")
  if [ "$create_status" != 201 ] || ! jq -e '.tag_name == env.RELEASE_TAG and .target_commitish == env.RELEASE_COMMIT and .prerelease == true and (.id | type == "number")' "$created_response" >/dev/null; then
    echo "Unable to create the staged Gitee release." >&2
    exit 1
  fi
  release_id=$(jq -r '.id' "$created_response")
elif [ "$release_status" = 200 ]; then
  if ! jq -e '.tag_name == env.RELEASE_TAG and .target_commitish == env.RELEASE_COMMIT and .prerelease == true and (.id | type == "number")' "$release_response" >/dev/null; then
    echo "Refusing to alter an existing final or mismatched Gitee release." >&2
    exit 1
  fi
  release_id=$(jq -r '.id' "$release_response")
else
    echo "Unable to determine existing Gitee release state." >&2
    exit 1
fi

attachments_api="$release_api/$release_id/attach_files"
attachments="${RUNNER_TEMP:-/tmp}/gitee-attachments.json"
fetch_attachments() {
  local status
  status=$(curl -sS --connect-timeout 20 --max-time 60 \
    -o "$attachments" \
    -w '%{http_code}' \
    -H "$gitee_authorization" \
    --url-query 'per_page=100' \
    "$attachments_api")
  [ "$status" = 200 ]
  jq -e '
    type == "array" and
    all(.[]; (.id | type == "number") and (.name | type == "string") and (.size | type == "number")) and
    (group_by(.name) | all(.[]; length == 1))
  ' "$attachments" >/dev/null
}

fetch_attachments
while IFS=$'\t' read -r expected_checksum expected_size name; do
  match_count=$(jq --arg name "$name" '[.[] | select(.name == $name)] | length' "$attachments")
  if [ "$match_count" -eq 0 ]; then
    upload_response="${RUNNER_TEMP:-/tmp}/gitee-upload.json"
    upload_status=$(curl -sS --connect-timeout 30 --max-time 900 \
      -o "$upload_response" \
      -w '%{http_code}' \
      -X POST \
      -H "$gitee_authorization" \
      -F "file=@$RELEASE_ASSETS_DIRECTORY/$name;filename=$name;type=application/octet-stream" \
      "$attachments_api")
    if [ "$upload_status" != 201 ] && [ "$upload_status" != 200 ]; then
      echo "Failed to upload Gitee asset: $name" >&2
      exit 1
    fi
    fetch_attachments
    match_count=$(jq --arg name "$name" '[.[] | select(.name == $name)] | length' "$attachments")
  fi
  if [ "$match_count" -ne 1 ]; then
    echo "Gitee release contains a missing or duplicate asset: $name" >&2
    exit 1
  fi
  attached_size=$(jq -r --arg name "$name" '.[] | select(.name == $name) | .size' "$attachments")
  if [ "$attached_size" != "$expected_size" ]; then
    echo "Existing Gitee asset has an unexpected size: $name" >&2
    exit 1
  fi
done < "$manifest"

fetch_attachments
test "$(jq 'length' "$attachments")" -eq 4
while IFS= read -r actual_name; do
  awk -F '\t' -v name="$actual_name" '$3 == name { found = 1 } END { exit !found }' "$manifest" || {
    echo "Gitee prerelease contains an unexpected asset." >&2
    exit 1
  }
done < <(jq -r '.[].name' "$attachments")

verified_directory=$(mktemp -d "${RUNNER_TEMP:-/tmp}/gitee-verified.XXXXXX")
trap 'rm -f "$askpass"; rm -rf "$verified_directory"' EXIT
while IFS=$'\t' read -r expected_checksum expected_size name; do
  attachment_id=$(jq -r --arg name "$name" '.[] | select(.name == $name) | .id' "$attachments")
  curl -fsSL --connect-timeout 30 --max-time 900 \
    -H "$gitee_authorization" \
    "$attachments_api/$attachment_id/download" \
    -o "$verified_directory/$name"
  test "$(stat -c '%s' "$verified_directory/$name")" = "$expected_size"
  test "$(sha256sum "$verified_directory/$name" | awk '{ print $1 }')" = "$expected_checksum"
done < "$manifest"

final_response="${RUNNER_TEMP:-/tmp}/gitee-release-final.json"
final_status=$(curl -sS --connect-timeout 20 --max-time 60 \
  -o "$final_response" \
  -w '%{http_code}' \
  -X PATCH \
  -H "$gitee_authorization" \
  -F "tag_name=$RELEASE_TAG" \
  -F "name=哒哒助手 $RELEASE_TAG" \
  -F "body=哒哒助手 $RELEASE_TAG 正式版。请使用仓库首页的版本化安装命令。" \
  -F "prerelease=false" \
  "$release_api/$release_id")
if [ "$final_status" != 200 ] || ! jq -e '.tag_name == env.RELEASE_TAG and .target_commitish == env.RELEASE_COMMIT and .prerelease == false' "$final_response" >/dev/null; then
  echo "Gitee release finalization failed." >&2
  exit 1
fi

public_directory=$(mktemp -d "${RUNNER_TEMP:-/tmp}/gitee-public.XXXXXX")
trap 'rm -f "$askpass"; rm -rf "$verified_directory" "$public_directory"' EXIT
while IFS=$'\t' read -r expected_checksum expected_size name; do
  public_url="https://gitee.com/$GITEE_REPOSITORY/releases/download/$RELEASE_TAG/$name"
  public_path="$public_directory/$name"
  downloaded=false
  for attempt in 1 2 3 4 5 6 7; do
    rm -f "$public_path"
    if curl -fsSL --proto '=https' --proto-redir '=https' --max-redirs 5 \
      --connect-timeout 30 --max-time 900 --max-filesize "$expected_size" \
      "$public_url" -o "$public_path" \
      && [ "$(stat -c '%s' "$public_path")" = "$expected_size" ] \
      && [ "$(sha256sum "$public_path" | awk '{ print $1 }')" = "$expected_checksum" ]; then
      downloaded=true
      break
    fi
    [ "$attempt" -eq 7 ] || sleep $((attempt * 5))
  done
  [ "$downloaded" = true ] || {
    echo "Gitee public asset did not become available with the expected bytes: $name" >&2
    exit 1
  }
done < "$manifest"

echo "Gitee release is final and all four public assets match GitHub."
