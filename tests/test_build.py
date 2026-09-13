"""Build entry-point routing only; no real signing, Keychain changes or services."""
import contextlib
import io
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
        for name in ['cargo', 'rustc', 'curl', 'make', 'cc', 'tar']:
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
