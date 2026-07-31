#!/bin/sh

set -eu

github_repository="Tbthr/dadaapi-codex-install-helper"
github_release_base="https://github.com/${github_repository}/releases/latest/download"
github_checksums_url="${github_release_base}/checksums.txt"
gitee_repository="lyq_power/dadaapi-codex-install-helper"
gitee_release_base="https://gitee.com/${gitee_repository}/releases/download"
gitee_checksums_url="https://gitee.com/${gitee_repository}/releases/download/latest/checksums.txt"
installer_user_agent="dada-assistant-installer"
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

path_count() {
  printf '%s\n' "$1" | /usr/bin/grep -c . || true
}

select_macos_asset() {
  asset_pattern="$1"
  checksum_matches="$(/usr/bin/grep -E "$asset_pattern" "$checksums_file" || true)"
  match_count="$(path_count "$checksum_matches")"
  if [ "$match_count" != "1" ]; then
    return 1
  fi

  expected_hash="$(printf '%s\n' "$checksum_matches" | /usr/bin/awk '{ print tolower($1) }')"
  asset_name="$(printf '%s\n' "$checksum_matches" | /usr/bin/sed -E 's/^[0-9a-fA-F]{64}[[:space:]]+//')"
  if [ "$asset_name" != "$(/usr/bin/basename "$asset_name")" ]; then
    return 1
  fi
  release_version="$(printf '%s\n' "$asset_name" | /usr/bin/sed -nE 's/^.*_([0-9]+\.[0-9]+\.[0-9]+)_universal\.dmg(\.zip)?$/\1/p')"
  [ -n "$release_version" ]
}

download_asset() {
  asset_url="$1"
  destination="$2"
  /usr/bin/curl \
    --fail \
    --silent \
    --show-error \
    --location \
    --retry 3 \
    --proto '=https' \
    --tlsv1.2 \
    --user-agent "$installer_user_agent" \
    --max-time 180 \
    "$asset_url" \
    --output "$destination"
}

trap cleanup EXIT HUP INT TERM

[ "$(/usr/bin/uname -s)" = "Darwin" ] || fail "哒哒助手的 macOS 安装命令只能在 Mac 上运行。"

architecture="${DADA_ASSISTANT_INSTALL_ARCH:-$(/usr/bin/uname -m)}"
case "$architecture" in
  arm64|x86_64) ;;
  *) fail "哒哒助手目前仅支持 Apple Silicon 和 Intel Mac，检测到：$architecture" ;;
esac

temporary_directory="$(/usr/bin/mktemp -d "${TMPDIR:-/tmp}/dada-assistant-install.XXXXXX")"
checksums_file="$temporary_directory/checksums.txt"
dmg_file="$temporary_directory/installer.dmg"
mount_point="$temporary_directory/mount"
/bin/mkdir -p "$mount_point"

printf '%s\n' "正在获取哒哒助手 v1.0 最新版本信息……"
if /usr/bin/curl \
  --fail \
  --silent \
  --show-error \
  --location \
  --retry 3 \
  --proto '=https' \
  --tlsv1.2 \
  --user-agent "$installer_user_agent" \
  --max-time 20 \
  "$gitee_checksums_url" \
  --output "$checksums_file" \
  && select_macos_asset '^[0-9a-fA-F]{64}[[:space:]]+.+_[0-9]+\.[0-9]+\.[0-9]+_universal\.dmg\.zip$'; then
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
    --user-agent "$installer_user_agent" \
    --max-time 20 \
  "$github_checksums_url" \
  --output "$checksums_file"
  metadata_source="GitHub"
  select_macos_asset '^[0-9a-fA-F]{64}[[:space:]]+.+_[0-9]+\.[0-9]+\.[0-9]+_universal\.dmg$' \
    || fail "GitHub 没有找到唯一的 macOS Universal 安装包校验记录。"
fi

download_file="$temporary_directory/$asset_name"
if [ "$metadata_source" = "Gitee" ]; then
  asset_url="${gitee_release_base}/v${release_version}/${asset_name}"
else
  asset_url="${github_release_base}/${asset_name}"
fi

printf '%s\n' "正在下载 ${asset_name}（版本信息来源：${metadata_source}）……"
if ! download_asset "$asset_url" "$download_file"; then
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
    --user-agent "$installer_user_agent" \
    --max-time 20 \
  "$github_checksums_url" \
  --output "$checksums_file"
  metadata_source="GitHub"
  select_macos_asset '^[0-9a-fA-F]{64}[[:space:]]+.+_[0-9]+\.[0-9]+\.[0-9]+_universal\.dmg$' \
    || fail "GitHub 没有找到唯一的 macOS Universal 安装包校验记录。"
  download_file="$temporary_directory/$asset_name"
  asset_url="${github_release_base}/${asset_name}"
  download_asset "$asset_url" "$download_file"
fi

actual_hash="$(/usr/bin/shasum -a 256 "$download_file" | /usr/bin/awk '{ print $1 }')"
[ "$actual_hash" = "$expected_hash" ] || fail "安装包 SHA-256 校验失败，已停止安装。"

if [ "${download_file##*.}" = "zip" ]; then
  extracted_directory="$temporary_directory/extracted"
  /bin/mkdir -p "$extracted_directory"
  /usr/bin/unzip -q "$download_file" -d "$extracted_directory"
  dmg_matches="$(/usr/bin/find "$extracted_directory" -type f -name '*.dmg' -print)"
  dmg_count="$(path_count "$dmg_matches")"
  [ "$dmg_count" = "1" ] || fail "安装包压缩文件中没有找到唯一的 DMG。"
  extracted_dmg="$dmg_matches"
  /bin/mv "$extracted_dmg" "$dmg_file"
else
  /bin/mv "$download_file" "$dmg_file"
fi

printf '%s\n' "下载与校验完成：v$release_version / macOS Universal ($architecture)"

if [ "${DADA_ASSISTANT_INSTALL_DRY_RUN:-0}" = "1" ]; then
  printf '%s\n' "Dry-run 验证成功，未安装应用。"
  exit 0
fi

/usr/bin/hdiutil attach "$dmg_file" -mountpoint "$mount_point" -nobrowse -readonly -quiet
mounted="1"

app_matches="$(/usr/bin/find "$mount_point" -type d -name '*.app' -print)"
app_count="$(path_count "$app_matches")"
[ "$app_count" = "1" ] || fail "DMG 中没有找到唯一的应用程序。"
source_app="$app_matches"
app_name="$(/usr/bin/basename "$source_app")"
/usr/bin/codesign --verify --deep --strict "$source_app" >/dev/null 2>&1 || fail "应用代码签名完整性校验失败。"

staged_app="$temporary_directory/$app_name"
/usr/bin/ditto "$source_app" "$staged_app"
/usr/bin/xattr -dr com.apple.quarantine "$staged_app" 2>/dev/null || true

if [ -w "/Applications" ]; then
  applications_directory="/Applications"
else
  applications_directory="$HOME/Applications"
  /bin/mkdir -p "$applications_directory"
fi

destination_app="$applications_directory/$app_name"
backup_app="$temporary_directory/previous-$app_name"

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
printf '%s\n' "哒哒助手 v1.0 已安装到：$destination_app"
