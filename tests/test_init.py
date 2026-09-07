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
                port = 4000
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
