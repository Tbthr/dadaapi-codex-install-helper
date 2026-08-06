#!/usr/bin/env bash

# CI 等待脚本：轮询 Gitee Release，直到本地 sync-gitee-locally.sh 完成同步。
# 完成信号 = 该 tag 的 Gitee Release 已公开（prerelease=false）且恰好包含四个正式资产。
# 本地脚本只有在上传、下载验真全部通过后才会把 Release 置为 final，因此
# prerelease=false 等价于“本地同步已全部成功”，后续冒烟与 GitHub Final 可以安全继续。

set -euo pipefail

gitee_wait_expected_names() {
  local version
  version="${RELEASE_TAG#v}"
  printf '%s\n' \
    "checksums.txt" \
    "Dada-Assistant_${version}_x64-setup.exe" \
    "Dada-Assistant_${version}_arm64-setup.exe" \
    "Dada-Assistant_${version}_universal.dmg"
}

gitee_wait_release_complete() {
  local release_path="$1"
  local attachments_path="$2"
  local release_id prerelease expected_json

  jq -e 'type == "object"' "$release_path" >/dev/null 2>&1 || return 1
  release_id=$(jq -r '.id // empty' "$release_path")
  [ -n "$release_id" ] || return 1
  prerelease=$(jq -r '.prerelease' "$release_path")
  [ "$prerelease" = "false" ] || return 1

  jq -e 'type == "array"' "$attachments_path" >/dev/null 2>&1 || return 1
  expected_json=$(gitee_wait_expected_names | LC_ALL=C sort | jq -R . | jq -s -c 'sort')
  jq -e --argjson expected "$expected_json" \
    '[.[].name] | sort == $expected and length == ($expected | length)' \
    "$attachments_path" >/dev/null
}

gitee_wait_fetch_release() {
  local output="$1"
  curl -sS --connect-timeout 20 --max-time 30 \
    -o "$output" \
    -w '%{http_code}' \
    -H "Authorization: token $GITEE_TOKEN" \
    "https://gitee.com/api/v5/repos/$GITEE_REPOSITORY/releases/tags/$RELEASE_TAG"
}

gitee_wait_fetch_attachments() {
  local release_id="$1"
  local output="$2"
  curl -sS --connect-timeout 20 --max-time 30 \
    -o "$output" \
    -w '%{http_code}' \
    -H "Authorization: token $GITEE_TOKEN" \
    --url-query 'per_page=100' \
    "https://gitee.com/api/v5/repos/$GITEE_REPOSITORY/releases/$release_id/attach_files"
}

gitee_wait_main() {
  local required name
  local poll_interval attempt release_id release_status attachments_status
  # 文件路径为全局变量：EXIT trap 在 main 返回后才执行，不能引用 local。

  required=(RELEASE_TAG GITEE_REPOSITORY GITEE_TOKEN)
  for name in "${required[@]}"; do
    if [ -z "${!name:-}" ]; then
      echo "Missing required environment variable: $name" >&2
      exit 1
    fi
  done
  if ! [[ "$RELEASE_TAG" =~ ^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]]; then
    echo "Invalid release tag." >&2
    exit 1
  fi

  poll_interval="${GITEE_WAIT_POLL_INTERVAL:-120}"
  attempt=0
  release_path=$(mktemp "${TMPDIR:-/tmp}/gitee-wait-release.XXXXXX")
  attachments_path=$(mktemp "${TMPDIR:-/tmp}/gitee-wait-attachments.XXXXXX")
  trap 'rm -f "$release_path" "$attachments_path"' EXIT

  while :; do
    attempt=$((attempt + 1))
    release_status=$(gitee_wait_fetch_release "$release_path" || true)
    if [ "$release_status" = 200 ] && \
       jq -e '.id | type == "number"' "$release_path" >/dev/null 2>&1; then
      release_id=$(jq -r '.id' "$release_path")
      attachments_status=$(gitee_wait_fetch_attachments "$release_id" "$attachments_path" || true)
      if [ "$attachments_status" = 200 ] && \
         gitee_wait_release_complete "$release_path" "$attachments_path"; then
        echo "Gitee Release $RELEASE_TAG 已由本地脚本同步并公开，四个正式资产齐全。" >&2
        return 0
      fi
    fi
    echo "Gitee Release $RELEASE_TAG 尚未完成本地同步（第 ${attempt} 次检查），${poll_interval} 秒后重试..." >&2
    sleep "$poll_interval"
  done
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  gitee_wait_main "$@"
fi
