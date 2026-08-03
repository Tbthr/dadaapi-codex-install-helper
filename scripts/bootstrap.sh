#!/bin/sh

set -eu

umask 077

github_repository="Tbthr/dadaapi-codex-install-helper"
gitee_repository="lyq_power/dadaapi-codex-install-helper"
bootstrap_user_agent="dada-assistant-bootstrap/1.0"
installer_script_tag="v1.0.1"
installer_script_sha256="410984f39ad07e00e3e891e1a425cca3a5bbe98d6462a999d0128e838f6c6b2f"
maximum_installer_script_bytes=1048576

temporary_directory=""
http_result=""
http_status=""

cleanup() {
  if [ -n "$temporary_directory" ] && [ -d "$temporary_directory" ]; then
    /bin/rm -rf "$temporary_directory"
  fi
}

fail() {
  printf '%s\n' "$1" >&2
  exit 1
}

validate_install_source() {
  case "$1" in
    auto|gitee|github) return 0 ;;
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

installer_script_url() {
  source_name="$1"

  case "$source_name" in
    gitee)
      printf 'https://gitee.com/%s/raw/%s/scripts/install.sh\n' "$gitee_repository" "$installer_script_tag"
      ;;
    github)
      printf 'https://raw.githubusercontent.com/%s/%s/scripts/install.sh\n' "$github_repository" "$installer_script_tag"
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
    --user-agent "$bootstrap_user_agent" \
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

verify_sha256() {
  path="$1"
  wanted_hash="$2"
  actual_hash=$(/usr/bin/shasum -a 256 "$path" | /usr/bin/awk '{ print tolower($1) }')
  [ "$actual_hash" = "$wanted_hash" ]
}

fetch_installer_script() {
  source_name="$1"
  installer_uri=$(installer_script_url "$source_name") || return 11
  installer_path="$temporary_directory/install.sh"

  http_get "$installer_uri" "$installer_path" 60 "$maximum_installer_script_bytes"
  case "$http_result" in
    retryable) return 10 ;;
    success) ;;
    *) return 11 ;;
  esac
  verify_sha256 "$installer_path" "$installer_script_sha256" || return 11
  return 0
}

main() {
  [ "$(/usr/bin/uname -s)" = "Darwin" ] || fail "哒哒助手的 macOS 安装命令只能在 Mac 上运行。"

  install_source="${DADA_ASSISTANT_INSTALL_SOURCE:-auto}"
  validate_install_source "$install_source" \
    || fail "DADA_ASSISTANT_INSTALL_SOURCE 必须为 auto、gitee 或 github。"

  temporary_directory=$(/usr/bin/mktemp -d "${TMPDIR:-/tmp}/dada-assistant-bootstrap.XXXXXX")
  trap cleanup EXIT HUP INT TERM

  printf '%s\n' "正在获取哒哒助手安装脚本……"
  effective_source="$install_source"
  case "$install_source" in
    auto)
      if fetch_installer_script gitee; then
        :
      else
        fetch_status=$?
        should_fallback_to_github auto gitee "$fetch_status" \
          || fail "Gitee 安装脚本未通过校验，已拒绝回退。"
        printf '%s\n' "Gitee 网络暂时不可用，正在切换 GitHub……"
        fetch_installer_script github || fail "GitHub 安装脚本不可用或未通过校验。"
        effective_source="github"
      fi
      ;;
    gitee|github)
      fetch_installer_script "$install_source" \
        || fail "指定的 ${install_source} 安装脚本不可用或未通过校验。"
      ;;
  esac

  DADA_ASSISTANT_INSTALL_SOURCE="$effective_source" /bin/sh "$installer_path"
}

if [ "${DADA_ASSISTANT_BOOTSTRAP_LIBRARY_ONLY:-0}" = "1" ]; then
  return 0 2>/dev/null || exit 0
fi

main "$@"
