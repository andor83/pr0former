"""Isolated certificate setup and real TLS checks; never changes OS trust or services."""
import http.client
import json
import os
from pathlib import Path
import pty
import shutil
import socket
import ssl
import subprocess
import tempfile
import time
import unittest

REPO = Path(__file__).resolve().parents[1]

def port():
    with socket.socket() as s:
        s.bind(('127.0.0.1', 0))
        return s.getsockname()[1]

class SSLTests(unittest.TestCase):
    def test_local_certificate_and_server_modes(self):
        binary = Path(os.environ.get('PR0_SSL_SERVER', REPO / 'target/debug/pr0-server')).resolve()
        self.assertTrue(binary.exists(), 'Build the server before running this integration test')
        with tempfile.TemporaryDirectory(prefix='pr0 TLS ') as tmp:
            root = Path(tmp)
            shutil.copy(REPO / 'init.sh', root / 'init.sh')
            server = root / 'target/release/pr0-server'
            server.parent.mkdir(parents=True)
            server.symlink_to(binary)
            env = {k:v for k,v in os.environ.items() if not k.startswith('PR0_')}
            env.update(PR0_DATA=str(root / 'data'), PR0_DISABLE_NATIVE_DEVICES='1')
            def run(*args):
                return subprocess.run(['/bin/bash', str(root / 'init.sh'), *args], env=env, capture_output=True, text=True, timeout=30)
            for args in [('--setup-ssl','--start'), ('--remove-ssl','--uas'), ('--no-ssl',)]:
                self.assertEqual(run(*args).returncode, 2)
            master, slave = pty.openpty()
            try:
                proc = subprocess.Popen(['/bin/bash', str(root / 'init.sh'), '--setup-ssl'], stdin=slave, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)
                os.write(master, b'localhost 127.0.0.1 ::1 studio.local\n')
                out, err = proc.communicate(timeout=30)
                self.assertEqual(proc.returncode, 0, err.decode())
            finally:
                os.close(master); os.close(slave)
            certs = root / 'certs'
            self.assertEqual((certs / 'server-key.pem').stat().st_mode & 0o777, 0o600)
            subprocess.run(['openssl','verify','-CAfile',str(certs/'ca.pem'),str(certs/'server.pem')],check=True,capture_output=True)
            original_certificate = (certs / 'server.pem').read_bytes()
            # Simulate certificates created by an older launcher; startup migrates
            # the whole directory while preserving certificate identity and mode.
            (root / '.local').mkdir(exist_ok=True)
            certs.rename(root / '.local/ssl')
            context = ssl.create_default_context(cafile=str(root / '.local/ssl/ca.pem'))
            # An older saved startup address must not defeat the managed TLS mode.
            (root / '.local/start-pr0former.sh').write_text('#!/bin/bash\nexport PR0_BIND=127.0.0.1:4000\nexport PR0_TLS_CERT=obsolete.pem\nexport PR0_TLS_KEY=obsolete-key.pem\nexec "$PWD/target/release/pr0-server"\n')
            for mode in ['tls', 'override', 'removed']:
                https_port, http_port = port(), port()
                env['PR0_HTTP_PORT'] = str(http_port)
                if mode == 'removed':
                    self.assertEqual(run('--remove-ssl').returncode, 0)
                    self.assertFalse((certs/'server.pem').exists())
                    self.assertFalse((certs/'ca-key.pem').exists())
                args = ['--start','--host','127.0.0.1','--port',str(https_port)]
                if mode == 'override': args += ['--no-ssl']
                proc = subprocess.Popen(['/bin/bash',str(root/'init.sh'),*args],env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE)
                try:
                    for _ in range(150):
                        try:
                            conn = http.client.HTTPSConnection('127.0.0.1',https_port,context=context,timeout=1) if mode=='tls' else http.client.HTTPConnection('127.0.0.1',https_port,timeout=1)
                            conn.request('GET','/api/status')
                            response=conn.getresponse()
                            self.assertEqual(response.status,200)
                            response.read(); conn.close()
                            break
                        except OSError:
                            if proc.poll() is not None: self.fail(str(proc.communicate()))
                            time.sleep(.05)
                    else: self.fail('Server did not start')
                    if mode=='tls':
                        self.assertEqual((certs/'server.pem').read_bytes(), original_certificate)
                        self.assertFalse((root/'.local/ssl').exists())
                        for version in [ssl.TLSVersion.TLSv1_2, ssl.TLSVersion.TLSv1_3]:
                            version_context = ssl.create_default_context(cafile=str(certs/'ca.pem'))
                            version_context.minimum_version = version
                            version_context.maximum_version = version
                            conn = http.client.HTTPSConnection('localhost', https_port, context=version_context)
                            conn.request('GET', '/api/status')
                            response = conn.getresponse()
                            self.assertEqual(response.status, 200)
                            response.read(); conn.close()
                        conn=http.client.HTTPConnection('127.0.0.1',http_port)
                        conn.request('GET','/a%20b?test=1')
                        response=conn.getresponse()
                        self.assertEqual(response.status,307)
                        self.assertEqual(response.getheader('Location'),f'https://127.0.0.1:{https_port}/a%20b?test=1')
                        response.read();conn.close()
                        conn=http.client.HTTPSConnection('localhost',https_port,context=context)
                        conn.request('POST','/api/register',json.dumps({'username':'ssl-test','password':'test1234'}),{'Content-Type':'application/json','X-Pr0former':'1'})
                        response=conn.getresponse();self.assertEqual(response.status,200)
                        self.assertIn('Secure',response.getheader('Set-Cookie'));response.read();conn.close()
                        browser = r"""const {chromium}=require('@playwright/test');(async()=>{const b=await chromium.launch();try{const p=await b.newPage({ignoreHTTPSErrors:true});await p.goto(process.argv[1]+'/api/status');if(!await p.evaluate(()=>isSecureContext && typeof navigator.mediaDevices.getUserMedia==='function'))throw Error('No secure audio context');}finally{await b.close()}})().catch(e=>{console.error(e);process.exit(1)})"""
                        subprocess.run(['node','-e',browser,f'https://127.0.0.1:{https_port}'],cwd=REPO/'web',check=True,capture_output=True,timeout=30)
                    elif mode=='override':
                        self.assertTrue((certs/'server.pem').exists())
                finally:
                    proc.terminate()
                    try: proc.communicate(timeout=10)
                    except subprocess.TimeoutExpired: proc.kill();proc.communicate()

if __name__ == '__main__': unittest.main()
