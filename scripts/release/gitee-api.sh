#!/usr/bin/env bash

gitee_release_is_absent() {
  status="$1"
  response_path="$2"

  [ "$status" = 404 ] || {
    [ "$status" = 200 ] && jq -e '. == null' "$response_path" >/dev/null
  }
}
