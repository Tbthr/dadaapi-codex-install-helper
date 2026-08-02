#!/bin/sh

set -eu

github_repository="Tbthr/dadaapi-codex-install-helper"
gitee_repository="lyq_power/dadaapi-codex-install-helper"
installer_user_agent="dada-assistant-installer/1.0"
expected_bundle_identifier="com.dadaapi.assistant"
maximum_checksum_bytes=65536
maximum_release_metadata_bytes=1048576
maximum_installer_bytes=1073741824

temporary_directory=""
mount_point=""
mounted="0"
http_result=""
http_status=""
asset_name=""
expected_hash=""
release_version=""
downloaded_file=""
metadata_source=""

cleanup() {
  if [ "$mounted" = "1" ] && [ -n "$mount_point" ]; then
    /usr/bin/hdiutil detach "$mount_point" -quiet >/dev/null 2>&1 || true
    mounted="0"
  fi

  if [ -n "$temporary_directory" ] && [ -d "$temporary_directory" ]; then
    /bin/rm -rf "$temporary_directory"
  fi
}

attach_dmg() {
  image_path="$1"

  if /usr/bin/hdiutil attach "$image_path" -mountpoint "$mount_point" -nobrowse -readonly -quiet; then
    mounted="1"
    return 0
  fi
  /usr/bin/hdiutil detach "$mount_point" -quiet >/dev/null 2>&1 || true

  # Some managed macOS runners reject an explicitly supplied mount directory.
  # Let hdiutil choose the system mount point, then retain that exact path for
  # bundle discovery and cleanup.
  attach_plist="$temporary_directory/attach.plist"
  if ! /usr/bin/hdiutil attach "$image_path" -nobrowse -readonly -plist > "$attach_plist"; then
    return 1
  fi
  automatic_mount_point=$(/usr/bin/plutil -p "$attach_plist" 2>/dev/null \
    | /usr/bin/sed -nE 's/.*"mount-point" => "(.*)"/\1/p' \
    | /usr/bin/head -n 1)
  [ -n "$automatic_mount_point" ] || return 1
  mount_point="$automatic_mount_point"
  mounted="1"
}

fail() {
  printf '%s\n' "$1" >&2
  exit 1
}

validate_install_version() {
  value="$1"
  if [ "$value" = "latest" ]; then
    return 0
  fi
  printf '%s\n' "$value" | /usr/bin/grep -Eq '^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$'
}

validate_install_source() {
  case "$1" in
    auto|gitee|github) return 0 ;;
    *) return 1 ;;
  esac
}

normalize_macos_architecture() {
  case "$1" in
    arm64|x86_64) printf '%s\n' "$1" ;;
    *) return 1 ;;
  esac
}

classify_http_result() {
  curl_exit="$1"
  status="$2"

  case "$status" in
    5[0-9][0-9])
      printf '%s\n' "retryable"
      return 0
      ;;
  esac

  case "$curl_exit" in
    0) ;;
    5|6|7|16|18|28|35|52|55|56|92|95)
      printf '%s\n' "retryable"
      return 0
      ;;
    *)
      printf '%s\n' "fatal"
      return 0
      ;;
  esac

  case "$status" in
    2[0-9][0-9]) printf '%s\n' "success" ;;
    *) printf '%s\n' "fatal" ;;
  esac
}

should_fallback_to_github() {
  [ "$1" = "auto" ] && [ "$2" = "gitee" ] && [ "$3" -eq 10 ]
}

ensure_https_url() {
  case "$1" in
    https://*) ;;
    *) return 1 ;;
  esac
  ! printf '%s\n' "$1" | /usr/bin/grep -Eq '[[:space:]@]'
}

checksums_url() {
  source_name="$1"
  requested_version="$2"

  case "$source_name:$requested_version" in
    github:latest)
      printf 'https://github.com/%s/releases/latest/download/checksums.txt\n' "$github_repository"
      ;;
    github:*)
      printf 'https://github.com/%s/releases/download/%s/checksums.txt\n' "$github_repository" "$requested_version"
      ;;
    gitee:latest)
      return 1
      ;;
    gitee:*)
      printf 'https://gitee.com/%s/releases/download/%s/checksums.txt\n' "$gitee_repository" "$requested_version"
      ;;
    *) return 1 ;;
  esac
}

gitee_latest_release_url() {
  printf 'https://gitee.com/api/v5/repos/%s/releases/latest\n' "$gitee_repository"
}

parse_gitee_latest_version() {
  metadata_path="$1"
  tag_name=$(/usr/bin/plutil -extract tag_name raw -o - "$metadata_path" 2>/dev/null) || return 1
  prerelease=$(/usr/bin/plutil -extract prerelease raw -o - "$metadata_path" 2>/dev/null) || return 1
  [ "$prerelease" = "false" ] || return 1
  [ "$tag_name" != "latest" ] || return 1
  validate_install_version "$tag_name" || return 1
  printf '%s\n' "$tag_name"
}

asset_url() {
  source_name="$1"
  version="$2"
  name="$3"

  case "$source_name" in
    github)
      printf 'https://github.com/%s/releases/download/v%s/%s\n' "$github_repository" "$version" "$name"
      ;;
    gitee)
      printf 'https://gitee.com/%s/releases/download/v%s/%s\n' "$gitee_repository" "$version" "$name"
      ;;
    *) return 1 ;;
  esac
}

http_get() {
  url="$1"
  destination="$2"
  timeout_seconds="$3"
  maximum_bytes="$4"
  partial_file="${destination}.part"

  http_result="fatal"
  http_status="000"
  /bin/rm -f "$partial_file" "$destination"
  ensure_https_url "$url" || return 0

  if http_status=$(/usr/bin/curl \
    --silent \
    --show-error \
    --location \
    --max-redirs 5 \
    --proto '=https' \
    --proto-redir '=https' \
    --tlsv1.2 \
    --connect-timeout 10 \
    --max-time "$timeout_seconds" \
    --max-filesize "$maximum_bytes" \
    --user-agent "$installer_user_agent" \
    --output "$partial_file" \
    --write-out '%{http_code}' \
    "$url"); then
    curl_exit=0
  else
    curl_exit=$?
  fi

  http_result="$(classify_http_result "$curl_exit" "$http_status")"
  if [ "$http_result" != "success" ]; then
    /bin/rm -f "$partial_file"
    return 0
  fi

  byte_count=$(/usr/bin/wc -c < "$partial_file" | /usr/bin/tr -d ' ')
  case "$byte_count" in
    ''|*[!0-9]*) http_result="fatal" ;;
    *)
      if [ "$byte_count" -gt "$maximum_bytes" ]; then
        http_result="fatal"
      fi
      ;;
  esac
  if [ "$http_result" != "success" ]; then
    /bin/rm -f "$partial_file"
    return 0
  fi

  /bin/mv "$partial_file" "$destination"
}

validate_checksum_contract() {
  checksum_path="$1"

  /usr/bin/awk '
    BEGIN { valid = 1; count = 0; x64 = 0; arm64 = 0; universal = 0; version = "" }
    {
      count++
      if (NF != 2 || length($1) != 64 || $1 !~ /^[0-9A-Fa-f]+$/) {
        valid = 0
        next
      }
      name = $2
      if (name ~ /[\\\/]/ || name ~ /^[-.]/) {
        valid = 0
        next
      }
      candidate = ""
      if (name ~ /^Dada-Assistant_[0-9]+\.[0-9]+\.[0-9]+_x64-setup\.exe$/) {
        x64++
        candidate = name
        sub(/_x64-setup\.exe$/, "", candidate)
      } else if (name ~ /^Dada-Assistant_[0-9]+\.[0-9]+\.[0-9]+_arm64-setup\.exe$/) {
        arm64++
        candidate = name
        sub(/_arm64-setup\.exe$/, "", candidate)
      } else if (name ~ /^Dada-Assistant_[0-9]+\.[0-9]+\.[0-9]+_universal\.dmg$/) {
        universal++
        candidate = name
        sub(/_universal\.dmg$/, "", candidate)
      } else {
        valid = 0
        next
      }
      sub(/^Dada-Assistant_/, "", candidate)
      if (candidate !~ /^[0-9]+\.[0-9]+\.[0-9]+$/) {
        valid = 0
      } else if (version == "") {
        version = candidate
      } else if (version != candidate) {
        valid = 0
      }
    }
    END { exit !(valid && count == 3 && x64 == 1 && arm64 == 1 && universal == 1) }
  ' "$checksum_path"
}

select_macos_asset() {
  checksum_path="$1"
  requested_version="$2"

  validate_checksum_contract "$checksum_path" || return 1
  checksum_match=$(/usr/bin/grep -E '^[0-9a-fA-F]{64}[[:space:]]+Dada-Assistant_[0-9]+\.[0-9]+\.[0-9]+_universal\.dmg$' "$checksum_path")
  [ "$(printf '%s\n' "$checksum_match" | /usr/bin/grep -c .)" = "1" ] || return 1

  expected_hash=$(printf '%s\n' "$checksum_match" | /usr/bin/awk '{ print tolower($1) }')
  asset_name=$(printf '%s\n' "$checksum_match" | /usr/bin/awk '{ print $2 }')
  [ "$asset_name" = "$(/usr/bin/basename "$asset_name")" ] || return 1
  release_version=$(printf '%s\n' "$asset_name" | /usr/bin/sed -nE 's/^Dada-Assistant_([0-9]+\.[0-9]+\.[0-9]+)_universal\.dmg$/\1/p')
  [ -n "$release_version" ] || return 1
  validate_install_version "v$release_version" || return 1

  if [ "$requested_version" != "latest" ] && [ "$requested_version" != "v$release_version" ]; then
    return 1
  fi
}

verify_sha256() {
  path="$1"
  wanted_hash="$2"
  actual_hash=$(/usr/bin/shasum -a 256 "$path" | /usr/bin/awk '{ print tolower($1) }')
  [ "$actual_hash" = "$wanted_hash" ]
}

fetch_release_from_source() {
  source_name="$1"
  requested_version="$2"
  asset_name=""
  expected_hash=""
  release_version=""
  downloaded_file=""
  metadata_source=""

  resolved_version="$requested_version"
  if [ "$source_name" = "gitee" ] && [ "$requested_version" = "latest" ]; then
    release_metadata_path="$temporary_directory/gitee-latest-release.json"
    release_metadata_uri=$(gitee_latest_release_url)
    http_get "$release_metadata_uri" "$release_metadata_path" 30 "$maximum_release_metadata_bytes"
    case "$http_result" in
      retryable) return 10 ;;
      success) ;;
      *) return 11 ;;
    esac
    resolved_version=$(parse_gitee_latest_version "$release_metadata_path") || return 11
    release_version=${resolved_version#v}
  fi

  checksum_path="$temporary_directory/checksums-${source_name}.txt"
  checksum_uri=$(checksums_url "$source_name" "$resolved_version") || return 11

  http_get "$checksum_uri" "$checksum_path" 30 "$maximum_checksum_bytes"
  case "$http_result" in
    retryable) return 10 ;;
    success) ;;
    *) return 11 ;;
  esac

  select_macos_asset "$checksum_path" "$resolved_version" || return 11
  installer_uri=$(asset_url "$source_name" "$release_version" "$asset_name") || return 11
  downloaded_file="$temporary_directory/$asset_name"
  http_get "$installer_uri" "$downloaded_file" 300 "$maximum_installer_bytes"
  case "$http_result" in
    retryable) return 10 ;;
    success) ;;
    *) return 11 ;;
  esac

  verify_sha256 "$downloaded_file" "$expected_hash" || return 11
  metadata_source="$source_name"
  return 0
}

verify_macos_application() {
  application_path="$1"

  /usr/bin/codesign --verify --deep --strict "$application_path" >/dev/null 2>&1 || return 1

  info_plist="$application_path/Contents/Info.plist"
  [ -f "$info_plist" ] || return 1
  bundle_identifier=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$info_plist" 2>/dev/null) || return 1
  [ "$bundle_identifier" = "$expected_bundle_identifier" ] || return 1
  executable_name=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$info_plist" 2>/dev/null) || return 1
  case "$executable_name" in
    ""|*/*) return 1 ;;
  esac
  executable_path="$application_path/Contents/MacOS/$executable_name"
  [ -f "$executable_path" ] || return 1
  architectures=$(/usr/bin/lipo -archs "$executable_path" 2>/dev/null) || return 1
  case " $architectures " in *" arm64 "*) ;; *) return 1 ;; esac
  case " $architectures " in *" x86_64 "*) ;; *) return 1 ;; esac
}

user_applications_directory() {
  printf '%s/Applications\n' "$HOME"
}

print_install_summary() {
  printf '%s\n' "下载与 SHA-256 校验完成：v${release_version} / macOS Universal (${architecture}，来源：${metadata_source})"
}

clear_application_quarantine() {
  /usr/bin/xattr -rd com.apple.quarantine "$1"
}

replace_application() {
  staged_path="$1"
  destination_path="$2"
  backup_path="$3"

  if [ -e "$destination_path" ]; then
    /bin/mv "$destination_path" "$backup_path" || return 1
  fi
  if /bin/mv "$staged_path" "$destination_path"; then
    return 0
  fi

  if [ -e "$backup_path" ]; then
    /bin/mv "$backup_path" "$destination_path" || true
  fi
  return 1
}

restore_previous_application() {
  destination_path="$1"
  backup_path="$2"

  if [ -e "$backup_path" ]; then
    /bin/rm -rf "$destination_path"
    /bin/mv "$backup_path" "$destination_path"
  fi
}

main() {
  [ "$(/usr/bin/uname -s)" = "Darwin" ] || fail "哒哒助手的 macOS 安装命令只能在 Mac 上运行。"

  install_version="${DADA_ASSISTANT_INSTALL_VERSION:-latest}"
  validate_install_version "$install_version" \
    || fail "DADA_ASSISTANT_INSTALL_VERSION 必须为 latest 或 vN.N.N。"
  install_source="${DADA_ASSISTANT_INSTALL_SOURCE:-auto}"
  validate_install_source "$install_source" \
    || fail "DADA_ASSISTANT_INSTALL_SOURCE 必须为 auto、gitee 或 github。"

  detected_architecture="${DADA_ASSISTANT_INSTALL_ARCH:-$(/usr/bin/uname -m)}"
  architecture=$(normalize_macos_architecture "$detected_architecture") \
    || fail "哒哒助手目前仅支持 Apple Silicon 和 Intel Mac，检测到：$detected_architecture"

  temporary_directory=$(/usr/bin/mktemp -d "${TMPDIR:-/tmp}/dada-assistant-install.XXXXXX")
  mount_point="$temporary_directory/mount"
  /bin/mkdir -p "$mount_point"
  trap cleanup EXIT HUP INT TERM

  printf '%s\n' "正在获取哒哒助手 ${install_version} 版本信息……"
  case "$install_source" in
    auto)
      if fetch_release_from_source gitee "$install_version"; then
        :
      else
        fetch_status=$?
        should_fallback_to_github auto gitee "$fetch_status" \
          || fail "Gitee 返回的版本信息、资产或校验结果不符合发布契约，已拒绝回退。"
        fallback_version="$install_version"
        if [ -n "$release_version" ]; then
          fallback_version="v$release_version"
        fi
        printf '%s\n' "Gitee 网络暂时不可用，正在切换 GitHub……"
        if ! fetch_release_from_source github "$fallback_version"; then
          fail "GitHub 版本信息或安装包获取失败。"
        fi
      fi
      ;;
    gitee|github)
      if ! fetch_release_from_source "$install_source" "$install_version"; then
        fail "指定的 ${install_source} 版本源不可用或未通过校验。"
      fi
      ;;
  esac

  print_install_summary
  /usr/bin/hdiutil verify "$downloaded_file" >/dev/null \
    || fail "DMG 结构校验失败，已停止安装。"
  attach_dmg "$downloaded_file" \
    || fail "DMG 挂载失败，已停止安装。"

  app_matches=$(/usr/bin/find "$mount_point" -maxdepth 1 -type d -name '哒哒助手.app' -print)
  app_count=$(printf '%s\n' "$app_matches" | /usr/bin/grep -c . || true)
  [ "$app_count" = "1" ] || fail "DMG 中没有找到唯一的顶层应用程序。"
  source_app="$app_matches"
  verify_macos_application "$source_app" \
    || fail "应用代码完整性、Bundle ID 或 Universal 架构校验失败。"

  if [ "${DADA_ASSISTANT_INSTALL_DRY_RUN:-0}" = "1" ]; then
    printf '%s\n' "Dry-run 验证成功，未安装应用。"
    return 0
  fi

  app_name=$(/usr/bin/basename "$source_app")
  staged_app="$temporary_directory/$app_name"
  /usr/bin/ditto "$source_app" "$staged_app"

  applications_directory=$(user_applications_directory)
  /bin/mkdir -p "$applications_directory"

  destination_app="$applications_directory/$app_name"
  backup_app="$temporary_directory/previous-$app_name"
  replace_application "$staged_app" "$destination_app" "$backup_app" \
    || fail "无法写入应用目录，原版本已恢复。"

  if ! clear_application_quarantine "$destination_app"; then
    restore_previous_application "$destination_app" "$backup_app" || true
    fail "无法清除应用的 macOS 下载隔离属性，原版本已恢复。"
  fi

  if ! /usr/bin/open "$destination_app"; then
    restore_previous_application "$destination_app" "$backup_app" || true
    fail "应用启动失败，已恢复原版本。"
  fi
  printf '%s\n' "哒哒助手 v$release_version 已安装到：$destination_app"
}

if [ "${DADA_ASSISTANT_INSTALL_LIBRARY_ONLY:-0}" = "1" ]; then
  return 0 2>/dev/null || exit 0
fi

main "$@"
