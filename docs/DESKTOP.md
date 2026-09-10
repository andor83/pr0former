# Desktop application

`./build.sh` builds a Tauri 2 application containing the existing Rust server,
the production Vue interface, and a pinned FFmpeg executable. The standalone
server remains available through `init.sh`; neither script installs a desktop
startup service as part of a build.

## Build

```sh
./build.sh                 # macOS .app; Linux AppImage and .deb
./build.sh --bundles dmg   # macOS distributable disk image
./build.sh --jobs 8
./build.sh --help
```

Run on the OS/architecture being packaged. Apple Silicon and Intel Macs need
separate builds; Linux builds likewise target the build machine. Cross compilation
and universal macOS bundles are not implemented by this script. Output is under
`desktop/src-tauri/target/release/bundle/`; on macOS, open
`macos/pr0former.app`. Builds do not launch the application.

Build dependencies are Cargo/Rust, Node/npm, Python 3, curl, make, a C compiler,
and tar with xz support. On macOS install Xcode Command Line Tools. Linux also
needs the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/):
WebKitGTK 4.1 and GTK development packages, plus ALSA/Opus development packages
for the server. For example, Debian/Ubuntu package names include
`build-essential libwebkit2gtk-4.1-dev libssl-dev librsvg2-dev patchelf
libasound2-dev libopus-dev`. Install GStreamer base/good/bad plugins and
`gstreamer1.0-libav` for multimedia testing. Build AppImages on the oldest supported
base distribution with the required WebKitGTK; an AppImage is not a guarantee of
compatibility with every Linux distribution. AppImage media bundling is enabled.

The first build downloads locked npm/Rust dependencies and the checksum-pinned
FFmpeg 8.1.1 source. FFmpeg is compiled locally and cached in `desktop/.build/`.
The application does not download FFmpeg at launch and does not invoke a system
FFmpeg. Import support is the formats supported by this particular build,
including FFmpeg's built-in audio decoders. Optional external codec libraries are
not enabled. The existing importer uses the `fd` protocol, preserves channels,
and converts to project-rate float WAV; its existing limits still apply.

FFmpeg is a separate executable built without GPL/nonfree/version3 components.
Its exact source archive, LGPL 2.1 text and build recipe are bundled under
`resources/licenses/ffmpeg`. Dependency license texts and an inventory are also
included. Retain these notices/source when redistributing. Release publishers
should review the inventory and any native libraries added to their build.

macOS local builds are not notarized releases. Configure the usual Tauri
`APPLE_*` signing/notarization environment variables for public distribution;
see [Tauri signing](https://v2.tauri.app/distribute/sign/macos/). Non-system
Mach-O libraries are copied into Frameworks with relocated dependency paths.
Microphone usage descriptions and the audio-input entitlement are supplied.
Physical permission prompts and signed distribution still require device testing.

## Launch and local authentication

Opening the app starts a private server on an OS-selected `127.0.0.1` port, then
opens the existing interface signed in as `admin`. The first account receives the
system administrator role for settings and user administration. It owns projects
it creates; project access still follows membership rules. The random account
password is discarded; no default password or HTTP authentication bypass exists.
Signing out remains effective; relaunch the app to obtain a fresh local session.
Sessions follow the server's existing 24-hour expiry and are revoked on clean exit.

The server sends its fresh session over the parent's private stdout pipe. Rust
sets an HttpOnly/SameSite=Strict cookie in the desktop webview before navigating.
The credential is never placed in a URL or the desktop log. There are no enabled
frontend Tauri capabilities. HTTP requests must use the actual localhost Host;
normal API authentication, CSRF headers, project authorization and WebSocket
origin validation remain enforced.

This initial desktop package serves localhost only. Other local browsers can
reach the HTTP server but do not inherit the desktop login. LAN sharing, remote
server selection and a keep-running tray mode are not included. Use the standalone server for existing LAN/HTTPS
collaboration. The desktop HTTP port does not require root or a self-signed cert.
Actual webview microphone/WebRTC behavior needs platform testing.

Closing the last project window/tab also stops its show and disables its engine
after the server’s five-second reconnect grace, even while the server stays open.

Closing the application closes its control pipe. The server stops device I/O,
finalizes active loops/recordings and waits for their writer barriers before exit.
The same EOF path handles a launcher crash. The launcher waits up to 30 seconds
before forcing an unresponsive server down; forced termination/power loss cannot
guarantee a completed recording. A file lock prevents concurrent desktop servers
using the same application data directory. A second launch shows an explanation.

## Files and troubleshooting

Default writable application directories:

- macOS: `~/Library/Application Support/org.pr0former.desktop/`
- Linux: `$XDG_DATA_HOME/org.pr0former.desktop/`, normally
  `~/.local/share/org.pr0former.desktop/`

Inside are `data/` (SQLite, settings, samples and loops), `recordings/`,
`desktop-server.log` and `desktop.lock`. The lock is held by the OS, so a leftover
file after a crash does not prevent another launch. Application updates do not
replace these files. The desktop does not adopt the repository's `data/` folder
or an existing server database automatically.

For an isolated profile or testing, set `PR0_DESKTOP_DATA` to an absolute directory.
`PR0_DESKTOP_DISABLE_NATIVE_DEVICES=1` disables physical device access in that
profile. Other inherited `PR0_*` server settings are deliberately removed by the
launcher; desktop-owned resource/data paths are passed explicitly. A server
startup failure is shown in the window; detailed errors are in the profile log.

Standalone server additions: `PR0_WEB_ROOT` selects the frontend directory and
`PR0_FFMPEG` selects the converter executable. Both retain their prior defaults.
`pr0-server --desktop` is the launcher's private protocol: stdout carries a session
credential, stdin closure requests shutdown, and bind/TLS overrides are ignored.
Do not use that mode as a public service command.

## Verification

`python3 tests/test_desktop.py` runs against the release server with temporary
profiles and disabled native devices. It checks private authentication, Host
validation, persistent owner identity, frontend serving, session revocation and
finalizing a two-channel recording on pipe closure. Rust tests cover refusing to
adopt existing server accounts and generating fresh sessions for one stable owner.
`web/e2e/desktop.spec.ts` exercises the desktop session/interface flow in Chromium;
it does not test native webview cookie storage or microphone permissions.

macOS/Linux hardware audio, physical MIDI, WebRTC capture/monitoring, sleep/wake,
long performances, signing/notarization and Linux distribution compatibility are
manual acceptance checks until actually performed. Desktop packaging does not
change DSP timing guarantees. See STATUS for the checks performed on this change.
