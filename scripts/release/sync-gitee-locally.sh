#!/usr/bin/env bash

# 本地同步脚本：把 GitHub Release 资产下载到本机，再由本机直连 Gitee 上传。
# 避免 GitHub Actions（美国）向 Gitee（国内）跨国上传的慢速/超时问题。
#
# 用法：
#   bash scripts/release/sync-gitee-locally.sh v1.0.1
#   或
#   RELEASE_TAG=v1.0.1 bash scripts/release/sync-gitee-locally.sh
#
# 前置要求：
#   - 仓库根目录存在 .env（包含 GITEE_REPOSITORY、GITEE_TOKEN），或通过环境变量提供
#   - 本机可访问 GitHub（下载资产）与 Gitee（上传）
#   - 可选：gh CLI 已登录；未安装时自动回退到 curl 下载

set -euo pipefail

script_directory=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repository_root=$(cd "$script_directory/../.." && pwd)

sync_gitee_load_env() {
  local env_file="${SYNC_GITEE_ENV_FILE:-$repository_root/.env}"
  local line key value
  if [ ! -f "$env_file" ]; then
    return 0
  fi
  # 只导出非空值：.env 中留空的 key 不会覆盖外部已导出的同名变量。
  while IFS= read -r line || [ -n "$line" ]; do
    case "$line" in
      ''|\#*) continue ;;
    esac
    key="${line%%=*}"
    value="${line#*=}"
    case "$key" in
      ''|*[!A-Za-z0-9_]*) continue ;;
    esac
    case "$value" in
      \"*\"|\'*\') value="${value:1:${#value}-2}" ;;
    esac
    if [ -n "$value" ]; then
      export "$key=$value"
    fi
  done < "$env_file"
}

sync_gitee_valid_tag() {
  [[ "$1" =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]
}

sync_gitee_github_repository() {
  local remote_url="$1" path
  case "$remote_url" in
    https://github.com/*)
      path="${remote_url#https://github.com/}"
      ;;
    git@github.com:*)
      path="${remote_url#git@github.com:}"
      ;;
    *)
      echo "Unsupported GitHub remote URL: $remote_url" >&2
      return 1
      ;;
  esac
  printf '%s\n' "${path%.git}"
}

sync_gitee_download_with_curl() {
  local gh_repository="$1" directory="$2"
  local x64_installer="$3" arm64_installer="$4" universal_dmg="$5"
  local release_json url name

  release_json=$(curl -fsSL --connect-timeout 20 --max-time 60 \
    "https://api.github.com/repos/$gh_repository/releases/tags/$RELEASE_TAG")
  jq -e '.assets | length >= 4' <<< "$release_json" >/dev/null
  for name in checksums.txt "$x64_installer" "$arm64_installer" "$universal_dmg"; do
    url=$(jq -r --arg name "$name" '.assets[] | select(.name == $name) | .browser_download_url' <<< "$release_json")
    if [ -z "$url" ] || [ "$url" = "null" ]; then
      echo "GitHub release $RELEASE_TAG is missing asset: $name" >&2
      return 1
    fi
    curl -fL --connect-timeout 20 --max-time 600 --retry 3 --retry-delay 5 \
      "$url" -o "$directory/$name"
  done
}

sync_gitee_main() {
  local required name version x64_installer arm64_installer universal_dmg
  local release_commit gh_repository remote_url
  # assets_directory 为全局变量：EXIT trap 在 main 返回后才执行，不能引用 local。

  sync_gitee_load_env

  RELEASE_TAG="${RELEASE_TAG:-${1:-}}"
  if [ -z "$RELEASE_TAG" ]; then
    echo "Missing release tag: pass it as the first argument or set RELEASE_TAG." >&2
    exit 1
  fi
  if ! sync_gitee_valid_tag "$RELEASE_TAG"; then
    echo "Invalid release tag: $RELEASE_TAG" >&2
    exit 1
  fi

  required=(GITEE_REPOSITORY GITEE_TOKEN)
  for name in "${required[@]}"; do
    if [ -z "${!name:-}" ]; then
      echo "Missing $name. Fill it in $repository_root/.env or export it." >&2
      exit 1
    fi
  done

  if ! git -C "$repository_root" rev-parse --verify -q "refs/tags/$RELEASE_TAG" >/dev/null; then
    echo "Fetching tag $RELEASE_TAG from origin..." >&2
    git -C "$repository_root" fetch origin "refs/tags/$RELEASE_TAG:refs/tags/$RELEASE_TAG"
  fi
  release_commit=$(git -C "$repository_root" rev-parse --verify "refs/tags/$RELEASE_TAG^{commit}")

  gh_repository="${GH_REPOSITORY:-}"
  if [ -z "$gh_repository" ]; then
    remote_url=$(git -C "$repository_root" remote get-url origin)
    gh_repository=$(sync_gitee_github_repository "$remote_url")
  fi

  assets_directory=$(mktemp -d "${TMPDIR:-/tmp}/dada-gitee-sync.XXXXXX")
  trap 'rm -rf "$assets_directory"' EXIT

  version="${RELEASE_TAG#v}"
  x64_installer="Dada-Assistant_${version}_x64-setup.exe"
  arm64_installer="Dada-Assistant_${version}_arm64-setup.exe"
  universal_dmg="Dada-Assistant_${version}_universal.dmg"

  echo "从 GitHub Release $gh_repository 下载 $RELEASE_TAG 的四个正式资产..." >&2
  if command -v gh >/dev/null 2>&1; then
    gh release download "$RELEASE_TAG" --repo "$gh_repository" --dir "$assets_directory" \
      --pattern checksums.txt --pattern "$x64_installer" --pattern "$arm64_installer" --pattern "$universal_dmg" \
      --clobber
  else
    sync_gitee_download_with_curl "$gh_repository" "$assets_directory" \
      "$x64_installer" "$arm64_installer" "$universal_dmg"
  fi

  export GITEE_REPOSITORY GITEE_TOKEN
  if [ -n "${GITEE_USERNAME:-}" ]; then
    export GITEE_USERNAME
  fi
  export GITHUB_WORKSPACE="$repository_root"
  export RELEASE_TAG
  export RELEASE_COMMIT="$release_commit"
  export RELEASE_ASSETS_DIRECTORY="$assets_directory"

  echo "开始上传并验真 Gitee Release（GitHub Actions 正在等待本地同步完成并自动接管后续流程）..." >&2
  bash "$script_directory/publish-gitee-release.sh"
  echo "Gitee Release $RELEASE_TAG 已同步完成；GitHub Actions 将自动继续 Gitee 冒烟与 GitHub Final。" >&2
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  sync_gitee_main "$@"
fi
