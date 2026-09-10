"""Real server process tests; no desktop services or physical audio devices."""
import json
import os
from pathlib import Path
import queue
import sqlite3
import subprocess
import tempfile
import threading
import time
import unittest
import urllib.request
import urllib.error

ROOT = Path(__file__).resolve().parents[1]

class DesktopTests(unittest.TestCase):
    def start_server(self, root):
        env = dict(os.environ, PR0_DATA=str(root / 'data'), PR0_RECORDINGS_ROOT=str(root / 'recordings'),
                   PR0_WEB_ROOT=str(ROOT / 'web/dist'), PR0_DISABLE_NATIVE_DEVICES='1',
                   PR0_BIND='0.0.0.0:1', PR0_PORT='1', PR0_TLS_CERT='/missing', PR0_TLS_KEY='/missing')
        log = open(root / 'stderr.log', 'w')
        proc = subprocess.Popen([str(ROOT / 'target/release/pr0-server'), '--desktop'], cwd=root,
                                env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=log, text=True)
        self.addCleanup(log.close)
        def cleanup():
            if proc.poll() is None:
                proc.stdin.close()
                try: proc.wait(10)
                except subprocess.TimeoutExpired: proc.kill(); proc.wait()
            proc.stdout.close()
        self.addCleanup(cleanup)
        ready = queue.Queue()
        def read():
            for line in proc.stdout:
                if line.startswith('PR0_DESKTOP_READY '): ready.put(json.loads(line.split(' ', 1)[1]))
        threading.Thread(target=read, daemon=True).start()
        return proc, ready.get(timeout=30)

    def request(self, ready, path, data=None, method=None, authenticated=True, host=None):
        headers = {'X-Pr0former':'1', 'Content-Type':'application/json'}
        if authenticated: headers['Cookie'] = 'pr0_session=' + ready['session']
        if host: headers['Host'] = host
        req = urllib.request.Request(ready['url'] + path, data=None if data is None else json.dumps(data).encode(), headers=headers, method=method)
        try:
            with urllib.request.urlopen(req, timeout=10) as response:
                body=response.read()
                return response.status, json.loads(body) if response.headers.get_content_type() == 'application/json' else body
        except urllib.error.HTTPError as error:
            with error: return error.code, error.read()

    def test_private_login_persistence_and_shutdown_finalizes_recording(self):
        with tempfile.TemporaryDirectory(prefix='pr0-desktop-test-') as directory:
            root=Path(directory)
            proc, ready=self.start_server(root)
            self.assertTrue(ready['url'].startswith('http://127.0.0.1:'))
            self.assertEqual(self.request(ready, '/api/me', authenticated=False)[0], 401)
            self.assertEqual(self.request(ready, '/api/me', host='attacker.invalid')[0], 403)
            status, owner=self.request(ready, '/api/me')
            self.assertEqual(status, 200); self.assertEqual(owner['username'], 'admin')
            self.assertIn(b'<!doctype html>', self.request(ready, '/')[1].lower())
            status, project=self.request(ready, '/api/projects', {'name':'Desktop archive','mode':'freeform'})
            self.assertEqual(status, 200)
            def node(id, kind, channels=1, parameters=None):
                return dict(id=id,kind=kind,label=id,x=0,y=0,channels=channels,parameters=parameters or {})
            project['parts']=[]
            project['graph']={'nodes':[node('Archive','record'),node('Tone','oscillator',2,{'frequency':440,'amplitude':.2}),node('Start','value',parameters={'value':0})],
                              'edges':[dict(id='audio',source='Tone',source_port='out',target='Archive',target_port='in'),dict(id='start',source='Start',source_port='out',target='Archive',target_port='start')]}
            path='/api/projects/'+project['id']
            self.assertEqual(self.request(ready,path,project,'PUT')[0],200)
            self.assertEqual(self.request(ready,path+'/engine',{'enabled':True})[0],200)
            current=self.request(ready,path)[1]['project']
            self.assertEqual(self.request(ready,path+'/parameter',{'node':'Start','parameter':'value','value':1,'revision':current['revision']},'PUT')[0],200)
            time.sleep(.4)
            proc.stdin.close(); self.assertEqual(proc.wait(15),0)
            takes=[json.loads(p.read_text()) for p in (root/'recordings').rglob('*.json')]
            self.assertEqual(len(takes),1); self.assertTrue(takes[0]['complete']); self.assertEqual(takes[0]['channels'],2)
            with sqlite3.connect(root/'data/pr0former.sqlite') as db:
                self.assertEqual(db.execute('SELECT count(*) FROM sessions WHERE token=?',(ready['session'],)).fetchone()[0],0)
            second, next_ready=self.start_server(root)
            self.assertEqual(self.request(next_ready,'/api/me')[1]['id'],owner['id'])
            self.assertNotEqual(next_ready['session'],ready['session'])
            self.assertEqual(self.request(next_ready,path)[0],200)
            second.stdin.close(); self.assertEqual(second.wait(15),0)

if __name__ == '__main__': unittest.main()
