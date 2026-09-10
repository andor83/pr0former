#!/bin/bash
# Desktop build only; init.sh remains the standalone server entry point. Bash 3.2 compatible.
set -euo pipefail
project_root="$(cd "$(dirname "$0")" && pwd)"
bundles=""
jobs="${PR0_BUILD_JOBS:-4}"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --help|-h)
      cat <<'HELP'
Usage: ./build.sh [--bundles app|dmg|appimage|deb|rpm] [--jobs N]

Build the pr0former Tauri desktop app for this machine's OS and architecture.
Default: macOS .app; Linux AppImage and .deb. Does not start the app or install services.
Builds and bundles pinned FFmpeg from source; first build needs internet access.
Build prerequisites: Rust/Cargo, Node/npm, Python 3, curl, make, C compiler, tar/xz.
macOS: Xcode Command Line Tools. Linux: Tauri WebKitGTK 4.1/GTK development packages,
ALSA/Opus development packages and GStreamer media plugins (see docs/DESKTOP.md).

Output: desktop/src-tauri/target/release/bundle/
For signed/notarized macOS builds, set Tauri's APPLE_* signing variables.
HELP
      exit 0 ;;
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
