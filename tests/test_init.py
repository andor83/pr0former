"""Isolated launcher checks; never installs or changes startup services."""
import os
from pathlib import Path
import pty
import shutil
import socket
import subprocess
import tempfile
import time
import unittest
import urllib.request


class LauncherTests(unittest.TestCase):
    @unittest.skipUnless(os.environ.get("PR0_REAL_SERVER"), "opt-in release server smoke check")
    def test_release_binding(self):
        with tempfile.TemporaryDirectory(prefix="pr0former real bind ") as directory:
            root = Path(directory)
            script = root / "init.sh"
            shutil.copyfile(Path(__file__).resolve().parents[1] / "init.sh", script)
            server = root / "target/release/pr0-server"
            server.parent.mkdir(parents=True)
            server.symlink_to(Path(os.environ["PR0_REAL_SERVER"]).resolve())
            env = {k: v for k, v in os.environ.items() if not k.startswith("PR0_")}
            env["PR0_DATA"] = str(root / "data")
            for saved in (False, True):
                port = 80
                args = []
                if saved:
                    with socket.socket() as sock:
                        sock.bind(("127.0.0.1", 0))
                        port = sock.getsockname()[1]
                    (root / ".local/start-pr0former.sh").write_text(
                        '#!/bin/bash\nexport PR0_BIND=0.0.0.0:3000\nexec "$PWD/target/release/pr0-server"\n')
                    args = ["--host", "localhost", "--port", str(port)]
                process = subprocess.Popen(["/bin/bash", str(script), "--start", *args],
                                           env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
                try:
                    for _ in range(100):
                        if process.poll() is not None:
                            self.fail(str(process.communicate()))
                        try:
                            with urllib.request.urlopen(f"http://localhost:{port}/api/status", timeout=1) as response:
                                self.assertEqual(response.status, 200)
                                break
                        except OSError:
                            time.sleep(0.05)
                    else:
                        self.fail("Server did not respond on requested port")
                    result = subprocess.run(["/bin/bash", str(script), "--stop"],
                                            capture_output=True, timeout=20)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    process.wait(timeout=5)
                finally:
                    if process.poll() is None:
                        process.kill()
                    process.communicate()

    def test_build_identity_and_remote_warnings(self):
        with tempfile.TemporaryDirectory(prefix="pr0former version ") as directory:
            root = Path(directory) / "checkout"
            root.mkdir()
            remote = Path(directory) / "remote.git"
            subprocess.run(["git", "init", "--bare", str(remote)], check=True, capture_output=True)
            def git(*args):
                return subprocess.check_output(["git", "-C", str(root), *args], stderr=subprocess.DEVNULL, text=True).strip()
            git("init", "-b", "main")
            git("config", "user.name", "Launcher test")
            git("config", "user.email", "test@example.invalid")
            (root / ".gitignore").write_text("init.sh\ntarget/\n.local/\nstarted\n")
            (root / "source").write_text("first")
            git("add", ".")
            git("commit", "-m", "first")
            git("remote", "add", "origin", str(remote))
            git("push", "-u", "origin", "main")
            old = git("rev-parse", "HEAD")
            shutil.copyfile(Path(__file__).resolve().parents[1] / "init.sh", root / "init.sh")
            server = root / "target/release/pr0-server"
            server.parent.mkdir(parents=True)
            def binary(hash, dirty="false"):
                server.write_text(f'#!/bin/bash\nif [ "${{1-}}" = --build-info ]; then echo "pr0former-build-info-v1 {hash} {dirty} 123"; exit; fi\necho started > started\n')
                server.chmod(0o700)
            def start():
                return subprocess.run(["/bin/bash", str(root / "init.sh"), "--start"], capture_output=True, text=True, timeout=12)
            binary(old)
            self.assertIn("matches the latest remote commit", start().stdout)
            (root / "source").write_text("second")
            git("add", "source"); git("commit", "-m", "second"); git("push")
            result = start()
            self.assertEqual(result.returncode, 0)
            self.assertIn("\x1b[31mWARNING", result.stderr)
            self.assertIn("does not match this checkout", result.stderr)
            self.assertIn("Remote tip is", result.stderr)
            binary(git("rev-parse", "HEAD"), "true")
            self.assertIn("compiled with uncommitted changes", start().stderr)
            git("remote", "set-url", "origin", str(remote) + "-missing")
            self.assertIn("could not be verified", start().stderr)
            server.write_text('#!/bin/bash\nif [ "$#" -ne 0 ]; then exit 99; fi\necho started > started\n')
            result = start()
            self.assertEqual(result.returncode, 0)
            self.assertIn("no Git build identity", result.stderr)

    def test_update_and_start(self):
        with tempfile.TemporaryDirectory(prefix="pr0former uas ") as directory:
            base = Path(directory)
            root, publisher, remote = base / "checkout", base / "publisher", base / "remote.git"
            root.mkdir()
            def git(at, *args):
                return subprocess.check_output(["git", "-C", str(at), *args],
                                               stderr=subprocess.DEVNULL, text=True).strip()
            subprocess.run(["git", "init", "--bare", str(remote)], check=True, capture_output=True)
            git(root, "init", "-b", "main")
            git(root, "config", "user.name", "Launcher test")
            git(root, "config", "user.email", "test@example.invalid")
            script = root / "init.sh"
            source = (Path(__file__).resolve().parents[1] / "init.sh").read_text()
            script.write_text(source)
            (root / ".gitignore").write_text("target/\n.local/\ntrace\n")
            (root / "source").write_text("initial")
            git(root, "add", "."); git(root, "commit", "-m", "initial")
            git(root, "remote", "add", "origin", str(remote))
            git(root, "push", "-u", "origin", "main")
            subprocess.run(["git", "clone", "-b", "main", str(remote), str(publisher)],
                           check=True, capture_output=True)
            git(publisher, "config", "user.name", "Launcher test")
            git(publisher, "config", "user.email", "test@example.invalid")
            # The fetched script mocks only compilation. --uas and --start are
            # real, so this also checks that the newly pulled launcher is used.
            build_stub = r'''if [ "${1-}" = --update ]; then
  cd -- "$(dirname -- "${BASH_SOURCE[0]}")"
  printf 'build\n' >> trace
  if [ "${PR0_TEST_BUILD_STATUS:-0}" != 0 ]; then exit "$PR0_TEST_BUILD_STATUS"; fi
  mkdir -p target/release
  cat > target/release/pr0-server <<'SERVER'
#!/bin/bash
printf 'start|%s|%s|%s\n' "${PR0_HOST-}" "${PR0_PORT-}" "${PR0_BIND-}" >> trace
if [ "${PR0_NO_SSL-}" = 1 ]; then printf "no-ssl\n" >> trace; fi
exit "${PR0_TEST_SERVER_STATUS:-0}"
SERVER
  chmod 700 target/release/pr0-server
  exit 0
fi
'''
            (publisher / "init.sh").write_text(source.replace("set -euo pipefail\n", "set -euo pipefail\n" + build_stub, 1))
            (publisher / "source").write_text("latest")
            git(publisher, "add", "."); git(publisher, "commit", "-m", "update launcher")
            git(publisher, "push")
            env = {k: v for k, v in os.environ.items() if not k.startswith("PR0_")}
            def run(*args):
                return subprocess.run(["/bin/bash", str(script), *args], cwd=base, env=env,
                                      capture_output=True, text=True, timeout=20)
            for flag in ["--start", "--stop", "--update", "--startup"]:
                self.assertEqual(run("--uas", flag).returncode, 2)
            self.assertIn("--uas", run("--help").stdout)
            for port in ["0", "65536", "bad"]:
                self.assertEqual(run("--uas", "--port", port).returncode, 2)
            self.assertFalse((root / "trace").exists())
            result = run("--uas", "--host", "::1", "--port", "04001")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(git(root, "rev-parse", "HEAD"), git(publisher, "rev-parse", "HEAD"))
            self.assertEqual((root / "source").read_text(), "latest")
            self.assertEqual((root / "trace").read_text(), "build\nstart|::1|4001|\n")
            self.assertTrue(list((root / ".local/manual-runs").glob("*.pid")))
            (root / ".local/start-pr0former.sh").write_text(
                '#!/bin/bash\nexport PR0_BIND=127.0.0.1:4321\nexec "$PWD/target/release/pr0-server"\n')
            (root / "trace").unlink()
            self.assertEqual(run("--uas", "--port", "4002").returncode, 0)
            self.assertEqual((root / "trace").read_text(), "build\nstart||4002|127.0.0.1:4321\n")
            (root / "trace").unlink()
            self.assertEqual(run("--uas", "--no-ssl").returncode, 0)
            self.assertIn("no-ssl\n", (root / "trace").read_text())
            (root / "trace").unlink()
            env["PR0_TEST_BUILD_STATUS"] = "23"
            self.assertEqual(run("--uas").returncode, 23)
            self.assertEqual((root / "trace").read_text(), "build\n")
            del env["PR0_TEST_BUILD_STATUS"]
            env["PR0_TEST_SERVER_STATUS"] = "7"
            self.assertEqual(run("--uas").returncode, 7)
            del env["PR0_TEST_SERVER_STATUS"]
            # Failed pulls, overlapping local edits and divergence never build
            # or launch the old binary, and leave local work intact.
            (root / "trace").unlink()
            git(root, "remote", "set-url", "origin", str(remote) + "-missing")
            self.assertNotEqual(run("--uas").returncode, 0)
            self.assertFalse((root / "trace").exists())
            git(root, "remote", "set-url", "origin", str(remote))
            (publisher / "source").write_text("third")
            git(publisher, "add", "source"); git(publisher, "commit", "-m", "third"); git(publisher, "push")
            (root / "source").write_text("local work")
            git(root, "config", "merge.autoStash", "true")
            git(root, "config", "pull.rebase", "true")
            self.assertNotEqual(run("--uas").returncode, 0)
            self.assertEqual((root / "source").read_text(), "local work")
            self.assertFalse((root / "trace").exists())
            git(root, "add", "source"); git(root, "commit", "-m", "local commit")
            local_head = git(root, "rev-parse", "HEAD")
            self.assertNotEqual(run("--uas").returncode, 0)
            self.assertEqual(git(root, "rev-parse", "HEAD"), local_head)
            self.assertFalse((root / "trace").exists())

    def test_bind_flags(self):
        with tempfile.TemporaryDirectory(prefix="pr0former bind ") as directory:
            root = Path(directory)
            script = root / "init.sh"
            shutil.copyfile(Path(__file__).resolve().parents[1] / "init.sh", script)
            server = root / "target/release/pr0-server"
            server.parent.mkdir(parents=True)
            server.write_text('#!/bin/bash\nprintf "%s|%s|%s|%s" "${PR0_HOST-}" "${PR0_PORT-}" "${PR0_BIND-}" "${PR0_TLS_CERT-}" > binding\n')
            server.chmod(0o700)
            env = {k: v for k, v in os.environ.items() if not k.startswith("PR0_")}
            def run(*args):
                return subprocess.run(["/bin/bash", str(script), *args], env=env,
                                      capture_output=True, text=True, timeout=10)
            for args in [("--host", "localhost"), ("--port", "4001"),
                         ("--start", "--host"), ("--start", "--port"),
                         ("--start", "--host", "bad host"),
                         ("--stop", "--port", "4001")]:
                self.assertEqual(run(*args).returncode, 2, args)
            for port in ["", "0", "65536", "99999999999999999999", "abc", "-1"]:
                self.assertEqual(run("--start", "--port", port).returncode, 2, port)
            for saved in (False, True):
                if saved:
                    (root / ".local/start-pr0former.sh").write_text(
                        '#!/bin/bash\nexport PR0_BIND=127.0.0.1:4321\nexport PR0_TLS_CERT=saved.pem\nexec "$PWD/target/release/pr0-server"\n')
                for host in ["localhost", "127.0.0.1", "::1", "[::1]"]:
                    result = run("--port", "04001", "--start", "--host", host)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    suffix = "127.0.0.1:4321|saved.pem" if saved else "|"
                    self.assertEqual((root / "binding").read_text(), f"{host}|4001|{suffix}")
                self.assertEqual(run("--start", "--port", "4002").returncode, 0)
                self.assertTrue((root / "binding").read_text().startswith("|4002|"))
                self.assertEqual(run("--start", "--host", "localhost").returncode, 0)
                self.assertTrue((root / "binding").read_text().startswith("localhost||"))

    def test_manual_lifecycle(self):
        with tempfile.TemporaryDirectory(prefix="pr0former launcher ") as directory:
            root = Path(directory)
            script = root / "init.sh"
            shutil.copyfile(Path(__file__).resolve().parents[1] / "init.sh", script)

            def run(*args):
                return subprocess.run(["/bin/bash", str(script), *args],
                                      capture_output=True, text=True, timeout=20)

            self.assertEqual(run("--stop").returncode, 0)
            self.assertEqual(run("--start").returncode, 1)
            for args in [("--start", "--stop"), ("--stop", "--startup"),
                         ("--start", "--startup")]:
                self.assertEqual(run(*args).returncode, 2)
            self.assertIn("--stop", run("--help").stdout)
            master, slave = pty.openpty()
            try:
                process = subprocess.Popen(["/bin/bash", str(script), "--startup"],
                                           stdin=slave, stdout=subprocess.PIPE,
                                           stderr=subprocess.PIPE, text=True)
                os.write(master, b"4\n")
                output, error = process.communicate(timeout=5)
                self.assertEqual(process.returncode, 0, error)
                self.assertIn("Startup configuration unchanged", output)
            finally:
                os.close(master)
                os.close(slave)
            server = root / "target/release/pr0-server"
            server.parent.mkdir(parents=True)
            server.write_text('#!/bin/bash\nprintf ready > ready\nexec /bin/sleep 120\n')
            server.chmod(0o700)
            for saved in (False, True):
                if saved:
                    (root / ".local/start-pr0former.sh").write_text(
                        '#!/bin/bash\nexec "$PWD/target/release/pr0-server"\n')
                process = subprocess.Popen(["/bin/bash", str(script), "--start"],
                                           stdout=subprocess.PIPE, stderr=subprocess.PIPE)
                try:
                    for _ in range(100):
                        if (root / "ready").exists():
                            break
                        if process.poll() is not None:
                            self.fail(str(process.communicate()))
                        time.sleep(0.05)
                    self.assertTrue((root / "ready").exists())
                    result = run("--stop")
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(process.wait(timeout=5), -15)
                    self.assertIn("No servers", run("--stop").stdout)
                finally:
                    if process.poll() is None:
                        process.kill()
                    process.communicate()
                (root / "ready").unlink()
            # A stale record pointing at this live test process must not kill it.
            records = root / ".local/manual-runs"
            (records / f"{os.getpid()}.pid").write_text("wrong start time\n")
            self.assertEqual(run("--stop").returncode, 0)
            self.assertFalse(list(records.glob("*.pid")))
            server.write_text("#!/bin/bash\nexit 7\n")
            self.assertEqual(run("--start").returncode, 7)
            self.assertEqual(run("--stop").returncode, 0)


if __name__ == "__main__":
    unittest.main()
