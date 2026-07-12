#!/bin/sh

set -eu

github_repository="ray7086/wocao-hub"
github_release_base="https://github.com/${github_repository}/releases/latest/download"
github_checksums_url="${github_release_base}/checksums.txt"
gitee_repository="codeTrees/wocao-hub"
gitee_release_base="https://gitee.com/${gitee_repository}/releases/download"
gitee_checksums_url="https://gitee.com/${gitee_repository}/raw/main/release/checksums.sha256"
temporary_directory=""
mount_point=""
mounted="0"

cleanup() {
  if [ "$mounted" = "1" ] && [ -n "$mount_point" ]; then
    /usr/bin/hdiutil detach "$mount_point" -quiet >/dev/null 2>&1 || true
  fi

  if [ -n "$temporary_directory" ] && [ -d "$temporary_directory" ]; then
    /bin/rm -rf "$temporary_directory"
  fi
}

fail() {
  printf '%s\n' "$1" >&2
  exit 1
}

trap cleanup EXIT HUP INT TERM

[ "$(/usr/bin/uname -s)" = "Darwin" ] || fail "Wocao Hub 的 macOS 安装命令只能在 Mac 上运行。"

architecture="${WOCAO_HUB_INSTALL_ARCH:-$(/usr/bin/uname -m)}"
case "$architecture" in
  arm64|x86_64) ;;
  *) fail "Wocao Hub 目前仅支持 Apple Silicon 和 Intel Mac，检测到：$architecture" ;;
esac

temporary_directory="$(/usr/bin/mktemp -d "${TMPDIR:-/tmp}/wocao-hub-install.XXXXXX")"
checksums_file="$temporary_directory/checksums.txt"
dmg_file="$temporary_directory/Wocao.Hub_universal.dmg"
download_file=""
mount_point="$temporary_directory/mount"
/bin/mkdir -p "$mount_point"

printf '%s\n' "正在获取 Wocao Hub 最新版本信息……"
if /usr/bin/curl \
  --fail \
  --silent \
  --show-error \
  --location \
  --retry 3 \
  --proto '=https' \
  --tlsv1.2 \
  --user-agent 'wocao-hub-installer' \
  --max-time 20 \
  "$gitee_checksums_url" \
  --output "$checksums_file"; then
  metadata_source="Gitee"
else
  printf '%s\n' "国内版本源暂时不可用，正在切换 GitHub……"
  /usr/bin/curl \
    --fail \
    --silent \
    --show-error \
    --location \
    --retry 3 \
    --proto '=https' \
    --tlsv1.2 \
    --user-agent 'wocao-hub-installer' \
    --max-time 20 \
    "$github_checksums_url" \
    --output "$checksums_file"
  metadata_source="GitHub"
fi

if [ "$metadata_source" = "Gitee" ]; then
  asset_pattern='^[0-9a-fA-F]{64}[[:space:]]+Wocao\.Hub_[0-9]+\.[0-9]+\.[0-9]+_universal\.dmg\.zip$'
  version_pattern='s/^Wocao\.Hub_([0-9]+\.[0-9]+\.[0-9]+)_universal\.dmg\.zip$/\1/p'
else
  asset_pattern='^[0-9a-fA-F]{64}[[:space:]]+Wocao\.Hub_[0-9]+\.[0-9]+\.[0-9]+_universal\.dmg$'
  version_pattern='s/^Wocao\.Hub_([0-9]+\.[0-9]+\.[0-9]+)_universal\.dmg$/\1/p'
fi

checksum_matches="$(/usr/bin/grep -E "$asset_pattern" "$checksums_file" || true)"
match_count="$(printf '%s\n' "$checksum_matches" | /usr/bin/grep -c . || true)"
[ "$match_count" = "1" ] || fail "没有找到唯一的 macOS Universal 安装包校验记录。"

expected_hash="$(printf '%s\n' "$checksum_matches" | /usr/bin/awk '{ print tolower($1) }')"
asset_name="$(printf '%s\n' "$checksum_matches" | /usr/bin/awk '{ print $2 }')"
release_version="$(printf '%s\n' "$asset_name" | /usr/bin/sed -nE "$version_pattern")"
[ -n "$release_version" ] || fail "无法解析 macOS 安装包版本。"
download_file="$temporary_directory/$asset_name"

printf '%s\n' "正在下载 ${asset_name}（版本信息来源：${metadata_source}）……"
if [ "$metadata_source" = "Gitee" ]; then
  asset_url="${gitee_release_base}/v${release_version}/${asset_name}"
else
  asset_url="${github_release_base}/${asset_name}"
fi

if ! /usr/bin/curl \
  --fail \
  --silent \
  --show-error \
  --location \
  --retry 3 \
  --proto '=https' \
  --tlsv1.2 \
  --user-agent 'wocao-hub-installer' \
  --max-time 180 \
  "$asset_url" \
  --output "$download_file"; then
  /bin/rm -f "$download_file"
  [ "$metadata_source" = "Gitee" ] || fail "GitHub 安装包下载失败。"
  printf '%s\n' "国内安装包下载暂时不可用，正在切换 GitHub……"

  /usr/bin/curl \
    --fail \
    --silent \
    --show-error \
    --location \
    --retry 3 \
    --proto '=https' \
    --tlsv1.2 \
    --user-agent 'wocao-hub-installer' \
    --max-time 20 \
    "$github_checksums_url" \
    --output "$checksums_file"

  checksum_matches="$(/usr/bin/grep -E '^[0-9a-fA-F]{64}[[:space:]]+Wocao\.Hub_[0-9]+\.[0-9]+\.[0-9]+_universal\.dmg$' "$checksums_file" || true)"
  match_count="$(printf '%s\n' "$checksum_matches" | /usr/bin/grep -c . || true)"
  [ "$match_count" = "1" ] || fail "GitHub 没有找到唯一的 macOS Universal 安装包校验记录。"
  expected_hash="$(printf '%s\n' "$checksum_matches" | /usr/bin/awk '{ print tolower($1) }')"
  asset_name="$(printf '%s\n' "$checksum_matches" | /usr/bin/awk '{ print $2 }')"
  release_version="$(printf '%s\n' "$asset_name" | /usr/bin/sed -nE 's/^Wocao\.Hub_([0-9]+\.[0-9]+\.[0-9]+)_universal\.dmg$/\1/p')"
  [ -n "$release_version" ] || fail "无法解析 GitHub macOS 安装包版本。"
  download_file="$dmg_file"

  /usr/bin/curl \
    --fail \
    --silent \
    --show-error \
    --location \
    --retry 3 \
    --proto '=https' \
    --tlsv1.2 \
    --user-agent 'wocao-hub-installer' \
    --max-time 180 \
    "${github_release_base}/${asset_name}" \
    --output "$download_file"
fi

actual_hash="$(/usr/bin/shasum -a 256 "$download_file" | /usr/bin/awk '{ print $1 }')"
[ "$actual_hash" = "$expected_hash" ] || fail "安装包 SHA-256 校验失败，已停止安装。"

if [ "${download_file##*.}" = "zip" ]; then
  extracted_directory="$temporary_directory/extracted"
  /bin/mkdir -p "$extracted_directory"
  /usr/bin/unzip -q "$download_file" -d "$extracted_directory"
  extracted_dmg="$(/usr/bin/find "$extracted_directory" -maxdepth 1 -type f \
    -name 'Wocao.Hub_*_universal.dmg' -print -quit)"
  [ -n "$extracted_dmg" ] || fail "国内安装包压缩文件中没有找到 DMG。"
  /bin/mv "$extracted_dmg" "$dmg_file"
elif [ "$download_file" != "$dmg_file" ]; then
  /bin/mv "$download_file" "$dmg_file"
fi

printf '%s\n' "下载与校验完成：v$release_version / macOS Universal ($architecture)"

if [ "${WOCAO_HUB_INSTALL_DRY_RUN:-0}" = "1" ]; then
  printf '%s\n' "Dry-run 验证成功，未安装应用。"
  exit 0
fi

/usr/bin/hdiutil attach "$dmg_file" -mountpoint "$mount_point" -nobrowse -readonly -quiet
mounted="1"

source_app="$(/usr/bin/find "$mount_point" -maxdepth 2 -type d -name 'Wocao Hub.app' -print -quit)"
[ -n "$source_app" ] || fail "DMG 中没有找到 Wocao Hub.app。"
/usr/bin/codesign --verify --deep --strict "$source_app" >/dev/null 2>&1 || fail "应用代码签名完整性校验失败。"

staged_app="$temporary_directory/Wocao Hub.app"
/usr/bin/ditto "$source_app" "$staged_app"
/usr/bin/xattr -dr com.apple.quarantine "$staged_app" 2>/dev/null || true

if [ -w "/Applications" ]; then
  applications_directory="/Applications"
else
  applications_directory="$HOME/Applications"
  /bin/mkdir -p "$applications_directory"
fi

destination_app="$applications_directory/Wocao Hub.app"
backup_app="$temporary_directory/Wocao Hub.previous.app"

if [ -e "$destination_app" ]; then
  /bin/mv "$destination_app" "$backup_app"
fi

if ! /bin/mv "$staged_app" "$destination_app"; then
  if [ -e "$backup_app" ]; then
    /bin/mv "$backup_app" "$destination_app" || true
  fi
  fail "无法写入应用目录，原版本已恢复。"
fi

/usr/bin/open "$destination_app"
printf '%s\n' "Wocao Hub 已安装到：$destination_app"
