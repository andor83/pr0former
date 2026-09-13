#!/bin/bash
# Desktop build only; init.sh remains the standalone server entry point. Bash 3.2 compatible.
set -euo pipefail
project_root="$(cd "$(dirname "$0")" && pwd)"
bundles=""
jobs="${PR0_BUILD_JOBS:-4}"
signing=ask
unlock_keychain=false
while [[ $# -gt 0 ]]; do
  case "$1" in
    --help|-h)
      cat <<'HELP'
Usage: ./build.sh [--bundles app|dmg|appimage|deb|rpm] [--jobs N]
                  [--sign | --notarize | --unsigned | --no-prompt | --notarize-only]
                  [--unlock-keychain]

Build the pr0former Tauri desktop app for this machine's OS and architecture.
Default: macOS .app; Linux AppImage and .deb. Does not start the app or install services.
Builds and bundles pinned FFmpeg from source; first build needs internet access.
Build prerequisites: Rust/Cargo, Node/npm, Python 3, curl, make, C compiler, tar/xz.
macOS: Xcode Command Line Tools. Linux: Tauri WebKitGTK 4.1/GTK development packages,
ALSA/Opus development packages and GStreamer media plugins (see docs/DESKTOP.md).

Output: desktop/src-tauri/target/release/bundle/
macOS also places pr0former.app in the project root after successful completion.
On macOS, an interactive terminal asks whether to sign and notarize.
--sign           Developer ID signing only.
--notarize       Sign, notarize with Keychain credentials, and staple.
--notarize-only  Notarize/staple the existing signed .app, without rebuilding.
--unsigned       Local ad-hoc build; ignore Apple signing/notarization credentials.
--no-prompt      Do not ask; retain Tauri's APPLE_* environment behavior.
--unlock-keychain  Prompt securely for the login Keychain password (use ssh -t).
Without a terminal, choose a signing option explicitly on macOS (SSH/CI).
PR0_NOTARY_PROFILE selects the Keychain profile (default pr0former-notary).
PR0_NOTARY_KEYCHAIN selects an explicit notarization Keychain file.
See docs/DESKTOP.md for SSH setup and secure Keychain unlocking.
HELP
      exit 0 ;;
    --unlock-keychain) unlock_keychain=true; shift ;;
    --sign|--notarize|--unsigned|--no-prompt|--notarize-only)
      [[ "$signing" == ask ]] || { echo 'Choose only one signing option.' >&2; exit 2; }
      signing="${1#--}"; shift ;;
    --bundles|--jobs)
      [[ $# -ge 2 && -n "$2" ]] || { echo "Missing value for $1" >&2; exit 2; }
      if [[ "$1" == --bundles ]]; then bundles=$2; else jobs=$2; fi
      shift 2 ;;
    *) echo "Unknown option: $1 (use --help)" >&2; exit 2 ;;
  esac
done
case "$jobs" in ''|*[!0-9]*|0) echo '--jobs must be a positive integer' >&2; exit 2;; esac
case "$(uname -s)" in
  Darwin) [[ -n "$bundles" ]] || bundles=app ;;
  Linux) [[ -n "$bundles" ]] || bundles=appimage,deb ;;
  *) echo 'Desktop builds currently support macOS and Linux.' >&2; exit 1 ;;
esac
if [[ "$(uname -s)" == Darwin ]]; then
  if [[ "$unlock_keychain" == true ]]; then
    [[ -t 0 ]] || { echo '--unlock-keychain requires a terminal; connect with ssh -t.' >&2; exit 2; }
    security unlock-keychain "$HOME/Library/Keychains/login.keychain-db"
  fi
  if [[ "$signing" == ask ]]; then
    [[ -t 0 ]] || { echo 'Noninteractive macOS build: choose --sign, --notarize, --unsigned, or --no-prompt.' >&2; exit 2; }
    read -r -p 'Sign this build with Developer ID? [y/N] ' answer || answer=n
    case "$answer" in
      y|Y|yes|YES)
        read -r -p 'Also notarize with Apple and staple the ticket? [y/N] ' answer || answer=n
        case "$answer" in y|Y|yes|YES) signing=notarize ;; *) signing=sign ;; esac ;;
      *) signing=unsigned ;;
    esac
  fi
  case "$signing" in
    sign) exec bash "$project_root/scripts/build-macos-signed.sh" --sign-only --bundles "$bundles" --jobs "$jobs" ;;
    notarize) exec bash "$project_root/scripts/build-macos-signed.sh" --bundles "$bundles" --jobs "$jobs" ;;
    notarize-only)
      [[ "$bundles" == app ]] || { echo '--notarize-only works on the existing .app; omit --bundles.' >&2; exit 2; }
      exec bash "$project_root/scripts/build-macos-signed.sh" --notarize-only ;;
    unsigned)
      unset APPLE_CERTIFICATE APPLE_CERTIFICATE_PASSWORD APPLE_ID APPLE_PASSWORD APPLE_API_KEY APPLE_API_ISSUER APPLE_API_KEY_PATH
      export APPLE_SIGNING_IDENTITY=- ;;
  esac
elif [[ "$unlock_keychain" == true || ( "$signing" != ask && "$signing" != no-prompt && "$signing" != unsigned ) ]]; then
  echo 'Signing/notarization options require macOS.' >&2; exit 2
fi
for dependency in cargo rustc npm python3 curl make cc tar; do
  command -v "$dependency" >/dev/null || { echo "Missing build dependency: $dependency" >&2; exit 1; }
done
cd "$project_root"
build_target="$(rustc -vV | sed -n 's/^host: //p')"
stage="$project_root/desktop/src-tauri"
cache="$project_root/desktop/.build/$build_target"
# Use known output directories even if the caller has a global CARGO_TARGET_DIR override.
unset CARGO_BUILD_TARGET
export CARGO_TARGET_DIR="$project_root/target"
npm ci --prefix web
npm run build --prefix web
cargo build --release --locked -p pr0-server -j "$jobs"
echo 'Preparing bundled FFmpeg (the first compilation can take several minutes)…'
bash "$project_root/scripts/build-ffmpeg.sh" "$cache" "$jobs"
mkdir -p "$stage/binaries" "$stage/resources/web" "$stage/resources/licenses/ffmpeg"
# Clear only generated frontend resources so renamed assets cannot linger between builds.
python3 - "$project_root/web/dist" "$stage/resources/web" <<'PY'
import shutil,sys
shutil.rmtree(sys.argv[2]); shutil.copytree(sys.argv[1],sys.argv[2])
PY
cp "$project_root/target/release/pr0-server" "$stage/binaries/pr0-server-$build_target"
cp "$cache/ffmpeg-8.1.1/ffmpeg" "$stage/binaries/ffmpeg-$build_target"
chmod +x "$stage/binaries/pr0-server-$build_target" "$stage/binaries/ffmpeg-$build_target"
cp "$cache/ffmpeg-8.1.1.tar.xz" "$stage/resources/licenses/ffmpeg/"
cp "$cache/ffmpeg-8.1.1/COPYING.LGPLv2.1" "$stage/resources/licenses/ffmpeg/"
cp "$project_root/scripts/build-ffmpeg.sh" "$stage/resources/licenses/ffmpeg/build.sh"
cp "$project_root/LICENSE" "$stage/resources/licenses/pr0former-MIT.txt"
cp "$project_root/desktop/THIRD_PARTY.md" "$stage/resources/licenses/"
python3 "$project_root/scripts/collect-desktop-licenses.py" "$project_root"
config_args=()
if [[ "$(uname -s)" == Darwin ]]; then
  python3 "$project_root/scripts/bundle-macos-libs.py" "$stage"
  config_args=(--config "$stage/bundle.generated.json")
fi
npm ci --prefix desktop
cd "$project_root/desktop"
export CARGO_TARGET_DIR="$stage/target"
./node_modules/.bin/tauri build --bundles "$bundles" "${config_args[@]}" -- --locked -j "$jobs"
echo "Desktop bundles: $stage/target/release/bundle/"
if [[ "${PR0_BUILD_DEFER_EXPORT:-0}" != 1 ]]; then
  bash "$project_root/scripts/export-desktop-app.sh"
fi
