#!/usr/bin/env bash
# Interactive pr0former setup. Compatible with macOS Bash 3.2 and Linux Bash.
set -euo pipefail

PROJECT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
SERVICE_ID="org.pr0former.server"
PLATFORM="$(uname -s)"
STARTUP_ONLY=false
START_ONLY=false
STOP_ONLY=false
RUN_DIR="$PROJECT_DIR/.local/manual-runs"
START_HOST=""
START_PORT=""

usage() {
  cat <<'HELP'
pr0former initial setup

  ./init.sh             Check/install dependencies, build, and offer startup setup
  ./init.sh --start     Start the built server in the foreground (Ctrl-C to stop)
  ./init.sh --stop      Stop servers launched by --start from this project
  ./init.sh --startup   Interactively enable or disable startup only
  ./init.sh --help      Show this help
  --host HOST          Override the bind host for --start (IPv4, IPv6, or hostname)
  --port PORT          Override the bind port for --start (1–65535)

macOS: startup uses a LaunchAgent for the current user, at login.
Linux: startup uses a systemd user service, at login.
The script asks before installing dependencies or changing startup services.
No administrator account or password is created; bootstrap in the web interface.
--start reuses .local/start-pr0former.sh when present, otherwise uses PR0_ environment settings.
--host and --port override only the specified part of that address for this launch.
HELP
}
while [ "$#" -gt 0 ]; do
  argument="$1"
  case "$argument" in
    --startup) STARTUP_ONLY=true ;;
    --start) START_ONLY=true ;;
    --stop) STOP_ONLY=true ;;
    --host|--port)
      if [ "$#" -lt 2 ] || [ -z "$2" ]; then
        printf '%s requires a value.\n' "$argument" >&2; exit 2
      fi
      if [ "$argument" = --host ]; then START_HOST="$2"; else START_PORT="$2"; fi
      shift ;;
    --help|-h) usage; exit 0 ;;
    *) printf 'Unknown option: %s\n' "$argument" >&2; usage >&2; exit 2 ;;
  esac
  shift
done

if [ -n "$START_HOST$START_PORT" ] && [ "$START_ONLY" != true ]; then
  printf '%s\n' '--host and --port require --start.' >&2; exit 2
fi
if [ -n "$START_HOST" ]; then
  case "$START_HOST" in
    -*|*[!a-zA-Z0-9.:[\]%-]*) printf 'Invalid host: %s\n' "$START_HOST" >&2; exit 2 ;;
  esac
fi
if [ -n "$START_PORT" ]; then
  case "$START_PORT" in *[!0-9]*) printf 'Port must be an integer from 1 to 65535.\n' >&2; exit 2 ;; esac
  if [ "${#START_PORT}" -gt 5 ] || [ "$((10#$START_PORT))" -lt 1 ] || [ "$((10#$START_PORT))" -gt 65535 ]; then
    printf 'Port must be an integer from 1 to 65535.\n' >&2; exit 2
  fi
  START_PORT="$((10#$START_PORT))"
fi

if { [ "$START_ONLY" = true ] && [ "$STARTUP_ONLY" = true ]; } ||
   { [ "$STOP_ONLY" = true ] && { [ "$START_ONLY" = true ] || [ "$STARTUP_ONLY" = true ]; }; }; then
  printf 'Use only one of --start, --stop, or --startup.\n' >&2
  exit 2
fi
if [ "$(id -u)" -eq 0 ]; then
  printf 'Run this script as your normal user, not root. It requests sudo when needed.\n' >&2
  exit 1
fi

# The start time and owner survive exec, and guard against stale, reused PIDs.
process_identity() {
  LC_ALL=C ps -p "$1" -o uid= -o lstart= 2>/dev/null
}

if [ "$STOP_ONLY" = true ]; then
  process_identity "$$" >/dev/null || { printf 'Cannot inspect running processes.\n' >&2; exit 1; }
  stopped=false
  failed=false
  for record in "$RUN_DIR"/*.pid; do
    [ -f "$record" ] || continue
    pid="${record##*/}"
    pid="${pid%.pid}"
    case "$pid" in ''|*[!0-9]*|0|1) continue ;; esac
    identity="$(cat "$record")"
    if [ -n "$identity" ] && [ "$(process_identity "$pid" || true)" = "$identity" ]; then
      if ! kill -TERM "$pid"; then
        failed=true
        continue
      fi
      stopped=true
      attempts=0
      while [ "$(process_identity "$pid" || true)" = "$identity" ]; do
        # An exited process awaiting its parent's wait is already stopped.
        case "$(LC_ALL=C ps -p "$pid" -o stat= 2>/dev/null || true)" in *Z*) break ;; esac
        if [ "$attempts" -ge 10 ]; then
          printf 'Process %s has not stopped after 10 seconds.\n' "$pid" >&2
          failed=true
          break
        fi
        sleep 1
        attempts=$((attempts + 1))
      done
      [ "$attempts" -lt 10 ] || continue
    fi
    rm -f -- "$record"
  done
  if [ "$failed" = true ]; then exit 1; fi
  if [ "$stopped" = true ]; then
    printf 'Stopped pr0former servers launched by --start.\n'
  else
    printf 'No servers launched by --start are running for this project.\n'
  fi
  exit 0
fi

if [ "$START_ONLY" = true ]; then
  if [ ! -x "$PROJECT_DIR/target/release/pr0-server" ]; then
    printf 'A release build is required. Run ./init.sh first (or cargo build --release).\n' >&2
    exit 1
  fi
  cd -- "$PROJECT_DIR"
  # These component overrides take precedence over PR0_BIND in saved scripts.
  if [ -n "$START_HOST" ]; then export PR0_HOST="$START_HOST"; fi
  if [ -n "$START_PORT" ]; then export PR0_PORT="$START_PORT"; fi
  mkdir -p "$RUN_DIR"
  chmod 700 "$RUN_DIR"
  identity="$(process_identity "$$")"
  [ -n "$identity" ] || { printf 'Cannot determine server process identity.\n' >&2; exit 1; }
  printf '%s\n' "$identity" > "$RUN_DIR/$$.tmp"
  mv -- "$RUN_DIR/$$.tmp" "$RUN_DIR/$$.pid"
  printf 'Starting pr0former in the foreground. Press Ctrl-C to stop.\n'
  if [ -f "$PROJECT_DIR/.local/start-pr0former.sh" ]; then
    exec /bin/bash "$PROJECT_DIR/.local/start-pr0former.sh"
  fi
  exec "$PROJECT_DIR/target/release/pr0-server"
fi

if [ ! -t 0 ]; then
  printf 'Setup needs an interactive terminal. Run ./init.sh in your terminal.\n' >&2
  exit 1
fi

ask() {
  local answer
  printf '\n%s [y/N] ' "$1"
  read -r answer || return 1
  case "$answer" in y|Y|yes|YES|Yes) return 0 ;; *) return 1 ;; esac
}
required() {
  local dependency="$1"
  if ! command -v "$dependency" >/dev/null 2>&1; then
    printf 'Required dependency is still unavailable: %s\nInstall it and rerun ./init.sh.\n' "$dependency" >&2
    exit 1
  fi
}
node_supported() {
  node -e 'const [major,minor]=process.versions.node.split(".").map(Number);process.exit(major>22||(major===22&&minor>=12)?0:1)'
}
rust_supported() {
  rustc --version | awk '{split($2,v,"."); exit !(v[1]>1 || (v[1]==1 && v[2]>=88))}'
}
activate_tools() {
  if [ -x "$HOME/.local/share/pr0former/node/bin/node" ]; then export PATH="$HOME/.local/share/pr0former/node/bin:$PATH"; fi
  if [ -x /opt/homebrew/bin/brew ]; then eval "$(/opt/homebrew/bin/brew shellenv)"; fi
  if [ -x /usr/local/bin/brew ]; then eval "$(/usr/local/bin/brew shellenv)"; fi
  if [ -f "$HOME/.cargo/env" ]; then . "$HOME/.cargo/env"; fi
}
install_dependencies() {
  printf '\nChecking build dependencies on %s…\n' "$PLATFORM"
  case "$PLATFORM" in
    Darwin)
      if ! xcode-select -p >/dev/null 2>&1; then
        if ask 'Install Apple Command Line Tools? This opens the macOS installer.'; then
          xcode-select --install
          printf 'Finish the Apple installer, then press Return to continue. '
          read -r _
          xcode-select -p >/dev/null 2>&1 || { printf 'Command Line Tools are not installed yet. Rerun setup after installation.\n'; exit 1; }
        else printf 'Command Line Tools are required.\n'; exit 1; fi
      fi
      activate_tools
      if ! command -v brew >/dev/null 2>&1; then
        if ask 'Install Homebrew from brew.sh to manage Node.js, CMake, pkg-config, and Opus?'; then
          local installer
          installer="$(mktemp -t pr0former-homebrew.XXXXXX)"
          curl --fail --show-error --location --proto '=https' --tlsv1.2 https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh -o "$installer"
          /bin/bash "$installer"
          rm -f -- "$installer"
          activate_tools
        else printf 'Install Homebrew or all dependencies manually, then rerun setup.\n'; exit 1; fi
      fi
      required brew
      local packages=()
      if ! command -v node >/dev/null 2>&1 || ! command -v npm >/dev/null 2>&1; then packages+=(node); fi
      command -v cmake >/dev/null 2>&1 || packages+=(cmake)
      command -v pkg-config >/dev/null 2>&1 || packages+=(pkgconf)
      if ! command -v pkg-config >/dev/null 2>&1 || ! pkg-config --exists opus; then packages+=(opus); fi
      if [ "${#packages[@]}" -gt 0 ]; then
        if ask "Install Homebrew dependencies: ${packages[*]}?"; then brew install "${packages[@]}"; else printf 'Dependency installation declined.\n'; exit 1; fi
      fi
      ;;
    Linux)
      printf 'Linux audio support is portable but has not yet been hardware-validated.\n'
      if ask 'Install compiler, audio, Node.js, CMake, TLS and package-config dependencies using the system package manager?'; then
        if command -v apt-get >/dev/null 2>&1; then
          sudo apt-get update
          sudo apt-get install -y build-essential curl ca-certificates git cmake pkg-config libasound2-dev libopus-dev libssl-dev nodejs npm
        elif command -v dnf >/dev/null 2>&1; then
          sudo dnf install -y gcc gcc-c++ make curl ca-certificates git cmake pkgconf-pkg-config alsa-lib-devel opus-devel openssl-devel nodejs npm
        elif command -v pacman >/dev/null 2>&1; then
          sudo pacman -S --needed base-devel curl ca-certificates git cmake pkgconf alsa-lib opus openssl nodejs npm
        else
          printf 'No supported package manager. Install a C/C++ toolchain, curl, git, CMake, pkg-config, ALSA/Opus/OpenSSL development packages, Node.js and npm.\n' >&2
        fi
      fi
      ;;
    *) printf 'Unsupported platform: %s. Use macOS or Linux.\n' "$PLATFORM" >&2; exit 1 ;;
  esac
  required curl
  activate_tools
  if ! command -v cargo >/dev/null 2>&1; then
    if ask 'Install stable Rust using the official rustup installer?'; then
      local rust_installer
      rust_installer="$(mktemp -t pr0former-rustup.XXXXXX)"
      curl --fail --show-error --location --proto '=https' --tlsv1.2 https://sh.rustup.rs -o "$rust_installer"
      sh "$rust_installer" -y --profile minimal --default-toolchain stable
      rm -f -- "$rust_installer"
      activate_tools
    else printf 'Rust is required.\n'; exit 1; fi
  fi
  required cargo; required rustc; required node; required npm; required cmake; required pkg-config
  if ! node_supported; then
    printf 'Node.js 22.12 or newer is required. Current version: %s\n' "$(node --version)"
    if [ "$PLATFORM" = Darwin ] && ask 'Install/upgrade Node.js with Homebrew?'; then
      brew install node
      brew upgrade node
      hash -r
    elif [ "$PLATFORM" = Linux ] && ask 'Install the official Node.js 24 binary into ~/.local/share/pr0former/node?'; then
      local node_arch node_archive node_temp node_destination
      case "$(uname -m)" in x86_64) node_arch=x64 ;; aarch64|arm64) node_arch=arm64 ;; *) printf 'Unsupported Node.js binary architecture.\n'; exit 1 ;; esac
      required sha256sum
      required tar
      node_temp="$(mktemp -d "${TMPDIR:-/tmp}/pr0former-node.XXXXXX")"
      curl --fail --show-error --location --proto '=https' --tlsv1.2 https://nodejs.org/dist/latest-v24.x/SHASUMS256.txt -o "$node_temp/SHASUMS256.txt"
      node_archive="$(awk -v arch="$node_arch" '$2 ~ ("^node-v24\\.[0-9]+\\.[0-9]+-linux-" arch "\\.tar\\.xz$") {print $2; exit}' "$node_temp/SHASUMS256.txt")"
      if [ -z "$node_archive" ]; then printf 'Could not find a matching official Node.js binary.\n'; exit 1; fi
      curl --fail --show-error --location --proto '=https' --tlsv1.2 "https://nodejs.org/dist/latest-v24.x/$node_archive" -o "$node_temp/$node_archive"
      (cd "$node_temp" && awk -v file="$node_archive" '$2 == file' SHASUMS256.txt | sha256sum --check -)
      node_destination="$HOME/.local/share/pr0former/node"
      mkdir -p "$node_destination"
      tar -xJf "$node_temp/$node_archive" --strip-components=1 -C "$node_destination"
      export PATH="$node_destination/bin:$PATH"
      printf 'Node.js installed. For future development shells, add %s/bin to PATH.\n' "$node_destination"
      rm -f -- "$node_temp/$node_archive" "$node_temp/SHASUMS256.txt"
      rmdir "$node_temp"
    else printf 'Install a current Node.js release from nodejs.org, then rerun setup.\n'; exit 1; fi
  fi
  if ! rust_supported; then
    if command -v rustup >/dev/null 2>&1 && ask 'Update Rust to the stable toolchain?'; then rustup update stable; else printf 'Rust 1.88 or newer is required.\n'; exit 1; fi
  fi
  # A version manager or directory override can still select an older tool after
  # its package has been upgraded. Check the tools the build will actually use.
  if ! node_supported; then
    printf 'The active Node.js is still too old. Select Node.js 22.12+ in your PATH and rerun setup.\n' >&2
    exit 1
  fi
  if ! rust_supported; then
    printf 'The active Rust is still too old. Select Rust 1.88+ (check rustup overrides) and rerun setup.\n' >&2
    exit 1
  fi
  pkg-config --exists opus || printf 'System Opus not detected; Cargo will build bundled Opus with CMake.\n'
  printf '\nDependency versions:\n'
  rustc --version
  cargo --version
  node --version
  npm --version
}

generate_startup() {
  local bind_address tls_cert tls_key
  printf '\nServer bind address [0.0.0.0:4000]: '
  read -r bind_address
  bind_address="${bind_address:-0.0.0.0:4000}"
  printf 'TLS certificate path (blank for HTTP/localhost): '
  read -r tls_cert
  tls_key=""
  if [ -n "$tls_cert" ]; then
    printf 'TLS private key path: '
    read -r tls_key
    if [ ! -r "$tls_cert" ] || [ ! -r "$tls_key" ]; then printf 'TLS files must exist and be readable.\n' >&2; return 1; fi
    case "$tls_cert:$tls_key" in /*:/*) ;; *) printf 'Use absolute paths for TLS files.\n' >&2; return 1 ;; esac
  elif [ "$bind_address" != '127.0.0.1:4000' ]; then
    printf 'Browser microphone access on other devices requires trusted HTTPS. Configure TLS before performance use.\n'
  fi
  mkdir -p "$PROJECT_DIR/.local" "$PROJECT_DIR/data/logs"
  chmod 700 "$PROJECT_DIR/.local"
  local startup_script="$PROJECT_DIR/.local/start-pr0former.sh"
  {
    printf '#!/usr/bin/env bash\nset -euo pipefail\n'
    printf 'cd -- %q\n' "$PROJECT_DIR"
    printf 'export PR0_BIND=%q\n' "$bind_address"
    printf 'export PR0_DATA=%q\n' "$PROJECT_DIR/data"
    if [ -n "$tls_cert" ]; then
      printf 'export PR0_TLS_CERT=%q\n' "$tls_cert"
      printf 'export PR0_TLS_KEY=%q\n' "$tls_key"
    fi
    printf 'exec %q\n' "$PROJECT_DIR/target/release/pr0-server"
  } > "$startup_script"
  chmod 700 "$startup_script"
  printf 'Generated %s\n' "$startup_script"
}
xml_escape() {
  # Paths become XML text, never shell code.
  printf '%s' "$1" | sed -e 's/\&/\&amp;/g' -e 's/</\&lt;/g' -e 's/>/\&gt;/g' -e 's/"/\&quot;/g' -e "s/'/\&apos;/g"
}
systemd_escape() {
  # systemd unit files interpret percent specifiers even inside quoted arguments.
  printf '%s' "$1" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g' -e 's/%/%%/g'
}
startup_menu() {
  local choice
  printf '\nStartup service\n  1) Generate and enable startup at login\n  2) Disable startup and stop the service\n  3) Generate a startup script only\n  4) Leave unchanged\nChoose [4]: '
  read -r choice
  case "${choice:-4}" in
    1|enable)
      if [ ! -x "$PROJECT_DIR/target/release/pr0-server" ]; then
        printf 'A release build is required. Run ./init.sh first (or cargo build --release).\n' >&2
        return 1
      fi
      case "$PLATFORM" in
        Darwin)
          local agent_dir="$HOME/Library/LaunchAgents"
          local plist="$agent_dir/$SERVICE_ID.plist"
          if [ -e "$plist" ] && ! ask "Replace the existing $SERVICE_ID LaunchAgent?"; then return 0; fi
          generate_startup
          mkdir -p "$agent_dir"
          cat > "$plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>Label</key><string>$SERVICE_ID</string>
<key>ProgramArguments</key><array><string>/bin/bash</string><string>$(xml_escape "$PROJECT_DIR/.local/start-pr0former.sh")</string></array>
<key>WorkingDirectory</key><string>$(xml_escape "$PROJECT_DIR")</string>
<key>RunAtLoad</key><true/>
<key>KeepAlive</key><dict><key>SuccessfulExit</key><false/></dict>
<key>ThrottleInterval</key><integer>10</integer>
<key>StandardOutPath</key><string>$(xml_escape "$PROJECT_DIR/data/logs/server.log")</string>
<key>StandardErrorPath</key><string>$(xml_escape "$PROJECT_DIR/data/logs/server-error.log")</string>
</dict></plist>
PLIST
          plutil -lint "$plist"
          launchctl bootout "gui/$(id -u)/$SERVICE_ID" >/dev/null 2>&1 || true
          launchctl enable "gui/$(id -u)/$SERVICE_ID"
          launchctl bootstrap "gui/$(id -u)" "$plist"
          printf 'Startup enabled and server started. Logs: %s/data/logs\n' "$PROJECT_DIR"
          ;;
        Linux)
          required systemctl
          local unit_dir="$HOME/.config/systemd/user"
          local unit="$unit_dir/pr0former.service"
          if [ -e "$unit" ] && ! ask 'Replace the existing pr0former user service?'; then return 0; fi
          generate_startup
          mkdir -p "$unit_dir"
          cat > "$unit" <<UNIT
[Unit]
Description=pr0former electroacoustic performance server
After=network.target

[Service]
Type=simple
ExecStart=/bin/bash "$(systemd_escape "$PROJECT_DIR/.local/start-pr0former.sh")"
WorkingDirectory="$(systemd_escape "$PROJECT_DIR")"
Restart=on-failure
RestartSec=10

[Install]
WantedBy=default.target
UNIT
          systemctl --user daemon-reload
          systemctl --user enable --now pr0former.service
          printf 'Startup enabled at login. Logs: journalctl --user -u pr0former -f\n'
          ;;
        *) printf 'Startup is supported only on macOS and Linux.\n' >&2; return 1 ;;
      esac
      ;;
    2|disable)
      if ! ask 'Disable startup and stop the running pr0former service?'; then return 0; fi
      case "$PLATFORM" in
        Darwin)
          launchctl bootout "gui/$(id -u)/$SERVICE_ID" >/dev/null 2>&1 || true
          launchctl disable "gui/$(id -u)/$SERVICE_ID"
          local plist="$HOME/Library/LaunchAgents/$SERVICE_ID.plist"
          if [ -f "$plist" ]; then rm -f -- "$plist"; fi
          ;;
        Linux)
          required systemctl
          systemctl --user disable --now pr0former.service
          ;;
        *) printf 'Unsupported startup platform.\n' >&2; return 1 ;;
      esac
      printf 'Startup disabled. Project data and generated startup script are retained.\n'
      ;;
    3|generate) generate_startup ;;
    4|'') printf 'Startup configuration unchanged.\n' ;;
    *) printf 'Invalid choice; no startup changes made.\n' >&2; return 1 ;;
  esac
}

printf '\npr0former · interactive setup\nProject: %s\n' "$PROJECT_DIR"
if [ "$STARTUP_ONLY" = true ]; then startup_menu; exit 0; fi
install_dependencies
printf '\nInstalling frontend dependencies and building the application…\n'
(cd "$PROJECT_DIR/web" && npm ci && npm run build)
(cd "$PROJECT_DIR" && cargo build --release --locked)
if ask 'Run automated Rust and frontend unit tests?'; then
  (cd "$PROJECT_DIR" && cargo test --workspace --locked)
  (cd "$PROJECT_DIR/web" && npm test)
fi
printf '\nBuild complete. Start manually with:\n  cd -- %q\n  ./init.sh --start\nThen open http://127.0.0.1:4000 to create the first account.\n' "$PROJECT_DIR"
if ask 'Generate a startup script or configure automatic startup?'; then startup_menu; fi
printf '\nSetup complete. Reconfigure startup later with ./init.sh --startup.\n'
