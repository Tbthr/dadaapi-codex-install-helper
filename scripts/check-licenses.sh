#!/usr/bin/env bash

set -euo pipefail

audit_directory=$(mktemp -d "${TMPDIR:-/tmp}/dada-license-audit.XXXXXX")
trap 'rm -rf "$audit_directory"' EXIT

cargo_metadata="$audit_directory/cargo-metadata.json"
pnpm_licenses="$audit_directory/pnpm-licenses.json"
cargo metadata --format-version 1 --locked > "$cargo_metadata"
pnpm licenses list --prod --json > "$pnpm_licenses"

for required_notice in "Instrument Sans" "Phosphor Icons" "Simple Icons" "cfg_aliases"; do
  grep -Fq "$required_notice" THIRD_PARTY_NOTICES.md || {
    printf 'Missing required third-party notice: %s\n' "$required_notice" >&2
    exit 1
  }
done

jq -e '
  .bundle.license == "Apache-2.0" and
  ((.bundle.licenseFile? == "../../../LICENSE") or (.bundle.licenseFile? == null)) and
  .bundle.resources["../../../LICENSE"] == "LICENSE" and
  .bundle.resources["../../../THIRD_PARTY_NOTICES.md"] == "THIRD_PARTY_NOTICES.md"
' apps/desktop/src-tauri/tauri.conf.json >/dev/null || {
  echo "Tauri bundles must include LICENSE and THIRD_PARTY_NOTICES.md." >&2
  exit 1
}

jq -e '
  all(.packages[] | select(.source == null); .license == "Apache-2.0") and
  all(.packages[] | select(.source != null); (.license | type == "string" and length > 0))
' "$cargo_metadata" >/dev/null || {
  echo "Workspace metadata or a locked Cargo dependency has a missing/unapproved license declaration." >&2
  exit 1
}

is_disallowed_without_permissive_option() {
  license_expression="$1"
  if ! printf '%s\n' "$license_expression" | grep -Eq '(^|[^A-Z])((A|L)?GPL-[123]|SSPL|BUSL|Commons Clause|CC-BY-NC)'; then
    return 1
  fi
  if printf '%s\n' "$license_expression" | grep -Eq '(^|[ (])(Apache-2\.0|MIT|MIT-0|BSD-[123]-Clause|ISC|Zlib|Unlicense|0BSD)([ )]|$)' \
    && printf '%s\n' "$license_expression" | grep -Eq ' OR |/'; then
    return 1
  fi
  return 0
}

has_only_approved_license_tokens() {
  license_expression="$1"
  while IFS= read -r token; do
    case "$token" in
      AND|OR|WITH|0BSD|Apache-2.0|BSD|BSD-1-Clause|BSD-2-Clause|BSD-3-Clause|BSL-1.0|CC0-1.0|CDLA-Permissive-2.0|ISC|LGPL-2.1-or-later|LLVM-exception|MIT|MIT-0|MPL-2.0|OFL-1.1|Unicode-3.0|Unlicense|Zlib) ;;
      *) return 1 ;;
    esac
  done < <(printf '%s\n' "$license_expression" | sed -E 's/[()\/]/ /g' | tr '[:space:]' '\n' | sed '/^$/d')
}

while IFS=$'\t' read -r name version license_expression; do
  if ! has_only_approved_license_tokens "$license_expression"; then
    printf 'Unreviewed Cargo dependency license: %s %s (%s)\n' "$name" "$version" "$license_expression" >&2
    exit 1
  fi
  if is_disallowed_without_permissive_option "$license_expression"; then
    printf 'Disallowed Cargo dependency license: %s %s (%s)\n' "$name" "$version" "$license_expression" >&2
    exit 1
  fi
done < <(jq -r '.packages[] | select(.source != null) | [.name, .version, .license] | @tsv' "$cargo_metadata")

node - "$pnpm_licenses" <<'NODE'
const fs = require("node:fs");
const licenses = JSON.parse(fs.readFileSync(process.argv[2], "utf8"));
const entries = Object.entries(licenses);
if (entries.length === 0) {
  throw new Error("pnpm returned an empty production license inventory");
}

const approvedTokens = new Set([
  "AND", "OR", "WITH", "0BSD", "Apache-2.0", "BSD", "BSD-1-Clause",
  "BSD-2-Clause", "BSD-3-Clause", "BSL-1.0", "CC0-1.0",
  "CDLA-Permissive-2.0", "ISC", "LGPL-2.1-or-later", "LLVM-exception",
  "MIT", "MIT-0", "MPL-2.0", "OFL-1.1", "Unicode-3.0", "Unlicense",
  "Zlib",
]);
const permissive = /(?:^|[ (])(Apache-2\.0|MIT|MIT-0|BSD|BSD-[123]-Clause|ISC|Zlib|Unlicense|0BSD|CC0-1\.0|OFL-1\.1)(?:[ )]|$)/;
const disallowed = /(?:^|[^A-Z])((?:A|L)?GPL-[123]|SSPL|BUSL|Commons Clause|CC-BY-NC)/;
for (const [expression, packages] of entries) {
  if (!expression || !Array.isArray(packages) || packages.length === 0) {
    throw new Error(`invalid pnpm license record: ${expression}`);
  }
  if (disallowed.test(expression) && !(permissive.test(expression) && / OR |\//.test(expression))) {
    throw new Error(`disallowed pnpm production license: ${expression}`);
  }
  const tokens = expression.replace(/[()/]/g, " ").trim().split(/\s+/).filter(Boolean);
  if (tokens.some((token) => !approvedTokens.has(token))) {
    throw new Error(`unreviewed pnpm production license: ${expression}`);
  }
  for (const dependency of packages) {
    if (!dependency.name || !dependency.license) {
      throw new Error(`missing pnpm license metadata in ${expression}`);
    }
  }
}
NODE

cargo_count=$(jq '[.packages[] | select(.source != null)] | length' "$cargo_metadata")
pnpm_count=$(jq '[to_entries[].value[]] | length' "$pnpm_licenses")
printf 'License audit passed: %s Cargo packages and %s pnpm production packages.\n' "$cargo_count" "$pnpm_count"
