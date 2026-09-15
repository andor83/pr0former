#!/bin/bash
# Desktop build only; init.sh remains the standalone server entry point. Bash 3.2 compatible.
set -euo pipefail
project_root="$(cd "$(dirname "$0")" && pwd)"
original_args=("$@")
bundles=""
jobs="${PR0_BUILD_JOBS:-4}"
signing=ask
unlock_keychain=false
install_deps=false
while [[ $# -gt 0 ]]; do
  case "$1" in
    --help|-h)
      cat <<'HELP'
Usage: ./build.sh [--bundles app|dmg|appimage|deb|rpm|nsis|msi] [--jobs N]
                  [--install-deps]
                  [--sign | --notarize | --unsigned | --no-prompt | --notarize-only]
                  [--unlock-keychain]

Build the pr0former Tauri desktop app for this machine's OS and architecture.
Default: macOS .app; Linux AppImage and .deb; Windows NSIS and MSI.
On Windows, run build.ps1 directly or run this file from Git Bash.
Does not start the app or install services.
Builds and bundles pinned FFmpeg from source; first build needs internet access.
Build prerequisites: Rust/Cargo, Node/npm, Python 3, CMake, curl, make, C compiler, tar/xz.
macOS: Xcode Command Line Tools. Linux: Tauri WebKitGTK 4.1/GTK development packages,
ALSA/Opus development packages and GStreamer media plugins (see docs/DESKTOP.md).
Windows: Visual Studio 2022 C++ Build Tools and MSYS2 UCRT64 (see docs/DESKTOP.md).

Missing dependencies are reported with platform-specific install commands. In a
terminal the script offers to install supported dependencies; --install-deps accepts
that installation noninteractively. System installers may request administrator access.

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
    --install-deps) install_deps=true; shift ;;
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
platform="$(uname -s)"
case "$platform" in
  Darwin) [[ -n "$bundles" ]] || bundles=app ;;
  Linux) [[ -n "$bundles" ]] || bundles=appimage,deb ;;
  MINGW*|MSYS*|CYGWIN*)
    command -v powershell.exe >/dev/null 2>&1 || {
      echo 'Windows builds require Windows PowerShell. Run .\\build.ps1 from PowerShell.' >&2
      exit 1
    }
    exec powershell.exe -NoProfile -ExecutionPolicy Bypass -File "$project_root/build.ps1" "${original_args[@]}" ;;
  *) echo 'Desktop builds support macOS, Linux, and Windows.' >&2; exit 1 ;;
esac
if [[ "$platform" == Darwin ]]; then
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

activate_tools() {
  if ! command -v node >/dev/null 2>&1 && [[ -x "$HOME/.local/share/pr0former/node/bin/node" ]]; then
    export PATH="$HOME/.local/share/pr0former/node/bin:$PATH"
  fi
  if [[ "$platform" == Darwin ]] && ! command -v brew >/dev/null 2>&1; then
    if [[ -x /opt/homebrew/bin/brew ]]; then eval "$(/opt/homebrew/bin/brew shellenv)"
    elif [[ -x /usr/local/bin/brew ]]; then eval "$(/usr/local/bin/brew shellenv)"; fi
  fi
  if ! command -v cargo >/dev/null 2>&1 && [[ -f "$HOME/.cargo/env" ]]; then . "$HOME/.cargo/env"; fi
}
ask_dependency_install() {
  [[ "$install_deps" == true ]] && return 0
  [[ -t 0 ]] || return 1
  local answer
  printf '\nInstall the missing build dependencies now? [y/N] '
  read -r answer || return 1
  case "$answer" in y|Y|yes|YES|Yes) return 0 ;; *) return 1 ;; esac
}
install_rust() {
  command -v curl >/dev/null 2>&1 || return 1
  local installer
  installer="$(mktemp -t pr0former-rustup.XXXXXX)"
  curl --fail --show-error --location --proto '=https' --tlsv1.2 https://sh.rustup.rs -o "$installer"
  sh "$installer" -y --profile minimal --default-toolchain stable
  rm -f -- "$installer"
  activate_tools
}
install_macos_dependencies() {
  if ! xcode-select -p >/dev/null 2>&1; then
    xcode-select --install || true
    echo 'Finish the Apple Command Line Tools installer, then rerun ./build.sh.' >&2
    exit 1
  fi
  local packages=()
  command -v node >/dev/null 2>&1 && command -v npm >/dev/null 2>&1 || packages+=(node)
  command -v python3 >/dev/null 2>&1 || packages+=(python)
  command -v cmake >/dev/null 2>&1 || packages+=(cmake)
  command -v pkg-config >/dev/null 2>&1 || packages+=(pkgconf)
  if ! command -v cargo >/dev/null 2>&1 || ! command -v rustc >/dev/null 2>&1; then install_rust; fi
  if [[ "${#packages[@]}" -gt 0 ]]; then
    command -v brew >/dev/null 2>&1 || {
      echo 'Homebrew is needed to install missing Node.js, Python, CMake, or pkg-config automatically.' >&2
      echo 'Install it from https://brew.sh, then rerun ./build.sh --install-deps.' >&2
      exit 1
    }
    brew install "${packages[@]}"
  fi
  activate_tools
}
install_linux_dependencies() {
  local admin=()
  if [[ "$(id -u)" -ne 0 ]]; then
    command -v sudo >/dev/null 2>&1 || { echo 'sudo is required to install Linux system packages as a non-root user.' >&2; return 1; }
    admin=(sudo)
  fi
  if command -v apt-get >/dev/null 2>&1; then
    "${admin[@]}" apt-get update
    "${admin[@]}" apt-get install -y build-essential curl ca-certificates file wget cmake pkg-config python3 nodejs npm xz-utils \
      libwebkit2gtk-4.1-dev libssl-dev librsvg2-dev patchelf libasound2-dev libopus-dev \
      libayatana-appindicator3-dev libxdo-dev gstreamer1.0-tools gstreamer1.0-plugins-base \
      gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-libav \
      libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev
  elif command -v dnf >/dev/null 2>&1; then
    "${admin[@]}" dnf install -y gcc gcc-c++ make curl ca-certificates file wget cmake pkgconf-pkg-config python3 nodejs npm xz \
      webkit2gtk4.1-devel gtk3-devel openssl-devel librsvg2-devel patchelf alsa-lib-devel opus-devel \
      libappindicator-gtk3-devel libX11-devel libxdo-devel gstreamer1-plugins-base \
      gstreamer1-plugins-good gstreamer1-plugins-bad-free gstreamer1-libav \
      gstreamer1-devel gstreamer1-plugins-base-devel
  elif command -v pacman >/dev/null 2>&1; then
    "${admin[@]}" pacman -S --needed base-devel curl ca-certificates file wget cmake pkgconf python nodejs npm xz \
      webkit2gtk-4.1 gtk3 openssl librsvg patchelf alsa-lib opus libappindicator-gtk3 xdotool \
      gst-plugins-base gst-plugins-good gst-plugins-bad gst-libav
  else
    echo 'No supported package manager was found.' >&2
    return 1
  fi
  if ! command -v cargo >/dev/null 2>&1 || ! command -v rustc >/dev/null 2>&1; then install_rust; fi
  activate_tools
}
node_supported() {
  node -e 'const [a,b]=process.versions.node.split(".").map(Number);process.exit(a>22||(a===22&&b>=12)?0:1)'
}
rust_supported() {
  rustc --version | awk '{split($2,v,"."); exit !(v[1]>1 || (v[1]==1 && v[2]>=88))}'
}
install_current_node() {
  if [[ "$platform" == Darwin ]]; then
    command -v brew >/dev/null 2>&1 || return 1
    brew install node || brew upgrade node
    hash -r
    return
  fi
  local node_arch node_archive node_temp node_destination
  case "$(uname -m)" in x86_64) node_arch=x64 ;; aarch64|arm64) node_arch=arm64 ;; *) return 1 ;; esac
  command -v sha256sum >/dev/null 2>&1 || return 1
  node_temp="$(mktemp -d "${TMPDIR:-/tmp}/pr0former-node.XXXXXX")"
  curl --fail --show-error --location --proto '=https' --tlsv1.2 \
    https://nodejs.org/dist/latest-v24.x/SHASUMS256.txt -o "$node_temp/SHASUMS256.txt"
  node_archive="$(awk -v arch="$node_arch" '$2 ~ ("^node-v24\\.[0-9]+\\.[0-9]+-linux-" arch "\\.tar\\.xz$") {print $2; exit}' "$node_temp/SHASUMS256.txt")"
  [[ -n "$node_archive" ]] || { echo 'Could not find a matching official Node.js 24 binary.' >&2; return 1; }
  curl --fail --show-error --location --proto '=https' --tlsv1.2 \
    "https://nodejs.org/dist/latest-v24.x/$node_archive" -o "$node_temp/$node_archive"
  (cd "$node_temp" && awk -v file="$node_archive" '$2 == file' SHASUMS256.txt | sha256sum --check -)
  node_destination="$HOME/.local/share/pr0former/node"
  mkdir -p "$node_destination"
  tar -xJf "$node_temp/$node_archive" --strip-components=1 -C "$node_destination"
  export PATH="$node_destination/bin:$PATH"
  rm -f -- "$node_temp/$node_archive" "$node_temp/SHASUMS256.txt"
  rmdir "$node_temp"
  echo "Node.js installed in $node_destination. Add $node_destination/bin to PATH for future shells."
}
dependency_report() {
  local dependency missing=()
  for dependency in cargo rustc node npm python3 curl make cc tar cmake; do
    command -v "$dependency" >/dev/null 2>&1 || missing+=("$dependency")
  done
  if [[ "$platform" == Darwin ]]; then
    xcode-select -p >/dev/null 2>&1 || missing+=("Xcode Command Line Tools")
  else
    command -v pkg-config >/dev/null 2>&1 || missing+=("pkg-config")
    command -v patchelf >/dev/null 2>&1 || missing+=("patchelf")
    command -v file >/dev/null 2>&1 || missing+=("file")
    command -v wget >/dev/null 2>&1 || missing+=("wget")
    command -v gst-inspect-1.0 >/dev/null 2>&1 || missing+=("gstreamer tools/plugins")
    if command -v pkg-config >/dev/null 2>&1; then
      for dependency in gtk+-3.0 webkit2gtk-4.1 alsa openssl librsvg-2.0 gstreamer-1.0; do
        pkg-config --exists "$dependency" || missing+=("$dependency development files")
      done
    fi
  fi
  echo "Missing desktop build prerequisites: ${missing[*]}" >&2
  if [[ "$platform" == Darwin ]]; then
    echo '  xcode-select --install' >&2
    echo '  brew install node python cmake pkgconf' >&2
    echo '  Rust 1.88+: https://rustup.rs' >&2
  else
    echo '  Debian/Ubuntu: sudo apt-get install build-essential curl ca-certificates file wget cmake pkg-config python3 nodejs npm xz-utils libwebkit2gtk-4.1-dev libssl-dev librsvg2-dev patchelf libasound2-dev libopus-dev libayatana-appindicator3-dev libxdo-dev gstreamer1.0-tools gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-libav libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev' >&2
    echo '  Fedora: sudo dnf install gcc gcc-c++ make curl ca-certificates file wget cmake pkgconf-pkg-config python3 nodejs npm xz webkit2gtk4.1-devel gtk3-devel openssl-devel librsvg2-devel patchelf alsa-lib-devel opus-devel libappindicator-gtk3-devel libX11-devel libxdo-devel gstreamer1-plugins-base gstreamer1-plugins-good gstreamer1-plugins-bad-free gstreamer1-libav gstreamer1-devel gstreamer1-plugins-base-devel' >&2
    echo '  Arch: sudo pacman -S --needed base-devel curl ca-certificates file wget cmake pkgconf python nodejs npm xz webkit2gtk-4.1 gtk3 openssl librsvg patchelf alsa-lib opus libappindicator-gtk3 xdotool gst-plugins-base gst-plugins-good gst-plugins-bad gst-libav' >&2
    echo '  Rust 1.88+: https://rustup.rs' >&2
  fi
}
dependencies_ready() {
  local dependency
  for dependency in cargo rustc node npm python3 curl make cc tar cmake; do
    command -v "$dependency" >/dev/null 2>&1 || return 1
  done
  if [[ "$platform" == Darwin ]]; then
    xcode-select -p >/dev/null 2>&1 || return 1
  else
    command -v pkg-config >/dev/null 2>&1 || return 1
    command -v patchelf >/dev/null 2>&1 || return 1
    command -v file >/dev/null 2>&1 || return 1
    command -v wget >/dev/null 2>&1 || return 1
    command -v gst-inspect-1.0 >/dev/null 2>&1 || return 1
    pkg-config --exists gtk+-3.0 webkit2gtk-4.1 alsa openssl librsvg-2.0 gstreamer-1.0 || return 1
  fi
}
activate_tools
if ! dependencies_ready; then
  dependency_report
  if ask_dependency_install; then
    if [[ "$platform" == Darwin ]]; then install_macos_dependencies; else install_linux_dependencies; fi
  else
    echo 'Rerun with --install-deps to install supported packages automatically.' >&2
    exit 1
  fi
fi
dependencies_ready || { dependency_report; exit 1; }
if ! node_supported; then
  echo "Node.js 22.12 or newer is required; found $(node --version)." >&2
  if ask_dependency_install; then
    install_current_node || {
      if [[ "$platform" == Darwin ]]; then echo 'Install or update Homebrew, then run: brew install node' >&2
      else echo 'Install Node.js 22.12+ from https://nodejs.org.' >&2; fi
      exit 1
    }
  else
    if [[ "$platform" == Darwin ]]; then echo 'Run: brew install node (or brew upgrade node)' >&2
    else echo 'Install Node.js 22.12+ from https://nodejs.org, or rerun with --install-deps for a verified local Node.js 24 install.' >&2; fi
    exit 1
  fi
  node_supported || { echo "The active Node.js is still too old: $(node --version)." >&2; exit 1; }
fi
if ! rust_supported; then
  echo "Rust 1.88 or newer is required; found $(rustc --version)." >&2
  if ask_dependency_install && command -v rustup >/dev/null 2>&1; then rustup update stable
  else echo 'Run: rustup update stable (install rustup from https://rustup.rs if needed).' >&2; exit 1; fi
  rust_supported || { echo "The active Rust toolchain is still too old: $(rustc --version)." >&2; exit 1; }
fi
cd "$project_root"
build_target="$(rustc -vV | sed -n 's/^host: //p')"
stage="$project_root/desktop/src-tauri"
cache="$project_root/desktop/.build/$build_target"
# Use known output directories even if the caller has a global CARGO_TARGET_DIR override.
unset CARGO_BUILD_TARGET
export CARGO_TARGET_DIR="$project_root/target"
npm ci --prefix web
npm run build --prefix web
# The application runtime is compiled into the desktop binary by the Tauri build
# below; the standalone pr0-server executable is no longer bundled.
echo 'Preparing bundled FFmpeg (the first compilation can take several minutes)…'
bash "$project_root/scripts/build-ffmpeg.sh" "$cache" "$jobs"
mkdir -p "$stage/binaries" "$stage/resources/web" "$stage/resources/licenses/ffmpeg"
# Clear only generated frontend resources so renamed assets cannot linger between builds.
python3 - "$project_root/web/dist" "$stage/resources/web" <<'PY'
import shutil,sys
shutil.rmtree(sys.argv[2]); shutil.copytree(sys.argv[1],sys.argv[2])
PY
# FFmpeg is the only external binary. Remove a server sidecar staged by an
# older build so it is not relocated, signed or shipped.
rm -f "$stage/binaries/pr0-server-"*
cp "$cache/ffmpeg-8.1.1/ffmpeg" "$stage/binaries/ffmpeg-$build_target"
chmod +x "$stage/binaries/ffmpeg-$build_target"
cp "$cache/ffmpeg-8.1.1.tar.xz" "$stage/resources/licenses/ffmpeg/"
cp "$cache/ffmpeg-8.1.1/COPYING.LGPLv2.1" "$stage/resources/licenses/ffmpeg/"
cp "$project_root/scripts/build-ffmpeg.sh" "$stage/resources/licenses/ffmpeg/build.sh"
cp "$project_root/LICENSE" "$stage/resources/licenses/pr0former-MIT.txt"
cp "$project_root/desktop/THIRD_PARTY.md" "$stage/resources/licenses/"
python3 "$project_root/scripts/collect-desktop-licenses.py" "$project_root"
config_args=()
if [[ "$platform" == Darwin ]]; then
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
