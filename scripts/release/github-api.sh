#!/usr/bin/env bash

github_release_by_tag() {
  repository="$1"
  tag="$2"
  destination="$3"

  gh api --paginate --slurp "repos/$repository/releases?per_page=100" |
    jq --arg tag "$tag" '
      [.[][] | select(.tag_name == $tag)] |
      if length == 0 then null
      elif length == 1 then .[0]
      else error("duplicate GitHub releases for tag")
      end
    ' > "$destination"
}
