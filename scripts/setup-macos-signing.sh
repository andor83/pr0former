#!/bin/bash
# Run interactively, locally or over ssh -t. Never run this setup in tests/CI.
set +x
set -euo pipefail
if [[ "${1:-}" == --help || "${1:-}" == -h ]]; then
  cat <<'HELP'
Usage: ./scripts/setup-macos-signing.sh
Interactive setup, usable over ssh -t: unlock the login Keychain, authorize
Apple's signing tools for the selected Developer ID private key, and store
notarization credentials. Passwords are prompted for, never saved in this repo.
APPLE_SIGNING_IDENTITY selects an identity if multiple are installed.
PR0_NOTARY_PROFILE selects the profile (default pr0former-notary).
This updates access for that signing key; it does not enable SSH or services.
HELP
  exit 0
fi
[[ $# == 0 ]] || { echo 'Unexpected arguments; use --help.' >&2; exit 2; }
[[ "$(uname -s)" == Darwin && -t 0 ]] || { echo 'Run this on the Mac in a terminal, or via ssh -t.' >&2; exit 2; }
keychain="$HOME/Library/Keychains/login.keychain-db"
profile="${PR0_NOTARY_PROFILE:-pr0former-notary}"
echo 'Unlock the login Keychain using its password (normally your Mac login password).'
security unlock-keychain "$keychain"
identity="${APPLE_SIGNING_IDENTITY:-$(security find-identity -v -p codesigning "$keychain" | sed -n 's/.*"\(Developer ID Application:.*\)"/\1/p')}"
[[ "$identity" == 'Developer ID Application: '* && "$(printf '%s\n' "$identity" | wc -l | tr -d ' ')" == 1 ]] || {
  echo 'Set APPLE_SIGNING_IDENTITY to one Developer ID Application identity.' >&2; exit 1;
}
# Match the certificate's public-key hash to key metadata, then require a unique
# label. Never run a broad partition-list update on every key in login Keychain.
key_label="$(python3 - "$identity" "$keychain" <<'PY'
import re, subprocess, sys
identity, keychain = sys.argv[1:]
certificate = subprocess.check_output(['security', 'find-certificate', '-c', identity, keychain], text=True)
hashes = re.findall(r'"hpky"<blob>=0x([0-9A-Fa-f]+)', certificate)
if len(hashes) != 1:
    raise SystemExit('Cannot uniquely identify certificate public key.')
keys = subprocess.check_output(['security', 'find-key', '-t', 'private', '-s', keychain], text=True)
entries = []
for block in keys.split('keychain:')[1:]:
    label = re.search(r'0x00000001 <blob>="([^"\n]+)"', block)
    key_hash = re.search(r'0x00000006 <blob>=0x([0-9A-Fa-f]+)', block)
    if label and key_hash:
        entries.append((label[1], key_hash[1].lower()))
matching = [label for label, value in entries if value == hashes[0].lower()]
if len(matching) != 1 or sum(label == matching[0] for label, _ in entries) != 1:
    raise SystemExit('Cannot uniquely scope signing-key access; no access settings changed.')
print(matching[0])
PY
)"
printf 'Authorizing Apple signing tools for this private key only: %s\n' "$key_label"
echo 'Re-enter the login Keychain password to save its signing access settings.'
IFS= read -r -s -p 'Keychain password: ' signing_password
printf '\n'
trap 'unset signing_password' EXIT
# security requires the password argument for partition-list updates. Disable
# tracing and discard its key metadata output; never persist the password.
security set-key-partition-list -S apple-tool:,apple:,codesign: -t private -s -l "$key_label" -k "$signing_password" "$keychain" >/dev/null
unset signing_password
trap - EXIT
team="${identity##*(}"
team="${team%)}"
notary_keychain="${PR0_NOTARY_KEYCHAIN:-$keychain}"
if xcrun notarytool history --keychain-profile "$profile" --keychain "$notary_keychain" --output-format json >/dev/null 2>&1; then
  echo "Notarization profile $profile already works."
else
  echo 'Enter your Apple account and an app-specific password at the following prompts.'
  xcrun notarytool store-credentials "$profile" --team-id "$team" --keychain "$notary_keychain"
fi
project_root="$(cd "$(dirname "$0")/.." && pwd)"
APPLE_SIGNING_IDENTITY="$identity" PR0_NOTARY_KEYCHAIN="$notary_keychain" bash "$project_root/scripts/build-macos-signed.sh" --check
