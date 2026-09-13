#!/bin/bash
# Keep Tauri's build output for notarize-only; expose a complete copy at the root.
set -euo pipefail
[[ "$(uname -s)" == Darwin ]] || exit 0
project_root="$(cd "$(dirname "$0")/.." && pwd)"
app="$project_root/desktop/src-tauri/target/release/bundle/macos/pr0former.app"
[[ -d "$app" ]] || { echo "Missing built app: $app" >&2; exit 1; }
staging="$(mktemp -d "$project_root/.pr0former-export.XXXXXX")"
trap 'rm -rf "$staging"' EXIT
# ditto preserves the bundle's permissions, extended attributes and stapled ticket.
ditto "$app" "$staging/pr0former.app"
# Replace, rather than merge, so removed resources cannot survive a later build.
rm -rf "$project_root/pr0former.app"
mv "$staging/pr0former.app" "$project_root/pr0former.app"
echo "App ready: $project_root/pr0former.app"
