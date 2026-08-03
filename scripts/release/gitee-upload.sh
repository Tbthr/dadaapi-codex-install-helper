#!/usr/bin/env bash

gitee_file_size() {
  wc -c < "$1" | tr -d '[:space:]'
}

gitee_sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum -- "$1" | awk '{ print $1 }'
  else
    shasum -a 256 -- "$1" | awk '{ print $1 }'
  fi
}

gitee_verify_sha256_manifest() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum --check "$1"
  else
    shasum -a 256 --check "$1"
  fi
}

gitee_upload_attachment_request() {
  local asset_path="$1"
  local asset_name="$2"
  local attachments_api="$3"
  local authorization_header="$4"
  local response_path="$5"
  local maximum_seconds="$6"
  local attempt="$7"
  local maximum_attempts="$8"
  local asset_size curl_output curl_exit http_status time_connect time_starttransfer time_total size_upload

  if ! [[ "$maximum_seconds" =~ ^[1-9][0-9]*$ ]]; then
    echo "Invalid Gitee upload timeout." >&2
    return 2
  fi

  asset_size=$(gitee_file_size "$asset_path")
  printf 'Starting Gitee asset upload: name=%s bytes=%s attempt=%s/%s timeout=%ss\n' \
    "$asset_name" "$asset_size" "$attempt" "$maximum_attempts" "$maximum_seconds" >&2

  curl_exit=0
  curl_output=$(curl -sS --connect-timeout 30 --max-time "$maximum_seconds" \
    -o "$response_path" \
    -w $'%{http_code}\t%{time_connect}\t%{time_starttransfer}\t%{time_total}\t%{size_upload}' \
    -X POST \
    -H "$authorization_header" \
    -F "file=@$asset_path;filename=$asset_name;type=application/octet-stream" \
    "$attachments_api") || curl_exit=$?

  IFS=$'\t' read -r http_status time_connect time_starttransfer time_total size_upload <<< "$curl_output"
  printf 'Finished Gitee asset upload: name=%s attempt=%s/%s curl_exit=%s http=%s connect=%ss first_byte=%ss total=%ss uploaded_bytes=%s\n' \
    "$asset_name" "$attempt" "$maximum_attempts" "$curl_exit" "${http_status:-000}" \
    "${time_connect:-unknown}" "${time_starttransfer:-unknown}" "${time_total:-unknown}" \
    "${size_upload:-unknown}" >&2

  printf '%s\n' "${http_status:-000}"
  return "$curl_exit"
}
