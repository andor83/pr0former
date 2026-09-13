#!/bin/bash
# Local Developer ID builds with notarization credentials kept in Keychain.
set -euo pipefail
project_root="$(cd "$(dirname "$0")/.." && pwd)"
mode=full
jobs="${PR0_BUILD_JOBS:-4}"
profile="${PR0_NOTARY_PROFILE:-pr0former-notary}"
bundles=app
notary_args=(--keychain-profile "$profile")
if [[ -n "${PR0_NOTARY_KEYCHAIN:-}" ]]; then notary_args+=(--keychain "$PR0_NOTARY_KEYCHAIN"); fi
while [[ $# -gt 0 ]]; do
  case "$1" in
    --help|-h)
      cat <<'HELP'
Usage: ./scripts/build-macos-signed.sh [--sign-only | --notarize-only | --check] [--jobs N] [--bundles app|dmg|app,dmg]

Build a Developer ID signed .app, submit it to Apple, staple the ticket, and
verify Gatekeeper acceptance. Default Keychain profile: pr0former-notary.
--sign-only       Build and verify the signature without submitting to Apple.
--notarize-only   Submit and staple the existing signed .app without rebuilding.
--check          Check signing and notarization access without building/submitting.
--bundles        Build an app, disk image, or both (default app).
--jobs N         Build parallelism (default 4).

Set APPLE_SIGNING_IDENTITY if more than one Developer ID Application identity
is installed. Set PR0_NOTARY_PROFILE to use another notarytool Keychain profile.
Create credentials interactively with: xcrun notarytool store-credentials pr0former-notary
No passwords or private keys are stored in the repository. See docs/DESKTOP.md.
HELP
      exit 0 ;;
    --sign-only|--notarize-only|--check)
      [[ "$mode" == full ]] || { echo 'Choose only one mode.' >&2; exit 2; }
      mode="${1#--}"; shift ;;
    --bundles)
      [[ $# -ge 2 ]] || { echo "Missing --bundles value." >&2; exit 2; }
      bundles="$2"; shift 2 ;;
    --jobs)
      [[ $# -ge 2 ]] || { echo 'Missing --jobs value.' >&2; exit 2; }
      jobs="$2"; shift 2 ;;
    *) echo "Unknown option: $1 (use --help)" >&2; exit 2 ;;
  esac
done
[[ "$(uname -s)" == Darwin ]] || { echo 'This script requires macOS.' >&2; exit 1; }
case "$jobs" in ''|*[!0-9]*|0) echo '--jobs must be a positive integer.' >&2; exit 2;; esac
case "$bundles" in app|dmg|app,dmg|dmg,app) ;; *) echo 'Signed macOS bundles must be app, dmg, or app,dmg.' >&2; exit 2;; esac
if [[ "$mode" == notarize-only && "$bundles" != app ]]; then
  echo '--notarize-only expects the existing .app.' >&2; exit 2
fi
xcrun --find notarytool >/dev/null
xcrun --find stapler >/dev/null
app="$project_root/desktop/src-tauri/target/release/bundle/macos/pr0former.app"
if [[ "$mode" != sign-only ]]; then
  # Validate access before spending time on a build. Never print stored credentials.
  xcrun notarytool history "${notary_args[@]}" --output-format json >/dev/null || {
    echo "Cannot access notarization profile $profile. Run scripts/setup-macos-signing.sh in your local or SSH terminal." >&2; exit 1;
  }
fi
if [[ "$mode" != notarize-only ]]; then
  if [[ -z "${APPLE_SIGNING_IDENTITY:-}" ]]; then
    identities="$(security find-identity -v -p codesigning | sed -n 's/.*"\(Developer ID Application:.*\)"/\1/p')"
    [[ -n "$identities" && "$(printf '%s\n' "$identities" | wc -l | tr -d ' ')" == 1 ]] || {
      echo 'Set APPLE_SIGNING_IDENTITY to an installed Developer ID Application identity.' >&2; exit 1;
    }
    export APPLE_SIGNING_IDENTITY="$identities"
  fi
  [[ "$APPLE_SIGNING_IDENTITY" == 'Developer ID Application: '* ]] || {
    echo 'A Developer ID Application signing identity is required.' >&2; exit 1;
  }
  probe="$(mktemp -d "${TMPDIR:-/tmp}/pr0former-sign-check.XXXXXX")"
  trap 'rm -rf "$probe"' EXIT
  cp /usr/bin/true "$probe/probe"
  codesign --force --sign "$APPLE_SIGNING_IDENTITY" --options runtime --timestamp "$probe/probe" || {
    echo 'Signing key is unavailable. Unlock your Keychain or run scripts/setup-macos-signing.sh from an SSH terminal.' >&2; exit 1;
  }
  rm -rf "$probe"
  trap - EXIT
  if [[ "$mode" == check ]]; then echo 'Signing and notarization credential checks passed.'; exit 0; fi
  # This workflow notarizes explicitly with Keychain, after Tauri signs the bundle.
  (unset APPLE_CERTIFICATE APPLE_CERTIFICATE_PASSWORD APPLE_ID APPLE_PASSWORD APPLE_API_KEY APPLE_API_ISSUER APPLE_API_KEY_PATH
   PR0_BUILD_DEFER_EXPORT=1 bash "$project_root/build.sh" --no-prompt --bundles app --jobs "$jobs")
fi
codesign --verify --deep --strict --verbose=2 "$app"
signature="$(codesign --display --verbose=4 "$app" 2>&1)"
[[ "$signature" == *'Authority=Developer ID Application:'* && "$signature" == *'runtime'* ]] || {
  echo 'The app must have a Developer ID signature and hardened runtime.' >&2; exit 1;
}
if [[ "$mode" == sign-only ]]; then
  echo "Signed and sealed (not notarized): $app"
  if [[ "$bundles" == app ]]; then
    bash "$project_root/scripts/export-desktop-app.sh"
    exit 0
  fi
fi
archive="$(mktemp -d "${TMPDIR:-/tmp}/pr0former-notary.XXXXXX")"
trap 'rm -rf "$archive"' EXIT
if [[ "$mode" != sign-only ]]; then
ditto -c -k --keepParent "$app" "$archive/pr0former.zip"
# Preserve the submission result, including its ID, for later status/log retrieval.
result="$project_root/desktop/src-tauri/target/release/bundle/notarization.json"
xcrun notarytool submit "$archive/pr0former.zip" "${notary_args[@]}" --wait --output-format json > "$result"
cat "$result"
python3 - "$result" <<'PY'
import json, sys
result = json.load(open(sys.argv[1]))
if result.get('status') != 'Accepted':
    raise SystemExit('Notarization not accepted. Use xcrun notarytool log with the submission ID and Keychain profile.')
PY
xcrun stapler staple "$app"
xcrun stapler validate "$app"
codesign --verify --deep --strict --verbose=2 "$app"
spctl --assess --type execute --verbose=2 "$app"
echo "Signed, notarized, stapled, and accepted by Gatekeeper: $app"

fi
if [[ "$bundles" == *dmg* ]]; then
  # Build the image from the already stapled app so its offline ticket is inside.
  mkdir "$archive/image"
  ditto "$app" "$archive/image/pr0former.app"
  ln -s /Applications "$archive/image/Applications"
  dmg="$project_root/desktop/src-tauri/target/release/bundle/dmg/pr0former-$(uname -m).dmg"
  mkdir -p "$(dirname "$dmg")"
  hdiutil create -volname pr0former -srcfolder "$archive/image" -ov -format UDZO "$dmg"
  codesign --force --sign "$APPLE_SIGNING_IDENTITY" --timestamp "$dmg"
  codesign --verify --strict --verbose=2 "$dmg"
  if [[ "$mode" != sign-only ]]; then
    result="${dmg%.dmg}-notarization.json"
    xcrun notarytool submit "$dmg" "${notary_args[@]}" --wait --output-format json > "$result"
    cat "$result"
    python3 -c 'import json,sys; sys.exit(0 if json.load(open(sys.argv[1])).get("status") == "Accepted" else "DMG notarization not accepted; retrieve the submission log.")' "$result"
    xcrun stapler staple "$dmg"
    xcrun stapler validate "$dmg"
    spctl --assess --type open --context context:primary-signature --verbose=2 "$dmg"
  fi
  echo "Disk image: $dmg"
fi
bash "$project_root/scripts/export-desktop-app.sh"
