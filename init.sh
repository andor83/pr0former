#!/usr/bin/env bash
# Interactive pr0former setup. Compatible with macOS Bash 3.2 and Linux Bash.
set -euo pipefail

PROJECT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
SERVICE_ID="org.pr0former.server"
PLATFORM="$(uname -s)"
STARTUP_ONLY=false
START_ONLY=false
STOP_ONLY=false
RESTART_ONLY=false
UPDATE_ONLY=false
SIGN_ONLY=false
UPDATE_AND_START=false
UPDATE_AND_RESTART=false
SETUP_SSL=false
REMOVE_SSL=false
ALLOW_LOW_PORTS=false
NO_SSL=false
RUN_DIR="$PROJECT_DIR/.local/manual-runs"
START_HOST=""
START_PORT=""

usage() {
  cat <<'HELP'
pr0former initial setup

  ./init.sh             Check/install dependencies, build, and offer startup setup
  ./init.sh --start     Start the built server in the foreground (Ctrl-C to stop)
  ./init.sh --stop      Stop servers launched by --start and the startup service, if installed
  ./init.sh --restart   Restart the startup service (starting it if stopped); without one,
                        stop --start servers and start in the foreground
  ./init.sh --update    Rebuild frontend and release server using locked dependencies
  ./init.sh --sign      macOS: re-sign the built server with a stable code-signing identity
  ./init.sh --uas       Pull latest Git changes, rebuild, and start in the foreground
  ./init.sh --uar       Pull latest Git changes, rebuild, and restart (the startup service, or the foreground server)
  ./init.sh --startup   Interactively enable or disable startup only
  ./init.sh --setup-ssl Create a local certificate for HTTPS (restart to apply)
  ./init.sh --remove-ssl Remove managed certificates and use HTTP (restart to apply)
  ./init.sh --allow-low-ports
                        On Linux, allow the built server to bind ports 80/443
  ./init.sh --help      Show this help
  --no-ssl             Force HTTP for this --start/--uas/--restart/--uar foreground launch
  --host HOST          Override the bind host for --start/--uas/--restart/--uar (IPv4, IPv6, or hostname)
  --port PORT          Override the bind port for --start/--uas/--restart/--uar (1–65535)

macOS: startup uses a LaunchAgent for the current user, at login.
macOS: builds are code-signed with PR0_CODESIGN_IDENTITY, APPLE_SIGNING_IDENTITY, or the
single installed Developer ID Application / Apple Development identity, so the microphone
consent macOS records against the signature survives rebuilds. --sign re-signs a build.
Linux: startup uses a systemd user service, at login.
The script asks before installing dependencies or changing startup services.
On Linux, --allow-low-ports requests sudo only for setcap, not for the server.
No administrator account or password is created; bootstrap in the web interface.
--start prints the compiled Git revision and checks the tracked remote branch (up to 8 seconds).
Red warnings identify stale/dirty builds or an unverifiable version; startup still continues.
--start alone does not pull source or rebuild.
--stop leaves the startup service enabled at login; it starts again at the next login or --restart.
--restart rejects --no-ssl/--host/--port when a startup service is installed: the service always runs
.local/start-pr0former.sh, so regenerate that script with --startup to change its settings.
--start reuses .local/start-pr0former.sh when present, otherwise uses PR0_ environment settings.
--host and --port override only the specified part of that address for this launch.
--update requires installed build tools; it does not pull source or restart servers.
--uas pulls the current branch from its configured upstream (fast-forward only),
then runs --update and --start. Pull/build failures prevent startup.
It requires installed build tools and does not change startup services.
--uar does the same but finishes with --restart, so an installed startup service picks up the build.
macOS: when signing fails because the login Keychain is locked or codesign is not yet authorized for
the key, an interactive build offers to unlock the Keychain and to run scripts/setup-macos-signing.sh.
HTTPS defaults to 443 with redirects on 80; without SSL the default is HTTP on 80.
--setup-ssl/--remove-ssl take effect after restart and do not change startup services.
Trust certs/ca.pem on each client. On Linux, run --allow-low-ports once after
building, or use: PR0_HTTP_PORT=8080 ./init.sh --start --port 8443.
HELP
}
while [ "$#" -gt 0 ]; do
  argument="$1"
  case "$argument" in
    --startup) STARTUP_ONLY=true ;;
    --start) START_ONLY=true ;;
    --stop) STOP_ONLY=true ;;
    --restart) RESTART_ONLY=true ;;
    --update) UPDATE_ONLY=true ;;
    --sign) SIGN_ONLY=true ;;
    --uas) UPDATE_AND_START=true ;;
    --uar) UPDATE_AND_RESTART=true ;;
    --setup-ssl) SETUP_SSL=true ;;
    --remove-ssl) REMOVE_SSL=true ;;
    --allow-low-ports) ALLOW_LOW_PORTS=true ;;
    --no-ssl) NO_SSL=true ;;
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

mode_count=0
for selected in "$START_ONLY" "$STOP_ONLY" "$RESTART_ONLY" "$STARTUP_ONLY" "$UPDATE_ONLY" "$SIGN_ONLY" "$UPDATE_AND_START" "$UPDATE_AND_RESTART" "$SETUP_SSL" "$REMOVE_SSL" "$ALLOW_LOW_PORTS"; do
  if [ "$selected" = true ]; then mode_count=$((mode_count + 1)); fi
done
if [ "$mode_count" -gt 1 ]; then printf 'Select only one launcher action.\n' >&2; exit 2; fi
if [ "$NO_SSL" = true ] && [ "$START_ONLY" != true ] && [ "$UPDATE_AND_START" != true ] && [ "$RESTART_ONLY" != true ] && [ "$UPDATE_AND_RESTART" != true ]; then
  printf '%s\n' '--no-ssl requires --start, --uas, --restart, or --uar.' >&2; exit 2
fi

if [ -n "$START_HOST$START_PORT" ] && [ "$START_ONLY" != true ] && [ "$UPDATE_AND_START" != true ] && [ "$RESTART_ONLY" != true ] && [ "$UPDATE_AND_RESTART" != true ]; then
  printf '%s\n' '--host and --port require --start, --uas, --restart, or --uar.' >&2; exit 2
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
   { [ "$STOP_ONLY" = true ] && { [ "$START_ONLY" = true ] || [ "$STARTUP_ONLY" = true ]; }; } ||
   { [ "$UPDATE_ONLY" = true ] && { [ "$START_ONLY" = true ] || [ "$STOP_ONLY" = true ] || [ "$STARTUP_ONLY" = true ]; }; } ||
   { [ "$UPDATE_AND_START" = true ] && { [ "$START_ONLY" = true ] || [ "$STOP_ONLY" = true ] || [ "$UPDATE_ONLY" = true ] || [ "$STARTUP_ONLY" = true ]; }; }; then
  printf 'Use only one of --start, --stop, --restart, --update, --uas, --uar, or --startup.\n' >&2
  exit 2
fi
if [ "$(id -u)" -eq 0 ]; then
  printf 'Run this script as your normal user, not root. It requests sudo when needed.\nOn Linux, use ./init.sh --allow-low-ports to grant only the bind permission.\n' >&2
  exit 1
fi

capability_tool() {
  local name="$1" candidate
  candidate="$(command -v "$name" 2>/dev/null || true)"
  if [ -n "$candidate" ]; then printf '%s\n' "$candidate"; return 0; fi
  for candidate in "/usr/sbin/$name" "/sbin/$name"; do
    if [ -x "$candidate" ]; then printf '%s\n' "$candidate"; return 0; fi
  done
  return 1
}

has_low_port_permission() {
  local binary="$PROJECT_DIR/target/release/pr0-server" getcap_tool capabilities
  [ "$PLATFORM" = Linux ] || return 1
  [ -x "$binary" ] || return 1
  getcap_tool="$(capability_tool getcap || true)"
  [ -n "$getcap_tool" ] || return 1
  capabilities="$("$getcap_tool" "$binary" 2>/dev/null || true)"
  case "$capabilities" in *cap_net_bind_service*) return 0 ;; *) return 1 ;; esac
}

allow_low_ports() {
  local binary="$PROJECT_DIR/target/release/pr0-server" setcap_tool getcap_tool capabilities
  if [ "$PLATFORM" != Linux ]; then
    printf '%s\n' '--allow-low-ports is needed only on Linux.' >&2
    return 1
  fi
  if [ ! -x "$binary" ]; then
    printf 'A release build is required. Run ./init.sh first (or cargo build --release).\n' >&2
    return 1
  fi
  setcap_tool="$(capability_tool setcap || true)"
  getcap_tool="$(capability_tool getcap || true)"
  if [ -z "$setcap_tool" ] || [ -z "$getcap_tool" ]; then
    printf 'Linux capability tools are required. Install libcap2-bin (Debian/Ubuntu) or libcap (Fedora/Arch), then rerun ./init.sh --allow-low-ports.\n' >&2
    return 1
  fi
  if has_low_port_permission; then
    printf 'The release server can already bind Linux ports below 1024.\n'
    return 0
  fi
  command -v sudo >/dev/null 2>&1 || {
    printf 'sudo is required to grant the release binary low-port permission.\n' >&2
    return 1
  }
  printf 'Granting only CAP_NET_BIND_SERVICE to the release server (sudo may prompt)…\n'
  # Let sudo resolve setcap through its administrator-controlled secure_path;
  # never elevate a same-named executable supplied by the project or user PATH.
  sudo setcap cap_net_bind_service=+ep "$binary"
  capabilities="$("$getcap_tool" "$binary" 2>/dev/null || true)"
  case "$capabilities" in
    *cap_net_bind_service*)
      printf 'The release server can now bind ports 80 and 443 without running as root.\n'
      ;;
    *)
      printf 'setcap completed, but CAP_NET_BIND_SERVICE was not found on %s.\n' "$binary" >&2
      return 1
      ;;
  esac
}

linux_launch_may_need_low_ports() {
  local http_port
  [ "$PLATFORM" = Linux ] || return 1
  if [ -z "$START_PORT" ] || [ "$START_PORT" -lt 1024 ]; then return 0; fi
  [ "$NO_SSL" = true ] && return 1
  if [ -f "$PROJECT_DIR/certs/server.pem" ] || [ -n "${PR0_TLS_CERT:-}" ]; then
    http_port="${PR0_HTTP_PORT:-80}"
    case "$http_port" in ''|*[!0-9]*) return 0 ;; esac
    [ "${#http_port}" -gt 5 ] && return 0
    [ "$((10#$http_port))" -lt 1024 ] && return 0
  fi
  return 1
}

if [ "$ALLOW_LOW_PORTS" = true ]; then allow_low_ports; exit 0; fi

# The start time and owner survive exec, and guard against stale, reused PIDs.
process_identity() {
  LC_ALL=C ps -p "$1" -o uid= -o lstart= 2>/dev/null
}

# Stops servers recorded by --start. Sets MANUAL_STOPPED/MANUAL_FAILED rather than exiting so
# --restart can continue after the sweep.
MANUAL_STOPPED=false
MANUAL_FAILED=false
stop_manual_runs() {
  process_identity "$$" >/dev/null || { printf 'Cannot inspect running processes.\n' >&2; exit 1; }
  local record pid identity attempts
  for record in "$RUN_DIR"/*.pid; do
    [ -f "$record" ] || continue
    pid="${record##*/}"
    pid="${pid%.pid}"
    case "$pid" in ''|*[!0-9]*|0|1) continue ;; esac
    identity="$(cat "$record")"
    if [ -n "$identity" ] && [ "$(process_identity "$pid" || true)" = "$identity" ]; then
      if ! kill -TERM "$pid"; then
        MANUAL_FAILED=true
        continue
      fi
      MANUAL_STOPPED=true
      attempts=0
      while [ "$(process_identity "$pid" || true)" = "$identity" ]; do
        # An exited process awaiting its parent's wait is already stopped.
        case "$(LC_ALL=C ps -p "$pid" -o stat= 2>/dev/null || true)" in *Z*) break ;; esac
        if [ "$attempts" -ge 10 ]; then
          printf 'Process %s has not stopped after 10 seconds.\n' "$pid" >&2
          MANUAL_FAILED=true
          break
        fi
        sleep 1
        attempts=$((attempts + 1))
      done
      [ "$attempts" -lt 10 ] || continue
    fi
    rm -f -- "$record"
  done
}

# The startup service installed by --startup: a per-user LaunchAgent on macOS, a systemd
# user unit on Linux. These helpers are defined before `required`/`ask`, so they check tools
# themselves and never prompt.
service_plist() { printf '%s\n' "$HOME/Library/LaunchAgents/$SERVICE_ID.plist"; }
# The service counts as this project's only when it launches this directory's start
# script: another checkout (or a test copy of the launcher) must never stop or restart it.
service_installed() {
  local unit
  case "$PLATFORM" in
    Darwin) unit="$(service_plist)" ;;
    Linux) command -v systemctl >/dev/null 2>&1 && unit="$HOME/.config/systemd/user/pr0former.service" ;;
    *) return 1 ;;
  esac
  [ -n "${unit:-}" ] && [ -f "$unit" ] && grep -Fq -- "$PROJECT_DIR/.local/start-pr0former.sh" "$unit"
}
service_running() {
  case "$PLATFORM" in
    Darwin) launchctl print "gui/$(id -u)/$SERVICE_ID" 2>/dev/null | grep -q 'state = running' ;;
    Linux) systemctl --user is-active --quiet pr0former.service ;;
    *) return 1 ;;
  esac
}
# Stops the service without disabling it: it still starts at the next login.
service_stop() {
  case "$PLATFORM" in
    Darwin) launchctl bootout "gui/$(id -u)/$SERVICE_ID" >/dev/null 2>&1 || true ;;
    Linux) systemctl --user stop pr0former.service ;;
  esac
}
# Restarts a running service, or starts a stopped one.
service_restart() {
  case "$PLATFORM" in
    Darwin)
      if launchctl print "gui/$(id -u)/$SERVICE_ID" >/dev/null 2>&1; then
        launchctl kickstart -k "gui/$(id -u)/$SERVICE_ID"
      else
        launchctl enable "gui/$(id -u)/$SERVICE_ID"
        launchctl bootstrap "gui/$(id -u)" "$(service_plist)"
      fi ;;
    Linux) systemctl --user restart pr0former.service ;;
  esac
}

if [ "$STOP_ONLY" = true ]; then
  stop_manual_runs
  service_stopped=false
  if service_installed; then
    if service_running; then service_stopped=true; fi
    service_stop
  fi
  if [ "$MANUAL_FAILED" = true ]; then exit 1; fi
  if [ "$MANUAL_STOPPED" = true ]; then
    printf 'Stopped pr0former servers launched by --start.\n'
  fi
  if [ "$service_stopped" = true ]; then
    printf 'Stopped the pr0former startup service. It stays enabled at login; use --restart to start it now.\n'
  elif service_installed; then
    printf 'The pr0former startup service was not running.\n'
  fi
  if [ "$MANUAL_STOPPED" != true ] && [ "$service_stopped" != true ] && ! service_installed; then
    printf 'No servers launched by --start are running for this project, and no startup service is installed.\n'
  fi
  exit 0
fi

if [ "$RESTART_ONLY" = true ]; then
  if service_installed; then
    if [ "$NO_SSL" = true ] || [ -n "$START_HOST$START_PORT" ]; then
      printf '%s\n' '--no-ssl, --host and --port do not apply to the startup service; it runs .local/start-pr0former.sh. Regenerate it with ./init.sh --startup.' >&2
      exit 2
    fi
    # A foreground server would hold the port the service needs.
    stop_manual_runs
    if [ "$MANUAL_FAILED" = true ]; then exit 1; fi
    if [ "$MANUAL_STOPPED" = true ]; then printf 'Stopped pr0former servers launched by --start.\n'; fi
    if service_running; then
      service_restart
      printf 'Restarted the pr0former startup service.\n'
    else
      service_restart
      printf 'Started the pr0former startup service.\n'
    fi
    case "$PLATFORM" in
      Darwin) printf 'Logs: %s/data/logs\n' "$PROJECT_DIR" ;;
      Linux) printf 'Logs: journalctl --user -u pr0former -f\n' ;;
    esac
    exit 0
  fi
  stop_manual_runs
  if [ "$MANUAL_FAILED" = true ]; then exit 1; fi
  if [ "$MANUAL_STOPPED" = true ]; then
    printf 'Stopped pr0former servers launched by --start.\n'
  else
    printf 'No startup service is installed and no --start server is running; starting in the foreground.\n'
  fi
  start_args=(--start)
  if [ "$NO_SSL" = true ]; then start_args+=(--no-ssl); fi
  if [ -n "$START_HOST" ]; then start_args+=(--host "$START_HOST"); fi
  if [ -n "$START_PORT" ]; then start_args+=(--port "$START_PORT"); fi
  exec /bin/bash "$PROJECT_DIR/init.sh" "${start_args[@]}"
fi

if [ "$UPDATE_AND_START" = true ]; then
  command -v git >/dev/null 2>&1 || { printf 'Git is required for --uas.\n' >&2; exit 1; }
  printf '\nPulling the latest tracked Git branch…\n'
  # Never create a merge commit or automatically stash the user's edits.
  git -C "$PROJECT_DIR" -c pull.rebase=false -c merge.autoStash=false pull --ff-only
  # Re-read the pulled launcher so updates to the build/start steps take effect.
  /bin/bash "$PROJECT_DIR/init.sh" --update
  start_args=(--start)
  if [ "$NO_SSL" = true ]; then start_args+=(--no-ssl); fi
  if [ -n "$START_HOST" ]; then start_args+=(--host "$START_HOST"); fi
  if [ -n "$START_PORT" ]; then start_args+=(--port "$START_PORT"); fi
  exec /bin/bash "$PROJECT_DIR/init.sh" "${start_args[@]}"
fi

if [ "$UPDATE_AND_RESTART" = true ]; then
  command -v git >/dev/null 2>&1 || { printf 'Git is required for --uar.\n' >&2; exit 1; }
  printf '\nPulling the latest tracked Git branch…\n'
  git -C "$PROJECT_DIR" -c pull.rebase=false -c merge.autoStash=false pull --ff-only
  # Re-read the pulled launcher, then hand the rebuilt server to --restart: the startup
  # service when one is installed, otherwise a foreground start with the given options.
  /bin/bash "$PROJECT_DIR/init.sh" --update
  restart_args=(--restart)
  if [ "$NO_SSL" = true ]; then restart_args+=(--no-ssl); fi
  if [ -n "$START_HOST" ]; then restart_args+=(--host "$START_HOST"); fi
  if [ -n "$START_PORT" ]; then restart_args+=(--port "$START_PORT"); fi
  exec /bin/bash "$PROJECT_DIR/init.sh" "${restart_args[@]}"
fi

version_warning() {
  printf '\033[31mWARNING: %s\033[0m\n' "$1" >&2
}
check_start_version() {
  local binary="$PROJECT_DIR/target/release/pr0-server"
  local stamp hash dirty built head branch remote ref latest pid watchdog result
  hash=unknown; dirty=unknown; built=unknown
  # Older servers ignore CLI arguments and would start a second server here.
  # Probe only binaries that explicitly contain the build-info protocol marker.
  if LC_ALL=C grep -a -q 'pr0former-build-info-v1' "$binary"; then
    if stamp="$("$binary" --build-info)"; then
      read -r stamp hash dirty built <<< "$stamp"
    fi
  fi
  printf '\nRelease binary: git %s · built at Unix %s\n' "$hash" "$built"
  if [ "$hash" = unknown ]; then version_warning 'This binary has no Git build identity. Run ./init.sh --update.'; fi
  if [ "$dirty" = true ]; then version_warning 'This binary was compiled with uncommitted changes; it is not an exact Git revision.'; fi
  if ! command -v git >/dev/null 2>&1 || ! head="$(git -C "$PROJECT_DIR" rev-parse HEAD 2>/dev/null)"; then
    version_warning 'Cannot check Git: this checkout or Git is unavailable.'; return
  fi
  printf 'Local checkout: git %s\n' "$head"
  if [ "$hash" != unknown ] && [ "$hash" != "$head" ]; then
    version_warning 'The release binary does not match this checkout. Run ./init.sh --update before using the new code.'
  fi
  if [ -n "$(git -C "$PROJECT_DIR" status --porcelain --untracked-files=normal 2>/dev/null)" ]; then
    version_warning 'The checkout has uncommitted changes; rebuild with ./init.sh --update to include source edits.'
  fi
  branch="$(git -C "$PROJECT_DIR" symbolic-ref --quiet --short HEAD 2>/dev/null || true)"
  remote="$(git -C "$PROJECT_DIR" config --get "branch.$branch.remote" || true)"
  ref="$(git -C "$PROJECT_DIR" config --get "branch.$branch.merge" || true)"
  if [ -z "$branch" ] || [ -z "$remote" ] || [ -z "$ref" ]; then
    version_warning 'No tracked upstream branch; cannot verify the latest remote version.'; return
  fi
  printf 'Checking latest %s/%s…\n' "$remote" "${ref#refs/heads/}"
  # Query the remote without fetching, changing refs, or prompting for credentials.
  local output
  output="$(mktemp "${TMPDIR:-/tmp}/pr0former-version.XXXXXX")"
  GIT_TERMINAL_PROMPT=0 GIT_SSH_COMMAND='ssh -oBatchMode=yes -oConnectTimeout=5' \
    git -C "$PROJECT_DIR" -c credential.interactive=false ls-remote --exit-code "$remote" "$ref" > "$output" 2>/dev/null &
  pid=$!
  (sleep 8; kill -TERM "$pid" 2>/dev/null || true) </dev/null >/dev/null 2>&1 &
  watchdog=$!
  result=0; wait "$pid" || result=$?
  kill "$watchdog" 2>/dev/null || true
  wait "$watchdog" 2>/dev/null || true
  latest="$(awk 'NR==1 {print $1}' "$output")"
  rm -f "$output"
  if [ "$result" -ne 0 ] || [ -z "$latest" ]; then
    version_warning 'Remote check failed or timed out; the latest version could not be verified. Continuing offline.'
  elif [ "$latest" = "$hash" ] && [ "$dirty" = false ]; then
    printf 'Release binary matches the latest remote commit (%s).\n' "$latest"
  elif [ "$hash" != unknown ] && git -C "$PROJECT_DIR" merge-base --is-ancestor "$latest" "$hash" 2>/dev/null; then
    printf 'Binary revision includes remote tip %s.\n' "$latest"
  else
    version_warning "Remote tip is $latest; this binary is not verified to include it. Update your checkout, then run ./init.sh --update."
  fi
}

# Move existing local certificates without changing keys or client trust.
if [ ! -e "$PROJECT_DIR/certs" ] && [ -d "$PROJECT_DIR/.local/ssl" ]; then
  mv -- "$PROJECT_DIR/.local/ssl" "$PROJECT_DIR/certs"
fi

if [ "$START_ONLY" = true ]; then
  if [ ! -x "$PROJECT_DIR/target/release/pr0-server" ]; then
    printf 'A release build is required. Run ./init.sh first (or cargo build --release).\n' >&2
    exit 1
  fi
  cd -- "$PROJECT_DIR"
  if linux_launch_may_need_low_ports && ! has_low_port_permission; then
    version_warning 'Linux may deny ports 80/443. Run ./init.sh --allow-low-ports as your normal user; it will request sudo only for setcap.'
  fi
  check_start_version
  if [ "$NO_SSL" = true ]; then export PR0_NO_SSL=1; fi
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

if [ "$REMOVE_SSL" = true ]; then
  mkdir -p "$PROJECT_DIR/certs"
  chmod 700 "$PROJECT_DIR/certs"
  rm -f -- "$PROJECT_DIR/certs/server.pem" "$PROJECT_DIR/certs/server-key.pem" "$PROJECT_DIR/certs/ca.pem" "$PROJECT_DIR/certs/ca-key.pem"
  touch "$PROJECT_DIR/certs/disabled"
  printf 'Managed certificates removed. Restart to serve HTTP on port 80. External TLS settings are disabled until --setup-ssl.\n'
  exit 0
fi

if [ "$UPDATE_ONLY" != true ] && [ "$SIGN_ONLY" != true ] && [ ! -t 0 ]; then
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
  if [ "$PLATFORM" = Darwin ]; then
    if [ -x /opt/homebrew/bin/brew ]; then eval "$(/opt/homebrew/bin/brew shellenv)"; fi
    if [ -x /usr/local/bin/brew ]; then eval "$(/usr/local/bin/brew shellenv)"; fi
  fi
  if [ -f "$HOME/.cargo/env" ]; then . "$HOME/.cargo/env"; fi
}
require_ffmpeg() {
  required ffmpeg
  if ! ffmpeg -hide_banner -protocols 2>/dev/null | awk '$1 == "fd" { found=1 } END { exit !found }'; then
    printf 'Upgrade FFmpeg to a build with the fd protocol for seekable audio imports.\n' >&2
    exit 1
  fi
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
        if ask 'Install Homebrew from brew.sh to manage Node.js, CMake, pkg-config, Opus, and FFmpeg?'; then
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
      command -v ffmpeg >/dev/null 2>&1 || packages+=(ffmpeg)
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
          sudo apt-get install -y build-essential curl ca-certificates git cmake pkg-config libasound2-dev libopus-dev libssl-dev libcap2-bin nodejs npm ffmpeg openssl
        elif command -v dnf >/dev/null 2>&1; then
          sudo dnf install -y gcc gcc-c++ make curl ca-certificates git cmake pkgconf-pkg-config alsa-lib-devel opus-devel openssl-devel libcap nodejs npm ffmpeg openssl
        elif command -v pacman >/dev/null 2>&1; then
          sudo pacman -S --needed base-devel curl ca-certificates git cmake pkgconf alsa-lib opus openssl libcap nodejs npm ffmpeg openssl
        else
          printf 'No supported package manager. Install a C/C++ toolchain, curl, git, CMake, pkg-config, ALSA/Opus/OpenSSL development packages, libcap tools, FFmpeg, Node.js and npm.\n' >&2
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
  required cargo; required rustc; required node; required npm; required cmake; required pkg-config; require_ffmpeg
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

setup_ssl() {
  required openssl
  local directory="$PROJECT_DIR/certs" names name san index temporary default_names lan
  if [ -f "$directory/server.pem" ] && ! ask 'Replace the existing local certificate? Clients will need to trust the new CA.'; then return; fi
  default_names="localhost 127.0.0.1 ::1 $(hostname)"
  if command -v ifconfig >/dev/null 2>&1; then
    lan="$(ifconfig 2>/dev/null | awk '$1 == "inet" && $2 != "127.0.0.1" {print $2}')"
    default_names="$default_names $lan"
  elif command -v hostname >/dev/null 2>&1; then
    lan="$(hostname -I 2>/dev/null || true)"
    default_names="$default_names $lan"
  fi
  printf 'Hostnames and IP addresses clients will use, separated by spaces [%s]: ' "$default_names"
  read -r names
  names="${names:-$default_names}"
  san=""; index=0
  for name in $names; do
    case "$name" in ''|*[!a-zA-Z0-9.:-]*|-*) printf 'Invalid certificate name: %s\n' "$name" >&2; return 1 ;; esac
    index=$((index + 1))
    case "$name" in *:*|[0-9]*.[0-9]*.[0-9]*.[0-9]*) san="${san}IP.$index = $name
" ;; *) san="${san}DNS.$index = $name
" ;; esac
  done
  mkdir -p "$directory"
  chmod 700 "$directory"
  temporary="$(mktemp -d "$directory/pending.XXXXXX")"
  # Only temporary generated files are cleaned on failure; existing keys remain usable.
  trap 'rm -rf -- "$temporary"' EXIT
  (
    umask 077
    cat > "$temporary/ca.cnf" <<'CA'
[req]
prompt = no
distinguished_name = dn
x509_extensions = ca
[dn]
CN = pr0former Local CA
[ca]
basicConstraints = critical,CA:TRUE
keyUsage = critical,keyCertSign,cRLSign
subjectKeyIdentifier = hash
CA
    openssl req -x509 -newkey rsa:2048 -nodes -sha256 -days 3650 -config "$temporary/ca.cnf" -keyout "$temporary/ca-key.pem" -out "$temporary/ca.pem"
    cat > "$temporary/server.cnf" <<'SERVER'
[req]
prompt = no
distinguished_name = dn
[dn]
CN = pr0former local server
[server]
basicConstraints = critical,CA:FALSE
keyUsage = critical,digitalSignature,keyEncipherment
extendedKeyUsage = serverAuth
subjectAltName = @names
[names]
SERVER
    printf '%s' "$san" >> "$temporary/server.cnf"
    openssl req -new -newkey rsa:2048 -nodes -sha256 -config "$temporary/server.cnf" -keyout "$temporary/server-key.pem" -out "$temporary/server.csr"
    openssl x509 -req -sha256 -days 397 -in "$temporary/server.csr" -CA "$temporary/ca.pem" -CAkey "$temporary/ca-key.pem" -CAcreateserial -extfile "$temporary/server.cnf" -extensions server -out "$temporary/server.pem"
    for name in ca.pem ca-key.pem server.pem server-key.pem; do mv -f -- "$temporary/$name" "$directory/$name"; done
    rm -f -- "$directory/disabled"
  )
  rm -rf -- "$temporary"
  trap - EXIT
  printf '\nHTTPS configured on 443, with HTTP redirects on 80. Restart the server to apply.\nTrust this CA certificate on each client: %s/ca.pem\nOn iPad: install the certificate profile, then enable full trust in Settings > General > About > Certificate Trust Settings. Never share the key files.\n' "$directory"
}

generate_startup() {
  local bind_address tls_cert tls_key default_port
  printf '\nServer bind address [automatic: HTTP 80 / HTTPS 443]: '
  read -r bind_address
  printf 'External TLS certificate path (blank uses managed SSL when configured): '
  read -r tls_cert
  tls_key=""
  if [ -n "$tls_cert" ]; then
    printf 'TLS private key path: '
    read -r tls_key
    if [ ! -r "$tls_cert" ] || [ ! -r "$tls_key" ]; then printf 'TLS files must exist and be readable.\n' >&2; return 1; fi
    case "$tls_cert:$tls_key" in /*:/*) ;; *) printf 'Use absolute paths for TLS files.\n' >&2; return 1 ;; esac
  elif [ ! -f "$PROJECT_DIR/certs/server.pem" ]; then
    printf 'Browser microphone access on other devices requires trusted HTTPS. Configure TLS before performance use.\n'
  fi
  default_port=80
  if [ -n "$tls_cert" ] || [ -f "$PROJECT_DIR/certs/server.pem" ]; then default_port=443; fi
  bind_address="${bind_address:-0.0.0.0:$default_port}"
  # The server requires host:port; a bare port answer means "all IPv4 interfaces".
  case "$bind_address" in
    *[!0-9]*) ;;
    *) bind_address="0.0.0.0:$bind_address" ;;
  esac
  case "$bind_address" in
    *:*) ;;
    *) printf 'Bind address must be host:port (for example 0.0.0.0:443) or a port number.\n' >&2; return 1 ;;
  esac
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
          printf 'If macOS asks for microphone access, allow it on the Mac'"'"'s screen: the audio device list waits for that answer.\n'
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

# macOS ties microphone consent to the server's code signature. The linker's
# ad-hoc signature is a hash of each build, so every rebuild is a new, unconsented
# program: its first input-device probe waits on a privacy prompt that a
# LaunchAgent or ssh launch can never answer, and browsers hang on "Loading
# project". A stable identity carries one consent across rebuilds. Chosen from
# PR0_CODESIGN_IDENTITY, then APPLE_SIGNING_IDENTITY, then the single installed
# "Developer ID Application" or "Apple Development" identity. Pass `required` to
# fail instead of warning when signing is not possible.
# Sign and verify in one step; prints codesign's output on failure.
codesign_server() {
  codesign --force --sign "$1" --identifier "$SERVICE_ID" --timestamp=none "$2" 2>&1 \
    && codesign --verify --strict "$2" 2>/dev/null
}
# codesign reports a locked login Keychain, or a key it may not use yet, as an
# internal error. In a terminal, offer the two fixes in order: unlock the Keychain
# (the usual cause after a reboot or over ssh -t), then authorize codesign for the
# key with the signing setup script. Non-interactive builds keep the warning.
keychain_signing_blocked() {
  printf '%s' "$1" | grep -q 'errSecInternalComponent\|User interaction is not allowed\|The specified item could not be found in the keychain'
}
sign_server_binary() {
  local required="${1:-}" binary="$PROJECT_DIR/target/release/pr0-server"
  local identity="${PR0_CODESIGN_IDENTITY:-${APPLE_SIGNING_IDENTITY:-}}" prefix candidates output
  [ "$PLATFORM" = Darwin ] || return 0
  if [ -z "$identity" ]; then
    for prefix in 'Developer ID Application: ' 'Apple Development: '; do
      # A missing or failing security tool means "no identity", not a failed build.
      candidates="$({ security find-identity -v -p codesigning 2>/dev/null || true; } | sed -n "s/.*\"\($prefix[^\"]*\)\".*/\1/p" | sort -u)"
      if [ -n "$candidates" ] && [ "$(printf '%s\n' "$candidates" | wc -l | tr -d ' ')" = 1 ]; then identity="$candidates"; break; fi
    done
  fi
  if [ -z "$identity" ]; then
    printf '\n\033[31mNo code-signing identity found; the server keeps its ad-hoc signature.\033[0m\nmacOS then asks for microphone access again after every rebuild, on the Mac'"'"'s screen, and a server started by the LaunchAgent or over ssh waits on that dialog.\nInstall a Developer ID or Apple Development certificate (or set PR0_CODESIGN_IDENTITY), then run ./init.sh --sign.\n' >&2
    if [ "$required" = required ]; then return 1; fi
    return 0
  fi
  if output="$(codesign_server "$identity" "$binary")"; then
    printf 'Signed target/release/pr0-server as %s (identifier %s).\n' "$identity" "$SERVICE_ID"
    return 0
  fi
  if [ -t 0 ] && [ -t 1 ] && keychain_signing_blocked "$output"; then
    printf '\nSigning as %s needs the login Keychain: %s\n' "$identity" "$output" >&2
    if ask 'Unlock the login Keychain and retry signing? (asks for your Mac login password)'; then
      security unlock-keychain "$HOME/Library/Keychains/login.keychain-db" || true
      if output="$(codesign_server "$identity" "$binary")"; then
        printf 'Signed target/release/pr0-server as %s (identifier %s).\n' "$identity" "$SERVICE_ID"
        return 0
      fi
    fi
    if [ -x "$PROJECT_DIR/scripts/setup-macos-signing.sh" ] \
      && ask 'codesign is not yet authorized for this key. Run scripts/setup-macos-signing.sh to unlock the Keychain and authorize it? (Ctrl-C at the notarization prompt skips that part)'; then
      APPLE_SIGNING_IDENTITY="$identity" /bin/bash "$PROJECT_DIR/scripts/setup-macos-signing.sh" || true
      if output="$(codesign_server "$identity" "$binary")"; then
        printf 'Signed target/release/pr0-server as %s (identifier %s).\n' "$identity" "$SERVICE_ID"
        return 0
      fi
    fi
  fi
  printf '\n\033[31mSigning as %s failed:\033[0m %s\nThe server keeps its ad-hoc signature, so macOS asks for microphone access again after every rebuild.\nIf the login Keychain is locked or codesign is not yet authorized for this key, run ./init.sh --sign once from a terminal on the Mac, or run ./scripts/setup-macos-signing.sh over ssh -t first.\n' "$identity" "$output" >&2
  if [ "$required" = required ]; then return 1; fi
  return 0
}

build_application() {
  local restore_low_ports=false
  if has_low_port_permission; then restore_low_ports=true; fi
  printf '\nInstalling locked frontend dependencies and building the application…\n'
  (cd "$PROJECT_DIR/web" && npm ci && npm run build)
  (cd "$PROJECT_DIR" && cargo build --release --locked)
  sign_server_binary
  if [ "$restore_low_ports" = true ] && ! has_low_port_permission; then
    printf '\nThe rebuild replaced the capable server binary; restoring its low-port permission.\n'
    allow_low_ports
  fi
}

if [ "$SIGN_ONLY" = true ]; then
  if [ "$PLATFORM" != Darwin ]; then printf -- '--sign applies to macOS only.\n' >&2; exit 2; fi
  if [ ! -x "$PROJECT_DIR/target/release/pr0-server" ]; then
    printf 'A release build is required. Run ./init.sh first (or cargo build --release).\n' >&2
    exit 1
  fi
  sign_server_binary required
  exit 0
fi

if [ "$UPDATE_ONLY" = true ]; then
  printf '\npr0former · update build\nProject: %s\n' "$PROJECT_DIR"
  activate_tools
  required cargo; required rustc; required node; required npm; required cmake; required pkg-config; require_ffmpeg
  if ! node_supported || ! rust_supported; then
    printf 'Node.js 22.12+ and Rust 1.88+ are required. Run ./init.sh to update build tools.\n' >&2
    exit 1
  fi
  build_application
  printf '\nUpdate complete. Restart the server to use the rebuilt application.\n'
  exit 0
fi

printf '\npr0former · interactive setup\nProject: %s\n' "$PROJECT_DIR"
if [ "$STARTUP_ONLY" = true ]; then startup_menu; exit 0; fi
if [ "$SETUP_SSL" = true ]; then setup_ssl; exit 0; fi
install_dependencies
build_application
if [ "$PLATFORM" = Linux ] && ! has_low_port_permission && ask 'Allow the server to bind standard web ports 80 and 443? This runs sudo only for setcap.'; then allow_low_ports; fi
if ask 'Create a local HTTPS certificate for browser audio on LAN devices?'; then setup_ssl; fi
if ask 'Run automated Rust and frontend unit tests?'; then
  (cd "$PROJECT_DIR" && cargo test --workspace --locked)
  (cd "$PROJECT_DIR/web" && npm test)
fi
printf '\nBuild complete. Start manually with:\n  cd -- %q\n  ./init.sh --start\nThen open https://localhost (with SSL) or http://localhost to create the first account.\n' "$PROJECT_DIR"
if ask 'Generate a startup script or configure automatic startup?'; then startup_menu; fi
printf '\nSetup complete. Reconfigure startup later with ./init.sh --startup.\n'
