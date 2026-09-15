# Desktop application

`./build.sh` on macOS/Linux and `build.ps1` on Windows build a Tauri 2 application
containing the shared pr0former runtime, the production Vue interface, and a
pinned FFmpeg executable. The runtime is the same library the standalone server
runs (`pr0_runtime`), compiled into the desktop binary rather than shipped as a
separate server executable; FFmpeg remains the one bundled external binary. The
standalone server remains available through `init.sh`, unchanged; neither build
entry point installs a desktop startup service.

## Build

```sh
./build.sh                 # macOS .app; Linux AppImage and .deb
./build.sh --bundles dmg   # macOS distributable disk image
./build.sh --install-deps  # install supported missing prerequisites
./build.sh --jobs 8
./build.sh --help

# Windows PowerShell
.\build.ps1                         # NSIS .exe and MSI
.\build.ps1 --bundles nsis          # NSIS only
.\build.ps1 --install-deps --jobs 8
```

Run on the OS/architecture being packaged. Apple Silicon and Intel Macs need
separate builds; Linux builds likewise target the build machine. The Windows path
currently supports the native `x86_64-pc-windows-msvc` target. Cross compilation
and universal macOS bundles are not implemented by these scripts. Git Bash on
Windows may invoke `./build.sh`; it forwards the same arguments to `build.ps1`.
Output is under
`desktop/src-tauri/target/release/bundle/`; on macOS, open
`macos/pr0former.app`. Builds do not launch the application.

Successful macOS builds also place `pr0former.app` in the project root for easy
opening. This ignored copy is replaced on each successful build, after any
requested signing/notarization completes. Tauri's original bundle remains in its
build directory for `--notarize-only`; that command refreshes the root copy too.

### GitHub release builds

`.github/workflows/desktop-release.yml` runs the native build entry point on a
GitHub-hosted Windows x86-64 runner. macOS and Linux packages remain local builds
on the readily available native machines. The workflow runs only when started
manually in GitHub Actions or when a `v*` tag is pushed. Manual runs leave a
seven-day downloadable NSIS installer on the workflow run. A semantic version tag
such as `v0.1.0` also sets the bundle version and creates a GitHub **prerelease**
with the Windows installer attached. Windows installer signing is not configured
yet; inspect and test every bundle before publishing it as a non-prerelease.

The workflow caches Cargo downloads and the compiled, checksum-pinned Windows
FFmpeg tree. It does not cache the much larger Rust target trees. This repository
is public, so standard GitHub-hosted Windows runner compute is free under GitHub's
current Actions policy. Artifact/storage allowances still depend on the repository
owner's plan; the manual/tag-only triggers and seven-day artifact retention bound
the default usage. Current rates and quotas are documented in GitHub's Actions
billing documentation.

The scripts require Rust 1.88 or newer and Node.js 22.12 or newer. Missing tools
and native libraries are listed before building with platform-specific install
commands. In an interactive terminal the scripts offer to install supported
dependencies; `--install-deps` accepts that step noninteractively. Package managers
may request administrator access. Dependency downloads, Cargo/npm downloads, and
the FFmpeg source download still require network access.

Build dependencies are Cargo/Rust, Node/npm, Python 3, CMake, curl, make, a C
compiler, and tar with xz support. On macOS install Xcode Command Line Tools;
Homebrew is used when the script needs to add Node.js, Python, CMake, or pkg-config.
Linux also
needs the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/):
WebKitGTK 4.1 and GTK development packages, plus ALSA/Opus development packages
for the server. For example, Debian/Ubuntu package names include
`build-essential libwebkit2gtk-4.1-dev libssl-dev librsvg2-dev patchelf
libasound2-dev libopus-dev`. Install GStreamer base/good/bad plugins and
`gstreamer1.0-libav` for multimedia testing. Build AppImages on the oldest supported
base distribution with the required WebKitGTK; an AppImage is not a guarantee of
compatibility with every Linux distribution. AppImage media bundling is enabled.

Windows requires PowerShell, the MSVC Rust toolchain, Visual Studio 2022 C++ Build
Tools, CMake, and MSYS2 UCRT64 with GCC/make for the pinned FFmpeg build. The script
can install these with `winget` and `pacman`, or prints the exact commands when it
cannot. A newly installed system tool may require reopening PowerShell before the
build can continue. Tauri's normal Windows signing environment variables remain
available; macOS `--sign`/`--notarize` flags do not apply. MSI creation may require
Windows' VBSCRIPT optional feature. Native audio/MIDI, WebView2, sample import,
installer signing, and installation must be validated on Windows hardware or a VM.

The first build downloads locked npm/Rust dependencies and the checksum-pinned
FFmpeg 8.1.1 source. FFmpeg is compiled locally and cached in `desktop/.build/`.
The application does not download FFmpeg at launch and does not invoke a system
FFmpeg. Audio import now decodes in process first — WAV, AIFF/AIFF-C, CAF, FLAC,
MP3, MP4/M4A (AAC-LC, ALAC), ADTS AAC and Ogg (Vorbis, FLAC) — and reaches the
bundled FFmpeg only for containers or codecs the in-process decoder does not
recognize. A file refused for its channel count, its length, or because it is not
audio is never retried with FFmpeg. So the additional import support is the
formats supported by this particular build, including FFmpeg's built-in audio
decoders; optional external codec libraries are not enabled. The FFmpeg importer
still uses the `fd` protocol, and both importers preserve channels and convert to
project-rate float WAV with the existing limits.

FFmpeg is a separate executable built without GPL/nonfree/version3 components.
Its exact source archive, LGPL 2.1 text and build recipe are bundled under
`resources/licenses/ffmpeg`. Dependency license texts and an inventory are also
included. Retain these notices/source when redistributing. Release publishers
should review the inventory and any native libraries added to their build.

On macOS, `build.sh` asks whether to sign and then whether to notarize when run
in a terminal. Noninteractive macOS runs require an explicit signing option.
Tauri's usual `APPLE_*` variables remain supported with `build.sh --no-prompt`;
see [Tauri signing](https://v2.tauri.app/distribute/sign/macos/). Non-system
Mach-O libraries are copied into Frameworks with relocated dependency paths.
Microphone usage descriptions and the audio-input entitlement are supplied.
Physical permission prompts and signed distribution still require device testing.

### Signed and notarized macOS test builds

Install Xcode and a **Developer ID Application** certificate with its private key
in the login Keychain. Check with `security find-identity -v -p codesigning`.
The signing wrapper selects the identity automatically when exactly one exists;
otherwise set `APPLE_SIGNING_IDENTITY` to its full name. No certificate export is
needed for local builds.

Create a notarytool profile once in your own Terminal:

```sh
xcrun notarytool store-credentials pr0former-notary
```

Follow the interactive prompts using your Apple account, Developer Team ID, and
an **app-specific password** generated at [Apple Account](https://account.apple.com/).
Do not use your normal account password or put credentials in chat, source files,
or shell startup files. The tool validates credentials and saves them in Keychain.
An existing App Store Connect API key can also be stored with `notarytool`; see
[Apple's notarization documentation](https://developer.apple.com/documentation/technotes/tn3147-migrating-to-the-latest-notarization-tool).

```sh
./build.sh                                  # ask whether to sign and notarize
./build.sh --notarize                       # build, sign, notarize, staple, verify
./build.sh --sign                           # sign only
./build.sh --unsigned                       # local ad-hoc signing, no Apple credentials
./build.sh --notarize-only                  # finish an existing signed .app
./build.sh --notarize --bundles dmg          # also produce a notarized disk image
```

Set `PR0_NOTARY_PROFILE` to use a different Keychain profile. The wrapper checks
profile access before building, uses Tauri to sign nested code and the bundle,
submits a ZIP to Apple, staples the accepted ticket to the `.app`, then verifies
the signature, ticket, and Gatekeeper acceptance. It builds the current working
tree, including uncommitted changes. Signing seals the app's resources; editing
the bundle afterward invalidates that seal. Apple can take time to process a
submission; the result and submission ID are saved to
`desktop/src-tauri/target/release/bundle/notarization.json`. If rejected, retrieve
the log using `xcrun notarytool log ID --keychain-profile pr0former-notary`.

Open `desktop/src-tauri/target/release/bundle/macos/pr0former.app` after verification.
The wrapper does not open the app or enable services. The `--sign-only` result is
signed and sealed but has no notarization ticket; it is not a verified notarized
release. With `--bundles dmg` or `--bundles app,dmg`, the signing workflow creates
`bundle/dmg/pr0former-arm64.dmg` (or `pr0former-x86_64.dmg`) using `hdiutil`, with
an Applications shortcut. In notarized mode it contains the already stapled app;
the DMG is then separately signed, notarized, stapled, and assessed. Actual device
permissions and audio/MIDI performance still require manual acceptance testing.

### Building over SSH

Connect as the same macOS user who owns the certificate and Keychain. Remote
Login must already be enabled and reachable; these scripts never configure SSH,
firewalls, startup services, or SSH authentication. This Mac currently answers
SSH on port 22 and its local network hostname is `Squanchy.local`.

From another computer on the same network:

```sh
ssh -t andy@Squanchy.local 'cd /Users/andy/Projects/pr0former && ./build.sh --notarize --unlock-keychain'
```

The `-t` allocates a terminal for the secure Keychain password prompt. The
Keychain password is normally your Mac login password, distinct from the Apple
app-specific password already stored in `pr0former-notary`. Omit `--notarize`
if you want the build's signing/notarization questions as well. Unlocking does
not disable the Keychain's existing lock policy; unlock again if it relocks.
The Mac must stay awake and reachable throughout the build/submission.

If the signing key requests desktop approval or reports that interaction is not
allowed, run the one-time setup from that SSH terminal:

```sh
cd /Users/andy/Projects/pr0former
./scripts/setup-macos-signing.sh
```

This unlocks the login Keychain, matches the selected certificate to its unique
private-key label, and authorizes Apple's signing tools for that key. It leaves
other keys alone and does not export your certificate. It prompts for the login
Keychain password again for the access update; the `security` utility receives
that password as a transient process argument, never a saved setting or shell
history entry. Existing valid notarization credentials are reused. If absent,
it prompts to create them. Run setup directly, without shell tracing or a session
recorder. Only run it when signing access needs configuring.

For a session whose Keychain is already unlocked and authorized, no terminal is
needed:

```sh
ssh andy@Squanchy.local 'cd /Users/andy/Projects/pr0former && ./build.sh --notarize'
```

This is not an unattended password-unlocking mechanism. Reboots or automatic
Keychain locking may require the `ssh -t` command again. Test remote credentials
and signing before a long build using
`./scripts/build-macos-signed.sh --check` from the SSH session. That check signs a
disposable executable and validates notarization credentials; it submits no app.
`PR0_NOTARY_KEYCHAIN` can select another Keychain file for notarization. Signing
uses the user's Keychain search list. Ensure Cargo, Node/npm, and the other build
dependencies are on the remote shell's PATH. For remote networks use your existing
VPN/SSH hostname instead of the local `.local` name.

The remote workflow follows [Tauri's signing-tool access setup](https://v2.tauri.app/distribute/sign/macos/)
and [Apple's Keychain-based notarization workflow](https://developer.apple.com/documentation/technotes/tn3147-migrating-to-the-latest-notarization-tool).


## Source layout

`desktop/src-tauri` follows the Tauri 2 library entry pattern, because Tauri
starts a mobile application from a library rather than a `main`:

- `src/lib.rs` — module wiring and `run()`, the single entry point. It carries
  `#[cfg_attr(mobile, tauri::mobile_entry_point)]`.
- `src/main.rs` — the desktop launcher; it calls `run()` and nothing else.
- `src/desktop.rs` — the macOS/Linux/Windows host: menus, windows, discovery
  browsing, the connection chooser and the hosting dialog. Compiled only for
  desktop targets, along with `connections.rs`, `discovery.rs`, `hosting.rs`,
  `trust.rs` and `windows.rs`.
- `src/mobile.rs` — the iOS/iPadOS host, with `src/audio_session.rs` beside it
  carrying the `AVAudioSession` policy, the lifecycle bridge and the two typed
  capabilities the runtime is given there: the audio policy, and the route and
  capture-authorization capability. It runs on an iPad simulator and has never
  run on a device; the Xcode project under `gen/apple` is generated from
  `tauri.ios.conf.json` and is not tracked, so iOS project changes such as the
  linked Apple frameworks belong in that configuration. Native audio *input* is
  reported as unavailable on that host — see `docs/IPAD_RUNTIME_PLAN.md`
  Phase 5 for the measured reason and the work that would change it — and output
  is unaffected. See that document for what the rest does and does not mean.
- `src/runtime.rs` — shared by both hosts: the embedded `pr0_runtime`, its
  readiness contract, the one-time handoff that installs the private owner
  cookie, and the ordered shutdown. The exclusive profile lock and the bundled
  FFmpeg converter are desktop-only.

The desktop build, bundle contents and behavior are unchanged by this layout.

## Launch and local authentication

Opening the app starts a private server on an OS-selected `127.0.0.1` port, then
opens the existing interface signed in as `admin`. The first account receives the
system administrator role for settings and user administration. It owns projects
it creates; project access still follows membership rules. The random account
password is discarded; no default password or HTTP authentication bypass exists.
The bundled admin avatar opens the profile menu, where Sign out is disabled; its private session cannot sign out.
Quit the app to end it. User creation and invitations remain available for remote
clients, whose ordinary sessions can still sign out.
Sessions follow the server's existing 24-hour expiry and are revoked on clean exit.

The runtime creates its fresh session in process, never over a pipe, a network
response or a log line, and the application shell never receives it. Instead the
runtime publishes a one-time `/__session-bootstrap/<token>` URL on its own
private loopback listener; the shell navigates the webview there exactly once,
and the runtime answers with an HttpOnly/SameSite=Strict `pr0_session` cookie —
the same one login issues — and a `303` redirect to `/`, so the interface's own
address carries nothing. The token is 72 characters of operating-system
randomness, compared in constant time, destroyed by the first successful
redemption, and in any case after eight wrong tokens, two minutes, or shutdown.
It exists only on the private Host-guarded listener: local-network hosting
serves the router without it, and the standalone server never has one. This
replaced `WebviewWindow::set_cookie`, which aborts the process on iOS; no
credential is placed in a URL that survives its use, in the desktop log, or
anywhere JavaScript can read it. The bundled connection
dialog alone has permission to open a remote session; project windows have no
frontend Tauri permissions. HTTP requests must use the actual localhost Host;
normal API authentication, CSRF headers, project authorization and WebSocket
origin validation remain enforced.

The bundled server starts in localhost-only mode. Other local browsers can reach it but
do not inherit the desktop login. Use **Server → Connect to Server…**
(**Cmd/Ctrl+Shift+K**) to enter another server's full HTTP or HTTPS base address,
including its port when needed (for example, `https://studio.local:8443`). The
server opens in a separate window titled with its address; sign in with an account
on that server. Local projects and the bundled server remain available through
**Server → Bundled Engine**, which also reopens its window if closed.

Remote windows use private cookie storage, isolated from the bundled admin login,
even for another loopback port. Remote logins are not saved across app launches.
Server addresses with credentials, paths, query strings or fragments are rejected.
Only the packaged connection dialog can invoke the connection command; remote
pages cannot open native windows through that command. Remote servers still enforce
their normal authentication and project roles. Connection failures appear in the connection dialog instead of leaving a blank
remote window. On macOS an untrusted HTTPS certificate shows the server origin and
leaf SHA-256 fingerprint. Verify that fingerprint with the host, then choose
**Trust This Server**. The app remembers that exact certificate for that HTTPS
origin; a changed certificate requires another approval. This does not install a
CA in the Mac Keychain or disable certificate checking for other servers. Other
platforms currently require their system trust store; the future iPad shell has
not been implemented. Pinned servers must use their final URL without a redirect.

**Server → Local Network** lists compatible servers advertised through
`_pr0former._tcp.local.` mDNS/DNS-SD. Discovery is embedded in the app/server;
there is no additional Bonjour installer or startup service. New standalone
server builds advertise their LAN listener; older builds need updating or a
manual URL. `PR0_DISCOVERY=0` disables standalone advertising. Wi-Fi client
isolation, blocked multicast, or local-network permissions can prevent discovery;
manual addressing remains available.

### Hosting from the Mac

Choose **Server → Host on Local Network…**, select an available HTTPS port
(default 8443), and click **Start Hosting**. Hosting is opt-in for each launch.
The additional IPv4 LAN listener shares the bundled engine, accounts, and projects.
It requires normal authentication and project membership and rejects the private
desktop auto-login session. Stop Hosting closes LAN access while the private
engine stays available. Quitting the app stops both. Keep the laptop awake during
a performance; no keep-running tray mode is included.

First use creates a private, persistent CA and a server certificate under
`data/hosting/` in the application profile. The certificate covers the host's short
name, `.local` name, localhost, and current interface IPs. The server certificate
is renewed when addresses change or expiry approaches; the CA stays the same.
The dialog shows separate server and CA SHA-256 fingerprints: Tauri clients
compare the server fingerprint, while iPad certificate setup uses the CA fingerprint.
Private keys remain on the host and are never downloadable.

For iPads using Safari on the same Wi-Fi:

1. Open the **iPad certificate setup address** shown in the hosting dialog.
2. Compare its CA fingerprint with the laptop, then download the certificate profile.
3. Install the profile in Settings → General → VPN & Device Management.
4. Enable full trust in Settings → General → About → Certificate Trust Settings.
5. Open the HTTPS performance address and use an invited account.

Apple requires a separate full-trust step for manually installed certificate
profiles; see [Apple's certificate instructions](https://support.apple.com/en-gb/102390).
The HTTP setup listener serves only its instructions and public certificates,
with no project or login APIs. The HTTPS listener supplies Secure session cookies.
Create a project invitation on the laptop and use its `/?invite=...` path with the
**HTTPS hosting address**, replacing the private localhost address in a copied
invitation link. The certificate itself grants no project access.

A future Tauri iPad app would embed the same shared runtime this application now
uses; `docs/IPAD_RUNTIME_PLAN.md` holds its scope, phases and gates. No iPad
build, mobile device path or App Store submission exists yet, and the existing
macOS/Linux/Windows desktop packaging is separate.
Actual iPad certificate installation, microphone/WebRTC behavior, and performance
over Wi-Fi still require physical-device testing.

### Performance windows

With a performance open, **Window → New Window for This Performance**
(**Cmd/Ctrl+Shift+N**) opens another view of that same project and server session.
Each view can show Graph, Score, Conductor, Ensemble, Monitor, or Stage independently.
Native titles end with `pr0former (bundled)` for the bundled engine or
`pr0former (https://server:port)` for a remote HTTPS server (HTTP uses its actual scheme).
Up to eight windows per performance are supported. **Window → Tile Windows**
arranges open performance windows on the focused window's display.

The app saves window count, view, size, and screen position per server/performance
in `window-layouts.json`. Loading a performance restores its saved windows;
positions are clamped onto available displays after monitor changes. Maximized
and full-screen state are not restored. Remote sessions still require sign-in
after relaunch. Layout state stays outside project revisions and undo history.

Closing the last project window/tab also stops its show and disables its engine
after the server’s five-second reconnect grace, even while the server stays open.

Closing the application runs the runtime's single ordered shutdown in process: it
stops listening, drops local-network hosting and mDNS, stops device I/O,
finalizes active loops/recordings and waits for their writer barriers, then
revokes the private session. The application waits up to 30 seconds for that
sequence; there is no child process to terminate, so an overrun is reported
rather than forced, and a crash or power loss still cannot guarantee a completed
recording. A file lock held for the whole application lifetime prevents concurrent
desktop runtimes using the same application data directory. A second launch shows
an explanation.

## Files and troubleshooting

Default writable application directories:

- macOS: `~/Library/Application Support/org.pr0former.desktop/`
- Linux: `$XDG_DATA_HOME/org.pr0former.desktop/`, normally
  `~/.local/share/org.pr0former.desktop/`
- Windows: `%APPDATA%\org.pr0former.desktop\`

Inside are `data/` (SQLite, settings, samples and loops), `recordings/`,
`desktop-server.log` (runtime diagnostics, replaced at each launch),
`desktop.lock`, `trusted-servers.json` and `window-layouts.json`. The lock is held by the OS, so a leftover
file after a crash does not prevent another launch. Application updates do not
replace these files. The desktop does not adopt the repository's `data/` folder
or an existing server database automatically.

For an isolated profile or testing, set `PR0_DESKTOP_DATA` to an absolute directory.
`PR0_DESKTOP_DISABLE_NATIVE_DEVICES=1` disables physical device access in that
profile. Both are read by the desktop application itself and are never forwarded
as `PR0_*` runtime settings. Inherited `PR0_*` server settings cannot affect the
desktop application at all: the embedded runtime reads no environment variables,
so every filesystem root and capability — application data, recordings, the
bundled frontend, and the audio import capability built from the bundled FFmpeg
path — is passed to it explicitly. An
engine startup failure is shown in the window; detailed diagnostics are in the
profile log.

Standalone server additions: `PR0_WEB_ROOT` selects the frontend directory and
`PR0_FFMPEG` selects the fallback converter executable behind the in-process
decoder. Both retain their prior defaults.
`pr0-server --desktop` remains a private stdin/stdout adapter over the same
runtime API for tests and tooling: stdout carries a session credential, private
JSON stdin commands control opt-in hosting, stdin closure requests shutdown, and
bind/TLS environment overrides are ignored. The desktop application no longer
uses it — it embeds the runtime directly. Do not use that mode as a public
service command.

## App artwork

`web/public/app-icon.png` is the supplied original artwork, shared by the web header
and desktop branding. Desktop icon formats are generated with
`desktop/node_modules/.bin/tauri icon web/public/app-icon.png --output desktop/src-tauri/icons`.
The web build also includes a 32px favicon, a 180px Apple touch icon, and 192px/512px
manifest icons for browser installation. These assets are bundled locally.

## Verification

`python3 tests/test_desktop.py` runs against the release server with temporary
profiles and disabled native devices. It checks private authentication, Host
validation, persistent owner identity, frontend serving, session revocation and
finalizing a two-channel recording on pipe closure. It also checks hosted TLS validation, CA/profile consistency, invited account access, Secure cookies, rejection of the private desktop token over LAN, setup HTTP isolation, and hosting stop/restart. Rust tests cover refusing to
adopt existing server accounts and generating fresh sessions for one stable owner.
That suite exercises the `pr0-server --desktop` stdin/stdout adapter, which shares
its lifecycle, identity and hosting implementation with the embedded desktop
runtime but is no longer what the application runs.
`cargo test --manifest-path desktop/src-tauri/Cargo.toml` covers the host side
directly: the roots and capabilities every host hands the embedded runtime, the
loopback readiness contract — including that the session handoff is a
full-length single-use path on the engine's own origin — checked before the host
navigates anywhere, the
desktop profile roots and bundled converter, and the exclusive profile lock. The
readiness and shared-configuration cases compile on every target, so they also
cover the mobile host; the mobile-only configuration case compiles only for a
mobile target and has never been run. `crates/server` lifecycle tests start and stop
isolated embedded runtimes in one process without any child process.
`web/e2e/desktop.spec.ts` exercises the desktop session/interface flow in Chromium;
it does not test native webview cookie storage or microphone permissions.
In-process engine startup, LAN hosting and quit-time recording finalization
inside the packaged application remain manual acceptance checks.

macOS/Linux/Windows hardware audio, physical MIDI, WebRTC capture/monitoring,
sleep/wake, long performances, signing/notarization, Windows installation, and
Linux distribution compatibility are manual acceptance checks until actually
performed. Desktop packaging does not change DSP timing guarantees. See STATUS
for the checks performed on this change.
