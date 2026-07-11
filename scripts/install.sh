#!/bin/sh

set -eu

repository="ray7086/wocao-hub"
release_api="https://api.github.com/repos/${repository}/releases/latest"
expected_release_prefix="https://github.com/${repository}/releases/download/"
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
release_file="$temporary_directory/release.plist"
dmg_file="$temporary_directory/Wocao.Hub_universal.dmg"
mount_point="$temporary_directory/mount"
/bin/mkdir -p "$mount_point"

printf '%s\n' "正在获取 Wocao Hub 最新版本信息……"
/usr/bin/curl \
  --fail \
  --silent \
  --show-error \
  --location \
  --retry 3 \
  --proto '=https' \
  --tlsv1.2 \
  --header 'Accept: application/vnd.github+json' \
  --header 'X-GitHub-Api-Version: 2022-11-28' \
  --user-agent 'wocao-hub-installer' \
  "$release_api" \
  --output "$release_file"

/usr/bin/plutil -convert xml1 "$release_file" >/dev/null
release_tag="$(/usr/libexec/PlistBuddy -c 'Print :tag_name' "$release_file" 2>/dev/null)"

asset_name=""
asset_url=""
asset_digest=""
asset_index="0"

while candidate_name="$(/usr/libexec/PlistBuddy -c "Print :assets:${asset_index}:name" "$release_file" 2>/dev/null)"; do
  if printf '%s\n' "$candidate_name" | /usr/bin/grep -Eq '^Wocao\.Hub_[0-9]+\.[0-9]+\.[0-9]+_universal\.dmg$'; then
    [ -z "$asset_name" ] || fail "GitHub Release 中存在多个 macOS Universal 安装包。"
    asset_name="$candidate_name"
    asset_url="$(/usr/libexec/PlistBuddy -c "Print :assets:${asset_index}:browser_download_url" "$release_file")"
    asset_digest="$(/usr/libexec/PlistBuddy -c "Print :assets:${asset_index}:digest" "$release_file")"
  fi
  asset_index=$((asset_index + 1))
done

[ -n "$asset_name" ] || fail "没有找到 macOS Universal 安装包。"
case "$asset_url" in
  "${expected_release_prefix}"*) ;;
  *) fail "GitHub 返回了不可信的安装包地址。" ;;
esac

expected_hash="$(printf '%s\n' "$asset_digest" | /usr/bin/sed -nE 's/^sha256:([0-9a-fA-F]{64})$/\1/p' | /usr/bin/tr '[:upper:]' '[:lower:]')"
[ -n "$expected_hash" ] || fail "GitHub Release 没有提供有效的 SHA-256 摘要。"

printf '%s\n' "正在下载 ${asset_name}……"
/usr/bin/curl \
  --fail \
  --silent \
  --show-error \
  --location \
  --retry 3 \
  --proto '=https' \
  --tlsv1.2 \
  --user-agent 'wocao-hub-installer' \
  "$asset_url" \
  --output "$dmg_file"

actual_hash="$(/usr/bin/shasum -a 256 "$dmg_file" | /usr/bin/awk '{ print $1 }')"
[ "$actual_hash" = "$expected_hash" ] || fail "安装包 SHA-256 校验失败，已停止安装。"

printf '%s\n' "下载与校验完成：$release_tag / macOS Universal ($architecture)"

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
