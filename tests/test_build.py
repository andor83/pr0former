"""Build entry-point routing only; no real signing, Keychain changes or services."""
import contextlib
import io
import json
import os
from pathlib import Path
import pty
import select
import shutil
import subprocess
import tempfile
import time
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]

class BuildTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='pr0 build ')
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        shutil.copy(ROOT / 'build.sh', self.root / 'build.sh')
        (self.root / 'scripts').mkdir()
        (self.root / 'scripts/build-macos-signed.sh').write_text(
            '#!/bin/bash\nprintf "ROUTE"\nprintf " <%s>" "$@"\nprintf "\\n"\n')
        self.bin = self.root / 'bin'
        self.bin.mkdir()
        self.stub('uname', 'echo "${TEST_OS:-Darwin}"')
        self.stub('npm', 'echo "NPM identity=${APPLE_SIGNING_IDENTITY:-none} password=${APPLE_PASSWORD:-none}"; exit 79')
        self.stub('node', 'exit 0')
        self.stub('xcode-select', 'exit 0')
        self.stub('pkg-config', 'exit 0')
        for name in ['cargo', 'rustc', 'curl', 'make', 'cc', 'tar', 'cmake', 'patchelf',
                     'file', 'wget', 'gst-inspect-1.0']:
            self.stub(name, 'exit 0')
        self.env = {k: v for k, v in os.environ.items() if not k.startswith(('APPLE_', 'PR0_'))}
        self.env['PATH'] = str(self.bin) + ':' + os.environ['PATH']

    def stub(self, name, body):
        path = self.bin / name
        path.write_text('#!/bin/bash\n' + body + '\n')
        path.chmod(0o755)

    def run_build(self, *args):
        return subprocess.run(['/bin/bash', str(self.root / 'build.sh'), *args],
                              env=self.env, capture_output=True, text=True, timeout=5)

    def test_noninteractive_requires_choice_before_build(self):
        result = self.run_build()
        self.assertEqual(result.returncode, 2)
        self.assertNotIn('NPM', result.stdout)

    def test_explicit_modes_and_arguments(self):
        for mode, expected in [('--sign', '--sign-only'), ('--notarize', '--bundles'),
                               ('--notarize-only', '--notarize-only')]:
            result = self.run_build(mode)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn('<' + expected + '>', result.stdout)
        result = self.run_build('--sign', '--bundles', 'dmg', '--jobs', '8')
        self.assertIn('<--bundles> <dmg> <--jobs> <8>', result.stdout)

    def test_unsigned_removes_notarization_credentials(self):
        self.env.update(APPLE_SIGNING_IDENTITY='Developer ID Application: Test', APPLE_PASSWORD='test-value')
        result = self.run_build('--unsigned')
        self.assertEqual(result.returncode, 79)
        self.assertIn('identity=- password=none', result.stdout)

    def test_no_prompt_retains_legacy_environment_without_recursion(self):
        self.env['APPLE_SIGNING_IDENTITY'] = 'test-identity'
        result = self.run_build('--no-prompt')
        self.assertEqual(result.returncode, 79)
        self.assertIn('identity=test-identity', result.stdout)
        self.assertNotIn('ROUTE', result.stdout)

    def test_invalid_arguments(self):
        for args in [('--sign', '--unsigned'), ('--jobs', '0'), ('--unlock-keychain', '--sign'), ('--notarize-only', '--bundles', 'dmg')]:
            self.assertEqual(self.run_build(*args).returncode, 2)

    def test_linux_remains_noninteractive(self):
        self.env['TEST_OS'] = 'Linux'
        self.assertEqual(self.run_build().returncode, 79)
        self.assertEqual(self.run_build('--notarize').returncode, 2)

    def test_windows_git_bash_routes_to_powershell(self):
        self.env['TEST_OS'] = 'MINGW64_NT-10.0'
        self.stub('powershell.exe', 'printf "POWERSHELL <%s>\\n" "$@"; exit 78')
        result = self.run_build('--bundles', 'nsis', '--jobs', '6', '--install-deps')
        self.assertEqual(result.returncode, 78)
        self.assertIn('<-File>', result.stdout)
        self.assertIn('<--bundles>', result.stdout)
        self.assertIn('<nsis>', result.stdout)
        self.assertIn('<--install-deps>', result.stdout)

    def test_windows_native_script_has_pinned_ffmpeg_and_prerequisite_help(self):
        source = (ROOT / 'build.ps1').read_text()
        self.assertIn('ffmpeg-$Target.exe', source)
        # The application runtime is linked into the desktop binary, so no
        # server executable is built or staged for packaging any more.
        self.assertNotIn('pr0-server-$Target.exe', source)
        self.assertNotIn('-p pr0-server', source)
        self.assertIn("-Filter 'pr0-server-*'", source)
        self.assertIn('Microsoft.VisualStudio.2022.BuildTools', source)
        self.assertIn('python mingw-w64-ucrt-x86_64-gcc', source)
        self.assertIn('function Refresh-ProcessPath', source)
        self.assertIn("GetEnvironmentVariable('Path', 'Machine')", source)
        self.assertIn("Join-Path $env:ProgramFiles 'CMake\\bin'", source)
        self.assertIn('function Test-PythonCandidate', source)
        self.assertIn("Arguments @('-3', '--version')", source)
        ffmpeg_source = (ROOT / 'scripts/build-ffmpeg.sh').read_text()
        self.assertIn('make_target=ffmpeg.exe', ffmpeg_source)
        self.assertIn('make -j "$jobs" "$make_target"', ffmpeg_source)

    def test_desktop_embeds_the_runtime_and_ships_only_ffmpeg_externally(self):
        config = json.loads((ROOT / 'desktop/src-tauri/tauri.conf.json').read_text())
        self.assertEqual(config['bundle']['externalBin'], ['binaries/ffmpeg'])
        build = (ROOT / 'build.sh').read_text()
        self.assertNotIn('-p pr0-server', build)
        self.assertNotIn('binaries/pr0-server-$build_target', build)
        self.assertIn('rm -f "$stage/binaries/pr0-server-"*', build)
        manifest = (ROOT / 'desktop/src-tauri/Cargo.toml').read_text()
        # The shared runtime is a path dependency, and this separate workspace
        # must repeat the repository's vendored CPAL patch.
        self.assertIn('package = "pr0-server", path = "../../crates/server"', manifest)
        self.assertIn('cpal = { path = "../../vendor/cpal" }', manifest)
        # The application shell moved into the library target, so the
        # no-child-process contract is checked across the whole host.
        host = '\n'.join(
            path.read_text()
            for path in sorted((ROOT / 'desktop/src-tauri/src').glob('*.rs'))
        )
        self.assertNotIn('Stdio', host)
        self.assertNotIn('sidecar', host)
        # The staged sidecar name, not the prose name of the shared package.
        self.assertNotIn('pr0-server-', host)

    def test_tauri_host_uses_the_library_entry_point_with_a_thin_launcher(self):
        manifest = (ROOT / 'desktop/src-tauri/Cargo.toml').read_text()
        self.assertIn('name = "pr0former_desktop_lib"', manifest)
        self.assertIn('crate-type = ["staticlib", "cdylib", "rlib"]', manifest)
        lib = (ROOT / 'desktop/src-tauri/src/lib.rs').read_text()
        self.assertIn('#[cfg_attr(mobile, tauri::mobile_entry_point)]', lib)
        self.assertIn('pub fn run()', lib)
        # Desktop affordances are compiled only for desktop targets.
        for module in ['connections', 'desktop', 'discovery', 'hosting',
                       'trust', 'windows']:
            self.assertIn(f'#[cfg(desktop)]\nmod {module};', lib)
        self.assertIn('#[cfg(mobile)]\nmod mobile;', lib)
        # The launcher only starts the shared host.
        main = (ROOT / 'desktop/src-tauri/src/main.rs').read_text()
        self.assertIn('pr0former_desktop_lib::run()', main)
        self.assertNotIn('tauri::Builder', main)

    def test_ios_host_is_sandboxed_and_bundles_no_executable(self):
        config = json.loads(
            (ROOT / 'desktop/src-tauri/tauri.ios.conf.json').read_text())
        self.assertEqual(config['bundle']['externalBin'], [])
        self.assertEqual(config['app']['security']['capabilities'], [])
        self.assertEqual(config['bundle']['iOS']['minimumSystemVersion'], '14.0')
        self.assertEqual(config['bundle']['iOS']['infoPlist'], 'Info.ios.plist')
        self.assertTrue(config['identifier'])
        # Desktop packaging keeps the one bundled external binary.
        desktop = json.loads(
            (ROOT / 'desktop/src-tauri/tauri.conf.json').read_text())
        self.assertEqual(desktop['bundle']['externalBin'], ['binaries/ffmpeg'])
        self.assertNotEqual(config['identifier'], desktop['identifier'])
        # The mobile shell never names a converter, and the shared baseline
        # configuration decodes in process.
        mobile = (ROOT / 'desktop/src-tauri/src/mobile.rs').read_text()
        self.assertNotIn('ffmpeg', mobile.lower())
        self.assertNotIn('std::process', mobile)
        runtime = (ROOT / 'desktop/src-tauri/src/runtime.rs').read_text()
        self.assertIn('config.importer = pr0_runtime::import::pure_rust();', runtime)
        # The profile lock and the bundled converter are desktop-only.
        self.assertIn('#[cfg(desktop)]\nfn lock_profile', runtime)
        self.assertIn('#[cfg(desktop)]\nfn bundled_converter', runtime)
        # Purpose strings ship, and the background-audio entitlement does not.
        plist = (ROOT / 'desktop/src-tauri/Info.ios.plist').read_text()
        for key in ['NSMicrophoneUsageDescription',
                    'NSLocalNetworkUsageDescription', 'NSBonjourServices',
                    'UISupportedInterfaceOrientations~ipad']:
            self.assertIn(key, plist)
        self.assertNotIn('<key>UIBackgroundModes</key>', plist)
        # Xcode generates UIDeviceFamily from the target's
        # TARGETED_DEVICE_FAMILY build setting and warns that a user-supplied
        # one is ignored, so the merged plist must not carry it.
        self.assertNotIn('<key>UIDeviceFamily</key>', plist)

    def test_ios_audio_lifecycle_is_wired_to_the_runtime_and_stays_foreground(self):
        # The shared runtime owns suspend/resume of the native hardware, so
        # every host — not only iOS — gets one implementation of it, and the
        # transition happens on the orchestration worker rather than in a
        # device callback.
        runtime = (ROOT / 'crates/server/src/runtime.rs').read_text()
        for method in ['pub async fn suspend_audio', 'pub async fn resume_audio',
                       'pub fn suspend_audio_blocking', 'pub fn resume_audio_blocking',
                       'pub fn audio_suspended']:
            self.assertIn(method, runtime)
        self.assertIn('pub use runtime::{AUDIO_TRANSITION_TIMEOUT',
                      (ROOT / 'crates/server/src/lib.rs').read_text())
        # The iOS bridge configures the session before the runtime can open a
        # device, and maps the four system events onto those calls.
        session = (ROOT / 'desktop/src-tauri/src/audio_session.rs').read_text()
        for notification in ['AVAudioSessionInterruptionNotification',
                             'AVAudioSessionRouteChangeNotification',
                             'AVAudioSessionMediaServicesWereResetNotification',
                             'UIApplicationDidEnterBackgroundNotification',
                             'UIApplicationDidBecomeActiveNotification']:
            self.assertIn(notification, session)
        for call in ['setCategory_mode_options_error',
                     'setPreferredSampleRate_error',
                     'setPreferredIOBufferDuration_error']:
            self.assertIn(call, session)
        # Every Objective-C call is behind the iOS target gate; the policy
        # decisions compile everywhere so a build machine can test them.
        self.assertIn('#[cfg(target_os = "ios")]\nmod ios {', session)
        self.assertIn('mod audio_session;',
                      (ROOT / 'desktop/src-tauri/src/lib.rs').read_text())
        host = (ROOT / 'desktop/src-tauri/src/runtime.rs').read_text()
        self.assertIn('crate::audio_session::prepare(&audio);', host)
        self.assertIn('crate::audio_session::install(lifecycle, audio);', host)
        # Foreground only: no background-audio mode is claimed anywhere, and a
        # keep-awake policy is a separate, later decision.
        self.assertNotIn('UIBackgroundModes', session)
        # The iOS dependencies are target-scoped, so no desktop or server build
        # acquires AVFoundation bindings.
        manifest = (ROOT / 'desktop/src-tauri/Cargo.toml').read_text()
        self.assertIn('[target.\'cfg(target_os = "ios")\'.dependencies]', manifest)
        ios_section = manifest.split(
            '[target.\'cfg(target_os = "ios")\'.dependencies]')[1]
        self.assertIn('objc2-avf-audio', ios_section.split('[target.')[0])

    def test_ios_target_names_the_frameworks_its_rust_dependencies_link(self):
        # Cargo builds the iOS application as a `staticlib`, and a static
        # archive cannot carry the `cargo:rustc-link-lib=framework=…` and
        # `#[link(kind = "framework")]` directives its crates emit. Xcode does
        # the final link, so the iOS target has to name those frameworks
        # itself. Tauri renders `bundle.iOS.frameworks` into the generated
        # XcodeGen `project.yml`, which is why the list lives here rather than
        # in the untracked `gen/apple` project.
        config = json.loads(
            (ROOT / 'desktop/src-tauri/tauri.ios.conf.json').read_text())
        frameworks = config['bundle']['iOS']['frameworks']
        # CoreMIDI: `midir`/`coremidi`. CoreAudio and AudioToolbox: the vendored
        # CPAL iOS backend through `coreaudio-rs`/`objc2-audio-toolbox`.
        # CoreFoundation and Foundation: `core-foundation` and `objc2-*`.
        # AVFAudio: the `objc2-avf-audio` AVAudioSession bindings the iOS
        # audio lifecycle bridge uses.
        for framework in ['CoreMIDI', 'CoreAudio', 'AudioToolbox', 'AVFAudio',
                          'CoreFoundation', 'Foundation']:
            self.assertIn(framework, frameworks)
        # Linkage is fixed by naming frameworks, never by dropping the feature:
        # MIDI and the WebRTC monitor stay compiled into the mobile runtime.
        manifest = (ROOT / 'crates/server/Cargo.toml').read_text()
        self.assertIn('midir = "0.10"', manifest)
        self.assertIn('webrtc = "0.14"', manifest)
        # Desktop links through Cargo and must not acquire an iOS bundle
        # section, so desktop framework resolution is unchanged.
        desktop = json.loads(
            (ROOT / 'desktop/src-tauri/tauri.conf.json').read_text())
        self.assertNotIn('iOS', desktop['bundle'])

    def test_apple_mobile_c_dependencies_match_the_shipped_ios_minimum(self):
        # `audiopus_sys` builds libopus through CMake and the `cc` crate, which
        # fall back to the installed SDK version when no deployment target is
        # given, and a plain `cargo build` for an iOS target does exactly that.
        # Cargo's own `[env]` table reaches those build scripts whichever way
        # Cargo was started. If the key is present it must agree with the
        # version the iOS bundle ships, or every C object links with a
        # deployment-target mismatch against the application.
        config = json.loads(
            (ROOT / 'desktop/src-tauri/tauri.ios.conf.json').read_text())
        minimum = config['bundle']['iOS']['minimumSystemVersion']
        cargo_config = (ROOT / '.cargo/config.toml').read_text()
        for line in cargo_config.splitlines():
            if line.startswith('IPHONEOS_DEPLOYMENT_TARGET'):
                self.assertIn(f'"{minimum}"', line)
                break
        else:
            self.skipTest('.cargo/config.toml does not pin '
                          'IPHONEOS_DEPLOYMENT_TARGET yet; see '
                          'docs/IPAD_RUNTIME_PLAN.md')

    def test_audio_import_is_a_capability_with_a_pure_rust_baseline(self):
        # The desktop installs an importer capability, not a bare converter
        # path, and the bundled FFmpeg is only the fallback behind it.
        desktop = (ROOT / 'desktop/src-tauri/src/runtime.rs').read_text()
        self.assertIn('pr0_runtime::import::standard(', desktop)
        self.assertNotIn('config.ffmpeg', desktop)
        # The in-process decoder must stay pure Rust with an explicit, audited
        # feature set so the same importer can compile for iOS.
        manifest = (ROOT / 'crates/server/Cargo.toml').read_text()
        self.assertIn('symphonia = { version = "0.5', manifest)
        self.assertIn('default-features = false', manifest)
        for feature in ['"wav"', '"aiff"', '"caf"', '"flac"', '"mp3"',
                        '"isomp4"', '"aac"', '"alac"', '"pcm"', '"ogg"',
                        '"vorbis"']:
            self.assertIn(feature, manifest)
        for excluded in ['"mkv"', '"opus"', '"all"', '"all-codecs"',
                         '"all-formats"']:
            self.assertNotIn(excluded, manifest)

    def test_mobile_targets_gain_quickjs_bindgen_without_changing_desktop(self):
        manifest = (ROOT / 'crates/server/Cargo.toml').read_text()
        # The base dependency keeps rquickjs-sys' pregenerated bindings, so no
        # desktop, server or CI build acquires a libclang requirement.
        self.assertIn('rquickjs = { version = "0.13", default-features = false,'
                      ' features = ["std"] }', manifest)
        # Only the mobile targets generate bindings, and scripting is preserved
        # there rather than stubbed out.
        self.assertIn(
            "[target.'cfg(any(target_os = \"ios\", target_os = \"android\"))'"
            ".dependencies]", manifest)
        self.assertIn('rquickjs = { version = "0.13", default-features = false,'
                      ' features = [\n    "std",\n    "bindgen",\n] }', manifest)

    def test_the_converter_process_is_not_compiled_where_spawning_is_unavailable(self):
        gate = '#[cfg(not(any(target_os = "ios", target_os = "android")))]'
        importers = (ROOT / 'crates/server/src/import/mod.rs').read_text()
        self.assertIn(gate + '\npub mod ffmpeg;', importers)
        self.assertIn(gate + '\npub use ffmpeg::FfmpegImporter;', importers)
        # The in-process decoder is never gated: pure-Rust import is the one
        # path every host, including iOS, keeps.
        self.assertIn('\npub mod pure_rust;', importers)
        self.assertIn('\npub use pure_rust::SymphoniaImporter;', importers)
        # Naming a converter on a host without process spawning cannot add one.
        self.assertIn('return pure_rust();', importers)
        # The child process lives only in the gated importer.
        pure = (ROOT / 'crates/server/src/import/pure_rust.rs').read_text()
        self.assertNotIn('process::Command', pure)
        ffmpeg = (ROOT / 'crates/server/src/import/ffmpeg.rs').read_text()
        self.assertIn('std::process::Command', ffmpeg)

    def test_the_session_reaches_the_webview_through_a_one_time_runtime_handoff(self):
        # `WebviewWindow::set_cookie` aborts the process on iOS: wry 0.55's
        # WKWebView cookie helper pumps the main run loop re-entrantly and the
        # resulting panic cannot unwind through Tao's `extern "C"` observer.
        # The session is installed by the runtime itself instead, over a
        # one-time loopback URL the host navigates to once.
        host = (ROOT / 'desktop/src-tauri/src/runtime.rs').read_text()
        # The prose explaining why it is not used is allowed; the call is not.
        self.assertNotIn('.set_cookie(', host)
        self.assertNotIn('Cookie::build', host)
        self.assertIn('runtime.bootstrap_url()', host)
        self.assertIn('.navigate(handoff)', host)
        # Nothing in the mobile shell touches cookies at all.
        mobile = (ROOT / 'desktop/src-tauri/src/mobile.rs').read_text()
        self.assertNotIn('.set_cookie(', mobile)
        # The handoff exists only for embedded runtimes, lives on the private
        # Host-guarded listener, is single use, and is never cacheable.
        runtime = (ROOT / 'crates/server/src/runtime.rs').read_text()
        self.assertIn('pub fn bootstrap_url(&self)', runtime)
        self.assertIn('RuntimeMode::Server => {', runtime)
        self.assertIn('router.merge(bootstrap_router(endpoint))', runtime)
        self.assertIn('fn constant_time_eq(', runtime)
        self.assertIn('HttpOnly; SameSite=Strict; Path=/', runtime)
        self.assertIn('header::CACHE_CONTROL, "no-store"', runtime)
        self.assertIn('StatusCode::SEE_OTHER', runtime)
        # Explicitly excluded alternatives: no JavaScript-readable token and no
        # persistent URL credential.
        self.assertNotIn('initialization_script', mobile)
        # Desktop multi-window cookie copying is a separate, per-origin
        # feature and stays desktop-only.
        lib = (ROOT / 'desktop/src-tauri/src/lib.rs').read_text()
        self.assertIn('#[cfg(desktop)]\nmod windows;', lib)

    def test_the_ios_audio_session_policy_is_a_runtime_capability(self):
        # The recording category follows an actually enabled input, so it has
        # to be reconsidered when a device is opened rather than only at
        # launch: a performer who enables a microphone mid-session would
        # otherwise open it against a playback-only session.
        config = (ROOT / 'crates/server/src/config.rs').read_text()
        self.assertIn('pub trait AudioPolicy', config)
        self.assertIn('pub audio_policy: Option<Arc<dyn AudioPolicy>>', config)
        audio = (ROOT / 'crates/server/src/audio.rs').read_text()
        # Applied on the orchestration worker, immediately before every open.
        self.assertIn('fn prepare_platform_audio(', audio)
        self.assertEqual(audio.count('prepare_platform_audio(&config'), 3)
        session = (ROOT / 'desktop/src-tauri/src/audio_session.rs').read_text()
        self.assertIn('impl pr0_runtime::AudioPolicy for Policy', session)
        host = (ROOT / 'desktop/src-tauri/src/runtime.rs').read_text()
        self.assertIn('config.audio_policy = Some(crate::audio_session::policy());',
                      host)

    def test_release_workflow_uses_hosted_windows_runner_and_bounded_artifacts(self):
        source = (ROOT / '.github/workflows/desktop-release.yml').read_text()
        self.assertIn('runs-on: windows-latest', source)
        self.assertIn('runs-on: ubuntu-latest', source)
        self.assertNotIn('runs-on: macos-', source)
        self.assertNotIn('pull_request:', source)
        self.assertNotIn('Use stable Rust', source)
        self.assertNotIn('shell: pwsh', source)
        self.assertIn('ExecutionPolicy Bypass', source)
        self.assertIn('workflow_dispatch:', source)
        self.assertIn('- "v*"', source)
        self.assertIn(r'.\build.ps1 --install-deps --bundles nsis', source)
        self.assertEqual(source.count('retention-days: 7'), 1)
        self.assertEqual(source.count('contents: write'), 1)
        self.assertIn('needs: build-windows', source)
        self.assertIn('--repo "$GITHUB_REPOSITORY"', source)
        self.assertIn('actions/cache@55cc8345863c7cc4c66a329aec7e433d2d1c52a9', source)
        self.assertIn('tar xz zstd python', source)
        self.assertIn("$env:GITHUB_PATH", source)
        self.assertLess(source.index('Install MSYS2 FFmpeg toolchain'),
                        source.index('Use Node.js 24'))
        self.assertLess(source.index('Install MSYS2 FFmpeg toolchain'),
                        source.index('Cache Cargo downloads and compiled FFmpeg'))
        self.assertIn('actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a', source)
        self.assertIn('actions/download-artifact@3e5f45b2cfb9172054b4087a40e8e0b5a5461e7c', source)
        self.assertIn('--prerelease --verify-tag', source)
        self.assertNotIn('--draft', source)

    def test_interactive_choices(self):
        for answers, expected in [('y\ny\n', 'ROUTE <--bundles>'),
                                  ('y\nn\n', 'ROUTE <--sign-only>'),
                                  ('n\n', 'NPM identity=-')]:
            master, slave = pty.openpty()
            proc = subprocess.Popen(['/bin/bash', str(self.root / 'build.sh')], env=self.env,
                                    stdin=slave, stdout=slave, stderr=slave)
            os.close(slave)
            try:
                os.write(master, answers.encode())
                output = b''
                deadline = time.monotonic() + 5
                while time.monotonic() < deadline:
                    if select.select([master], [], [], .1)[0]:
                        try:
                            data = os.read(master, 65536)
                        except OSError:
                            break
                        if not data:
                            break
                        output += data
                proc.wait(timeout=1)
                self.assertIn(expected, output.decode())
            finally:
                if proc.poll() is None:
                    proc.kill()
                proc.wait()
                os.close(master)

class SigningKeySelectionTests(unittest.TestCase):
    def select(self, keys):
        source = (ROOT / 'scripts/setup-macos-signing.sh').read_text()
        code = source.split("<<'PY'\n", 1)[1].split("\nPY\n", 1)[0]
        output = io.StringIO()
        with patch('sys.argv', ['selector', 'Developer ID Application: Test', 'test.keychain']), \
             patch('subprocess.check_output', side_effect=['"hpky"<blob>=0xAABB', keys]), \
             contextlib.redirect_stdout(output):
            exec(compile(code, '<key-selector>', 'exec'), {})
        return output.getvalue().strip()

    def test_matches_certificate_to_only_its_key(self):
        keys = 'keychain: test\n0x00000001 <blob>="wanted"\n0x00000006 <blob>=0xAABB\n'
        keys += 'keychain: test\n0x00000001 <blob>="unrelated"\n0x00000006 <blob>=0xCCDD\n'
        self.assertEqual(self.select(keys), 'wanted')

    def test_duplicate_label_or_missing_key_fails_closed(self):
        with self.assertRaises(SystemExit):
            self.select('')
        keys = 'keychain: test\n0x00000001 <blob>="duplicate"\n0x00000006 <blob>=0xAABB\n'
        keys += 'keychain: test\n0x00000001 <blob>="duplicate"\n0x00000006 <blob>=0xCCDD\n'
        with self.assertRaises(SystemExit):
            self.select(keys)

if __name__ == '__main__':
    unittest.main()
